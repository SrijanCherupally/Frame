use super::*;

impl FrameApp {
    /// Exercise the actual native panels for screenshot smoke tests.
    pub fn prepare_smoke_view(&mut self, view: &str) -> anyhow::Result<()> {
        match view {
            "graphics" => self.tab = Tab::Graphics,
            "vectors" => {
                self.open_effects(super::effects::Panel::Vector);
                self.new_effect_layer(super::effects::Panel::Vector);
            }
            "3d" => {
                self.open_effects(super::effects::Panel::Scene);
                self.new_effect_layer(super::effects::Panel::Scene);
            }
            "tracking" => self.open_effects(super::effects::Panel::Tracking),
            "tracking-demo" => self.prepare_tracking_demo()?,
            "blur" => self.open_effects(super::effects::Panel::Blur),
            "motion" => self.tab = Tab::Motion,
            "animation" => {
                self.activate_workspace(WorkMode::Motion);
                self.motion_window = true;
                if let Some(i) = self.selected_index() {
                    presets::Motion::Arc.apply(&mut self.project.clips[i], self.project.fps);
                    self.project.clips[i].keyframes[0].easing = crate::animation::Easing::Bezier {
                        x1: 0.22,
                        y1: 0.,
                        x2: 0.36,
                        y2: 1.,
                    };
                    let time = self.project.clips[i].start + 0.35;
                    self.seek(time);
                    self.motion_key = 0;
                }
            }
            "themes" => self.theme_window = true,
            "edit" => self.activate_workspace(WorkMode::Edit),
            "color" => self.activate_workspace(WorkMode::Color),
            "audio" => self.activate_workspace(WorkMode::Audio),
            "midnight" => self.theme = Theme::Midnight,
            "graphite" => self.theme = Theme::Graphite,
            "amethyst" => self.theme = Theme::Amethyst,
            "forest" => self.theme = Theme::Forest,
            "dawn" => self.theme = Theme::Dawn,
            "formats" => {
                self.template_window = true;
                self.template_new = true;
            }
            _ => anyhow::bail!("Unknown smoke view: {view}"),
        }
        Ok(())
    }

    pub fn restore_document(&mut self, mut doc: Document) {
        match doc.restore(&self.storage, &self.assets) {
            Ok(restored) => self.finish_document(doc, restored),
            Err(e) => self.notify(format!("Could not open project: {e}")),
        }
    }
    pub(super) fn finish_document(&mut self, doc: Document, restored: Vec<Asset>) {
        if let Err(e) = self.storage.snapshot(&self.project) {
            self.notify(format!("Could not preserve current project: {e}"));
            return;
        }
        let missing = restored.iter().filter(|a| !a.path.is_file()).count();
        for a in restored {
            if a.path.is_file()
                && matches!(a.status.as_str(), "processing" | "proxying")
                && let Some(tx) = &self.imports
            {
                let _ = tx.send(Import {
                    path: a.path.clone(),
                    id: Some(a.id.clone()),
                });
            }
            Arc::make_mut(&mut self.assets).insert(a.id.clone(), a);
        }
        self.history.remember(&self.project);
        self.project = doc.project;
        self.time = doc.workspace.playhead.min(self.project.duration());
        self.zoom = if doc.workspace.zoom > 0. {
            doc.workspace.zoom.clamp(0.03, 150.)
        } else {
            32.
        };
        self.selected = doc
            .workspace
            .selected
            .filter(|id| self.project.clips.iter().any(|c| &c.id == id));
        self.modified();
        self.notify(if missing > 0 {
            format!("Project restored. {missing} offline originals: use Retry / relink in Media.")
        } else {
            "Project restored with timeline, animation and media links.".into()
        });
    }

    pub(super) fn format_dialog(&mut self, ctx: &egui::Context) {
        if !self.template_window {
            return;
        }
        let mut open = true;
        let mut chosen = None;
        egui::Window::new(if self.template_new{"Start a new story"}else{"Choose your canvas"})
            .open(&mut open).collapsible(false).resizable(false).default_width(660.)
            .show(ctx,|ui|{
                ui.label(RichText::new("VIDEO FORMATS").small().color(palette().accent));
                ui.label(if self.template_new{"Choose a starting format. Customize it anytime in Settings."}else{"Change the canvas and frame rate together. Your clips stay editable."});
                ui.add_space(12.);
                egui::ScrollArea::vertical().max_height(520.).show(ui,|ui|{
                    egui::Grid::new("formats").num_columns(2).spacing([12.,12.]).show(ui,|ui|{
                        for (i,preset) in presets::FORMATS.iter().enumerate(){
                            let accent=if preset.width<preset.height{palette().violet}else if preset.width==preset.height{palette().amber}else if preset.width>=3840{palette().cyan}else{palette().accent};
                            let response=ui.allocate_ui_with_layout(Vec2::new(308.,112.),egui::Layout::top_down(egui::Align::Min),|ui|{
                                card().show(ui,|ui|{
                                    ui.set_min_width(280.);
                                    ui.horizontal(|ui|{
                                        let (r,_)=ui.allocate_exact_size(Vec2::new(42.,40.),Sense::hover());
                                        let aspect=preset.width as f32/preset.height as f32;
                                        let size=if aspect>1.{Vec2::new(36.,36./aspect)}else{Vec2::new(36.*aspect,36.)};
                                        ui.painter().rect_stroke(Rect::from_center_size(r.center(),size),3.,Stroke::new(1.5_f32,accent),StrokeKind::Inside);
                                        ui.vertical(|ui|{ui.label(RichText::new(preset.name).strong());ui.label(RichText::new(format!("{} × {}  ·  {} fps",preset.width,preset.height,preset.fps)).small().color(accent));});
                                    });
                                    ui.label(RichText::new(preset.description).small().color(palette().muted));
                                    if ui.button(if self.template_new{"Create project"}else{"Apply format"}).clicked(){chosen=Some(i);}
                                });
                            });
                            response.response.on_hover_text(preset.description);
                            if i%2==1{ui.end_row();}
                        }
                    });
                });
                ui.add_space(8.);
                ui.label(RichText::new("Need another size? Settings supports custom even pixel dimensions and 12–60 fps.").small().color(palette().muted));
            });
        self.template_window = open;
        if let Some(i) = chosen {
            let mut next = if self.template_new {
                Project::default()
            } else {
                self.project.clone()
            };
            match presets::FORMATS[i].apply(&mut next) {
                Ok(()) => {
                    if self.template_new {
                        self.switch(next);
                    } else {
                        self.history.remember(&self.project);
                        self.project = next;
                        self.modified();
                    }
                    self.template_window = false;
                }
                Err(e) => self.notify(format!(
                    "Cannot apply format: {e}. A clip may be shorter than one frame."
                )),
            }
        }
    }

    pub(super) fn graphics_library(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("EXPLAIN IT VISUALLY")
                .small()
                .color(palette().accent),
        );
        ui.label(RichText::new("Editable diagrams, labels and data. Add a graphic, then customize it in Properties.").small().color(palette().muted));
        ui.horizontal_wrapped(|ui| {
            if ui.button("Vector Studio").clicked() {
                self.open_effects(super::effects::Panel::Vector);
            }
            if ui.button("3D Studio").clicked() {
                self.open_effects(super::effects::Panel::Scene);
            }
        });
        ui.add_space(8.);
        egui::CollapsingHeader::new("Ready-made explanation scenes")
            .default_open(true)
            .show(ui, |ui| {
                for &(scene, name, description) in presets::SCENES {
                    if ui
                        .add_sized(
                            [ui.available_width(), 40.],
                            egui::Button::new(format!("+ {name}")),
                        )
                        .on_hover_text(description)
                        .clicked()
                    {
                        let clips = scene.clips(self.time, self.project.fps);
                        if self.project.clips.len() + clips.len() > 2000 || self.time + 6. > 86400.
                        {
                            self.notify("Scene would exceed the timeline or clip limit");
                            continue;
                        }
                        self.history.remember(&self.project);
                        self.selected = clips.first().map(|c| c.id.clone());
                        self.project.clips.extend(clips);
                        self.modified();
                    }
                }
            });
        ui.separator();
        for &(graphic, label, text) in presets::GRAPHICS {
            let id = format!("graphic-{graphic:?}");
            if !self.thumb_cache.contains_key(&id) {
                let c = Clip {
                    kind: Kind::Graphic,
                    graphic,
                    text: text.into(),
                    color: "#56dae0".into(),
                    font_size: 48,
                    ..Clip::default()
                };
                let path = self.storage.root.join("preview").join(format!("{id}.png"));
                if render::graphic_image(&c, &path, 320, 180).is_ok()
                    && let Ok(img) = image::open(&path)
                {
                    let rgba = img.to_rgba8();
                    let texture = ui.ctx().load_texture(
                        &id,
                        egui::ColorImage::from_rgba_unmultiplied([320, 180], rgba.as_raw()),
                        egui::TextureOptions::LINEAR,
                    );
                    self.thumb_cache.insert(id.clone(), texture);
                }
            }
            egui::Frame::new()
                .fill(palette().card)
                .stroke(Stroke::new(1.0_f32, palette().border))
                .corner_radius(8.)
                .inner_margin(9.)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    if let Some(texture) = self.thumb_cache.get(&id) {
                        let width = ui.available_width();
                        let response = ui.add(
                            egui::Image::new(texture)
                                .fit_to_exact_size(Vec2::new(width, width * 0.45))
                                .sense(Sense::click()),
                        );
                        if response.clicked() {
                            self.add_layer(Kind::Graphic, graphic, text, 48);
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(label).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("+ Add").clicked() {
                                self.add_layer(Kind::Graphic, graphic, text, 48);
                            }
                        });
                    });
                });
            ui.add_space(4.);
        }
    }

    pub(super) fn motion_library(&mut self, ui: &mut egui::Ui) {
        let index = self
            .selected_index()
            .filter(|i| self.project.clips[*i].is_visual());
        ui.label(
            RichText::new("A LITTLE MOTION. MORE MEANING.")
                .small()
                .color(palette().accent),
        );
        ui.label(RichText::new(if index.is_some(){"Choose a motion for your selected clip. Replaces its current keyframes; undo is available."}else{"Select a visual clip on the timeline, then choose its motion."}).small().color(palette().muted));
        if ui.button("Open Motion Studio →").clicked() {
            self.motion_window = true;
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Tracking").clicked() {
                self.open_effects(super::effects::Panel::Tracking);
            }
            if ui.button("Motion blur").clicked() {
                self.open_effects(super::effects::Panel::Blur);
            }
        });
        ui.add_space(8.);
        card().show(ui,|ui| {
            section_title(ui,"Preset controls",palette().violet);
            ui.add(egui::Slider::new(&mut self.motion_settings.duration,0.1..=5.).text("Arrival").suffix(" s"));
            ui.add(egui::Slider::new(&mut self.motion_settings.amount,0.05..=2.).text("Intensity"));
            egui::ComboBox::from_id_salt("preset-ease").selected_text(self.motion_settings.easing.name()).show_ui(ui,|ui|{for &(e,name) in crate::animation::EASINGS {ui.selectable_value(&mut self.motion_settings.easing,e,name);}});
            ui.label(RichText::new("Apply to replace this layer's animation. Slow pans and loops span its full duration.").small().color(palette().muted));
        });
        ui.add_space(10.);
        for (number, &(motion, label, description)) in presets::MOTIONS.iter().enumerate() {
            let accent = [
                palette().violet,
                palette().cyan,
                palette().accent,
                palette().amber,
            ][number % 4];
            let applied = card()
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(26.), Sense::hover());
                        ui.painter()
                            .rect_filled(r, 6., mix(palette().card, accent, 0.2));
                        ui.painter().text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("{:02}", number + 1),
                            egui::FontId::monospace(11.),
                            accent,
                        );
                        ui.label(RichText::new(label).strong().size(13.));
                    });
                    ui.label(RichText::new(description).small().color(palette().muted));
                    ui.add_enabled(
                        index.is_some(),
                        egui::Button::new(RichText::new("Apply motion  →").color(accent))
                            .fill(mix(palette().card, accent, 0.12)),
                    )
                    .clicked()
                })
                .inner;
            ui.add_space(5.);
            if applied && let Some(i) = index {
                self.history.remember(&self.project);
                motion.apply_with(
                    &mut self.project.clips[i],
                    self.project.fps,
                    self.motion_settings,
                );
                self.seek(self.project.clips[i].start);
                self.modified();
                self.notify(format!(
                    "{label} applied. Adjust the keyframes in Properties."
                ));
            }
        }
    }

    pub(super) fn activate_workspace(&mut self, workspace: WorkMode) {
        self.workspace = workspace;
        (self.tab, self.property_tab) = match workspace {
            WorkMode::Edit => (Tab::Media, PropertyTab::Properties),
            WorkMode::Color => (Tab::Effects, PropertyTab::Color),
            WorkMode::Audio => (Tab::Audio, PropertyTab::Audio),
            WorkMode::Motion => (Tab::Motion, PropertyTab::Properties),
        };
        if matches!(workspace, WorkMode::Color | WorkMode::Motion)
            && self
                .selected_index()
                .is_none_or(|i| !self.project.clips[i].is_visual())
        {
            self.selected = self
                .project
                .clips
                .iter()
                .find(|c| c.is_visual())
                .map(|c| c.id.clone());
        }
    }
    pub(super) fn workspace_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("workspaces")
            .exact_height(38.)
            .frame(surface(palette().panel, 7))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("WORKSPACE").size(9.).color(palette().muted));
                    ui.add_space(12.);
                    for (workspace, label, color) in [
                        (WorkMode::Edit, "Edit", palette().accent),
                        (WorkMode::Color, "Color", palette().amber),
                        (WorkMode::Audio, "Audio", palette().cyan),
                        (WorkMode::Motion, "Motion", palette().violet),
                    ] {
                        let active = self.workspace == workspace;
                        if ui
                            .add(
                                egui::Button::new(RichText::new(label).color(if active {
                                    palette().text
                                } else {
                                    palette().muted
                                }))
                                .fill(if active {
                                    mix(palette().card, color, 0.2)
                                } else {
                                    palette().panel
                                })
                                .stroke(if active {
                                    Stroke::new(1.0_f32, color)
                                } else {
                                    Stroke::NONE
                                })
                                .min_size(Vec2::new(70., 24.)),
                            )
                            .clicked()
                        {
                            self.activate_workspace(workspace);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("PROJECT  /  PROGRAM  /  TIMELINE")
                                .size(9.)
                                .color(palette().muted),
                        );
                    });
                });
            });
    }

    pub(super) fn choose_theme(&mut self, theme: Theme, ctx: &egui::Context) {
        self.theme = theme;
        if let Err(error) = atomic_json(
            &self.storage.root.join("preferences.json"),
            &Preferences { theme },
        ) {
            self.notify(format!(
                "Theme changed, but could not save the preference: {error}"
            ));
        }
        ctx.request_repaint();
    }

    pub(super) fn theme_dialog(&mut self, ctx: &egui::Context) {
        if !self.theme_window {
            return;
        }
        let mut open = true;
        let mut chosen = None;
        egui::Window::new("Make Frame feel like yours").open(&mut open).default_width(610.).resizable(false).collapsible(false).show(ctx,|ui|{
            ui.label(RichText::new("APPEARANCE").small().color(palette().accent));
            ui.label("Choose a theme. Your videos and exported colors stay unchanged.");
            ui.add_space(12.);
            egui::ScrollArea::vertical().id_salt("themes_scroll").max_height(500.).show(ui,|ui|{
            egui::Grid::new("theme_gallery").num_columns(2).spacing([12.,12.]).show(ui,|ui|{
                for (index,theme) in THEMES.iter().copied().enumerate(){
                    let colors=theme.colors();
                    ui.allocate_ui_with_layout(Vec2::new(290.,175.),egui::Layout::top_down(egui::Align::Min),|ui|{
                    card().show(ui,|ui|{
                        ui.set_width(265.);
                        let (r,response)=ui.allocate_exact_size(Vec2::new(265.,82.),Sense::click());
                        ui.painter().rect_filled(r,7.,colors.canvas);
                        ui.painter().rect_filled(Rect::from_min_size(r.min,Vec2::new(r.width(),12.)),5.,colors.rail);
                        ui.painter().rect_filled(Rect::from_min_size(r.min+Vec2::new(6.,18.),Vec2::new(48.,56.)),4.,colors.panel);
                        ui.painter().rect_filled(Rect::from_min_size(r.min+Vec2::new(204.,18.),Vec2::new(53.,56.)),4.,colors.panel);
                        ui.painter().rect_filled(Rect::from_min_size(r.min+Vec2::new(210.,25.),Vec2::new(41.,18.)),3.,colors.card);
                        ui.painter().rect_filled(Rect::from_min_size(r.min+Vec2::new(62.,59.),Vec2::new(134.,15.)),3.,colors.card);
                        for (i,color) in colors.tracks().iter().enumerate(){ui.painter().circle_filled(r.min+Vec2::new(76.+i as f32*23.,38.),5.,*color);}
                        if self.theme==theme{ui.painter().rect_stroke(r,7.,Stroke::new(2.0_f32,palette().accent),StrokeKind::Inside);}
                        ui.horizontal(|ui|{ui.label(RichText::new(theme.name()).strong());if self.theme==theme{ui.label(RichText::new("ACTIVE").small().color(palette().accent));}});
                        ui.label(RichText::new(theme.description()).small().color(palette().muted));
                        if response.clicked()||ui.button(if self.theme==theme{"Selected"}else{"Use theme"}).clicked(){chosen=Some(theme);}
                    });
                    });
                    if index%2==1{ui.end_row();}
                }
            });
            });
            ui.add_space(8.);
            ui.label(RichText::new("Saved on this device. Applies across the workspace, galleries and timeline.").small().color(palette().muted));
        });
        self.theme_window = open;
        if let Some(theme) = chosen {
            self.choose_theme(theme, ctx);
        }
    }

    pub(super) fn appearance_controls(&mut self, ui: &mut egui::Ui, c: &mut Clip) {
        self.chart_controls(ui, c);
        if c.kind != Kind::Graphic {
            return;
        }
        ui.add(egui::Slider::new(&mut c.stroke_width, 1.0..=40.).text("Line weight"));
        ui.horizontal(|ui| {
            ui.label("Background");
            let mut rgb = hex_color(&c.secondary_color);
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                c.secondary_color = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            }
        });
        if matches!(
            c.graphic,
            Graphic::Circle | Graphic::Rectangle | Graphic::Rocket
        ) {
            ui.checkbox(&mut c.fill, "Fill shape");
        }
        if matches!(c.graphic, Graphic::Progress | Graphic::Comparison) {
            ui.add(egui::Slider::new(&mut c.graphic_value, 0.0..=1.).text("Value"));
        }
    }
}
