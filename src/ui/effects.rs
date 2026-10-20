use super::*;
use crate::{
    animation::Easing,
    scene3d::{self, CameraKey, PoseKey, Primitive},
    tracking,
    vector::{self, Node, VectorShape},
};
use anyhow::Context;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Panel {
    Vector,
    Scene,
    Tracking,
    Blur,
}
enum Message {
    Reference(tracking::GrayFrame, String),
    Tracked(Box<tracking::Track>),
    Progress(f64),
    Still(image::RgbaImage),
    Error(String),
}
struct Task {
    group: ProcessGroup,
    handle: JoinHandle<()>,
    rx: Receiver<Message>,
}
pub(super) struct VfxState {
    pub open: bool,
    pub panel: Panel,
    node: usize,
    target_shape: bool,
    object: usize,
    source: String,
    target: String,
    settings: tracking::Settings,
    reference: Option<TextureHandle>,
    signature: String,
    result: Option<tracking::Track>,
    task: Option<Task>,
    status: String,
    smoothing: usize,
    preserve: bool,
    still: Option<TextureHandle>,
    still_signature: String,
    aspect: f64,
    seek: Option<f64>,
}
impl Default for VfxState {
    fn default() -> Self {
        Self {
            open: false,
            panel: Panel::Vector,
            node: 0,
            target_shape: false,
            object: 0,
            source: String::new(),
            target: String::new(),
            settings: tracking::Settings::default(),
            reference: None,
            signature: String::new(),
            result: None,
            task: None,
            status: String::new(),
            smoothing: 1,
            preserve: true,
            still: None,
            still_signature: String::new(),
            aspect: 16. / 9.,
            seek: None,
        }
    }
}
impl Drop for VfxState {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.group.cancel();
            let _ = task.handle.join();
        }
    }
}
impl VfxState {
    fn launch(
        &mut self,
        work: impl FnOnce(ProcessGroup, Sender<Message>) -> anyhow::Result<()> + Send + 'static,
    ) {
        if self.task.is_some() {
            return;
        }
        let group = ProcessGroup::default();
        let worker = group.clone();
        let (tx, rx) = unbounded();
        let handle = thread::spawn(move || {
            if let Err(e) = work(worker, tx.clone()) {
                let _ = tx.send(Message::Error(e.to_string()));
            }
        });
        self.task = Some(Task { group, handle, rx });
    }
    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(task) = &self.task {
            while let Ok(message) = task.rx.try_recv() {
                match message {
                    Message::Reference(frame, signature) => {
                        self.reference = Some(ctx.load_texture(
                            "tracking-reference",
                            egui::ColorImage::from_gray(
                                [frame.width as usize, frame.height as usize],
                                &frame.pixels,
                            ),
                            egui::TextureOptions::LINEAR,
                        ));
                        self.signature = signature;
                        self.status = "Choose a textured point, then analyze its movement.".into();
                    }
                    Message::Tracked(result) => {
                        self.status = format!(
                            "{} samples · minimum confidence {:.0}%{}",
                            result.points.len(),
                            result
                                .points
                                .iter()
                                .map(|p| p.confidence)
                                .fold(1., f64::min)
                                * 100.,
                            if result.lost {
                                " · feature lost; partial track retained"
                            } else {
                                " · complete"
                            }
                        );
                        self.result = Some(*result);
                    }
                    Message::Progress(p) => self.status = format!("Tracking {:.0}%…", p * 100.),
                    Message::Still(image) => {
                        self.still = Some(ctx.load_texture(
                            "3d-still",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [image.width() as usize, image.height() as usize],
                                image.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        ))
                    }
                    Message::Error(error) => {
                        self.status = error;
                        self.still_signature.clear();
                    }
                }
            }
            if task.handle.is_finished() {
                let task = self.task.take().unwrap();
                let _ = task.handle.join();
            } else {
                ctx.request_repaint_after(Duration::from_millis(40));
            }
        }
    }
}
fn read_small(path: &std::path::Path) -> anyhow::Result<String> {
    anyhow::ensure!(
        std::fs::metadata(path)?.len() <= 1024 * 1024,
        "Geometry files must be at most 1 MiB"
    );
    Ok(std::fs::read_to_string(path)?)
}
fn vector_editor(
    ui: &mut egui::Ui,
    c: &mut Clip,
    state: &mut VfxState,
    time: f64,
) -> anyhow::Result<()> {
    let Some(shape) = c.vector.as_mut() else {
        ui.label("Create or select a vector layer to edit its geometry.");
        return Ok(());
    };
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut state.target_shape, false, "Base path");
        if ui
            .add_enabled(
                !shape.target.is_empty(),
                egui::Button::new("Morph target").selected(state.target_shape),
            )
            .clicked()
        {
            state.target_shape = true;
        }
        if ui.button("Create target from base").clicked() {
            shape.target = shape.nodes.clone();
            state.target_shape = true;
        }
        if ui.button("Clear target").clicked() {
            shape.target.clear();
            state.target_shape = false;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Stroke");
        let mut rgb = hex_color(&c.color);
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            c.color = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
        }
        ui.label("Fill");
        let mut rgb = hex_color(&c.secondary_color);
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            c.secondary_color = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut shape.closed, "Closed path");
        ui.checkbox(&mut c.fill, "Fill");
        ui.add(
            egui::DragValue::new(&mut c.stroke_width)
                .range(1. ..=40.)
                .suffix(" px stroke"),
        );
        ui.add(
            egui::DragValue::new(&mut shape.draw_duration)
                .range(0. ..=60.)
                .speed(0.05)
                .suffix(" s draw-on"),
        );
        if !shape.target.is_empty() {
            ui.add(
                egui::DragValue::new(&mut shape.morph_duration)
                    .range(0. ..=60.)
                    .speed(0.05)
                    .suffix(" s morph"),
            );
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::Slider::new(&mut shape.stroke_start, 0. ..=shape.stroke_end).text("Stroke start"),
        );
        ui.add(egui::Slider::new(&mut shape.stroke_end, shape.stroke_start..=1.).text("End"));
    });
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 300.), Sense::click());
    let plot = rect.shrink(20.);
    let painter = ui.painter_at(rect);
    let pos = |p: [f64; 2]| {
        Pos2::new(
            plot.left() + plot.width() * p[0] as f32 / 100.,
            plot.top() + plot.height() * p[1] as f32 / 100.,
        )
    };
    painter.rect_filled(rect, 8., palette().canvas);
    for i in 0..=4 {
        let f = i as f64 * 25.;
        painter.line_segment(
            [pos([f, 0.]), pos([f, 100.])],
            Stroke::new(1_f32, palette().border),
        );
        painter.line_segment(
            [pos([0., f]), pos([100., f])],
            Stroke::new(1_f32, palette().border),
        );
    }
    let content_time = c.content_offset + time * c.content_rate;
    let points: Vec<_> = shape.polyline(content_time).into_iter().map(pos).collect();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(3_f32, palette().cyan),
    ));
    let nodes = if state.target_shape && !shape.target.is_empty() {
        &mut shape.target
    } else {
        &mut shape.nodes
    };
    state.node = state.node.min(nodes.len().saturating_sub(1));
    let mut hit = false;
    for (i, node) in nodes.iter_mut().enumerate() {
        let anchor = pos(node.point);
        let r = ui.interact(
            Rect::from_center_size(anchor, Vec2::splat(18.)).intersect(rect),
            ui.id().with(("vector-node", i)),
            Sense::click_and_drag(),
        );
        if r.clicked() || r.dragged() {
            state.node = i;
            hit = true;
        }
        if r.dragged()
            && let Some(p) = r.interact_pointer_pos()
        {
            let next = [
                ((p.x - plot.left()) / plot.width() * 100.).clamp(-100., 200.) as f64,
                ((p.y - plot.top()) / plot.height() * 100.).clamp(-100., 200.) as f64,
            ];
            let delta = [next[0] - node.point[0], next[1] - node.point[1]];
            for point in [&mut node.incoming, &mut node.outgoing] {
                point[0] = (point[0] + delta[0]).clamp(-100., 200.);
                point[1] = (point[1] + delta[1]).clamp(-100., 200.);
            }
            node.point = next;
        }
        painter.circle_filled(
            pos(node.point),
            5.,
            if state.node == i {
                palette().accent
            } else {
                palette().muted
            },
        );
        if state.node == i {
            for (j, point) in [&mut node.incoming, &mut node.outgoing]
                .into_iter()
                .enumerate()
            {
                if *point == node.point {
                    continue;
                }
                let p = pos(*point);
                painter.line_segment([pos(node.point), p], Stroke::new(1_f32, palette().amber));
                painter.circle_filled(p, 4., palette().amber);
                let r = ui.interact(
                    Rect::from_center_size(p, Vec2::splat(14.)).intersect(rect),
                    ui.id().with(("vector-handle", i, j)),
                    Sense::drag(),
                );
                if r.dragged()
                    && let Some(p) = r.interact_pointer_pos()
                {
                    point[0] =
                        ((p.x - plot.left()) / plot.width() * 100.).clamp(-100., 200.) as f64;
                    point[1] =
                        ((p.y - plot.top()) / plot.height() * 100.).clamp(-100., 200.) as f64;
                    hit = true;
                }
            }
        }
    }
    if response.clicked()
        && !hit
        && !state.target_shape
        && let Some(p) = response.interact_pointer_pos()
        && nodes.len() < 128
    {
        let p = [
            ((p.x - plot.left()) / plot.width() * 100.).clamp(0., 100.) as f64,
            ((p.y - plot.top()) / plot.height() * 100.).clamp(0., 100.) as f64,
        ];
        nodes.push(Node::corner(p));
        state.node = nodes.len() - 1;
        shape.target.clear();
    }
    let nodes = if state.target_shape && !shape.target.is_empty() {
        &mut shape.target
    } else {
        &mut shape.nodes
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Node {} / {}", state.node + 1, nodes.len()));
    });
    let nodes = if state.target_shape && !shape.target.is_empty() {
        &mut shape.target
    } else {
        &mut shape.nodes
    };
    let n = state.node.min(nodes.len() - 1);
    let node = &mut nodes[n];
    ui.horizontal_wrapped(|ui| {
        ui.label("X");
        ui.add(egui::DragValue::new(&mut node.point[0]).range(-100. ..=200.));
        ui.label("Y");
        ui.add(egui::DragValue::new(&mut node.point[1]).range(-100. ..=200.));
        if ui.button("Smooth handles").clicked() {
            node.incoming = [(node.point[0] - 10.).max(-100.), node.point[1]];
            node.outgoing = [(node.point[0] + 10.).min(200.), node.point[1]];
        }
        if ui.button("Corner").clicked() {
            node.incoming = node.point;
            node.outgoing = node.point;
        }
    });
    let remove = nodes.len() > 2 && ui.button("Delete selected node (clears morph)").clicked();
    if remove {
        shape.nodes.remove(n);
        shape.target.clear();
        state.target_shape = false;
        state.node = 0;
    }
    ui.label(RichText::new("Click empty canvas to add a corner. Drag anchors and gold handles. Matching base/target nodes morph smoothly; draw-on follows path length. A zero draw-on duration shows the complete stroke.").small().color(palette().muted));
    if ui.button("Import SVG path…").clicked()
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("SVG", &["svg"])
            .pick_file()
    {
        *shape = vector::parse_svg(&read_small(&path)?)?;
        state.node = 0;
        state.target_shape = false;
    }
    if ui.button("Export SVG geometry…").clicked()
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("SVG", &["svg"])
            .set_file_name("Frame-path.svg")
            .save_file()
    {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        use std::io::Write;
        file.write_all(shape.svg().as_bytes())?;
    }
    Ok(())
}
fn scene_editor(
    ui: &mut egui::Ui,
    c: &mut Clip,
    state: &mut VfxState,
    time: f64,
) -> anyhow::Result<()> {
    let Some(scene) = c.scene3d.as_mut() else {
        ui.label("Create or select a 3D scene layer.");
        return Ok(());
    };
    let clock = (c.content_offset + time * c.content_rate).clamp(0., 86400.);
    ui.columns(2,|columns|->anyhow::Result<()> {
    let ui=&mut columns[0];
    ui.horizontal_wrapped(|ui| {
        ui.label("Camera");
        ui.add(
            egui::DragValue::new(&mut scene.camera.yaw)
                .range(-3600. ..=3600.)
                .suffix("° yaw"),
        );
        ui.add(
            egui::DragValue::new(&mut scene.camera.pitch)
                .range(-89. ..=89.)
                .suffix("° pitch"),
        );
        ui.add(
            egui::DragValue::new(&mut scene.camera.distance)
                .range(2. ..=30.)
                .speed(0.05)
                .suffix(" distance"),
        );
        ui.add(
            egui::DragValue::new(&mut scene.camera.fov)
                .range(15. ..=100.)
                .suffix("° lens"),
        );
    });
    ui.horizontal_wrapped(|ui| {
        ui.add(egui::Slider::new(&mut scene.orbit_speed, -90. ..=90.).text("Camera orbit °/s"));
        ui.add(egui::Slider::new(&mut scene.ambient, 0. ..=1.).text("Ambient light"));
        ui.checkbox(&mut scene.wireframe, "Wireframe");
            ui.checkbox(&mut scene.antialias,"Edge antialiasing");
    });
    ui.horizontal_wrapped(|ui| {
        if ui.button("Record camera pose").clicked() {
            scene.camera_keys.retain(|k| (k.time - clock).abs() > 1e-8);
            if scene.camera_keys.len() < 100 {
                scene.camera_keys.push(CameraKey {
                    time: clock,
                    camera: scene.camera.clone(),
                    easing: Easing::Smoother,
                });
                scene.camera_keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            }
        }
        if ui.button("Clear camera keys").clicked() {
            scene.camera_keys.clear();
        }
        ui.label(format!("{} camera keys", scene.camera_keys.len()));
    });
    egui::CollapsingHeader::new("Camera keyframes").show(ui,|ui|{
        let mut load=None;let mut remove=None;
        for (i,key) in scene.camera_keys.iter().enumerate(){ui.horizontal(|ui|{ui.label(format!("{:.3} s",key.time));if ui.small_button("Load").clicked(){load=Some(i);}
if ui.small_button("Delete").clicked(){remove=Some(i);}});}
        if let Some(i)=load {scene.camera=scene.camera_keys[i].camera.clone();state.seek=Some(scene.camera_keys[i].time);}
        if let Some(i)=remove {scene.camera_keys.remove(i);}
    });
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        for (primitive, name) in [
            (Primitive::Cube, "+ Cube"),
            (Primitive::Sphere, "+ Sphere"),
            (Primitive::Cylinder, "+ Cylinder"),
            (Primitive::Rocket, "+ Rocket"),
        ] {
            if ui
                .add_enabled(scene.objects.len() < 16, egui::Button::new(name))
                .clicked()
            {
                scene.objects.push(scene3d::Object {
                    name: format!("{primitive:?}"),
                    primitive,
                    pose: scene3d::Pose {
                        position: [(scene.objects.len() as f64 * 0.6).min(3.), 0., 0.],
                        ..scene3d::Pose::default()
                    },
                    ..scene3d::Object::default()
                });
                state.object = scene.objects.len() - 1;
            }
        }
    });
    state.object = state.object.min(scene.objects.len() - 1);
    egui::ComboBox::from_id_salt("scene-object")
        .selected_text(&scene.objects[state.object].name)
        .show_ui(ui, |ui| {
            for (i, o) in scene.objects.iter().enumerate() {
                ui.selectable_value(&mut state.object, i, &o.name);
            }
        });
    let object = &mut scene.objects[state.object];
    ui.text_edit_singleline(&mut object.name);
    ui.horizontal_wrapped(|ui| {
        let mut rgb = hex_color(&object.color);
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            object.color = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
        }
        for i in 0..3 {
            ui.label(["X", "Y", "Z"][i]);
            ui.add(
                egui::DragValue::new(&mut object.pose.position[i])
                    .range(-10. ..=10.)
                    .speed(0.05),
            );
        }
        ui.add(
            egui::DragValue::new(&mut object.pose.scale)
                .range(0.01..=5.)
                .speed(0.02)
                .suffix("× size"),
        );
    });
    ui.horizontal_wrapped(|ui| {
        for i in 0..3 {
            ui.label(["Rotate X", "Y", "Z"][i]);
            ui.add(
                egui::DragValue::new(&mut object.pose.rotation[i])
                    .range(-3600. ..=3600.)
                    .suffix("°"),
            );
        }
    });
    ui.horizontal_wrapped(|ui| {
        for i in 0..3 {
            ui.label(["Spin X", "Y", "Z"][i]);
            ui.add(
                egui::DragValue::new(&mut object.spin[i])
                    .range(-360. ..=360.)
                    .suffix("°/s"),
            );
        }
    });
    ui.horizontal_wrapped(|ui| {
        if ui.button("Record object pose").clicked() {
            object.keys.retain(|k| (k.time - clock).abs() > 1e-8);
            if object.keys.len() < 100 {
                object.keys.push(PoseKey {
                    time: clock,
                    pose: object.pose.clone(),
                    easing: Easing::Smoother,
                });
                object.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            }
        }
        if ui.button("Clear object keys").clicked() {
            object.keys.clear();
        }
        ui.label(format!("{} object keys", object.keys.len()));
    });
    egui::CollapsingHeader::new("Object keyframes").show(ui,|ui|{
        let mut load=None;let mut remove=None;
        for (i,key) in object.keys.iter().enumerate(){ui.horizontal(|ui|{ui.label(format!("{:.3} s",key.time));if ui.small_button("Load").clicked(){load=Some(i);}
if ui.small_button("Delete").clicked(){remove=Some(i);}});}
        if let Some(i)=load {object.pose=object.keys[i].pose.clone();state.seek=Some(object.keys[i].time);}
        if let Some(i)=remove {object.keys.remove(i);}
    });
    ui.label(RichText::new("With keys, adjust the pose fields and click Record at each content time. Spin adds continuous rotation. Camera keys animate orbit, pitch, distance and lens.").small().color(palette().muted));
    if ui.button("Import OBJ into selected object…").clicked()
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("OBJ mesh", &["obj"])
            .pick_file()
    {
        object.mesh = scene3d::parse_obj(&read_small(&path)?)?;
        object.primitive = Primitive::Mesh;
        object.name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .take(100)
            .collect();
    }
    if scene.objects.len() > 1 && ui.button("Delete selected object").clicked() {
        scene.objects.remove(state.object);
        state.object = 0;
    }
    let ui=&mut columns[1];
    section_title(ui,"Shaded preview",palette().cyan);
    if let Some(texture) = &state.still {
        let width = ui.available_width().min(640.);
        ui.add(egui::Image::new(texture).fit_to_exact_size(Vec2::new(width, width * texture.size()[1] as f32/texture.size()[0] as f32)));
    }
    ui.label(RichText::new("Perspective, depth-tested geometry and directional shading. This preview updates in the background as you edit.").small().color(palette().muted));
    Ok(())
    })?;
    let signature = serde_json::to_string(&(scene, clock, state.aspect))?;
    if state.task.is_none() && state.still_signature != signature {
        let clip = c.clone();
        state.still_signature = signature;
        let height = (640. / state.aspect).round().clamp(240., 1280.) as u32;
        state.launch(move |group, tx| {
            tx.send(Message::Still(crate::procedural::image(
                &clip, time, 640, height, &group,
            )?))?;
            Ok(())
        });
    }
    Ok(())
}
impl FrameApp {
    pub(super) fn prepare_tracking_demo(&mut self) -> anyhow::Result<()> {
        let doc = Document::read(std::path::Path::new("examples/vfx/tracking-practice.frame"))?;
        self.restore_document(doc);
        let source = self
            .project
            .clips
            .iter()
            .find(|c| c.kind == Kind::Video)
            .context("Tracking example has no source")?
            .clone();
        let asset = self
            .assets
            .get(source.asset_id.as_ref().unwrap())
            .context("Tracking example media is unavailable")?
            .clone();
        self.vfx.source = source.id.clone();
        self.vfx.target = self
            .project
            .clips
            .iter()
            .find(|c| c.kind == Kind::Title)
            .unwrap()
            .id
            .clone();
        self.vfx.settings = tracking::Settings {
            duration: 2.5,
            seed: [20.625, 45.],
            ..tracking::Settings::default()
        };
        self.open_effects(Panel::Tracking);
        let settings = self.vfx.settings.clone();
        let signature = format!(
            "{}:{}:{}:{}:{}",
            source.id,
            source.source_in,
            source.speed,
            settings.start,
            asset.path.display()
        );
        let log = self.storage.root.join("tracking-demo.log");
        self.vfx.launch(move |group, tx| {
            tx.send(Message::Reference(
                tracking::reference_frame(&asset, &source, 0., &group)?,
                signature,
            ))?;
            tx.send(Message::Tracked(Box::new(tracking::analyze(
                &asset,
                &source,
                &settings,
                &log,
                &group,
                |_| {},
            )?)))?;
            Ok(())
        });
        Ok(())
    }
    pub(super) fn open_effects(&mut self, panel: Panel) {
        self.vfx.open = true;
        self.vfx.panel = panel;
    }
    pub(super) fn new_effect_layer(&mut self, panel: Panel) {
        if self.project.clips.len() >= 2000 || self.time + 5. > 86400. {
            self.notify("Layer or timeline limit reached");
            return;
        }
        let c = Clip {
            kind: Kind::Graphic,
            name: if panel == Panel::Scene {
                "3D scene"
            } else {
                "Vector path"
            }
            .into(),
            track: 0,
            start: self.time,
            color: "#56dae0".into(),
            vector: (panel == Panel::Vector).then(VectorShape::default),
            scene3d: (panel == Panel::Scene).then(scene3d::Scene::default),
            ..Clip::default()
        };
        self.history.remember(&self.project);
        self.selected = Some(c.id.clone());
        self.project.clips.push(c);
        self.vfx.object = 0;
        self.vfx.node = 0;
        self.vfx.target_shape = false;
        self.modified();
    }
    fn tracking_panel(&mut self, ui: &mut egui::Ui) -> anyhow::Result<()> {
        let videos: Vec<_> = self
            .project
            .clips
            .iter()
            .filter(|c| c.kind == Kind::Video)
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect();
        if videos.is_empty() {
            ui.label("Import a video and place it on the timeline to track a feature.");
            return Ok(());
        }
        if !videos.iter().any(|(id, _)| *id == self.vfx.source) {
            self.vfx.source = videos[0].0.clone();
        }
        let before = self.vfx.source.clone();
        egui::ComboBox::from_id_salt("tracking-source")
            .selected_text(
                videos
                    .iter()
                    .find(|(id, _)| *id == self.vfx.source)
                    .unwrap()
                    .1
                    .clone(),
            )
            .show_ui(ui, |ui| {
                for (id, name) in &videos {
                    ui.selectable_value(&mut self.vfx.source, id.clone(), name);
                }
            });
        if self.vfx.source != before {
            if let Some(task) = &self.vfx.task {
                task.group.cancel();
            }
            self.vfx.reference = None;
            self.vfx.result = None;
        }
        let source = self
            .project
            .clips
            .iter()
            .find(|c| c.id == self.vfx.source)
            .unwrap()
            .clone();
        let asset = source
            .asset_id
            .as_ref()
            .and_then(|id| self.assets.get(id))
            .context("Source media is unavailable")?
            .clone();
        self.vfx.settings.start = self
            .vfx
            .settings
            .start
            .min((source.duration - 1. / self.project.fps as f64).max(0.));
        self.vfx.settings.duration = self
            .vfx
            .settings
            .duration
            .min(source.duration - self.vfx.settings.start);
        ui.horizontal_wrapped(|ui| {
            ui.label("Start in clip");
            ui.add(
                egui::DragValue::new(&mut self.vfx.settings.start)
                    .range(0. ..=(source.duration - 1. / self.project.fps as f64).max(0.))
                    .speed(0.1)
                    .suffix(" s"),
            );
            ui.add(
                egui::DragValue::new(&mut self.vfx.settings.duration)
                    .range(
                        (1. / self.project.fps as f64)
                            ..=(source.duration - self.vfx.settings.start).min(20.),
                    )
                    .speed(0.1)
                    .suffix(" s range"),
            );
        });
        let signature = format!(
            "{}:{}:{}:{}:{}",
            source.id,
            source.source_in,
            source.speed,
            self.vfx.settings.start,
            asset.path.display()
        );
        let fresh = self.vfx.signature == signature && self.vfx.reference.is_some();
        if ui
            .add_enabled(
                self.vfx.task.is_none(),
                egui::Button::new("Load reference frame"),
            )
            .clicked()
        {
            let start = self.vfx.settings.start;
            let clip = source.clone();
            let a = asset.clone();
            let signature = signature.clone();
            self.vfx.launch(move |group, tx| {
                tx.send(Message::Reference(
                    tracking::reference_frame(&a, &clip, start, &group)?,
                    signature,
                ))?;
                Ok(())
            });
        }
        if let Some(texture) = &self.vfx.reference {
            let width = ui.available_width().min(700.);
            let aspect = texture.size()[1] as f32 / texture.size()[0] as f32;
            let response = ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(Vec2::new(width, width * aspect))
                    .sense(Sense::click()),
            );
            if response.clicked()
                && let Some(p) = response.interact_pointer_pos()
            {
                self.vfx.settings.seed = [
                    ((p.x - response.rect.left()) / response.rect.width() * 100.).clamp(0., 100.)
                        as f64,
                    ((p.y - response.rect.top()) / response.rect.height() * 100.).clamp(0., 100.)
                        as f64,
                ];
                self.vfx.result = None;
            }
            let p = Pos2::new(
                response.rect.left()
                    + response.rect.width() * self.vfx.settings.seed[0] as f32 / 100.,
                response.rect.top()
                    + response.rect.height() * self.vfx.settings.seed[1] as f32 / 100.,
            );
            let size = Vec2::new(
                response.rect.width() / texture.size()[0] as f32,
                response.rect.height() / texture.size()[1] as f32,
            ) * (self.vfx.settings.patch_radius * 2 + 1) as f32;
            ui.painter().rect_stroke(
                Rect::from_center_size(p, size),
                0.,
                Stroke::new(2_f32, palette().amber),
                StrokeKind::Inside,
            );
            if let Some(result) = &self.vfx.result {
                let points = result
                    .points
                    .iter()
                    .map(|p| {
                        Pos2::new(
                            response.rect.left()
                                + response.rect.width() * p.position[0] as f32 / 100.,
                            response.rect.top()
                                + response.rect.height() * p.position[1] as f32 / 100.,
                        )
                    })
                    .collect();
                ui.painter().add(egui::Shape::line(
                    points,
                    Stroke::new(2_f32, palette().cyan),
                ));
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::Slider::new(&mut self.vfx.settings.patch_radius, 3..=20).text("Patch radius"),
            );
            ui.add(egui::Slider::new(&mut self.vfx.settings.search_radius, 2..=40).text("Search"));
            ui.add(
                egui::Slider::new(&mut self.vfx.settings.min_confidence, 0.1..=0.99)
                    .text("Min confidence"),
            );
        });
        if ui
            .add_enabled(
                fresh && self.vfx.task.is_none(),
                egui::Button::new("Analyze movement"),
            )
            .clicked()
        {
            let settings = self.vfx.settings.clone();
            let log = self.storage.root.join("tracking.log");
            self.vfx.result = None;
            self.vfx.launch(move |group, tx| {
                let result = tracking::analyze(&asset, &source, &settings, &log, &group, |p| {
                    let _ = tx.send(Message::Progress(p));
                })?;
                tx.send(Message::Tracked(Box::new(result)))?;
                Ok(())
            });
        }
        if !fresh {
            ui.label(
                RichText::new("Load a reference after changing the source or start time.")
                    .small()
                    .color(palette().amber),
            );
        }
        let targets: Vec<_> = self
            .project
            .clips
            .iter()
            .filter(|c| c.is_visual() && c.id != self.vfx.source)
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect();
        if !targets.iter().any(|(id, _)| *id == self.vfx.target) {
            self.vfx.target = targets.first().map_or(String::new(), |t| t.0.clone());
        }
        if !targets.is_empty() {
            egui::ComboBox::from_id_salt("tracking-target")
                .selected_text(
                    targets
                        .iter()
                        .find(|(id, _)| *id == self.vfx.target)
                        .unwrap()
                        .1
                        .clone(),
                )
                .show_ui(ui, |ui| {
                    for (id, name) in &targets {
                        ui.selectable_value(&mut self.vfx.target, id.clone(), name);
                    }
                });
        }
        ui.checkbox(&mut self.vfx.preserve, "Preserve label's starting offset");
        ui.add(egui::Slider::new(&mut self.vfx.smoothing, 0..=4).text("Path smoothing"));
        if ui
            .add_enabled(
                self.vfx.result.is_some() && !targets.is_empty(),
                egui::Button::new("Apply track to target layer"),
            )
            .clicked()
        {
            let track = self.vfx.result.as_ref().unwrap();
            anyhow::ensure!(
                self.project.clips.contains(&track.source),
                "Source changed since analysis; track it again"
            );
            let i = self
                .project
                .clips
                .iter()
                .position(|c| c.id == self.vfx.target)
                .unwrap();
            let mut c = self.project.clips[i].clone();
            tracking::apply(
                track,
                &mut c,
                &self.project,
                self.vfx.smoothing,
                self.vfx.preserve,
            )?;
            let mut next = self.project.clone();
            next.clips[i] = c;
            next.validate()?;
            self.history.remember(&self.project);
            self.project = next;
            self.modified();
        }
        ui.label(RichText::new("Translation point tracking: choose a textured feature. Analysis stops when confidence drops. Apply creates editable X/Y keys; it does not identify objects, solve camera motion or follow through an occlusion.").small().color(palette().muted));
        Ok(())
    }
    pub(super) fn effects_studio(&mut self, ctx: &egui::Context) {
        self.vfx.poll(ctx);
        self.vfx.aspect = self.project.width as f64 / self.project.height as f64;
        if !self.vfx.open {
            return;
        }
        let mut open = true;
        egui::Window::new("VFX Studio").open(&mut open).default_size([1020.,720.]).min_width(700.).vscroll(true).show(ctx,|ui|{
            ui.horizontal_wrapped(|ui|{for (panel,name) in [(Panel::Vector,"Vectors & morphing"),(Panel::Scene,"3D scenes"),(Panel::Tracking,"Tracking"),(Panel::Blur,"Motion blur")]{ui.selectable_value(&mut self.vfx.panel,panel,name);}});ui.separator();
            if let Some(task)=&self.vfx.task&&ui.button("Cancel current operation").clicked(){task.group.cancel();}
            if !self.vfx.status.is_empty(){ui.label(RichText::new(&self.vfx.status).small().color(palette().muted));}
            match self.vfx.panel {
                Panel::Tracking=>{if let Err(e)=self.tracking_panel(ui){self.vfx.status=e.to_string();}},
                Panel::Blur=>{
                    ui.label(RichText::new("SUB-FRAME COMPOSITION MOTION BLUR").color(palette().violet).strong());
                    let mut samples=self.project.motion_blur_samples;
                    ui.horizontal(|ui|{for (n,label) in [(1,"Off"),(2,"2 samples"),(4,"4 samples"),(8,"8 samples")]{ui.selectable_value(&mut samples,n,label);}});
                    if samples!=self.project.motion_blur_samples{self.history.remember(&self.project);self.project.motion_blur_samples=samples;self.modified();}
                    let mut angle=self.project.motion_shutter*360.;
                    if ui.add(egui::Slider::new(&mut angle,36. ..=360.).text("Animation shutter").suffix("°")).changed(){let down=ctx.input(|i|i.pointer.any_down());if !self.property_drag||!down {self.history.remember(&self.project);}self.property_drag=down;self.project.motion_shutter=angle/360.;self.modified();}
                    ui.label("Animation and 3D motion are evaluated multiple times within each output frame, then composited samples are averaged. A 180° shutter gives a shorter, crisper exposure; 360° creates a longer trail. Stationary objects stay sharp.");
                    ui.label(RichText::new("Preview uses the same samples at reduced resolution. More samples increase rendering cost. This is separate from Clean Timelapse's source-frame blending; it does not add optical-flow detail to camera footage.").color(palette().muted));
                },
                panel=>{
                    if ui.button(if panel==Panel::Scene{"+ New 3D scene layer"}else{"+ New vector layer"}).clicked(){self.new_effect_layer(panel);}
                    let Some(index)=self.selected_index() else{ui.label("Select a layer on the timeline.");return;};let old=self.project.clips[index].clone();let mut c=old.clone();
                    ui.label(RichText::new(&c.name).strong());let mut time=(self.time-c.start).clamp(0.,c.duration);if ui.add(egui::Slider::new(&mut time,0. ..=c.duration).text("Clip time").suffix(" s")).changed(){self.seek(c.start+time);}
                    let result=if panel==Panel::Vector {vector_editor(ui,&mut c,&mut self.vfx,time)}else{scene_editor(ui,&mut c,&mut self.vfx,time)};
                    if let Err(e)=result {self.vfx.status=e.to_string();}
                    if let Some(time)=self.vfx.seek.take() && c.content_rate.abs()>1e-12 {self.seek(c.start+((time-c.content_offset)/c.content_rate).clamp(0.,c.duration));}
                    if c!=old {let mut next=self.project.clone();next.clips[index]=c;match next.validate(){Ok(())=>{let down=ctx.input(|i|i.pointer.any_down());if !self.property_drag||!down{self.history.remember(&self.project);}self.property_drag=down;self.project=next;self.modified();},Err(e)=>self.vfx.status=e.to_string()}}
                }
            }
        });
        self.vfx.open = open;
    }
}
