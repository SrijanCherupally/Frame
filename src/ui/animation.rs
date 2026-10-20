use super::*;
use crate::animation::{EASINGS, Easing, MotionPath, Reveal};

fn ease_picker(ui: &mut egui::Ui, easing: &mut Easing, id: &str) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(easing.name())
        .show_ui(ui, |ui| {
            for &(value, label) in EASINGS {
                ui.selectable_value(easing, value, label);
            }
        });
}
fn curve_editor(ui: &mut egui::Ui, key: &mut Keyframe) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 180.), Sense::hover());
    let plot = rect.shrink(12.);
    let pos = |x: f64, y: f64| {
        Pos2::new(
            plot.left() + x as f32 * plot.width(),
            plot.bottom() - y as f32 * plot.height(),
        )
    };
    ui.painter().rect_filled(rect, 8., palette().canvas);
    for i in 0..=4 {
        let f = i as f64 / 4.;
        ui.painter().line_segment(
            [pos(f, 0.), pos(f, 1.)],
            Stroke::new(1.0_f32, palette().border),
        );
        ui.painter().line_segment(
            [pos(0., f), pos(1., f)],
            Stroke::new(1.0_f32, palette().border),
        );
    }
    if let Easing::Bezier {
        mut x1,
        mut y1,
        mut x2,
        mut y2,
    } = key.easing
    {
        for (i, (x, y, anchor)) in [
            (&mut x1, &mut y1, pos(0., 0.)),
            (&mut x2, &mut y2, pos(1., 1.)),
        ]
        .into_iter()
        .enumerate()
        {
            let point = pos(*x, *y);
            let response = ui.interact(
                Rect::from_center_size(point, Vec2::splat(20.)),
                ui.id().with(("ease-handle", i)),
                Sense::drag(),
            );
            if response.dragged()
                && let Some(p) = response.interact_pointer_pos()
            {
                *x = ((p.x - plot.left()) / plot.width()).clamp(0., 1.) as f64;
                *y = ((plot.bottom() - p.y) / plot.height()).clamp(0., 1.) as f64;
                key.domain = [0., 1.];
            }
            ui.painter()
                .line_segment([anchor, pos(*x, *y)], Stroke::new(1.5_f32, palette().amber));
            ui.painter().circle_filled(pos(*x, *y), 6., palette().amber);
        }
        key.easing = Easing::Bezier { x1, y1, x2, y2 };
    }
    let points = (0..=100)
        .map(|i| {
            let x = i as f64 / 100.;
            pos(x, key.easing.sample(x))
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        Stroke::new(2.5_f32, palette().violet),
    ));
}
fn path_editor(ui: &mut egui::Ui, c: &mut Clip, selected: usize, local: f64) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 230.), Sense::hover());
    let plot = rect.shrink(18.);
    let painter = ui.painter_at(rect);
    let pos = |p: [f64; 2]| {
        Pos2::new(
            plot.left() + p[0] as f32 / 100. * plot.width(),
            plot.top() + p[1] as f32 / 100. * plot.height(),
        )
    };
    painter.rect_filled(rect, 8., palette().canvas);
    for i in 0..=4 {
        let f = i as f64 * 25.;
        painter.line_segment(
            [pos([f, 0.]), pos([f, 100.])],
            Stroke::new(1.0_f32, palette().border.gamma_multiply(0.5)),
        );
        painter.line_segment(
            [pos([0., f]), pos([100., f])],
            Stroke::new(1.0_f32, palette().border.gamma_multiply(0.5)),
        );
    }
    for (i, pair) in c.keyframes.windows(2).enumerate() {
        let a = [pair[0].transform.x, pair[0].transform.y];
        let b = [pair[1].transform.x, pair[1].transform.y];
        let points = (0..=64)
            .map(|j| {
                let t = j as f64 / 64.;
                pos(pair[0]
                    .path
                    .map_or([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t], |p| {
                        p.point(a, b, t)
                    }))
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(
                if i == selected { 3.0_f32 } else { 1.5_f32 },
                if i == selected {
                    palette().cyan
                } else {
                    palette().muted
                },
            ),
        ));
    }
    if selected + 1 < c.keyframes.len()
        && let Some(mut path) = c.keyframes[selected].path
    {
        let anchors = [
            pos([
                c.keyframes[selected].transform.x,
                c.keyframes[selected].transform.y,
            ]),
            pos([
                c.keyframes[selected + 1].transform.x,
                c.keyframes[selected + 1].transform.y,
            ]),
        ];
        for (i, point) in [&mut path.control1, &mut path.control2]
            .into_iter()
            .enumerate()
        {
            let response = ui.interact(
                Rect::from_center_size(pos(*point), Vec2::splat(20.)),
                ui.id().with(("path-handle", i)),
                Sense::drag(),
            );
            if response.dragged()
                && let Some(p) = response.interact_pointer_pos()
            {
                point[0] = ((p.x - plot.left()) / plot.width() * 100.).clamp(-100., 200.) as f64;
                point[1] = ((p.y - plot.top()) / plot.height() * 100.).clamp(-100., 200.) as f64;
            }
            painter.line_segment(
                [anchors[i], pos(*point)],
                Stroke::new(1.5_f32, palette().amber),
            );
            painter.circle_filled(pos(*point), 6., palette().amber);
        }
        c.keyframes[selected].path = Some(path);
    }
    for (i, key) in c.keyframes.iter_mut().enumerate() {
        let point = pos([key.transform.x, key.transform.y]);
        let response = ui.interact(
            Rect::from_center_size(point, Vec2::splat(18.)),
            ui.id().with(("path-anchor", i)),
            Sense::drag(),
        );
        if response.dragged()
            && let Some(p) = response.interact_pointer_pos()
        {
            key.transform.x = ((p.x - plot.left()) / plot.width() * 100.).clamp(-100., 200.) as f64;
            key.transform.y = ((p.y - plot.top()) / plot.height() * 100.).clamp(-100., 200.) as f64;
        }
        painter.circle_filled(
            point,
            if i == selected { 5. } else { 3.5 },
            palette().violet,
        );
    }
    let value = c.value(local);
    painter.circle_stroke(
        pos([value.x, value.y]),
        8.,
        Stroke::new(2.0_f32, palette().accent),
    );
}
impl FrameApp {
    pub(super) fn chart_controls(&mut self, ui: &mut egui::Ui, c: &mut Clip) {
        if c.kind != Kind::Graphic || c.graphic != Graphic::Chart {
            return;
        }
        ui.label(format!("{} data points", c.chart_data.len()));
        if ui.button("Import time / value CSV…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Chart CSV", &["csv", "tsv", "txt"])
                .pick_file()
        {
            let result = (|| -> anyhow::Result<Vec<[f64; 2]>> {
                anyhow::ensure!(
                    std::fs::metadata(&path)?.len() <= 1024 * 1024,
                    "Chart CSV must be at most 1 MiB"
                );
                crate::animation::parse_chart_csv(&std::fs::read_to_string(path)?)
            })();
            match result {
                Ok(data) => {
                    c.chart_data = data;
                    self.notify(
                        "Chart data embedded in project. Use a left-to-right reveal to animate it.",
                    );
                }
                Err(e) => self.notify(format!("Could not import chart: {e}")),
            }
        }
        if !c.chart_data.is_empty() && ui.small_button("Use illustrated example").clicked() {
            c.chart_data.clear();
        }
    }
    pub(super) fn animation_studio(&mut self, ctx: &egui::Context) {
        if !self.motion_window {
            return;
        }
        let mut open = true;
        egui::Window::new("Motion Studio").open(&mut open).default_size([1000.,680.]).min_width(760.).vscroll(true).show(ctx,|ui| {
            ui.label(RichText::new("CHOREOGRAPH YOUR EXPLANATION").small().color(palette().violet));
            let Some(index)=self.selected_index().filter(|i|self.project.clips[*i].is_visual()) else {ui.label("Select a visual layer on the timeline to edit its animation.");return;};
            let old=self.project.clips[index].clone();let mut c=old.clone();
            c.keyframes.sort_by(|a,b|a.time.total_cmp(&b.time));
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&c.name).strong());
                if ui.button("▶ Preview from start").clicked(){self.seek(c.start);self.playing=true;}
                if ui.button("Copy animation").clicked(){self.animation_clipboard=Some(c.clone());}
                if ui.add_enabled(self.animation_clipboard.is_some(),egui::Button::new("Paste & fit")).on_hover_text("Replace all transform animation, positions, paths and reveal settings; fit timing to this clip.").clicked(){edit::paste_animation(self.animation_clipboard.as_ref().unwrap(),&mut c);}
                if ui.add_enabled(!c.keyframes.is_empty(),egui::Button::new("Reverse motion")).clicked(){edit::reverse_animation(&mut c);}
                if ui.button("Clear motion").clicked(){c.transform=c.value((self.time-c.start).clamp(0.,c.duration));c.keyframes.clear();}
            });
            ui.separator();
            ui.columns(2,|columns| {
                let ui=&mut columns[0];
                card().show(ui,|ui| {
                    ui.set_min_width(ui.available_width());
                    section_title(ui,"Keyframes & channels",palette().violet);
                    ui.horizontal(|ui| {
                        if ui.button("+ At playhead").clicked(){let local=(self.time-c.start).clamp(0.,c.duration);edit::add_keyframe(&mut c,local,self.project.fps);self.motion_key=c.keyframes.iter().position(|k|(k.time-local).abs()<=0.5/self.project.fps as f64+1e-8).unwrap_or(0);}
                        if c.keyframes.is_empty() && ui.button("Create start / end").clicked(){edit::add_keyframe(&mut c,0.,self.project.fps);let end=c.duration;edit::add_keyframe(&mut c,end,self.project.fps);}
                    });
                    let mut remove=None;
                    egui::ScrollArea::vertical().id_salt("studio-key-list").max_height(100.).show(ui,|ui| {
                        ui.horizontal_wrapped(|ui| {for (i,key) in c.keyframes.iter().enumerate(){if ui.selectable_label(self.motion_key==i,format!("◇ {:.3} s",key.time)).clicked(){self.motion_key=i;self.seek(c.start+key.time);}}});
                    });
                    self.motion_key=self.motion_key.min(c.keyframes.len().saturating_sub(1));
                    if !c.keyframes.is_empty() {
                        let i=self.motion_key;let lo=if i==0 {0.}else{c.keyframes[i-1].time+1e-6};let hi=c.keyframes.get(i+1).map_or(c.duration.max(c.keyframes[i].time),|k|k.time-1e-6);
                        let key=&mut c.keyframes[i];
                        ui.horizontal(|ui| {ui.label("Time");let bounds=lo.min(key.time)..=hi.max(key.time);ui.add(egui::DragValue::new(&mut key.time).range(bounds).speed(1./self.project.fps as f64).suffix(" s"));if ui.small_button("Delete key").clicked(){remove=Some(i);}});
                        ui.horizontal(|ui| {ui.label("X");ui.add(egui::DragValue::new(&mut key.transform.x).range(-100. ..=200.).suffix("%"));ui.label("Y");ui.add(egui::DragValue::new(&mut key.transform.y).range(-100. ..=200.).suffix("%"));});
                        ui.add(egui::Slider::new(&mut key.transform.scale,0.05..=4.).text("Scale"));
                        ui.add(egui::Slider::new(&mut key.transform.rotation,-360. ..=360.).text("Rotation"));
                        ui.add(egui::Slider::new(&mut key.transform.opacity,0. ..=1.).text("Opacity"));
                    }
                    if let Some(i)=remove{c.keyframes.remove(i);}
                });
                ui.add_space(8.);
                card().show(ui,|ui| {
                    section_title(ui,"Reveal & explain",palette().amber);
                    self.chart_controls(ui,&mut c);
                    egui::ComboBox::from_id_salt("studio-reveal").selected_text(format!("{:?}",c.reveal)).show_ui(ui,|ui| {for (v,name) in [(Reveal::None,"No reveal"),(Reveal::LeftToRight,"Wipe left → right"),(Reveal::BottomToTop,"Grow bottom → top"),(Reveal::Iris,"Iris reveal")] {ui.selectable_value(&mut c.reveal,v,name);}});
                    if c.reveal!=Reveal::None {ui.add(egui::DragValue::new(&mut c.reveal_duration).range(0.05..=60.).speed(0.05).suffix(" s"));if ui.small_button("Restart reveal").clicked(){c.reveal_offset=0.;}}
                    ui.label(RichText::new("Smooth reveals work on footage, text and diagrams in both preview and export. A wipe reveals the existing drawing; it does not trace its strokes.").small().color(palette().muted));
                });
                let ui=&mut columns[1];
                card().show(ui,|ui| {
                    section_title(ui,"Timing curve",palette().violet);
                    if self.motion_key+1<c.keyframes.len() {
                        let key=&mut c.keyframes[self.motion_key];let before=key.easing;
                        ease_picker(ui,&mut key.easing,"studio-ease");if key.easing!=before{key.domain=[0.,1.];}
                        curve_editor(ui,key);
                        if key.domain!=[0.,1.] {ui.label(RichText::new("This cut preserves a section of the original easing curve.").small().color(palette().muted));}
                        ui.label(RichText::new("Curve applies from the selected key to the next. Drag gold handles for custom Bézier timing.").small().color(palette().muted));
                    }else{ui.label("Select a key with another key after it to edit the segment's timing.");}
                });
                ui.add_space(8.);
                card().show(ui,|ui| {
                    section_title(ui,"Spatial motion path",palette().cyan);
                    if self.motion_key+1<c.keyframes.len() {
                        let i=self.motion_key;let mut curved=c.keyframes[i].path.is_some();
                        if ui.checkbox(&mut curved,"Curved Bézier segment").changed(){
                            let a=&c.keyframes[i].transform;let b=&c.keyframes[i+1].transform;
                            c.keyframes[i].path=curved.then_some(MotionPath {control1:[a.x+(b.x-a.x)/3.,a.y+(b.y-a.y)/3.],control2:[a.x+(b.x-a.x)*2./3.,a.y+(b.y-a.y)*2./3.]});
                        }
                    }
                    let local=(self.time-c.start).clamp(0.,c.duration);
                    path_editor(ui,&mut c,self.motion_key,local);
                    ui.label(RichText::new("Drag anchors and gold handles. Ring = playhead position. Diagram coordinates match the canvas percentages.").small().color(palette().muted));
                });
            });
            if c!=old {
                let mut next=self.project.clone();next.clips[index]=c;
                match next.validate(){
                    Ok(())=>{let down=ctx.input(|i|i.pointer.any_down());if !self.property_drag||!down{self.history.remember(&self.project);}self.property_drag=down;self.project=next;self.modified();},
                    Err(e)=>self.notify(format!("Animation edit rejected: {e}")),
                }
            }
        });
        self.motion_window = open;
    }
}
