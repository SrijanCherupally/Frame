use super::*;

impl FrameApp {
    /// Exercise the actual native panels for screenshot smoke tests.
    pub fn prepare_smoke_view(&mut self, view: &str) -> anyhow::Result<()> {
        match view {
            "graphics" => self.tab = Tab::Graphics,
            "motion" => self.tab = Tab::Motion,
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
                ui.label(RichText::new("VIDEO FORMATS").small().color(GREEN));
                ui.label(if self.template_new{"Choose a starting format. Customize it anytime in Settings."}else{"Change the canvas and frame rate together. Your clips stay editable."});
                ui.add_space(12.);
                egui::ScrollArea::vertical().max_height(520.).show(ui,|ui|{
                    egui::Grid::new("formats").num_columns(2).spacing([12.,12.]).show(ui,|ui|{
                        for (i,preset) in presets::FORMATS.iter().enumerate(){
                            let response=ui.allocate_ui_with_layout(Vec2::new(308.,112.),egui::Layout::top_down(egui::Align::Min),|ui|{
                                egui::Frame::new().fill(Color32::from_rgb(33,39,44)).corner_radius(9.).inner_margin(13.).show(ui,|ui|{
                                    ui.set_min_width(280.);
                                    ui.horizontal(|ui|{
                                        let (r,_)=ui.allocate_exact_size(Vec2::new(42.,40.),Sense::hover());
                                        let aspect=preset.width as f32/preset.height as f32;
                                        let size=if aspect>1.{Vec2::new(36.,36./aspect)}else{Vec2::new(36.*aspect,36.)};
                                        ui.painter().rect_stroke(Rect::from_center_size(r.center(),size),3.,Stroke::new(1.5_f32,GREEN),StrokeKind::Inside);
                                        ui.vertical(|ui|{ui.label(RichText::new(preset.name).strong());ui.label(RichText::new(format!("{} × {}  ·  {} fps",preset.width,preset.height,preset.fps)).small().color(GREEN));});
                                    });
                                    ui.label(RichText::new(preset.description).small().color(MUTED));
                                    if ui.button(if self.template_new{"Create project"}else{"Apply format"}).clicked(){chosen=Some(i);}
                                });
                            });
                            response.response.on_hover_text(preset.description);
                            if i%2==1{ui.end_row();}
                        }
                    });
                });
                ui.add_space(8.);
                ui.label(RichText::new("Need another size? Settings supports custom even pixel dimensions and 12–60 fps.").small().color(MUTED));
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
        ui.label(RichText::new("EXPLAIN IT VISUALLY").small().color(GREEN));
        ui.label(RichText::new("Editable diagrams, labels and data. Add a graphic, then customize it in Properties.").small().color(MUTED));
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
                    color: "#c6e99d".into(),
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
                .fill(Color32::from_rgb(30, 35, 40))
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
                .color(GREEN),
        );
        ui.label(RichText::new(if index.is_some(){"Choose a motion for your selected clip. Replaces its current keyframes; undo is available."}else{"Select a visual clip on the timeline, then choose its motion."}).small().color(MUTED));
        ui.add_space(10.);
        for &(motion, label, description) in presets::MOTIONS {
            let response = ui.add_enabled(
                index.is_some(),
                egui::Button::new(RichText::new(format!("{label}\n{description}")).size(12.))
                    .min_size(Vec2::new(ui.available_width(), 68.)),
            );
            if response.clicked()
                && let Some(i) = index
            {
                self.history.remember(&self.project);
                motion.apply(&mut self.project.clips[i], self.project.fps);
                self.seek(self.project.clips[i].start);
                self.modified();
                self.notify(format!(
                    "{label} applied. Adjust the keyframes in Properties."
                ));
            }
        }
    }

    pub(super) fn appearance_controls(&mut self, ui: &mut egui::Ui, c: &mut Clip) {
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
