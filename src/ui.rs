mod acceleration;
mod animation;
mod context;
mod effects;
mod theme;
mod tools;
use theme::*;

use crate::{
    document::{Document, Workspace},
    edit::{self, History},
    media::{self, Asset},
    model::*,
    presets,
    preview::{Preview, PreviewEvent},
    process::ProcessGroup,
    render::{self, Job},
    storage::{Storage, atomic_json, load_project},
};

use crossbeam_channel::{Receiver, Sender, unbounded};

use eframe::egui::{
    self, Color32, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextureHandle, Vec2,
};

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

enum Event {
    Asset(Asset),
    Files(Vec<PathBuf>),
    Message(String),
    Job(Job),
    Acceleration(String),
    Document(Document, Vec<Asset>),
}

struct Import {
    path: PathBuf,
    id: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Media,
    Audio,
    Text,
    Graphics,
    Transitions,
    Effects,
    Motion,
}

#[derive(Clone, Copy, PartialEq)]
enum PropertyTab {
    Properties,
    Audio,
    Color,
}

#[derive(Clone, Copy, PartialEq)]
enum WorkMode {
    Edit,
    Color,
    Audio,
    Motion,
}

struct Drag {
    clip: Clip,
    pointer: f32,
    edge: i8,
}

pub struct FrameApp {
    pub project: Project,
    pub assets: Arc<HashMap<String, Asset>>,
    storage: Storage,
    history: History,
    selected: Option<String>,
    tab: Tab,
    property_tab: PropertyTab,
    time: f64,
    playing: bool,
    preview: Preview,
    preview_texture: Option<TextureHandle>,
    preview_dirty: Option<Instant>,
    preview_running: bool,
    events: Receiver<Event>,
    send: Sender<Event>,
    imports: Option<Sender<Import>>,
    import_group: ProcessGroup,
    import_threads: Vec<JoinHandle<()>>,
    exports: Option<(ProcessGroup, JoinHandle<()>)>,
    acceleration: crate::acceleration::Preferences,
    acceleration_task: Option<(ProcessGroup, JoinHandle<()>)>,
    acceleration_result: String,
    pub job: Option<Job>,
    thumb_cache: HashMap<String, TextureHandle>,
    search: String,
    zoom: f32,
    snap: bool,
    drag: Option<Drag>,
    dirty: Option<Instant>,
    last_error: String,
    notice: Option<(String, Instant)>,
    saved: String,
    export_window: bool,
    settings_window: bool,
    preview_error_window: bool,
    theme_window: bool,
    motion_window: bool,
    vfx: effects::VfxState,
    motion_key: usize,
    motion_settings: presets::MotionSettings,
    animation_clipboard: Option<Clip>,
    clip_clipboard: Option<Clip>,
    clipboard_text: Option<String>,
    rename_clip: Option<(String, String)>,
    media_details: Option<Asset>,
    timeline_context: (f64, usize),
    workspace: WorkMode,
    theme: Theme,
    applied_theme: Option<Theme>,
    template_window: bool,
    template_new: bool,
    help_window: bool,
    range_window: bool,
    snapshots_window: bool,
    range: [f64; 2],
    export_size: [u32; 2],
    property_drag: bool,
    quit_error: bool,
}

impl FrameApp {
    pub fn new(storage: Storage) -> anyhow::Result<Self> {
        media::seed(&storage.root)?;

        let mut assets = storage.assets();

        let project = storage.load()?.unwrap_or_else(demo_project);

        let selected = project.clips.first().map(|c| c.id.clone());

        let (tx, rx) = unbounded();

        let (import_tx, import_rx) = unbounded::<Import>();

        let (proxy_tx, proxy_rx) = unbounded::<Asset>();

        let group = ProcessGroup::default();

        let root = storage.root.clone();

        let tx2 = tx.clone();

        let group2 = group.clone();

        let probe_thread = thread::spawn(move || {
            while let Ok(task) = import_rx.recv() {
                if group2.cancelled() {
                    break;
                }

                match media::probe(&task.path, &group2) {
                    Ok(mut a) => {
                        if let Some(id) = task.id {
                            a.id = id;
                        }

                        let dir = root.join("media").join(&a.id);

                        let result = std::fs::create_dir_all(&dir).and_then(|_| {
                            atomic_json(&dir.join("meta.json"), &a).map_err(std::io::Error::other)
                        });

                        if let Err(e) = result {
                            let _ = tx2.send(Event::Message(format!(
                                "Could not save imported media: {e}"
                            )));

                            continue;
                        }

                        let _ = tx2.send(Event::Asset(a.clone()));

                        let _ = proxy_tx.send(a);
                    }

                    Err(e) => {
                        let _ = tx2.send(Event::Message(format!(
                            "Could not import {}: {e}",
                            task.path.file_name().unwrap_or_default().to_string_lossy()
                        )));
                    }
                }
            }
        });

        let root = storage.root.clone();

        let tx2 = tx.clone();

        let group2 = group.clone();

        let proxy_thread = thread::spawn(move || {
            while let Ok(a) = proxy_rx.recv() {
                if group2.cancelled() {
                    break;
                }

                let mut failed = a.clone();

                match media::prepare(a, &root, &group2, |a| {
                    let _ = tx2.send(Event::Asset(a));
                }) {
                    Ok(a) => {
                        let _ = tx2.send(Event::Asset(a));
                    }

                    Err(e) => {
                        if !group2.cancelled() {
                            failed.status = "error".into();

                            failed.error = Some(e.to_string());

                            let _ = atomic_json(
                                &root.join("media").join(&failed.id).join("meta.json"),
                                &failed,
                            );

                            let _ = tx2.send(Event::Asset(failed));
                        }
                    }
                }
            }
        });

        for a in assets.values_mut() {
            if matches!(a.status.as_str(), "processing" | "proxying") {
                a.status = "processing".into();

                let _ = import_tx.send(Import {
                    path: a.path.clone(),
                    id: Some(a.id.clone()),
                });
            }
        }

        let theme = std::fs::read(storage.root.join("preferences.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Preferences>(&bytes).ok())
            .unwrap_or_default()
            .theme;
        let acceleration = crate::acceleration::Preferences::load(&storage.root)?;
        acceleration.apply();
        Ok(Self {
            project,
            assets: Arc::new(assets),
            storage,
            history: History::default(),
            selected,
            tab: Tab::Media,
            property_tab: PropertyTab::Properties,
            time: 2.5,
            playing: false,
            preview: Preview::default(),
            preview_texture: None,
            preview_dirty: Some(Instant::now() - Duration::from_secs(1)),
            preview_running: false,
            events: rx,
            send: tx,
            imports: Some(import_tx),
            import_group: group,
            import_threads: vec![probe_thread, proxy_thread],
            exports: None,
            acceleration,
            acceleration_task: None,
            acceleration_result: String::new(),
            job: None,
            thumb_cache: HashMap::new(),
            search: String::new(),
            zoom: 32.,
            snap: true,
            drag: None,
            dirty: None,
            last_error: String::new(),
            notice: None,
            saved: "All changes saved".into(),
            export_window: false,
            settings_window: false,
            preview_error_window: false,
            theme_window: false,
            motion_window: false,
            vfx: effects::VfxState::default(),
            motion_key: 0,
            motion_settings: presets::MotionSettings::default(),
            animation_clipboard: None,
            clip_clipboard: None,
            clipboard_text: None,
            rename_clip: None,
            media_details: None,
            timeline_context: (0., 1),
            workspace: WorkMode::Edit,
            theme,
            applied_theme: None,
            template_window: false,
            template_new: false,
            help_window: false,
            range_window: false,
            snapshots_window: false,
            range: [0., 1. / 30.],
            export_size: [1920, 1080],
            property_drag: false,
            quit_error: false,
        })
    }

    pub fn configure(ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();

        style.visuals = if palette().light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };

        style.visuals.panel_fill = palette().panel;
        style.visuals.window_fill = palette().panel;
        style.visuals.override_text_color = Some(palette().text);
        style.visuals.window_corner_radius = egui::CornerRadius::same(12);
        style.visuals.window_highlight_topmost = false;
        style.visuals.menu_corner_radius = egui::CornerRadius::same(10);
        style.visuals.window_stroke = Stroke::new(1.0_f32, palette().border);
        style.visuals.window_shadow = egui::epaint::Shadow {
            offset: [0, 12],
            blur: 32,
            spread: 2,
            color: Color32::from_black_alpha(100),
        };
        style.visuals.extreme_bg_color = palette().canvas;
        style.visuals.faint_bg_color = palette().card;
        style.visuals.selection.bg_fill = mix(palette().card, palette().accent, 0.35);
        style.visuals.selection.stroke = Stroke::new(1.0_f32, palette().text);
        style.visuals.hyperlink_color = palette().cyan;
        style.visuals.widgets.noninteractive.bg_stroke =
            Stroke::new(1.0_f32, palette().border.gamma_multiply(0.65));
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, palette().muted);
        for (widget, color) in [
            (&mut style.visuals.widgets.inactive, palette().control),
            (
                &mut style.visuals.widgets.hovered,
                mix(palette().control, palette().accent, 0.22),
            ),
            (
                &mut style.visuals.widgets.active,
                mix(palette().control, palette().accent, 0.4),
            ),
            (
                &mut style.visuals.widgets.open,
                mix(palette().control, palette().accent, 0.25),
            ),
        ] {
            widget.corner_radius = egui::CornerRadius::same(7);
            widget.bg_fill = color;
            widget.weak_bg_fill = color;
            widget.bg_stroke = Stroke::NONE;
            widget.fg_stroke = Stroke::new(1.0_f32, palette().text);
        }

        style.spacing.item_spacing = Vec2::new(10., 8.);

        style.spacing.button_padding = Vec2::new(11., 7.);
        style.spacing.menu_margin = egui::Margin::same(8);

        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.));

        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(12.));

        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(11.));

        ctx.set_style(style);

        let mut fonts = egui::FontDefinitions::default();

        fonts.font_data.insert(
            "frame".into(),
            egui::FontData::from_static(include_bytes!("../assets/fonts/DejaVuSans.ttf")).into(),
        );

        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "frame".into());

        ctx.set_fonts(fonts);
    }

    fn notify(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
    }

    fn modified(&mut self) {
        self.dirty = Some(Instant::now());

        self.saved = "Saving…".into();

        self.preview_dirty = Some(Instant::now());

        if self.playing {
            self.playing = false;

            self.preview.stop();

            self.preview_running = false;
        }
    }

    fn save(&mut self) -> bool {
        match self.storage.save(&self.project) {
            Ok(()) => {
                self.saved = "All changes saved".into();

                self.dirty = None;

                true
            }

            Err(e) => {
                self.saved = "Save failed — download a backup".into();

                self.last_error = e.to_string();

                false
            }
        }
    }

    fn select(&mut self, id: String) {
        self.selected = Some(id);
    }

    fn selected_index(&self) -> Option<usize> {
        self.selected
            .as_ref()
            .and_then(|id| self.project.clips.iter().position(|c| &c.id == id))
    }

    fn import_picker(&self, relink: Option<String>) {
        let tx = self.send.clone();

        let imports = self.imports.as_ref().unwrap().clone();

        thread::spawn(move || {
            let picker = rfd::FileDialog::new().add_filter(
                "Video, audio and images",
                &[
                    "mp4", "mov", "mkv", "webm", "avi", "mxf", "mts", "mp3", "wav", "aac", "m4a",
                    "flac", "aiff", "ogg", "opus", "png", "jpg", "jpeg", "webp",
                ],
            );

            if let Some(id) = relink {
                if let Some(path) = picker.pick_file() {
                    let _ = imports.send(Import { path, id: Some(id) });
                }
            } else if let Some(files) = picker.pick_files() {
                let _ = tx.send(Event::Files(files));
            }
        });
    }

    fn add_asset(&mut self, id: &str, at: Option<f64>, track: Option<usize>) {
        let Some(a) = self.assets.get(id) else {
            return;
        };

        if !a.ready() {
            self.notify(
                a.error
                    .clone()
                    .unwrap_or_else(|| "Media is still preparing its preview".into()),
            );

            return;
        }

        let kind = a.kind;

        let track = track.unwrap_or(if kind == Kind::Audio { 4 } else { 1 });

        if track >= 3 && !a.has_audio {
            self.notify("This file has no audio stream for an audio track");

            return;
        }

        let kind = if track >= 3 { Kind::Audio } else { kind };

        let start = at.unwrap_or_else(|| {
            self.project
                .clips
                .iter()
                .filter(|c| c.track == track)
                .map(Clip::end)
                .fold(0., f64::max)
        });

        let c = Clip {
            asset_id: Some(id.into()),
            kind,
            name: a.name.clone(),
            start,
            track,
            duration: if kind == Kind::Image { 5. } else { a.duration },
            ..Clip::default()
        };

        self.history.remember(&self.project);

        self.selected = Some(c.id.clone());

        self.project.clips.push(c);

        self.time = start;

        self.modified();
    }

    fn add_layer(&mut self, kind: Kind, graphic: Graphic, text: &str, size: u32) {
        let c = Clip {
            kind,
            graphic,
            text: text.into(),
            name: text.lines().next().unwrap_or("Graphic").into(),
            font_size: size,
            track: 0,
            start: self.time,
            transition: Transition::Fade,
            ..Clip::default()
        };

        self.history.remember(&self.project);

        self.selected = Some(c.id.clone());

        self.project.clips.push(c);

        self.property_tab = PropertyTab::Properties;

        self.modified();
    }

    fn split(&mut self) {
        let Some(id) = self.selected.clone() else {
            return;
        };

        let mut p = self.project.clone();

        if edit::split(&mut p, &id, self.time) {
            self.history.remember(&self.project);

            self.project = p;

            self.modified();
        } else {
            self.notify("Place the playhead inside the selected clip to split");
        }
    }

    fn delete(&mut self) {
        if let Some(id) = self.selected.take() {
            self.history.remember(&self.project);

            self.project.clips.retain(|c| c.id != id);

            self.modified();
        }
    }

    fn duplicate(&mut self) {
        if let Some(i) = self.selected_index() {
            let mut c = self.project.clips[i].clone();

            c.id = uid();

            c.start = c.end();

            if c.end() > 86400. {
                return;
            }

            self.history.remember(&self.project);

            self.selected = Some(c.id.clone());

            self.project.clips.push(c);

            self.modified();
        }
    }

    fn toggle_play(&mut self) {
        if self.project.clips.is_empty() {
            return;
        }

        self.playing = !self.playing;

        if self.time >= self.project.duration() {
            self.time = 0.;
        }

        self.preview_dirty = Some(Instant::now() - Duration::from_secs(1));

        if !self.playing {
            self.preview.stop();

            self.preview_running = false;
        }
    }

    fn seek(&mut self, time: f64) {
        self.time = time.clamp(0., self.project.duration().max(0.));

        self.playing = false;

        self.preview.stop();

        self.preview_running = false;

        self.preview_dirty = Some(Instant::now());
    }

    fn switch(&mut self, p: Project) {
        if let Err(e) = p
            .validate()
            .and_then(|_| self.storage.snapshot(&self.project).map(|_| ()))
        {
            self.notify(format!("Could not preserve project before switching: {e}"));

            return;
        }

        self.history.remember(&self.project);

        self.project = p;

        self.selected = self.project.clips.first().map(|c| c.id.clone());

        self.time = 0.;

        self.modified();
    }

    fn open_project(&self) {
        let tx = self.send.clone();
        let assets = self.assets.clone();
        let root = self.storage.root.clone();

        thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Frame project", &["frame", "json"])
                .pick_file()
            {
                let event = match Document::read(&path).and_then(|mut doc| {
                    let restored = doc.restore_at(&root, &assets)?;
                    Ok((doc, restored))
                }) {
                    Ok((doc, restored)) => Event::Document(doc, restored),
                    Err(e) => Event::Message(e.to_string()),
                };

                let _ = tx.send(event);
            }
        });
    }

    fn backup(&self) {
        let doc = Document::capture(
            &self.project,
            &self.assets,
            Workspace {
                playhead: self.time,
                zoom: self.zoom,
                selected: self.selected.clone(),
            },
        );

        let tx = self.send.clone();
        let name: String = self
            .project
            .name
            .chars()
            .filter(|ch| !ch.is_control() && !"<>:\"/\\|?*".contains(*ch))
            .take(100)
            .collect();
        let filename = format!(
            "{}.frame",
            if name.trim().is_empty() {
                "project"
            } else {
                name.trim()
            }
        );
        thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Frame project", &["frame"])
                .set_file_name(filename)
                .save_file()
            {
                let event = match doc.save(&path) {
                    Ok(()) => Event::Message(
                        "Frame project saved with timeline, graphics, keyframes and media links."
                            .into(),
                    ),
                    Err(e) => Event::Message(e.to_string()),
                };

                let _ = tx.send(event);
            }
        });
    }

    fn export(&mut self) {
        if self.exports.as_ref().is_some_and(|(_, h)| !h.is_finished()) {
            self.notify("An export is already running");

            return;
        }

        let mut project = self.project.clone();

        project.width = self.export_size[0];

        project.height = self.export_size[1];

        if let Err(e) = project.validate() {
            self.notify(e.to_string());

            return;
        }

        if project.clips.is_empty() {
            self.notify("Add clips before exporting");

            return;
        }

        let assets = self.assets.clone();
        let encoder = self.acceleration.encoder;

        let tx = self.send.clone();

        let root = self.storage.root.clone();

        let group = ProcessGroup::default();

        let token = group.clone();

        self.job = Some(Job {
            id: uid(),
            status: "Choosing destination".into(),
            progress: 0.,
            error: None,
            output: None,
        });

        let handle = thread::spawn(move || {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("MP4 video", &["mp4"])
                .set_file_name("frame-export.mp4")
                .save_file()
            else {
                let _ = tx.send(Event::Message(
                    "Export destination selection cancelled".into(),
                ));

                return;
            };

            let mut job = Job {
                id: uid(),
                status: "Rendering".into(),
                progress: 0.,
                error: None,
                output: Some(path.clone()),
            };

            let dir = root.join("jobs").join(&job.id);

            let _ = std::fs::create_dir_all(&dir);

            let _ = tx.send(Event::Job(job.clone()));

            let mut last = Instant::now() - Duration::from_secs(1);

            let mut state_error = None;

            let result = render::export_with_encoder(
                &project,
                &assets,
                &dir,
                &path,
                &token,
                encoder,
                |v, status| {
                    job.status = status.into();
                    if last.elapsed() > Duration::from_millis(300) || v >= 1. {
                        job.progress = v;

                        if let Err(e) = render::persist_job(&dir, &job) {
                            state_error = Some(e.to_string());

                            token.cancel();
                        }

                        let _ = tx.send(Event::Job(job.clone()));

                        last = Instant::now();
                    }
                },
            );

            match result {
                Ok(()) => {
                    if !job.status.starts_with("Complete") {
                        job.status = "Complete".into();
                    }

                    job.progress = 1.;
                }

                Err(e) => {
                    job.status = if token.cancelled() {
                        "Cancelled"
                    } else {
                        "Failed"
                    }
                    .into();

                    job.error = Some(state_error.unwrap_or_else(|| e.to_string()));
                }
            }

            let _ = render::persist_job(&dir, &job);

            let _ = tx.send(Event::Job(job));
        });

        self.exports = Some((group, handle));
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Acceleration(result) => self.acceleration_result = result,
                Event::Asset(a) => {
                    let id = a.id.clone();

                    Arc::make_mut(&mut self.assets).insert(id.clone(), a);

                    self.thumb_cache.remove(&id);

                    if !self.playing {
                        self.preview_dirty = Some(Instant::now());
                    }
                }

                Event::Files(files) => {
                    if files.len() + self.assets.len() > 2000 {
                        self.notify("Library limit: 2,000 media items");
                    } else {
                        for path in files {
                            if path
                                .extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("frame"))
                            {
                                let tx = self.send.clone();
                                let assets = self.assets.clone();
                                let root = self.storage.root.clone();
                                thread::spawn(move || {
                                    let event = match Document::read(&path).and_then(|mut doc| {
                                        let restored = doc.restore_at(&root, &assets)?;
                                        Ok((doc, restored))
                                    }) {
                                        Ok((doc, restored)) => Event::Document(doc, restored),
                                        Err(e) => Event::Message(e.to_string()),
                                    };
                                    let _ = tx.send(event);
                                });
                                continue;
                            }
                            if let Some(tx) = &self.imports {
                                let _ = tx.send(Import { path, id: None });
                            }
                        }
                    }
                }

                Event::Message(msg) => self.notify(msg),
                Event::Job(job) => self.job = Some(job),
                Event::Document(doc, restored) => self.finish_document(doc, restored),
            }
        }

        while let Some(frame) = self.preview.next_event() {
            match frame {
                PreviewEvent::Frame {
                    pixels,
                    width,
                    height,
                    time,
                } => {
                    let image =
                        egui::ColorImage::from_rgb([width as usize, height as usize], &pixels);

                    if let Some(texture) = &mut self.preview_texture {
                        texture.set(image, egui::TextureOptions::LINEAR);
                    } else {
                        self.preview_texture =
                            Some(ctx.load_texture("preview", image, egui::TextureOptions::LINEAR));
                    }

                    if self.playing {
                        self.time = time;
                    }
                }

                PreviewEvent::Error(error) => {
                    self.last_error = error;

                    self.playing = false;

                    self.preview_running = false;
                }

                PreviewEvent::Finished => {
                    self.preview_running = false;

                    if self.playing {
                        let next = self.time + 1. / self.project.fps as f64;

                        if next >= self.project.duration() - 0.02 {
                            self.playing = false;

                            self.time = self.project.duration();
                        } else {
                            self.time = next;

                            self.preview_dirty = Some(Instant::now() - Duration::from_secs(1));
                        }
                    }
                }
            }
        }

        if self
            .preview_dirty
            .is_some_and(|t| t.elapsed() >= Duration::from_millis(90))
        {
            self.preview_dirty = None;

            self.last_error.clear();

            self.preview.start(
                self.project.clone(),
                self.assets.clone(),
                self.time
                    .min((self.project.duration() - 1. / self.project.fps as f64).max(0.)),
                self.playing,
                self.storage.root.clone(),
                true,
            );

            self.preview_running = true;
        }

        if self
            .dirty
            .is_some_and(|t| t.elapsed() > Duration::from_millis(750))
        {
            self.save();
        }

        if self
            .notice
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed() > Duration::from_secs(7))
        {
            self.notice = None;
        }

        let files = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect::<Vec<_>>()
        });

        if !files.is_empty() {
            let _ = self.send.send(Event::Files(files));
        }

        if self.playing
            || self.preview_running
            || self.dirty.is_some()
            || self
                .assets
                .values()
                .any(|a| matches!(a.status.as_str(), "processing" | "proxying"))
            || self.exports.as_ref().is_some_and(|(_, h)| !h.is_finished())
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }

        ctx.input(|i| {
            for (key, action, native) in [
                (
                    egui::Key::C,
                    context::ClipCommand::Copy,
                    i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                ),
                (
                    egui::Key::X,
                    context::ClipCommand::Cut,
                    i.events.iter().any(|e| matches!(e, egui::Event::Cut)),
                ),
                (egui::Key::D, context::ClipCommand::Duplicate, false),
            ] {
                if (native || (i.modifiers.command && i.key_pressed(key)))
                    && let Some(id) = self.selected.clone()
                {
                    self.clip_command(&id, action);
                }
            }
            if (i.modifiers.command && i.key_pressed(egui::Key::V))
                || i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))
            {
                self.paste_clip(self.time, None);
            }
            if i.key_pressed(egui::Key::Space) {
                self.toggle_play();
            }

            if i.key_pressed(egui::Key::S) && !i.modifiers.command {
                self.split();
            }

            if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                self.delete();
            }

            if i.key_pressed(egui::Key::ArrowRight) {
                self.seek(
                    self.time + if i.modifiers.shift { 10. } else { 1. } / self.project.fps as f64,
                );
            }

            if i.key_pressed(egui::Key::ArrowLeft) {
                self.seek(
                    self.time - if i.modifiers.shift { 10. } else { 1. } / self.project.fps as f64,
                );
            }

            if i.modifiers.command && i.key_pressed(egui::Key::Z) {
                if i.modifiers.shift {
                    self.history.redo(&mut self.project);
                } else {
                    self.history.undo(&mut self.project);
                }

                self.modified();
            }

            if i.modifiers.command && i.key_pressed(egui::Key::S) {
                self.save();
            }
        });
    }

    fn top(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top")
            .exact_height(56.)
            .frame(surface(palette().rail, 12))
            .show(ctx, |ui| {
                ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                ui.horizontal_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(27., 26.), Sense::hover());

                    for (i, width) in [24., 17., 7.].iter().enumerate() {
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                rect.min + Vec2::new(0., i as f32 * 9.),
                                Vec2::new(*width, 6.),
                            ),
                            1.,
                            palette().accent,
                        );
                    }

                    ui.label(RichText::new("frame").size(23.).strong());

                    ui.add_space(4.);

                    ui.separator();

                    ui.menu_button(self.project.name.clone(), |ui| {
                        if ui.button("New project").clicked() {
                            self.template_new = true;
                            self.template_window = true;

                            ui.close();
                        }

                        if ui.button("Open project…").clicked() {
                            self.open_project();

                            ui.close();
                        }

                        if ui.button("Save project as .frame…").clicked() {
                            self.backup();

                            ui.close();
                        }

                        if ui.button("Save snapshot").clicked() {
                            let msg = match self.storage.snapshot(&self.project) {
                                Ok(_) => "Project snapshot saved".into(),
                                Err(e) => e.to_string(),
                            };

                            self.notify(msg);

                            ui.close();
                        }

                        if ui.button("Restore snapshot…").clicked() {
                            self.snapshots_window = true;

                            ui.close();
                        }

                        if ui.button("Load rocket demo").clicked() {
                            self.switch(demo_project());

                            self.seek(2.5);

                            ui.close();
                        }
                    });

                    ui.label(RichText::new(&self.saved).small().color(palette().muted));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Export video").color(palette().on_accent()),
                                )
                                .fill(palette().accent),
                            )
                            .clicked()
                        {
                            self.export_size = [self.project.width, self.project.height];

                            self.export_window = true;
                        }

                        if ui.button("?").clicked() {
                            self.help_window = true;
                        }

                        if ui.button("Save .frame").clicked() {
                            self.backup();
                        }
                        if ui
                            .button(format!(
                                "{} × {} · {} fps",
                                self.project.width, self.project.height, self.project.fps
                            ))
                            .on_hover_text("Project format and presets")
                            .clicked()
                        {
                            self.template_new = false;
                            self.template_window = true;
                        }
                        if ui
                            .button("Theme")
                            .on_hover_text("Choose an appearance theme")
                            .clicked()
                        {
                            self.theme_window = true;
                        }
                        if ui.button("Settings").clicked() {
                            self.settings_window = true;
                        }
                    });
                });
            });
    }

    fn library(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("rail")
            .exact_width(84.)
            .frame(surface(palette().rail, 7))
            .resizable(false)
            .show(ctx, |ui| {
                ui.style_mut().spacing.button_padding.x = 3.;
                ui.add_space(12.);

                for (tab, icon, name, _accent) in [
                    (Tab::Media, "▣", "Media", palette().accent),
                    (Tab::Audio, "♫", "Audio", palette().cyan),
                    (Tab::Text, "T", "Text", palette().violet),
                    (Tab::Graphics, "◇", "Graphics", palette().amber),
                    (Tab::Transitions, "▱", "Transitions", palette().cyan),
                    (Tab::Motion, "↗", "Motion", palette().violet),
                    (Tab::Effects, "≋", "Effects", palette().coral),
                ] {
                    let response = ui.add_sized(
                        [70., 58.],
                        egui::Button::new(
                            RichText::new(format!("{icon}\n{name}")).size(11.).color(
                                if self.tab == tab {
                                    palette().text
                                } else {
                                    palette().muted
                                },
                            ),
                        )
                        .fill(if self.tab == tab {
                            mix(palette().rail, palette().accent, 0.10)
                        } else {
                            palette().rail
                        })
                        .stroke(Stroke::NONE),
                    );

                    if self.tab == tab {
                        let r = Rect::from_min_size(
                            response.rect.left_top() + Vec2::new(1., 13.),
                            Vec2::new(2., 26.),
                        );
                        ui.painter().rect_filled(r, 2., palette().accent);
                    }
                    if response.clicked() {
                        self.tab = tab;
                    }
                }
            });

        egui::SidePanel::left("library")
            .default_width(268.)
            .frame(surface(palette().panel, 12))
            .min_width(190.)
            .max_width(370.)
            .show(ctx, |ui| {
                ui.set_min_width(ui.available_width());
                ui.add_space(8.);

                ui.heading(match self.tab {
                    Tab::Media => "Project media",
                    Tab::Audio => "Audio bin",
                    Tab::Text => "Titles & text",
                    Tab::Graphics => "Explanation graphics",
                    Tab::Transitions => "Transitions",
                    Tab::Effects => "Color & effects",
                    Tab::Motion => "Motion studio",
                });

                ui.label(
                    RichText::new(match self.tab {
                        Tab::Media | Tab::Audio => "Video, audio and images",
                        Tab::Text => "Titles and typography",
                        Tab::Graphics => "Shapes, charts and scenes",
                        Tab::Motion => "Animation and keyframes",
                        Tab::Transitions => "Between your scenes",
                        Tab::Effects => "Color and finishing",
                    })
                    .size(11.)
                    .color(palette().muted),
                );
                ui.add_space(13.);

                if matches!(self.tab, Tab::Media | Tab::Audio) {
                    if ui
                        .add_sized(
                            [ui.available_width(), 34.],
                            egui::Button::new(
                                RichText::new("+  Import media").color(palette().text),
                            )
                            .fill(palette().control),
                        )
                        .clicked()
                    {
                        self.import_picker(None);
                    }

                    ui.add(
                        egui::TextEdit::singleline(&mut self.search).hint_text("Search your media"),
                    );

                    ui.add_space(12.);

                    let mut assets: Vec<_> = self
                        .assets
                        .values()
                        .filter(|a| {
                            (self.tab != Tab::Audio || a.kind == Kind::Audio)
                                && a.name.to_lowercase().contains(&self.search.to_lowercase())
                        })
                        .cloned()
                        .collect();

                    assets.sort_by(|a, b| a.name.cmp(&b.name));

                    ui.label(
                        RichText::new(format!("PROJECT MEDIA   /   {} files", assets.len()))
                            .small()
                            .color(palette().muted),
                    );

                    ui.add_space(5.);

                    let cols = 2;

                    let rows = assets.len().div_ceil(cols);

                    egui::ScrollArea::vertical()
                        .id_salt("media_scroll")
                        .show_rows(ui, 164., rows, |ui, range| {
                            for row in range {
                                ui.horizontal_top(|ui| {
                                    let width = (ui.available_width() - 10.) / 2.;

                                    for col in 0..cols {
                                        if let Some(a) = assets.get(row * cols + col) {
                                            card().inner_margin(6.).show(ui, |ui| {
                                                ui.vertical(|ui| {
                                                    let width = (width - 12.).max(40.);
                                                    ui.set_width(width);

                                                    let (rect, response) = ui.allocate_exact_size(
                                                        Vec2::new(width, 76.),
                                                        Sense::click_and_drag(),
                                                    );

                                                    let texture = self.thumbnail(ctx, &a.id);

                                                    if let Some(texture) = texture {
                                                        ui.painter().image(
                                                            texture.id(),
                                                            rect,
                                                            Rect::from_min_max(
                                                                Pos2::ZERO,
                                                                Pos2::new(1., 1.),
                                                            ),
                                                            Color32::WHITE,
                                                        );
                                                    } else {
                                                        ui.painter().rect_filled(
                                                            rect,
                                                            5.,
                                                            palette().card,
                                                        );

                                                        ui.painter().text(
                                                            rect.center(),
                                                            egui::Align2::CENTER_CENTER,
                                                            if a.kind == Kind::Audio {
                                                                "♫"
                                                            } else {
                                                                "…"
                                                            },
                                                            egui::FontId::proportional(26.),
                                                            palette().accent,
                                                        );
                                                    }

                                                    ui.painter().rect_stroke(
                                                        rect,
                                                        6.,
                                                        Stroke::new(
                                                            1.0_f32,
                                                            if response.hovered() {
                                                                palette().cyan
                                                            } else {
                                                                palette().border
                                                            },
                                                        ),
                                                        StrokeKind::Inside,
                                                    );
                                                    response
                                                        .context_menu(|ui| self.media_menu(ui, a));
                                                    if response.double_clicked() {
                                                        self.add_asset(&a.id, None, None);
                                                    }

                                                    if response.drag_started() {
                                                        response.dnd_set_drag_payload(a.id.clone());
                                                    }

                                                    let add = Rect::from_min_size(
                                                        rect.right_top() - Vec2::new(25., -4.),
                                                        Vec2::splat(21.),
                                                    );

                                                    ui.painter().rect_filled(
                                                        add,
                                                        3.,
                                                        mix(palette().card, palette().cyan, 0.25),
                                                    );

                                                    ui.painter().text(
                                                        add.center(),
                                                        egui::Align2::CENTER_CENTER,
                                                        "+",
                                                        egui::FontId::proportional(14.),
                                                        palette().accent,
                                                    );

                                                    if ui
                                                        .interact(
                                                            add,
                                                            ui.id().with((&a.id, "add")),
                                                            Sense::click(),
                                                        )
                                                        .clicked()
                                                    {
                                                        self.add_asset(&a.id, None, None);
                                                    }

                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(&a.name).size(10.),
                                                        )
                                                        .truncate(),
                                                    )
                                                    .on_hover_text(&a.name);

                                                    ui.label(
                                                        RichText::new(if a.demo {
                                                            "Sample illustration".into()
                                                        } else if a.status == "ready" {
                                                            format!(
                                                                "{} · {}",
                                                                if a.kind == Kind::Audio {
                                                                    "Audio"
                                                                } else {
                                                                    "Linked original"
                                                                },
                                                                format_bytes(a.size)
                                                            )
                                                        } else {
                                                            a.status.clone()
                                                        })
                                                        .size(9.)
                                                        .color(if a.status == "error" {
                                                            Color32::LIGHT_RED
                                                        } else {
                                                            palette().muted
                                                        }),
                                                    );

                                                    if a.error.is_some() {
                                                        if ui
                                                            .small_button("Retry / relink")
                                                            .on_hover_text(
                                                                a.error.as_deref().unwrap_or(""),
                                                            )
                                                            .clicked()
                                                        {
                                                            self.import_picker(Some(a.id.clone()));
                                                        }
                                                    } else if !a.demo
                                                        && ui
                                                            .small_button("Relink original")
                                                            .clicked()
                                                    {
                                                        self.import_picker(Some(a.id.clone()));
                                                    }
                                                });
                                            });
                                        }
                                    }
                                });
                            }
                        });
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                        Tab::Text => {
                            ui.label(
                                RichText::new("A few words. A whole new story.")
                                    .small()
                                    .color(palette().muted),
                            );

                            for (name, text, size) in [
                                ("Statement", "YOUR NEXT\nGREAT STORY", 108),
                                ("Section title", "THE BUILD", 76),
                                ("Subtitle", "A small step toward something bigger.", 34),
                                ("Technical label", "APOGEE  /  1,250 M", 40),
                            ] {
                                ui.add_space(10.);

                                if ui
                                    .add_sized(
                                        [ui.available_width(), 75.],
                                        egui::Button::new(
                                            RichText::new(format!("{text}\n\n+ {name}"))
                                                .color(palette().text)
                                                .size(14.),
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.add_layer(Kind::Title, Graphic::Arrow, text, size);
                                }
                            }
                        }

                        Tab::Graphics => self.graphics_library(ui),
                        Tab::Motion => self.motion_library(ui),

                        Tab::Transitions => {
                            ui.label(
                                RichText::new("Select a clip, then choose its entrance.")
                                    .small()
                                    .color(palette().muted),
                            );

                            for (t, label) in [
                                (Transition::None, "Clean cut"),
                                (Transition::Fade, "Fade in & out"),
                                (Transition::Slide, "Slide from left"),
                                (Transition::Zoom, "Gentle zoom"),
                                (Transition::Wipe, "Wipe reveal"),
                            ] {
                                ui.add_space(10.);

                                if ui
                                    .add_sized(
                                        [ui.available_width(), 50.],
                                        egui::Button::new(label),
                                    )
                                    .clicked()
                                {
                                    if let Some(i) = self.selected_index() {
                                        self.history.remember(&self.project);

                                        self.project.clips[i].transition = t;

                                        self.modified();
                                    } else {
                                        self.notify("Select a clip on the timeline first");
                                    }
                                }
                            }
                        }

                        Tab::Effects => {
                            ui.label(
                                RichText::new("Find the feeling of your film.")
                                    .small()
                                    .color(palette().muted),
                            );

                            for (name, b, ct, s, w) in [
                                ("Original", 0., 1., 1., 0.),
                                ("Golden hour", 0.03, 1.1, 1.15, 0.5),
                                ("Cinematic cool", -0.04, 1.2, 0.8, -0.45),
                                ("Documentary", 0.02, 1.15, 0.85, 0.1),
                                ("Monochrome", 0., 1.2, 0., 0.),
                            ] {
                                ui.add_space(10.);

                                if ui
                                    .add_sized([ui.available_width(), 50.], egui::Button::new(name))
                                    .clicked()
                                {
                                    if let Some(i) = self.selected_index() {
                                        self.history.remember(&self.project);

                                        let c = &mut self.project.clips[i];

                                        c.brightness = b;

                                        c.contrast = ct;

                                        c.saturation = s;

                                        c.warmth = w;

                                        self.modified();
                                    } else {
                                        self.notify("Select a visual clip first");
                                    }
                                }
                            }
                        }

                        _ => {}
                    });
                }
            });
    }

    fn thumbnail(&mut self, ctx: &egui::Context, id: &str) -> Option<TextureHandle> {
        if let Some(t) = self.thumb_cache.get(id) {
            return Some(t.clone());
        }

        let path = self.storage.root.join("media").join(id).join("thumb.jpg");

        let img = image::open(path).ok()?.to_rgb8();

        let texture = ctx.load_texture(
            id,
            egui::ColorImage::from_rgb([img.width() as usize, img.height() as usize], img.as_raw()),
            egui::TextureOptions::LINEAR,
        );

        if self.thumb_cache.len() >= 64 {
            self.thumb_cache.clear();
        }

        self.thumb_cache.insert(id.into(), texture.clone());

        Some(texture)
    }

    fn viewer(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(palette().canvas).inner_margin(18.))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").color(palette().accent).size(9.));

                    ui.label(RichText::new("Preview").strong());

                    ui.label(
                        RichText::new(format!("{} × {}", self.project.width, self.project.height))
                            .small()
                            .color(palette().muted),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("Draft quality")
                                .size(9.)
                                .color(palette().muted),
                        );
                    });
                });

                ui.add_space(12.);

                let available =
                    Vec2::new(ui.available_width(), (ui.available_height() - 88.).max(80.));

                let ratio = self.project.width as f32 / self.project.height as f32;

                let width = available.x.min(available.y * ratio);

                let height = width / ratio;

                let (area, _) = ui.allocate_exact_size(available, Sense::hover());

                let rect = Rect::from_center_size(area.center(), Vec2::new(width, height));

                ui.painter().rect_filled(rect, 4., Color32::BLACK);
                ui.interact(rect, ui.id().with("viewer_menu"), Sense::click())
                    .context_menu(|ui| {
                        if ui
                            .button(if self.playing { "Pause" } else { "Play" })
                            .clicked()
                        {
                            self.toggle_play();
                            ui.close();
                        }
                        if ui.button("Go to beginning").clicked() {
                            self.seek(0.);
                            ui.close();
                        }
                        if ui.button("Refresh preview").clicked() {
                            self.preview_dirty = Some(Instant::now() - Duration::from_secs(1));
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Project settings…").clicked() {
                            self.settings_window = true;
                            ui.close();
                        }
                        if ui.button("Export video…").clicked() {
                            self.export_size = [self.project.width, self.project.height];
                            self.export_window = true;
                            ui.close();
                        }
                    });

                if let Some(texture) = &self.preview_texture {
                    ui.painter().image(
                        texture.id(),
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                        Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        if self.preview_running {
                            "Preparing preview…"
                        } else if !self.last_error.is_empty() {
                            "Preview unavailable"
                        } else {
                            "Your story starts here"
                        },
                        egui::FontId::proportional(15.),
                        palette().muted,
                    );
                }

                ui.painter().rect_stroke(
                    rect,
                    4.,
                    Stroke::new(1.0_f32, palette().border),
                    StrokeKind::Inside,
                );

                if !self.last_error.is_empty() {
                    ui.painter().rect_filled(
                        Rect::from_min_max(
                            rect.left_bottom() - Vec2::new(0., 60.),
                            rect.right_bottom(),
                        ),
                        0.,
                        Color32::from_black_alpha(210),
                    );

                    ui.painter().text(
                        rect.left_bottom() + Vec2::new(12., -30.),
                        egui::Align2::LEFT_CENTER,
                        self.last_error
                            .lines()
                            .next()
                            .unwrap_or("Preview failed")
                            .chars()
                            .take(100)
                            .collect::<String>(),
                        egui::FontId::proportional(11.),
                        Color32::LIGHT_RED,
                    );
                }

                if !self.last_error.is_empty() {
                    ui.horizontal(|ui| {
                        if ui.button("Preview error details…").clicked() {
                            self.preview_error_window = true;
                        }
                        if ui.button("Retry preview").clicked() {
                            self.preview_error_window = false;
                            self.preview_dirty = Some(Instant::now() - Duration::from_secs(1));
                        }
                    });
                }

                ui.add_space(12.);

                ui.horizontal(|ui| {
                    ui.label(RichText::new(timecode(self.time, self.project.fps)).monospace());

                    ui.label(
                        RichText::new(format!(
                            "/ {}",
                            timecode(self.project.duration(), self.project.fps)
                        ))
                        .small()
                        .color(palette().muted),
                    );

                    ui.add_space((ui.available_width() / 2. - 160.).max(0.));

                    if ui.button("|◀").on_hover_text("Beginning").clicked() {
                        self.seek(0.);
                    }

                    if ui.button("‹").on_hover_text("Previous frame").clicked() {
                        self.seek(self.time - 1. / self.project.fps as f64);
                    }

                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(if self.playing { "Ⅱ" } else { "▶" })
                                    .color(palette().on_accent()),
                            )
                            .fill(palette().accent)
                            .min_size(Vec2::new(38., 30.)),
                        )
                        .clicked()
                    {
                        self.toggle_play();
                    }

                    if ui.button("›").on_hover_text("Next frame").clicked() {
                        self.seek(self.time + 1. / self.project.fps as f64);
                    }

                    if ui.button("▶|").clicked() {
                        self.seek(self.project.duration());
                    }
                });

                ui.add_space(8.);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("■  Originals preserved")
                            .small()
                            .color(palette().success),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("A little closer to your next great story.")
                                .small()
                                .color(palette().muted),
                        );
                    });
                });
            });
    }

    fn inspector(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("inspector").default_width(300.).min_width(260.).max_width(390.).frame(surface(palette().panel,12)).show(ctx,|ui|{

  ui.label(RichText::new(match self.workspace {WorkMode::Edit=>"EFFECT CONTROLS",WorkMode::Color=>"COLOR",WorkMode::Audio=>"AUDIO CONTROLS",WorkMode::Motion=>"MOTION CONTROLS"}).size(10.).color(palette().muted));
  ui.add_space(6.);
  let props_label=RichText::new("Properties").color(if self.property_tab==PropertyTab::Properties{palette().text}else{palette().accent});
  let audio_label=RichText::new("Audio").color(if self.property_tab==PropertyTab::Audio{palette().text}else{palette().cyan});
  let color_label=RichText::new("Color").color(if self.property_tab==PropertyTab::Color{palette().text}else{palette().amber});
  ui.horizontal(|ui|{
ui.selectable_value(&mut self.property_tab,PropertyTab::Properties,props_label);
ui.selectable_value(&mut self.property_tab,PropertyTab::Audio,audio_label);
ui.selectable_value(&mut self.property_tab,PropertyTab::Color,color_label);
}
);
ui.separator();

  let Some(index)=self.selected_index()else{
ui.add_space(40.);
ui.label(RichText::new("Make it yours").size(18.).color(palette().accent));
ui.label("Select a timeline clip to adjust its properties.");
return;
}
;
if let Some(media_id)=self.project.clips[index].asset_id.clone()

 && !self.assets.contains_key(&media_id){
ui.colored_label(Color32::LIGHT_RED,"Original is not linked in this library");
if ui.button("Locate missing original…").clicked(){
self.import_picker(Some(media_id));
}
}

let old=self.project.clips[index].clone();
let mut c=old.clone();

  ui.add_space(8.);
ui.label(RichText::new(&c.name).strong());
ui.label(RichText::new(format!("{:?} LAYER",c.kind).to_uppercase()).size(9.).color(palette().muted));
ui.separator();

  egui::ScrollArea::vertical().show(ui,|ui|{
match self.property_tab {

   PropertyTab::Properties=>{

    if matches!(c.kind,Kind::Title|Kind::Graphic){
card().show(ui,|ui|{
ui.set_min_width(ui.available_width());
section_title(ui,"Text & appearance",palette().amber);
if ui.add(egui::TextEdit::multiline(&mut c.text).char_limit(3000).desired_rows(3).desired_width(ui.available_width())).changed(){
c.name=c.text.lines().next().unwrap_or("Text").chars().take(70).collect();
}

     ui.horizontal(|ui|{
ui.label("Color");
let mut rgb=hex_color(&c.color);
if ui.color_edit_button_srgb(&mut rgb).changed(){
c.color=format!("#{:02x}{:02x}{:02x}",rgb[0],rgb[1],rgb[2]);
}

{
ui.add(egui::DragValue::new(&mut c.font_size).range(12..=800).suffix(" px"));
}
}
);
self.appearance_controls(ui, &mut c);
});
ui.add_space(10.);
}

    if c.is_visual(){
card().show(ui,|ui|{
ui.set_min_width(ui.available_width());
ui.horizontal(|ui|{
section_title(ui,"Transform",palette().accent);
if ui.small_button("Reset").clicked(){
c.transform=Transform::default();
c.keyframes.clear();
}
}
);
let local=(self.time-c.start).clamp(0.,c.duration);
let mut trans=c.value(local);
let before=trans.clone();
ui.horizontal(|ui|{
ui.label("X");
ui.add(egui::DragValue::new(&mut trans.x).range(-100.0..=200.0).suffix("%"));
ui.label("Y");
ui.add(egui::DragValue::new(&mut trans.y).range(-100.0..=200.0).suffix("%"));
}
);
ui.add(egui::Slider::new(&mut trans.scale,0.05..=4.).text("Scale").suffix("×"));
ui.add(egui::Slider::new(&mut trans.rotation,-180.0..=180.0).text("Rotation").suffix("°"));
ui.add(egui::Slider::new(&mut trans.opacity,0.0..=1.0).text("Opacity"));
if trans!=before{
if c.keyframes.is_empty(){
c.transform=trans;
}
else{
edit::add_keyframe(&mut c,local,self.project.fps);
let t=(local*self.project.fps as f64).round()/self.project.fps as f64;
if let Some(k)=c.keyframes.iter_mut().find(|k|(k.time-t).abs()<1e-8){
k.transform=trans;
}
}
}
});
ui.add_space(10.);
}

    card().show(ui,|ui|{
ui.set_min_width(ui.available_width());
section_title(ui,"Timing",palette().cyan);
egui::ComboBox::from_id_salt("clip_track").selected_text(TRACKS[c.track]).show_ui(ui,|ui|{
for(i,name)in TRACKS.iter().enumerate(){
ui.selectable_value(&mut c.track,i,*name);
}
}
);

    ui.horizontal(|ui|{
ui.label("Start");
ui.add(egui::DragValue::new(&mut c.start).range(0.0..=(86400.-c.duration)).speed(1./self.project.fps as f64).suffix(" s"));
}
);

    if matches!(c.kind,Kind::Video|Kind::Audio){
let mut speed=c.speed;
ui.horizontal(|ui|{
ui.label("Speed");
ui.add(egui::DragValue::new(&mut speed).range(0.000001..=1e12).speed(0.1).max_decimals(6).suffix("×"));
}
);
if speed!=c.speed&& let Err(e)=c.set_speed(speed,self.project.fps){
self.notify(e.to_string());
}

     let mut target=c.duration;
ui.horizontal(|ui|{
ui.label("Target duration");
ui.add(egui::DragValue::new(&mut target).range((1./self.project.fps as f64)..=86400.).speed(0.1).max_decimals(3).suffix(" s"));
}
);
if target!=c.duration{
let speed=c.duration*c.speed/target;
if let Err(e)=c.set_speed(speed,self.project.fps){
self.notify(e.to_string());
}
}

     ui.horizontal_wrapped(|ui|{
for speed in [0.25,0.5,1.,2.,8.,32.,720.]{
if ui.small_button(format!("{speed}×")).clicked()&& let Err(e)=c.set_speed(speed,self.project.fps){
self.notify(e.to_string());
}
}
}
);
ui.label(RichText::new("4 hours → 20 seconds = 720×").small().color(palette().accent));

    }
else{
ui.horizontal(|ui|{
ui.label("Duration");
ui.add(egui::DragValue::new(&mut c.duration).range((1./self.project.fps as f64)..=(86400.-c.start)).speed(0.1).suffix(" s"));
}
);
}

    });
ui.add_space(10.);
if c.is_visual(){
card().show(ui,|ui|{
ui.set_min_width(ui.available_width());
ui.horizontal(|ui|{
section_title(ui,"Animation",palette().violet);
if ui.small_button("+ Keyframe").clicked()&&c.keyframes.len()<200{
edit::add_keyframe(&mut c,(self.time-old.start).clamp(0.,old.duration),self.project.fps);
}
}
);
if ui.button("Open Motion Studio →").clicked(){self.motion_window=true;}
if ui.button("Browse motion presets →").clicked(){self.tab=Tab::Motion;}
if !c.keyframes.is_empty() && ui.small_button("Clear animation").clicked(){c.transform=c.value((self.time-c.start).clamp(0.,c.duration));c.keyframes.clear();}
ui.label(RichText::new("Add a keyframe, move the playhead, then change the transform.").small().color(palette().muted));
let mut remove=None;
egui::ScrollArea::vertical().id_salt("keyframe_list").max_height(130.).show(ui,|ui|{
for(i,k)in c.keyframes.iter().enumerate(){
ui.horizontal(|ui|{
if ui.small_button(format!("◇ {}",timecode(k.time,self.project.fps))).clicked(){
self.seek(c.start+k.time);
}

if ui.small_button("×").clicked(){
remove=Some(i);
}
}
);
}

});
if let Some(i)=remove{
c.keyframes.remove(i);
}

if c.keyframes.is_empty(){
ui.label(RichText::new("No keyframes yet").small().color(palette().muted));
}
});
ui.add_space(12.);

     section_title(ui,"Mask & transition",palette().amber);
egui::ComboBox::from_id_salt("mask").selected_text(format!("{:?}",c.mask)).show_ui(ui,|ui|{
for v in [Mask::None,Mask::Circle,Mask::Rectangle,Mask::Text]{
ui.selectable_value(&mut c.mask,v,format!("{v:?}"));
}
}
);

    }

    if c.mask==Mask::Text {
ui.label("Footage inside text");
ui.add(egui::TextEdit::multiline(&mut c.text).char_limit(3000).desired_rows(2));
ui.add(egui::DragValue::new(&mut c.font_size).range(12..=800).suffix(" px"));
}

    egui::ComboBox::from_id_salt("transition").selected_text(format!("{:?}",c.transition)).show_ui(ui,|ui|{
for v in [Transition::None,Transition::Fade,Transition::Slide,Transition::Zoom,Transition::Wipe]{
ui.selectable_value(&mut c.transition,v,format!("{v:?}"));
}
}
);
if c.transition!=Transition::None{
ui.add(egui::Slider::new(&mut c.fade_duration,0.05..=5.).text("Length").suffix(" s"));
}

   }
,
   PropertyTab::Audio=>{
card().show(ui,|ui|{
ui.set_min_width(ui.available_width());
section_title(ui,"Audio mix",palette().cyan);
if matches!(c.kind,Kind::Video|Kind::Audio){
ui.add(egui::Slider::new(&mut c.volume,0.0..=2.0).text("Volume").suffix("×"));
if ui.button(if c.volume==0.{
"Unmute clip"}
else{
"Mute clip"}
).clicked(){
c.volume=if c.volume==0.{
1.}
else{
0.}
;
}
ui.label(RichText::new("Pitch-preserved retiming and fades apply in both preview and export. Native audio preview requires FFplay and an available audio device.").small().color(palette().muted));
}
else{
ui.label("This layer has no audio. Select a video or audio clip.");
}
});
}
,
   PropertyTab::Color=>{
card().show(ui,|ui|{
ui.set_min_width(ui.available_width());
ui.horizontal(|ui|{
section_title(ui,"Color adjustments",palette().amber);
if ui.small_button("Reset").clicked(){
c.brightness=0.;
c.contrast=1.;
c.saturation=1.;
c.warmth=0.;
c.blur=0.;
}
}
);
ui.add(egui::Slider::new(&mut c.brightness,-1.0..=1.0).text("Brightness"));
ui.add(egui::Slider::new(&mut c.contrast,0.0..=3.0).text("Contrast"));
ui.add(egui::Slider::new(&mut c.saturation,0.0..=3.0).text("Saturation"));
ui.add(egui::Slider::new(&mut c.warmth,-1.0..=1.0).text("Temperature"));
ui.add(egui::Slider::new(&mut c.blur,0.0..=20.0).text("Softness"));

    if c.kind==Kind::Video{
ui.add_space(15.);
egui::Frame::new().fill(mix(palette().card,palette().cyan,0.12)).inner_margin(12.).corner_radius(6.).show(ui,|ui|{
ui.label(RichText::new("✦ Clean Timelapse").color(palette().accent).strong());
ui.label(RichText::new("Blend the motion. Keep the detail.").small());
ui.horizontal(|ui|{
for speed in [8.,32.,720.]{
if ui.add_enabled(c.duration*c.speed/speed>=1./self.project.fps as f64,egui::Button::new(format!("{speed}×"))).clicked()&& c.set_speed(speed,self.project.fps).is_ok(){
c.motion_blur=true;
c.deflicker=true;
c.volume=0.;
}
}
}
);
ui.checkbox(&mut c.motion_blur,"Motion blending");
ui.add(egui::Slider::new(&mut c.shutter,0.1..=1.0).text("Strength"));
ui.checkbox(&mut c.deflicker,"Reduce exposure flicker");
ui.label(RichText::new("Up to 16 source samples per exposure window. Temporal effects apply on export.").small().color(palette().muted));
}
);
}

   });
   }

  }
}
);

  if c!=old{
let pointer_down=ctx.input(|i|i.pointer.any_down());
if !self.property_drag||!pointer_down{
self.history.remember(&self.project);
}
self.property_drag=pointer_down;
self.project.clips[index]=c;
self.modified();
}

if !ctx.input(|i|i.pointer.any_down()){
self.property_drag=false;
}

 }
);
    }

    fn timeline(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("timeline")
            .default_height(322.)
            .frame(surface(palette().rail, 10))
            .min_height(280.)
            .max_height(480.)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.history.can_undo(), egui::Button::new("↶"))
                        .on_hover_text("Undo")
                        .clicked()
                    {
                        self.history.undo(&mut self.project);

                        self.modified();
                    }

                    if ui
                        .add_enabled(self.history.can_redo(), egui::Button::new("↷"))
                        .on_hover_text("Redo")
                        .clicked()
                    {
                        self.history.redo(&mut self.project);

                        self.modified();
                    }

                    ui.separator();
                    ui.label(
                        RichText::new("TIMELINE")
                            .size(10.)
                            .color(palette().text)
                            .strong(),
                    );
                    ui.separator();

                    if ui
                        .button("Split")
                        .on_hover_text("Split at playhead (S)")
                        .clicked()
                    {
                        self.split();
                    }

                    if ui.button("Duplicate").clicked() {
                        self.duplicate();
                    }

                    if ui.button("Delete").clicked() {
                        self.delete();
                    }

                    let can_detach=self.selected_index().is_some_and(|i|{let c=&self.project.clips[i];c.kind==Kind::Video && c.volume>0. && c.asset_id.as_ref().and_then(|id|self.assets.get(id)).is_some_and(|a|a.has_audio)});
                    if ui.add_enabled(can_detach,egui::Button::new("Detach audio")).on_hover_text("Move this clip's source audio to its own editable voiceover-track clip for J/L cuts.").clicked() && let Some(id)=self.selected.clone() {
                        let mut next=self.project.clone();
                        if let Some(audio)=edit::detach_audio(&mut next,&id){self.history.remember(&self.project);self.project=next;self.selected=Some(audio);self.modified();}
                    }
                    ui.separator();

                    ui.checkbox(&mut self.snap, "Snap");

                    if ui.button("Remove frames").clicked() {
                        self.range = [self.time, self.time + 1. / self.project.fps as f64];

                        self.range_window = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Fit").clicked() {
                            self.zoom = ((ui.available_width() + 550.) as f64
                                / self.project.duration().max(10.))
                                as f32;

                            self.zoom = self.zoom.clamp(0.03, 150.);
                        }

                        ui.add(
                            egui::Slider::new(&mut self.zoom, 0.03..=150.)
                                .logarithmic(true)
                                .show_value(false)
                                .max_decimals(2),
                        );

                        ui.label(
                            RichText::new("Timeline zoom")
                                .small()
                                .color(palette().muted),
                        );
                    });
                });

                ui.separator();

                let row = 43.;

                let ruler = 26.;

                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(155.);

                        ui.add_sized(
                            [155., ruler],
                            egui::Label::new(
                                RichText::new(format!("{} FPS  /  TRACKS", self.project.fps))
                                    .small()
                                    .color(palette().muted),
                            ),
                        );

                        for (track, name) in TRACKS.iter().enumerate() {
                            ui.allocate_ui(Vec2::new(155., row - 9.), |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("▏").color(palette().tracks()[track]));

                                    ui.label(RichText::new(*name).size(10.));

                                    let active = if track >= 3 {
                                        self.project.muted_tracks.contains(&track)
                                    } else {
                                        self.project.hidden_tracks.contains(&track)
                                    };

                                    if ui
                                        .small_button(if active { "○" } else { "●" })
                                        .on_hover_text(if track >= 3 {
                                            "Mute / unmute track"
                                        } else {
                                            "Hide / show track"
                                        })
                                        .clicked()
                                    {
                                        self.history.remember(&self.project);

                                        let list = if track >= 3 {
                                            &mut self.project.muted_tracks
                                        } else {
                                            &mut self.project.hidden_tracks
                                        };

                                        if list.contains(&track) {
                                            list.retain(|t| *t != track);
                                        } else {
                                            list.push(track);
                                        }

                                        self.modified();
                                    }
                                });
                            });
                        }
                    });

                    egui::ScrollArea::horizontal()
                        .id_salt("timeline_scroll")
                        .max_height(ruler + row * 5. + 8.)
                        .auto_shrink([false, true])
                        .show_viewport(ui, |ui, viewport| {
                            let width = ((self.project.duration() + 8.).max(30.) as f32
                                * self.zoom)
                                .max(viewport.width());

                            let height = ruler + row * 5.;

                            let (rect, response) =
                                ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

                            let painter = ui.painter_at(rect);

                            let origin = rect.min;
                            painter.rect_filled(
                                Rect::from_min_size(origin, Vec2::new(width, ruler)),
                                0.,
                                palette().panel,
                            );
                            for (track, color) in palette().tracks().iter().enumerate() {
                                painter.rect_filled(
                                    Rect::from_min_size(
                                        origin + Vec2::new(0., ruler + row * track as f32),
                                        Vec2::new(width, row),
                                    ),
                                    0.,
                                    mix(
                                        palette().canvas,
                                        *color,
                                        if track % 2 == 0 { 0.07 } else { 0.11 },
                                    ),
                                );
                            }

                            let step = if self.zoom < 0.5 {
                                600.
                            } else if self.zoom < 3. {
                                60.
                            } else if self.zoom < 20. {
                                10.
                            } else if self.zoom < 45. {
                                5.
                            } else {
                                2.
                            };

                            let left =
                                (viewport.left() / self.zoom / step).floor().max(0.) as usize;

                            let right = (viewport.right() / self.zoom / step).ceil() as usize + 1;

                            for i in left..right {
                                let x = origin.x + i as f32 * step * self.zoom;

                                painter.line_segment(
                                    [
                                        Pos2::new(x, origin.y + ruler - 5.),
                                        Pos2::new(x, origin.y + height),
                                    ],
                                    Stroke::new(
                                        1.0_f32,
                                        mix(palette().canvas, palette().border, 0.35),
                                    ),
                                );

                                painter.text(
                                    Pos2::new(x + 5., origin.y + 8.),
                                    egui::Align2::LEFT_TOP,
                                    timecode(i as f64 * step as f64, self.project.fps),
                                    egui::FontId::monospace(9.),
                                    palette().muted,
                                );
                            }

                            for track in 0..5 {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(0., ruler + row * track as f32),
                                        origin + Vec2::new(width, ruler + row * track as f32),
                                    ],
                                    Stroke::new(
                                        1.0_f32,
                                        mix(palette().canvas, palette().border, 0.5),
                                    ),
                                );
                            }

                            // Snapshot only visible clips. Keyframes/text for off-screen footage must not
                            // be cloned on every redraw of a large timeline.
                            let view_start = viewport.left() as f64 / self.zoom as f64;
                            let view_end = viewport.right() as f64 / self.zoom as f64;
                            let clips: Vec<_> = self
                                .project
                                .clips
                                .iter()
                                .filter(|c| c.end() >= view_start && c.start <= view_end)
                                .cloned()
                                .collect();

                            for c in clips.iter() {
                                let x = origin.x + c.start as f32 * self.zoom;

                                let cw = (c.duration as f32 * self.zoom).max(4.);

                                if x + cw < rect.left() + viewport.left()
                                    || x > rect.left() + viewport.right()
                                {
                                    continue;
                                }

                                let peers: Vec<_> = clips
                                    .iter()
                                    .filter(|other| {
                                        other.track == c.track
                                            && other.start < c.end()
                                            && other.end() > c.start
                                    })
                                    .collect();

                                let lane =
                                    peers.iter().position(|other| other.id == c.id).unwrap_or(0);

                                let ch = (row - 7.) / peers.len().max(1) as f32;

                                let clip_rect = Rect::from_min_size(
                                    Pos2::new(
                                        x,
                                        origin.y
                                            + ruler
                                            + row * c.track as f32
                                            + 3.
                                            + lane as f32 * ch,
                                    ),
                                    Vec2::new(cw, ch - 1.),
                                );

                                let selected = self.selected.as_deref() == Some(&c.id);

                                let mut color = palette().tracks()[c.track];

                                if self.project.hidden_tracks.contains(&c.track)
                                    || self.project.muted_tracks.contains(&c.track)
                                {
                                    color = color.gamma_multiply(0.4);
                                }

                                painter.rect_filled(
                                    clip_rect,
                                    5.,
                                    mix(palette().card, color, if selected { 0.26 } else { 0.12 }),
                                );
                                if clip_rect.height() > 8. {
                                    painter.rect_filled(
                                        Rect::from_min_size(
                                            clip_rect.min + Vec2::new(2., 1.),
                                            Vec2::new((clip_rect.width() - 4.).max(0.), 2.),
                                        ),
                                        1.,
                                        color,
                                    );
                                }

                                painter.rect_stroke(
                                    clip_rect,
                                    4.,
                                    Stroke::new(
                                        if selected { 1.5_f32 } else { 1.0_f32 },
                                        if selected {
                                            palette().accent
                                        } else {
                                            palette().border
                                        },
                                    ),
                                    StrokeKind::Inside,
                                );

                                if cw > 22. {
                                    painter.with_clip_rect(clip_rect.shrink(2.)).text(
                                        clip_rect.left_center() + Vec2::new(7., 0.),
                                        egui::Align2::LEFT_CENTER,
                                        &c.name,
                                        egui::FontId::proportional(11.),
                                        palette().text,
                                    );
                                }

                                for key in &c.keyframes {
                                    let x = clip_rect.left() + key.time as f32 * self.zoom;

                                    painter.circle_filled(
                                        Pos2::new(x, clip_rect.bottom() - 5.),
                                        2.,
                                        palette().accent,
                                    );
                                }

                                if c.transition != Transition::None {
                                    let fade = (c.fade_duration.min(c.duration / 2.) as f32
                                        * self.zoom)
                                        .min(cw / 2.);

                                    painter.line_segment(
                                        [
                                            clip_rect.left_bottom(),
                                            clip_rect.left_top() + Vec2::new(fade, 0.),
                                        ],
                                        Stroke::new(1.0_f32, palette().accent.gamma_multiply(0.5)),
                                    );
                                }

                                let response = ui.interact(
                                    clip_rect,
                                    ui.id().with(&c.id),
                                    Sense::click_and_drag(),
                                );

                                if response.clicked() || response.secondary_clicked() { self.select(c.id.clone()); }
                                response.context_menu(|ui|self.clip_menu(ui,c));

                                if response.drag_started() {
                                    self.history.remember(&self.project);

                                    self.select(c.id.clone());

                                    let pointer = response
                                        .interact_pointer_pos()
                                        .unwrap_or(clip_rect.center())
                                        .x;

                                    let edge = if pointer - clip_rect.left() < 8. {
                                        -1
                                    } else if clip_rect.right() - pointer < 8. {
                                        1
                                    } else {
                                        0
                                    };

                                    self.drag = Some(Drag {
                                        clip: c.clone(),
                                        pointer,
                                        edge,
                                    });
                                }

                                if response.dragged()
                                    && let Some(d) = &self.drag
                                    && d.clip.id == c.id
                                {
                                    let pointer = response
                                        .interact_pointer_pos()
                                        .unwrap_or(clip_rect.center())
                                        .x;

                                    let delta = (pointer - d.pointer) as f64 / self.zoom as f64;

                                    let original = d.clip.clone();

                                    let mut edited = original.clone();

                                    if d.edge == 0 {
                                        edited.start = (original.start + delta).max(0.);

                                        if self.snap {
                                            let points = clips
                                                .iter()
                                                .filter(|other| {
                                                    other.id != c.id && other.track == c.track
                                                })
                                                .flat_map(|other| [other.start, other.end()])
                                                .chain([0., self.time]);

                                            for point in points {
                                                if (edited.start - point).abs()
                                                    < 7. / self.zoom as f64
                                                {
                                                    edited.start = point;

                                                    break;
                                                }

                                                if (edited.end() - point).abs()
                                                    < 7. / self.zoom as f64
                                                {
                                                    edited.start =
                                                        (point - edited.duration).max(0.);

                                                    break;
                                                }
                                            }
                                        }

                                        edited.start = (edited.start * self.project.fps as f64)
                                            .round()
                                            / self.project.fps as f64;
                                    } else if d.edge == 1 {
                                        let max = original
                                            .asset_id
                                            .as_ref()
                                            .and_then(|id| self.assets.get(id))
                                            .filter(|_| {
                                                matches!(original.kind, Kind::Video | Kind::Audio)
                                            })
                                            .map_or(86400. - original.start, |a| {
                                                (a.duration - original.source_in) / original.speed
                                            });

                                        edited.duration = (original.duration + delta).clamp(
                                            1. / self.project.fps as f64,
                                            max.max(1. / self.project.fps as f64),
                                        );
                                    } else {
                                        let min =
                                            if matches!(original.kind, Kind::Video | Kind::Audio) {
                                                -original
                                                    .start
                                                    .min(original.source_in / original.speed)
                                            } else {
                                                -original.start
                                            };

                                        let delta = delta.clamp(
                                            min,
                                            original.duration - 1. / self.project.fps as f64,
                                        );

                                        edited.start += delta;

                                        edited.duration -= delta;

                                        edited.source_in =
                                            (edited.source_in + delta * edited.speed).max(0.);

                                        edited.keyframes = edit::slice_keyframes(
                                            &original,
                                            delta,
                                            original.duration,
                                        );
                                        edited.reveal_offset =
                                            (original.reveal_offset + delta).max(0.);
                                        edited.content_offset=original.content_offset+delta*original.content_rate;
                                    }

                                    if let Some(index) =
                                        self.project.clips.iter().position(|c| c.id == edited.id)
                                    {
                                        self.project.clips[index] = edited;

                                        self.modified();
                                    }
                                }

                                if response.drag_stopped() {
                                    self.drag = None;
                                }
                            }

                            if response.secondary_clicked() && let Some(pos)=response.interact_pointer_pos(){
                                self.timeline_context=((((pos.x-origin.x)/self.zoom) as f64).max(0.),((pos.y-origin.y-ruler)/row).floor().clamp(0.,4.) as usize);
                            }
                            response.context_menu(|ui|self.timeline_menu(ui));
                            if response.clicked()
                                && let Some(pos) = response.interact_pointer_pos()
                            {
                                self.seek(((pos.x - origin.x) / self.zoom) as f64);
                            }

                            if let Some(asset) = response.dnd_release_payload::<String>()
                                && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
                            {
                                let at = ((pos.x - origin.x) / self.zoom) as f64;

                                let track = ((pos.y - origin.y - ruler) / row).floor().clamp(0., 4.)
                                    as usize;

                                self.add_asset(&asset, Some(at.max(0.)), Some(track));
                            }

                            let px = origin.x + self.time as f32 * self.zoom;

                            painter.line_segment(
                                [
                                    Pos2::new(px, origin.y + 8.),
                                    Pos2::new(px, origin.y + height),
                                ],
                                Stroke::new(1.0_f32, palette().accent),
                            );

                            painter.rect_filled(
                                Rect::from_center_size(
                                    Pos2::new(px, origin.y + 5.),
                                    Vec2::new(9., 10.),
                                ),
                                1.,
                                palette().accent,
                            );

                            let scrub = ui.interact(
                                Rect::from_min_size(rect.min, Vec2::new(width, ruler)),
                                ui.id().with("scrub"),
                                Sense::click_and_drag(),
                            );

                            if (scrub.clicked() || scrub.dragged())
                                && let Some(pos) = scrub.interact_pointer_pos()
                            {
                                self.seek(((pos.x - origin.x) / self.zoom) as f64);
                            }
                        });
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "● Native Rust engine  /  {} clips  /  {} total",
                            self.project.clips.len(),
                            timecode(self.project.duration(), self.project.fps)
                        ))
                        .small()
                        .color(palette().muted),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("Space  Play / pause    S  Split    ← →  Frame by frame")
                                .small()
                                .color(palette().muted),
                        );
                    });
                });
            });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        self.context_dialogs(ctx);
        let mut details = self.preview_error_window && !self.last_error.is_empty();
        egui::Window::new("Preview error details").open(&mut details).default_width(650.).resizable(true).show(ctx, |ui| {
            ui.label("Copy these details when reporting a preview problem. Diagnostic logs stay in the cache's diagnostics folder (the latest five are retained).");
            if ui.button("Copy diagnostic details").clicked() { ctx.copy_text(self.last_error.clone()); }
            egui::ScrollArea::both().max_height(400.).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(&self.last_error).monospace()).selectable(true));
            });
        });
        self.preview_error_window = details;
        let mut open = self.export_window;

        egui::Window::new("Export video").open(&mut open).collapsible(false).resizable(false).default_width(410.).show(ctx,|ui|{
ui.label(RichText::new("Output settings").small().color(palette().muted));
ui.add_space(10.);
ui.label("Render from your originals. MP4 · H.264 · AAC.");
ui.label(RichText::new(format!("{}  /  {} fps  /  {} clips",timecode(self.project.duration(),self.project.fps),self.project.fps,self.project.clips.len())).small().color(palette().muted));
ui.add_space(10.);
egui::ComboBox::from_id_salt("export_size").selected_text(format!("{} × {}",self.export_size[0],self.export_size[1])).show_ui(ui,|ui|{
for(size,label)in[([640,360],"360p · Fast test"),([1280,720],"720p"),([1920,1080],"1080p"),([3840,2160],"4K UHD"),([1080,1920],"Vertical / Shorts"),([1080,1080],"Square")]{
ui.selectable_value(&mut self.export_size,size,label);
}
}
);

ui.add_space(10.);
self.acceleration_controls(ui, "export");
   if let Some(job)=&self.job{
ui.add_space(12.);
ui.label(RichText::new(&job.status).strong().color(if job.status=="Failed"{
Color32::LIGHT_RED}
else{
palette().accent}
));
ui.add(egui::ProgressBar::new(job.progress as f32).show_percentage());
if let Some(error)=&job.error{
ui.label(RichText::new(error).small().color(Color32::LIGHT_RED));
}

if job.status.starts_with("Complete")&& let Some(path)=&job.output{
ui.label(RichText::new(format!("Saved to {}",path.display())).small());
}
}

   let running=self.exports.as_ref().is_some_and(|(_,h)|!h.is_finished());
ui.add_space(10.);
if ui.add_enabled(!running,egui::Button::new(RichText::new("Export video…").color(palette().on_accent())).fill(palette().accent)).clicked(){
self.export();
}

if running&&ui.button("Cancel export").clicked()&& let Some((group,_))=&self.exports{
group.cancel();
}
ui.label(RichText::new("Choose a new filename. Existing videos are never overwritten. Keep Frame open until rendering finishes.").small().color(palette().muted));
}
);

        self.export_window = open;

        let mut open = self.settings_window;

        egui::Window::new("Project settings")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                let old = self.project.clone();

                ui.label("Project name");

                ui.text_edit_singleline(&mut self.project.name);

                ui.add_space(10.);

                if ui.button("Browse video format presets…").clicked() {self.template_new=false;self.template_window=true;}
                ui.horizontal(|ui| {ui.label("Width");ui.add(egui::DragValue::new(&mut self.project.width).range(240..=3840).speed(2.));ui.label("Height");ui.add(egui::DragValue::new(&mut self.project.height).range(240..=3840).speed(2.));});
                egui::ComboBox::from_id_salt("project_fps").selected_text(format!("{} fps",self.project.fps)).show_ui(ui,|ui|{for fps in [12,15,24,25,30,48,50,60]{ui.selectable_value(&mut self.project.fps,fps,format!("{fps} fps"));}});

                ui.add_space(10.);

                self.acceleration_controls(ui, "settings");
                ui.add_space(10.);
                ui.label(
                    RichText::new(format!(
                        "Cache and projects: {}",
                        self.storage.root.display()
                    ))
                    .small()
                    .color(palette().muted),
                );

                if self.project != old {
                    if self.project.validate().is_ok() {
                        self.history.remember(&old);

                        self.modified();
                    } else {
                        self.project = old;

                        self.notify(
                            "Use even dimensions from 240 to 3840. Every clip must last at least one frame at the chosen fps.",
                        );
                    }
                }
            });

        self.settings_window = open;

        let mut open = self.range_window;

        egui::Window::new("Remove frames & close gap")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Delete this range across every track. Undo is available.");

                ui.horizontal(|ui| {
                    ui.label("From");

                    ui.add(
                        egui::DragValue::new(&mut self.range[0])
                            .range(0.0..=86400.)
                            .speed(1. / self.project.fps as f64)
                            .suffix(" s"),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("To");

                    ui.add(
                        egui::DragValue::new(&mut self.range[1])
                            .range(0.0..=86400.)
                            .speed(1. / self.project.fps as f64)
                            .suffix(" s"),
                    );
                });

                let frames = ((self.range[1] - self.range[0]) * self.project.fps as f64).round();

                if ui
                    .add_enabled(
                        frames > 0.,
                        egui::Button::new(format!("Remove {} frames", frames.max(0.))),
                    )
                    .clicked()
                {
                    self.history.remember(&self.project);

                    edit::remove_range(&mut self.project, self.range[0], self.range[1]);

                    self.time = self.range[0].min(self.project.duration());

                    self.modified();

                    self.range_window = false;
                }
            });

        self.range_window &= open;

        let mut open = self.snapshots_window;

        egui::Window::new("Project snapshots")
            .open(&mut open)
            .default_width(420.)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Your work is preserved before switching projects.")
                        .small()
                        .color(palette().muted),
                );

                egui::ScrollArea::vertical()
                    .max_height(320.)
                    .show(ui, |ui| {
                        for (name, path) in self.storage.snapshots() {
                            if ui.button(format!("↶  {name}")).clicked() {
                                match load_project(&path) {
                                    Ok(p) => {
                                        self.switch(p);

                                        self.snapshots_window = false;
                                    }

                                    Err(e) => self.notify(e.to_string()),
                                }
                            }
                        }
                    });
            });

        self.snapshots_window &= open;

        let mut open = self.help_window;

        egui::Window::new("Find your flow").open(&mut open).resizable(false).show(ctx,|ui|{
ui.heading("A little less clicking");
for(action,key)in[("Play / pause","Space"),("Split selected clip","S"),("Previous / next frame","← / →"),("Jump 10 frames","Shift + ← / →"),("Delete clip","Delete"),("Undo / redo","Ctrl/Cmd Z / Shift Z"),("Save project","Ctrl/Cmd S")]{
ui.horizontal(|ui|{
ui.label(action);
ui.label(RichText::new(key).monospace().color(palette().accent));
}
);
}
ui.separator();
ui.label("Drag original files into the app to link them. Use + to add media to the timeline. Drag clip edges to trim. GPU draws the interface; FFmpeg processes video on CPU. Preview audio requires FFplay.");
ui.label(RichText::new("This version has not reached Resolve/Premiere parity. Optical flow, speed ramps, tracking, HDR and arbitrary masks are not implemented.").small().color(palette().muted));
}
);

        self.help_window = open;

        if let Some((text, _)) = &self.notice {
            egui::Area::new("notice".into())
                .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0., -35.))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(palette().card)
                        .stroke(Stroke::new(1.0_f32, palette().accent))
                        .inner_margin(14.)
                        .corner_radius(7.)
                        .show(ui, |ui| {
                            ui.label(RichText::new(text).color(palette().text));
                        });
                });
        }

        if self.quit_error {
            egui::Window::new("Frame could not save your latest edits")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Stay open to download a project backup, or close without saving.");

                    ui.label(RichText::new(&self.last_error).color(Color32::LIGHT_RED));

                    if ui.button("Save project as .frame…").clicked() {
                        self.backup();
                    }

                    if ui.button("Stay in Frame").clicked() {
                        self.quit_error = false;
                    }

                    if ui.button("Close without saving latest edits").clicked() {
                        self.dirty = None;

                        self.quit_error = false;

                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
        }
    }

    pub fn draw(&mut self, ctx: &egui::Context) {
        self.theme.activate();
        if self.applied_theme != Some(self.theme) {
            Self::configure(ctx);
            self.applied_theme = Some(self.theme);
        }
        self.poll(ctx);

        self.shortcuts(ctx);

        self.top(ctx);
        self.workspace_bar(ctx);

        self.timeline(ctx);

        self.library(ctx);

        self.inspector(ctx);

        self.viewer(ctx);

        self.dialogs(ctx);
        if let Some(text) = self.clipboard_text.take() {
            ctx.copy_text(text);
        }
        self.format_dialog(ctx);
        self.theme_dialog(ctx);
        self.animation_studio(ctx);
        self.effects_studio(ctx);
    }
}

impl eframe::App for FrameApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty.is_some() && !self.save() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);

            self.quit_error = true;
        }

        self.draw(ctx);
    }
}

impl Drop for FrameApp {
    fn drop(&mut self) {
        self.preview.stop();

        self.import_group.cancel();

        self.imports = None;

        for handle in self.import_threads.drain(..) {
            let _ = handle.join();
        }

        if let Some((group, handle)) = self.acceleration_task.take() {
            group.cancel();
            let _ = handle.join();
        }
        if let Some((group, handle)) = self.exports.take() {
            group.cancel();

            let _ = handle.join();
        }

        if self.dirty.is_some() {
            let _ = self.storage.save(&self.project);
        }
    }
}

pub fn demo_project() -> Project {
    let mut p = Project {
        name: "A little closer to space".into(),
        ..Project::default()
    };

    for (id, name, start, duration) in [
        ("demo-0", "01 · Launch day", 0., 8.),
        ("demo-1", "02 · From the workbench", 8., 7.),
        ("demo-2", "03 · Into the sky", 15., 9.),
    ] {
        p.clips.push(Clip {
            asset_id: Some(id.into()),
            kind: Kind::Image,
            name: name.into(),
            start,
            duration,
            transition: Transition::Fade,
            ..Clip::default()
        });
    }

    let a = Transform {
        x: 34.,
        y: 46.,
        scale: 0.8,
        ..Transform::default()
    };

    let b = Transform {
        x: 34.,
        y: 42.,
        scale: 0.84,
        ..Transform::default()
    };

    p.clips.push(Clip {
        kind: Kind::Title,
        name: "A LITTLE CLOSER TO SPACE".into(),
        text: "A LITTLE CLOSER\nTO SPACE".into(),
        font_size: 108,
        track: 0,
        duration: 6.,
        transform: a.clone(),
        transition: Transition::Fade,
        keyframes: vec![
            Keyframe {
                time: 0.,
                transform: a,
                ..Keyframe::default()
            },
            Keyframe {
                time: 6.,
                transform: b,
                ..Keyframe::default()
            },
        ],
        ..Clip::default()
    });

    p.clips.push(Clip {
        kind: Kind::Title,
        name: "THE ROCKET DIARIES / EP. 001".into(),
        text: "THE ROCKET DIARIES   /   EP. 001".into(),
        font_size: 25,
        track: 0,
        duration: 6.,
        transform: Transform {
            x: 34.,
            y: 61.,
            ..Transform::default()
        },
        color: "#b2ccbd".into(),
        transition: Transition::Fade,
        ..Clip::default()
    });

    p.clips.push(Clip {
        kind: Kind::Graphic,
        graphic: Graphic::LowerThird,
        name: "Build annotation".into(),
        text: "01  /  FROM THE WORKBENCH".into(),
        track: 0,
        start: 9.,
        duration: 5.,
        color: "#c6e99d".into(),
        transition: Transition::Slide,
        ..Clip::default()
    });

    p
}

fn hex_color(s: &str) -> [u8; 3] {
    let n = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0xffffff);

    [(n >> 16) as u8, (n >> 8) as u8, n as u8]
}

fn format_bytes(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1} GB", n as f64 / 1e9)
    } else {
        format!("{:.1} MB", n as f64 / 1e6)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn input(events: Vec<egui::Event>, modifiers: egui::Modifiers) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 960.))),
            events,
            modifiers,
            ..Default::default()
        }
    }

    #[test]
    fn native_ui_draws_and_keyboard_edits_can_undo() {
        let temp = tempfile::TempDir::new().unwrap();

        let mut app = FrameApp::new(Storage::open(temp.path().into()).unwrap()).unwrap();

        let ctx = egui::Context::default();

        FrameApp::configure(&ctx);

        let output = ctx.run(input(vec![], egui::Modifiers::default()), |ctx| {
            app.draw(ctx)
        });

        assert!(
            output.shapes.len() > 30,
            "Native editor must paint its actual controls and timeline"
        );

        assert_eq!(app.project.clips.len(), 6);

        let event = egui::Event::Key {
            key: egui::Key::S,
            physical_key: Some(egui::Key::S),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        };

        let _ = ctx.run(input(vec![event], egui::Modifiers::default()), |ctx| {
            app.draw(ctx)
        });

        assert_eq!(
            app.project.clips.len(),
            7,
            "S must split the selected native timeline clip"
        );

        app.history.undo(&mut app.project);

        assert_eq!(app.project.clips.len(), 6);

        app.project.validate().unwrap();
    }

    #[test]
    fn galleries_and_portable_workspace_draw_in_the_native_editor() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut app = FrameApp::new(Storage::open(temp.path().join("library")).unwrap()).unwrap();
        let ctx = egui::Context::default();
        FrameApp::configure(&ctx);
        for view in ["graphics", "motion", "formats"] {
            app.prepare_smoke_view(view).unwrap();
            let output = ctx.run(input(vec![], egui::Modifiers::default()), |ctx| {
                app.draw(ctx)
            });
            assert!(output.shapes.len() > 30);
        }
        assert!(app.thumb_cache.contains_key("graphic-Rocket"));
        let mut p = Project {
            name: "Restored explanation".into(),
            clips: presets::Scene::Forces.clips(0., 30),
            ..Project::default()
        };
        p.muted_tracks = vec![4];
        let selected = p.clips[1].id.clone();
        let path = temp.path().join("explanation.frame");
        Document::capture(
            &p,
            &HashMap::new(),
            Workspace {
                playhead: 2.,
                zoom: 0.1,
                selected: Some(selected.clone()),
            },
        )
        .save(&path)
        .unwrap();
        app.restore_document(Document::read(&path).unwrap());
        assert_eq!(app.project, p);
        assert_eq!(app.time, 2.);
        assert_eq!(app.zoom, 0.1);
        assert_eq!(app.selected.as_ref(), Some(&selected));
        assert!(!app.storage.snapshots().is_empty());
    }

    #[test]
    fn themes_apply_immediately_preserve_edits_and_survive_restart() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join("library");
        {
            let mut app = FrameApp::new(Storage::open(root.clone()).unwrap()).unwrap();
            let project = app.project.clone();
            let ctx = egui::Context::default();
            FrameApp::configure(&ctx);
            for theme in THEMES {
                app.choose_theme(theme, &ctx);
                let output = ctx.run(input(vec![], egui::Modifiers::default()), |ctx| {
                    app.draw(ctx)
                });
                assert!(output.shapes.len() > 30);
                assert_eq!(
                    app.project, project,
                    "Appearance must not modify footage, timeline or exports"
                );
                assert_eq!(ctx.style().visuals.dark_mode, !theme.colors().light);
                assert_eq!(ctx.style().visuals.panel_fill, theme.colors().panel);
            }
            app.theme_window = true;
            let output = ctx.run(input(vec![], egui::Modifiers::default()), |ctx| {
                app.draw(ctx)
            });
            assert!(output.shapes.len() > 60);
        }
        let app = FrameApp::new(Storage::open(root).unwrap()).unwrap();
        assert_eq!(app.theme, Theme::Dawn);
    }

    #[test]
    fn workspace_modes_open_relevant_controls_without_changing_the_timeline() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut app = FrameApp::new(Storage::open(temp.path().into()).unwrap()).unwrap();
        let original = app.project.clone();
        let ctx = egui::Context::default();
        FrameApp::configure(&ctx);
        for (mode, tab, property) in [
            (WorkMode::Edit, Tab::Media, PropertyTab::Properties),
            (WorkMode::Color, Tab::Effects, PropertyTab::Color),
            (WorkMode::Audio, Tab::Audio, PropertyTab::Audio),
            (WorkMode::Motion, Tab::Motion, PropertyTab::Properties),
        ] {
            app.activate_workspace(mode);
            assert!(app.tab == tab && app.property_tab == property);
            let output = ctx.run(input(vec![], egui::Modifiers::default()), |ctx| {
                app.draw(ctx)
            });
            assert!(output.shapes.len() > 30);
            assert_eq!(app.project, original);
        }
    }

    #[test]
    fn motion_studio_draws_custom_curves_paths_and_chart_controls_in_each_theme() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut app = FrameApp::new(Storage::open(temp.path().into()).unwrap()).unwrap();
        app.prepare_smoke_view("animation").unwrap();
        let i = app.selected_index().unwrap();
        app.project.clips[i].kind = Kind::Graphic;
        app.project.clips[i].asset_id = None;
        app.project.clips[i].graphic = Graphic::Chart;
        app.project.clips[i].keyframes[0].easing = crate::animation::Easing::Bezier {
            x1: 0.22,
            y1: 0.,
            x2: 0.36,
            y2: 1.,
        };
        let original = app.project.clone();
        let ctx = egui::Context::default();
        for theme in THEMES {
            app.choose_theme(theme, &ctx);
            let out = ctx.run(input(vec![], egui::Modifiers::default()), |ctx| {
                app.draw(ctx)
            });
            assert!(out.shapes.len() > 100);
            assert_eq!(app.project, original);
            app.project.validate().unwrap();
        }
    }

    #[test]
    fn motion_studio_handles_tightly_spaced_keys_and_off_canvas_paths() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut app = FrameApp::new(Storage::open(temp.path().into()).unwrap()).unwrap();
        app.prepare_smoke_view("animation").unwrap();
        let i = app.selected_index().unwrap();
        for (n, key) in app.project.clips[i].keyframes.iter_mut().enumerate() {
            key.time = n as f64 * 1e-8;
        }
        app.project.clips[i].keyframes[0].path = Some(crate::animation::MotionPath {
            control1: [-100., 200.],
            control2: [200., -100.],
        });
        app.motion_key = 1;
        app.project.validate().unwrap();
        let ctx = egui::Context::default();
        let original = app.project.clone();
        let out = ctx.run(input(vec![], egui::Modifiers::default()), |ctx| {
            app.draw(ctx)
        });
        assert!(out.shapes.len() > 100);
        assert_eq!(app.project, original);
    }

    #[test]
    fn vfx_studio_draws_vectors_scene_tracking_and_blur_without_changing_content() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut app = FrameApp::new(Storage::open(temp.path().into()).unwrap()).unwrap();
        let ctx = egui::Context::default();
        for view in ["vectors", "3d", "tracking", "blur"] {
            app.prepare_smoke_view(view).unwrap();
            let before = app.project.clone();
            for _ in 0..3 {
                let output = ctx.run(input(vec![], egui::Modifiers::default()), |ctx| {
                    app.draw(ctx)
                });
                assert!(output.shapes.len() > 50);
            }
            assert_eq!(app.project, before);
            app.project.validate().unwrap();
        }
    }

    #[test]
    fn switching_projects_preserves_a_restorable_snapshot() {
        let temp = tempfile::TempDir::new().unwrap();

        let mut app = FrameApp::new(Storage::open(temp.path().into()).unwrap()).unwrap();

        let original = app.project.clone();

        app.switch(Project::default());

        assert!(app.project.clips.is_empty());

        let snaps = app.storage.snapshots();

        assert_eq!(snaps.len(), 1);

        assert_eq!(load_project(&snaps[0].1).unwrap(), original);
    }

    #[test]
    fn pending_native_edits_are_saved_on_drop() {
        let temp = tempfile::TempDir::new().unwrap();

        let root = temp.path().to_owned();

        {
            let mut app = FrameApp::new(Storage::open(root.clone()).unwrap()).unwrap();

            app.project.name = "Saved at native shutdown".into();

            app.modified();
        }

        let storage = Storage::open(root).unwrap();

        assert_eq!(
            storage.load().unwrap().unwrap().name,
            "Saved at native shutdown"
        );
    }
}
