//! Context menus share transactional actions with keyboard shortcuts.
use super::*;
#[derive(Clone, Copy)]
pub(super) enum ClipCommand {
    Copy,
    Cut,
    Duplicate,
    Delete,
    Split,
    PasteAnimation,
    ResetTransform,
    ResetColor,
    ClearAnimation,
    Move(usize),
    Detach,
    Mute,
}
impl FrameApp {
    pub(super) fn clip_command(&mut self, id: &str, action: ClipCommand) {
        let Some(index) = self.project.clips.iter().position(|c| c.id == id) else {
            return;
        };
        let source = self.project.clips[index].clone();
        if matches!(action, ClipCommand::Copy | ClipCommand::Cut) {
            self.clip_clipboard = Some(source.clone());
            // Native integrations emit Paste only when the OS clipboard has text.
            // Keep the editable clip internally and publish a harmless identity marker.
            self.clipboard_text = Some(format!("Frame clip · {}", source.name));
            if matches!(action, ClipCommand::Copy) {
                self.notify("Clip copied · paste at the playhead");
                return;
            }
        }
        let mut next = self.project.clone();
        let mut selected = Some(id.to_owned());
        match action {
            ClipCommand::Copy => return,
            ClipCommand::Cut | ClipCommand::Delete => {
                next.clips.remove(index);
            }
            ClipCommand::Duplicate => {
                let mut c = source.clone();
                c.id = uid();
                c.start = c.end();
                selected = Some(c.id.clone());
                next.clips.push(c);
            }
            ClipCommand::Split => {
                if !edit::split(&mut next, id, self.time) {
                    self.notify("Place the playhead inside this clip to split");
                    return;
                }
            }
            ClipCommand::PasteAnimation => {
                if let Some(c) = &self.animation_clipboard {
                    edit::paste_animation(c, &mut next.clips[index]);
                }
            }
            ClipCommand::ResetTransform => {
                next.clips[index].transform = Transform::default();
                next.clips[index].keyframes.clear();
            }
            ClipCommand::ResetColor => {
                let c = &mut next.clips[index];
                c.brightness = 0.;
                c.contrast = 1.;
                c.saturation = 1.;
                c.warmth = 0.;
                c.blur = 0.;
            }
            ClipCommand::ClearAnimation => {
                next.clips[index].keyframes.clear();
            }
            ClipCommand::Move(track) => {
                next.clips[index].track = track;
            }
            ClipCommand::Detach => {
                if !source
                    .asset_id
                    .as_ref()
                    .and_then(|id| self.assets.get(id))
                    .is_some_and(|a| a.has_audio)
                {
                    return;
                }
                let Some(audio) = edit::detach_audio(&mut next, id) else {
                    return;
                };
                selected = Some(audio);
            }
            ClipCommand::Mute => {
                next.clips[index].volume = if source.volume > 0. { 0. } else { 1. };
            }
        }
        self.commit_context(next, selected);
    }
    fn commit_context(&mut self, next: Project, selected: Option<String>) {
        if next == self.project {
            return;
        }
        if let Err(e) = next.validate() {
            self.notify(e.to_string());
            return;
        }
        self.history.remember(&self.project);
        self.project = next;
        self.selected = selected.filter(|id| self.project.clips.iter().any(|c| &c.id == id));
        self.modified();
    }
    pub(super) fn paste_clip(&mut self, at: f64, track: Option<usize>) {
        let Some(mut c) = self.clip_clipboard.clone() else {
            return;
        };
        if let Some(track) = track
            && ((c.kind == Kind::Audio && track < 3) || (c.kind != Kind::Audio && track >= 3))
        {
            self.notify("Paste audio on an audio track, and visual clips on a visual track");
            return;
        }
        c.id = uid();
        c.start = (at * self.project.fps as f64).round().max(0.) / self.project.fps as f64;
        if let Some(track) = track {
            c.track = track;
        }
        let id = c.id.clone();
        let mut next = self.project.clone();
        next.clips.push(c);
        self.commit_context(next, Some(id));
    }
    pub(super) fn clip_menu(&mut self, ui: &mut egui::Ui, c: &Clip) {
        ui.set_min_width(220.);
        ui.add(egui::Label::new(RichText::new(&c.name).strong()).truncate())
            .on_hover_text(&c.name);
        ui.label(
            RichText::new(format!(
                "{} · {}",
                TRACKS[c.track],
                timecode(c.duration, self.project.fps)
            ))
            .small()
            .color(palette().muted),
        );
        ui.separator();
        for (label, key, action, enabled) in [
            (
                "Split at playhead",
                "S",
                ClipCommand::Split,
                self.time > c.start + 1. / self.project.fps as f64 - 1e-8
                    && self.time < c.end() - 1. / self.project.fps as f64 + 1e-8,
            ),
            ("Copy clip", "Ctrl+C", ClipCommand::Copy, true),
            ("Cut clip", "Ctrl+X", ClipCommand::Cut, true),
            ("Duplicate", "Ctrl+D", ClipCommand::Duplicate, true),
        ] {
            if ui
                .add_enabled(enabled, egui::Button::new(label).shortcut_text(key))
                .clicked()
            {
                self.clip_command(&c.id, action);
                ui.close();
            }
        }
        if ui.button("Rename…").clicked() {
            self.rename_clip = Some((c.id.clone(), c.name.clone()));
            ui.close();
        }
        ui.menu_button("Move to track", |ui| {
            for (track, name) in TRACKS.iter().enumerate() {
                let compatible = if c.kind == Kind::Audio {
                    track >= 3
                } else {
                    track < 3
                };
                if ui
                    .add_enabled(compatible && track != c.track, egui::Button::new(*name))
                    .clicked()
                {
                    self.clip_command(&c.id, ClipCommand::Move(track));
                    ui.close();
                }
            }
        });
        if matches!(c.kind, Kind::Audio | Kind::Video) {
            if ui
                .button(if c.volume > 0. {
                    "Mute clip"
                } else {
                    "Unmute clip (100%)"
                })
                .clicked()
            {
                self.clip_command(&c.id, ClipCommand::Mute);
                ui.close();
            }
            let detach = c.kind == Kind::Video
                && c.volume > 0.
                && c.asset_id
                    .as_ref()
                    .and_then(|id| self.assets.get(id))
                    .is_some_and(|a| a.has_audio);
            if ui
                .add_enabled(detach, egui::Button::new("Detach audio"))
                .clicked()
            {
                self.clip_command(&c.id, ClipCommand::Detach);
                ui.close();
            }
        }
        if c.is_visual() {
            ui.separator();
            if ui.button("Open Motion Studio…").clicked() {
                self.selected = Some(c.id.clone());
                self.motion_window = true;
                ui.close();
            }
            if c.vector.is_some() && ui.button("Edit vector…").clicked() {
                self.open_effects(super::effects::Panel::Vector);
                ui.close();
            }
            if c.scene3d.is_some() && ui.button("Edit 3D scene…").clicked() {
                self.open_effects(super::effects::Panel::Scene);
                ui.close();
            }
            if ui.button("Copy animation").clicked() {
                self.animation_clipboard = Some(c.clone());
                ui.close();
            }
            if ui
                .add_enabled(
                    self.animation_clipboard.is_some(),
                    egui::Button::new("Paste animation & fit"),
                )
                .clicked()
            {
                self.clip_command(&c.id, ClipCommand::PasteAnimation);
                ui.close();
            }
            ui.menu_button("Reset", |ui| {
                for (label, action) in [
                    ("Transform & transform keys", ClipCommand::ResetTransform),
                    ("Color & blur", ClipCommand::ResetColor),
                    ("Transform keys only", ClipCommand::ClearAnimation),
                ] {
                    if ui.button(label).clicked() {
                        self.clip_command(&c.id, action);
                        ui.close();
                    }
                }
            });
        }
        ui.separator();
        if ui
            .add(
                egui::Button::new(RichText::new("Delete clip").color(palette().coral))
                    .shortcut_text("Delete"),
            )
            .clicked()
        {
            self.clip_command(&c.id, ClipCommand::Delete);
            ui.close();
        }
    }
    pub(super) fn media_menu(&mut self, ui: &mut egui::Ui, a: &Asset) {
        ui.set_min_width(210.);
        ui.add(egui::Label::new(RichText::new(&a.name).strong()).truncate())
            .on_hover_text(&a.name);
        ui.separator();
        ui.add_enabled_ui(a.ready(), |ui| {
            if ui.button("Add at playhead").clicked() {
                self.add_asset(&a.id, Some(self.time), None);
                ui.close();
            }
            if ui.button("Append to timeline").clicked() {
                self.add_asset(&a.id, None, None);
                ui.close();
            }
            if a.has_audio && ui.button("Add audio only").clicked() {
                self.add_asset(&a.id, Some(self.time), Some(3));
                ui.close();
            }
        });
        ui.separator();
        if !a.demo && ui.button("Relink original…").clicked() {
            self.import_picker(Some(a.id.clone()));
            ui.close();
        }
        if ui.button("Copy source path").clicked() {
            ui.ctx().copy_text(a.path.display().to_string());
            ui.close();
        }
        if ui.button("Media details…").clicked() {
            self.media_details = Some(a.clone());
            ui.close();
        }
    }
    pub(super) fn timeline_menu(&mut self, ui: &mut egui::Ui) {
        let (at, track) = self.timeline_context;
        ui.label(
            RichText::new(format!(
                "{} · {}",
                TRACKS[track],
                timecode(at, self.project.fps)
            ))
            .small()
            .color(palette().muted),
        );
        if ui.button("Move playhead here").clicked() {
            self.seek(at);
            ui.close();
        }
        if ui
            .add_enabled(
                self.clip_clipboard.is_some(),
                egui::Button::new("Paste clip here").shortcut_text("Ctrl+V"),
            )
            .clicked()
        {
            self.paste_clip(at, Some(track));
            ui.close();
        }
        ui.separator();
        if ui.button("Add title here").clicked() {
            let c = Clip {
                kind: Kind::Title,
                name: "Title".into(),
                text: "Your title".into(),
                font_size: 76,
                track: 0,
                start: (at * self.project.fps as f64).round() / self.project.fps as f64,
                transition: Transition::Fade,
                ..Default::default()
            };
            let id = c.id.clone();
            let mut next = self.project.clone();
            next.clips.push(c);
            self.commit_context(next, Some(id.clone()));
            if self.project.clips.iter().any(|c| c.id == id) {
                self.seek(at);
            }
            ui.close();
        }
        if ui.button("Import media…").clicked() {
            self.import_picker(None);
            ui.close();
        }
        if ui
            .button(if self.snap {
                "Disable snapping"
            } else {
                "Enable snapping"
            })
            .clicked()
        {
            self.snap = !self.snap;
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(
                self.history.can_undo(),
                egui::Button::new("Undo").shortcut_text("Ctrl+Z"),
            )
            .clicked()
        {
            self.history.undo(&mut self.project);
            self.modified();
            ui.close();
        }
        if ui
            .add_enabled(
                self.history.can_redo(),
                egui::Button::new("Redo").shortcut_text("Ctrl+Shift+Z"),
            )
            .clicked()
        {
            self.history.redo(&mut self.project);
            self.modified();
            ui.close();
        }
    }
    pub(super) fn context_dialogs(&mut self, ctx: &egui::Context) {
        if let Some((id, mut name)) = self.rename_clip.clone() {
            let mut open = true;
            let mut apply = false;
            egui::Window::new("Rename clip")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    let response = ui.text_edit_singleline(&mut name);
                    if !response.has_focus() && !response.lost_focus() {
                        response.request_focus();
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !name.trim().is_empty() && name.len() <= 800,
                                egui::Button::new("Save name"),
                            )
                            .clicked()
                        {
                            apply = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.rename_clip = None;
                        }
                    });
                    if ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !name.trim().is_empty()
                        && name.len() <= 800
                    {
                        apply = true;
                    }
                });
            if apply {
                let mut next = self.project.clone();
                if let Some(c) = next.clips.iter_mut().find(|c| c.id == id) {
                    c.name = name.trim().into();
                }
                self.commit_context(next, Some(id));
                self.rename_clip = None;
            } else if open && self.rename_clip.is_some() {
                self.rename_clip = Some((id, name));
            } else {
                self.rename_clip = None;
            }
        }
        if let Some(a) = self.media_details.clone() {
            let mut open = true;
            egui::Window::new("Media details")
                .open(&mut open)
                .default_width(400.)
                .show(ctx, |ui| {
                    ui.heading(&a.name);
                    ui.label(format!(
                        "{:?} · {} · {}",
                        a.kind,
                        format_bytes(a.size),
                        a.status
                    ));
                    ui.label(format!(
                        "{} × {} · {:.3} fps · {:.3} seconds",
                        a.width, a.height, a.fps, a.duration
                    ));
                    ui.label(if a.has_audio {
                        "Audio stream present"
                    } else {
                        "No audio stream"
                    });
                    ui.add(
                        egui::Label::new(a.path.display().to_string())
                            .wrap()
                            .selectable(true),
                    );
                    if ui.button("Copy source path").clicked() {
                        ctx.copy_text(a.path.display().to_string());
                    }
                    if let Some(error) = &a.error {
                        ui.label(RichText::new(error).color(palette().coral));
                    }
                });
            if !open {
                self.media_details = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_edits_are_targeted_validated_and_undoable() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = FrameApp::new(Storage::open(dir.path().to_owned()).unwrap()).unwrap();
        let before = app.project.clone();
        let id = app.project.clips[0].id.clone();
        app.clip_command(&id, ClipCommand::Copy);
        assert_eq!(app.project, before);
        app.paste_clip(1., Some(0));
        assert_eq!(app.project.clips.len(), before.clips.len() + 1);
        let pasted = app.selected.clone().unwrap();
        assert_ne!(pasted, id);
        assert_eq!(
            app.project
                .clips
                .iter()
                .find(|c| c.id == pasted)
                .unwrap()
                .start,
            1.
        );
        app.history.undo(&mut app.project);
        assert_eq!(app.project, before);
        app.clip_command(&id, ClipCommand::Cut);
        assert!(!app.project.clips.iter().any(|c| c.id == id));
        app.history.undo(&mut app.project);
        assert_eq!(app.project, before);
        app.clip_command(&id, ClipCommand::Move(2));
        assert_eq!(
            app.project.clips.iter().find(|c| c.id == id).unwrap().track,
            2
        );
        app.history.undo(&mut app.project);
        assert_eq!(app.project, before);
        app.paste_clip(86400., None);
        assert_eq!(
            app.project, before,
            "Reject overflow without mutating the project"
        );
        app.paste_clip(0., Some(4));
        assert_eq!(
            app.project, before,
            "Visual clips cannot paste onto audio tracks"
        );
    }
    #[test]
    fn native_clipboard_events_copy_cut_and_paste_clips() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = FrameApp::new(Storage::open(dir.path().to_owned()).unwrap()).unwrap();
        let ctx = egui::Context::default();
        let before = app.project.clips.len();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Copy],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(app.clip_clipboard.is_some());
        assert!(app.clipboard_text.is_some());
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Paste("Frame clip".into())],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert_eq!(app.project.clips.len(), before + 1);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Cut],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert_eq!(app.project.clips.len(), before);
    }
}
