use crate::{
    animation::{Easing, Exposure, Reveal},
    media::Asset,
    model::{Clip, Graphic, Keyframe, Kind, Mask, Project, Transform, Transition},
    process::{ProcessGroup, command},
    storage::atomic_json,
};
use anyhow::{Context, Result, bail, ensure};
use image::{Rgba, RgbaImage};
use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Stdio,
};
#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Export,
    PreviewVideo,
    PreviewAudio,
}
#[derive(Default)]
struct ProceduralSources(Vec<PathBuf>);
impl Drop for ProceduralSources {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}
pub struct Graph {
    pub args: Vec<OsString>,
    pub duration: f64,
    _sources: ProceduralSources,
}
impl Graph {
    /// Keep the graph alive until the command exits: it owns temporary sources.
    pub fn command(&self) -> std::process::Command {
        let mut cmd = command("ffmpeg");
        cmd.args(&self.args);
        cmd
    }
}
fn push(args: &mut Vec<OsString>, parts: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>) {
    args.extend(parts.into_iter().map(|s| s.as_ref().to_owned()));
}
fn prop(t: &Transform, p: &str) -> f64 {
    match p {
        "x" => t.x,
        "y" => t.y,
        "scale" => t.scale,
        "rotation" => t.rotation,
        _ => t.opacity,
    }
}
fn segment_expression(a: &Keyframe, b: &Keyframe, p: &str, clock: &str) -> String {
    let (v0, v1) = (prop(&a.transform, p), prop(&b.transform, p));
    let path = a.path.filter(|_| matches!(p, "x" | "y"));
    if v0 == v1 && path.is_none() {
        return v0.to_string();
    }
    let u = format!("({clock}-{})/{}", a.time, b.time - a.time);
    let [lo, hi] = a.domain;
    let (e0, e1) = (a.easing.sample(lo), a.easing.sample(hi));
    let fraction = if (e1 - e0).abs() < 1e-14 {
        a.easing.expression(&u)
    } else {
        format!(
            "(({})-({e0}))/{}",
            a.easing
                .expression(&format!("{lo}+{}*clip({u},0,1)", hi - lo)),
            e1 - e0
        )
    };
    if let Some(path) = path {
        let axis = usize::from(p == "y");
        format!(
            "st(9,({fraction}));pow(1-ld(9),3)*{v0}+3*pow(1-ld(9),2)*ld(9)*{}+3*(1-ld(9))*pow(ld(9),2)*{}+pow(ld(9),3)*{v1}",
            path.control1[axis], path.control2[axis]
        )
    } else {
        format!("{v0}+({})*({fraction})", v1 - v0)
    }
}
fn animated(c: &Clip, p: &str, clock: &str) -> String {
    let mut keys: Vec<_> = c.keyframes.iter().collect();
    keys.sort_by(|a, b| a.time.total_cmp(&b.time));
    if keys.is_empty() {
        return prop(&c.transform, p).to_string();
    }
    if keys.len() == 1 {
        return prop(&keys[0].transform, p).to_string();
    }
    let constant = prop(&keys[0].transform, p);
    if keys
        .iter()
        .all(|k| prop(&k.transform, p) == constant && (!matches!(p, "x" | "y") || k.path.is_none()))
    {
        return constant.to_string();
    }
    // A balanced decision tree avoids FFmpeg's expression recursion limit on dense tracks.
    fn tree(keys: &[&Keyframe], p: &str, clock: &str) -> String {
        if keys.len() == 2 {
            return segment_expression(keys[0], keys[1], p, clock);
        }
        let middle = keys.len() / 2;
        format!(
            "if(lt({clock},{}),{}, {})",
            keys[middle].time,
            tree(&keys[..=middle], p, clock),
            tree(&keys[middle..], p, clock)
        )
    }
    format!(
        "if(lte({clock},{}),{constant},if(gte({clock},{}),{},{}))",
        keys[0].time,
        keys.last().unwrap().time,
        prop(&keys.last().unwrap().transform, p),
        tree(&keys, p, clock)
    )
}
// Alpha is uniform across a layer, so evaluate it once per output frame.
// colorchannelmixer has a runtime alpha command; geq would evaluate the whole
// keyframe expression separately for every pixel of every frame.
fn opacity_filter(
    c: &Clip,
    phase: f64,
    duration: f64,
    fps: u32,
    index: usize,
    clock: (Exposure, f64),
) -> String {
    let mut keys: Vec<_> = c.keyframes.iter().collect();
    keys.sort_by(|a, b| a.time.total_cmp(&b.time));
    let alpha = |time: f64| {
        if keys.is_empty() {
            return c.transform.opacity;
        }
        if time <= keys[0].time {
            return keys[0].transform.opacity;
        }
        for pair in keys.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if time <= b.time {
                return a.transform.opacity
                    + (b.transform.opacity - a.transform.opacity)
                        * a.fraction((time - a.time) / (b.time - a.time));
            }
        }
        keys.last().unwrap().transform.opacity
    };
    let sample = |time: f64| {
        let local = phase + clock.0.time(time + clock.1) - clock.1;
        let value = alpha(local);
        if c.transition == Transition::Fade {
            let fade = c.fade_duration.min(c.duration / 2.);
            value
                * (local / fade)
                    .min((c.duration - local) / fade)
                    .clamp(0., 1.)
        } else {
            value
        }
    };
    let initial = sample(0.);
    let mut previous = initial;
    let mut commands = String::new();
    for frame in 1..(duration * fps as f64).ceil() as u32 {
        let t = frame as f64 / fps as f64;
        let value = sample(t);
        if (value - previous).abs() > 1e-12 {
            commands += &format!(
                "{:.9} colorchannelmixer@alpha{index} aa {value};",
                (t - 0.000001).max(0.)
            );
            previous = value;
        }
    }
    let mixer = format!(",colorchannelmixer@alpha{index}=aa={initial},format=rgba");
    if commands.is_empty() {
        mixer
    } else {
        format!(",sendcmd=c='{commands}'{mixer}")
    }
}

pub fn graphic_image(c: &Clip, path: &Path, w: u32, h: u32) -> Result<()> {
    if c.vector.is_some() || c.scene3d.is_some() {
        crate::procedural::image(c, 0., w, h, &ProcessGroup::default())?.save(path)?;
        return Ok(());
    }

    let mut pixels = RgbaImage::new(w, h);
    let color = parse_color(&c.color);
    let (wf, hf) = (w as f32, h as f32);
    let mut labels: Vec<(String, f32, f32, f32, bool)> = vec![];
    if c.kind == Kind::Title {
        labels.push((c.text.clone(), c.font_size as f32, 0.5, 0.5, true));
    } else {
        let mut pixmap = tiny_skia::Pixmap::new(w, h).context("Graphic canvas too large")?;
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], 255);
        paint.anti_alias = true;
        let secondary = parse_color(&c.secondary_color);
        let mut bg = tiny_skia::Paint::default();
        bg.set_color_rgba8(secondary[0], secondary[1], secondary[2], 235);
        let rect = |x: f32, y: f32, width: f32, height: f32| {
            tiny_skia::Rect::from_xywh(x * wf, y * hf, width * wf, height * hf).unwrap()
        };
        let mut pb = tiny_skia::PathBuilder::new();
        let line = |pb: &mut tiny_skia::PathBuilder, x: f32, y: f32, x2: f32, y2: f32| {
            pb.move_to(x * wf, y * hf);
            pb.line_to(x2 * wf, y2 * hf);
        };
        let panel = match c.graphic {
            Graphic::LowerThird => Some((0.06, 0.72, 0.65, 0.19)),
            Graphic::Callout => Some((0.43, 0.22, 0.5, 0.2)),
            Graphic::Stat => Some((0.28, 0.27, 0.44, 0.46)),
            Graphic::Equation => Some((0.15, 0.3, 0.7, 0.4)),
            Graphic::Step => Some((0.2, 0.35, 0.65, 0.3)),
            _ => None,
        };
        if let Some((x, y, rw, rh)) = panel {
            fill_graphic_rect(
                &mut pixmap,
                rect(x, y, rw, rh),
                &bg,
                tiny_skia::Transform::identity(),
                None,
            );
            fill_graphic_rect(
                &mut pixmap,
                rect(x, y, 0.005, rh),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            );
        }
        match c.graphic {
            Graphic::Trajectory => {
                line(&mut pb, 0.15, 0.8, 0.85, 0.8);
                pb.move_to(wf * 0.2, hf * 0.75);
                pb.cubic_to(
                    wf * 0.33,
                    hf * 0.05,
                    wf * 0.6,
                    hf * 0.12,
                    wf * 0.78,
                    hf * 0.75,
                );
                line(&mut pb, 0.7, 0.7, 0.78, 0.75);
                line(&mut pb, 0.78, 0.75, 0.8, 0.62);
                labels.push((c.text.clone(), c.font_size as f32, 0.5, 0.9, true));
            }
            Graphic::Orbit => {
                pb.push_oval(rect(0.17, 0.22, 0.66, 0.56));
                pb.push_oval(rect(0.455, 0.42, 0.09, 0.16));
                pb.push_rect(rect(0.73, 0.28, 0.035, 0.06));
                line(&mut pb, 0.75, 0.31, 0.82, 0.44);
                line(&mut pb, 0.82, 0.44, 0.78, 0.41);
                line(&mut pb, 0.82, 0.44, 0.825, 0.36);
                labels.push((c.text.clone(), c.font_size as f32, 0.5, 0.9, true));
            }
            Graphic::Wave | Graphic::Chart => {
                line(&mut pb, 0.15, 0.2, 0.15, 0.8);
                line(&mut pb, 0.15, 0.8, 0.85, 0.8);
                let mut samples = Vec::new();
                if c.graphic == Graphic::Chart && c.chart_data.len() >= 2 {
                    let lo = c.chart_data.first().unwrap()[0];
                    let hi = c.chart_data.last().unwrap()[0];
                    let min = c
                        .chart_data
                        .iter()
                        .map(|p| p[1])
                        .fold(f64::INFINITY, f64::min);
                    let max = c
                        .chart_data
                        .iter()
                        .map(|p| p[1])
                        .fold(f64::NEG_INFINITY, f64::max);
                    for p in &c.chart_data {
                        samples.push((
                            0.15 + ((p[0] - lo) / (hi - lo)) as f32 * 0.7,
                            if max == min {
                                0.5
                            } else {
                                0.75 - ((p[1] - min) / (max - min)) as f32 * 0.5
                            },
                        ));
                    }
                    labels.push((
                        format!("{lo:.2} → {hi:.2} s    {min:.2} … {max:.2}"),
                        c.font_size as f32 * 0.45,
                        0.5,
                        0.12,
                        true,
                    ));
                } else {
                    for i in 0..=160 {
                        let u = i as f32 / 160.;
                        let x = 0.15 + u * 0.7;
                        let y = if c.graphic == Graphic::Wave {
                            0.5 - (u * std::f32::consts::TAU * 3.).sin() * 0.22
                        } else {
                            0.75 - 0.5 * (1. - (-u * 4.).exp())
                                + 0.1 * (u * 12.).sin() * (-u * 2.).exp()
                        };
                        samples.push((x, y));
                    }
                }
                for (i, (x, y)) in samples.into_iter().enumerate() {
                    if i == 0 {
                        pb.move_to(x * wf, y * hf);
                    } else {
                        pb.line_to(x * wf, y * hf);
                    }
                }
                labels.push((c.text.clone(), c.font_size as f32, 0.5, 0.92, true));
            }
            Graphic::Arrow => {
                line(&mut pb, 0.3, 0.5, 0.7, 0.5);
                line(&mut pb, 0.62, 0.4, 0.7, 0.5);
                line(&mut pb, 0.7, 0.5, 0.62, 0.6);
            }
            Graphic::Circle => pb.push_oval(rect(0.35, 0.23, 0.3, 0.54)),
            Graphic::Rectangle => pb.push_rect(rect(0.25, 0.25, 0.5, 0.5)),
            Graphic::LowerThird => {
                labels.push((c.text.clone(), c.font_size as f32, 0.085, 0.815, false))
            }
            Graphic::Callout => {
                line(&mut pb, 0.25, 0.68, 0.38, 0.42);
                line(&mut pb, 0.38, 0.42, 0.55, 0.42);
                pb.push_oval(rect(0.242, 0.666, 0.016, 0.028));
                labels.push((c.text.clone(), c.font_size as f32, 0.46, 0.32, false));
            }
            Graphic::Dimension => {
                line(&mut pb, 0.23, 0.55, 0.77, 0.55);
                for x in [0.23, 0.77] {
                    line(&mut pb, x, 0.4, x, 0.7);
                }
                line(&mut pb, 0.27, 0.51, 0.23, 0.55);
                line(&mut pb, 0.23, 0.55, 0.27, 0.59);
                line(&mut pb, 0.73, 0.51, 0.77, 0.55);
                line(&mut pb, 0.77, 0.55, 0.73, 0.59);
                labels.push((c.text.clone(), c.font_size as f32, 0.5, 0.4, true));
            }
            Graphic::Axes => {
                line(&mut pb, 0.25, 0.22, 0.25, 0.75);
                line(&mut pb, 0.25, 0.75, 0.8, 0.75);
                line(&mut pb, 0.22, 0.28, 0.25, 0.22);
                line(&mut pb, 0.25, 0.22, 0.28, 0.28);
                line(&mut pb, 0.76, 0.7, 0.8, 0.75);
                line(&mut pb, 0.8, 0.75, 0.76, 0.8);
                for i in 1..6 {
                    let x = 0.25 + i as f32 * 0.08;
                    line(&mut pb, x, 0.74, x, 0.77);
                }
                labels.push((c.text.clone(), c.font_size as f32, 0.52, 0.87, true));
            }
            Graphic::Grid => {
                for i in 0..=10 {
                    let u = 0.1 + i as f32 * 0.08;
                    line(&mut pb, u, 0.1, u, 0.9);
                    line(&mut pb, 0.1, u, 0.9, u);
                }
            }
            Graphic::Step => {
                let mut parts = c.text.splitn(2, '\n');
                labels.push((parts.next().unwrap_or("01").into(), 112., 0.3, 0.5, true));
                labels.push((
                    parts.next().unwrap_or("BUILD STEP").into(),
                    c.font_size as f32,
                    0.4,
                    0.5,
                    false,
                ));
            }
            Graphic::Stat => {
                let mut parts = c.text.splitn(2, '\n');
                labels.push((
                    parts.next().unwrap_or("ALTITUDE").into(),
                    c.font_size as f32 * 0.65,
                    0.5,
                    0.39,
                    true,
                ));
                labels.push((
                    parts.next().unwrap_or("1,250 m").into(),
                    c.font_size as f32 * 1.7,
                    0.5,
                    0.56,
                    true,
                ));
            }
            Graphic::Progress => {
                fill_graphic_rect(
                    &mut pixmap,
                    rect(0.2, 0.5, 0.6, 0.065),
                    &bg,
                    tiny_skia::Transform::identity(),
                    None,
                );
                if c.graphic_value >= 0.0001 {
                    fill_graphic_rect(
                        &mut pixmap,
                        rect(0.2, 0.5, 0.6 * c.graphic_value as f32, 0.065),
                        &paint,
                        tiny_skia::Transform::identity(),
                        None,
                    );
                }
                labels.push((
                    format!("{}   {:.0}%", c.text, c.graphic_value * 100.),
                    c.font_size as f32,
                    0.5,
                    0.4,
                    true,
                ));
            }
            Graphic::Rocket => {
                pb.move_to(wf * 0.5, hf * 0.17);
                pb.line_to(wf * 0.55, hf * 0.32);
                pb.line_to(wf * 0.55, hf * 0.72);
                pb.line_to(wf * 0.45, hf * 0.72);
                pb.line_to(wf * 0.45, hf * 0.32);
                pb.close();
                line(&mut pb, 0.45, 0.55, 0.39, 0.76);
                line(&mut pb, 0.39, 0.76, 0.45, 0.72);
                line(&mut pb, 0.55, 0.55, 0.61, 0.76);
                line(&mut pb, 0.61, 0.76, 0.55, 0.72);
                line(&mut pb, 0.47, 0.75, 0.5, 0.9);
                line(&mut pb, 0.5, 0.9, 0.53, 0.75);
                line(&mut pb, 0.45, 0.32, 0.55, 0.32);
            }
            Graphic::Equation => labels.push((c.text.clone(), c.font_size as f32, 0.5, 0.5, true)),
            Graphic::Comparison => {
                let mut parts = c.text.splitn(2, '\n');
                for (y, value, label) in [
                    (0.4, 1., parts.next().unwrap_or("SIMULATION")),
                    (
                        0.65,
                        c.graphic_value as f32,
                        parts.next().unwrap_or("FLIGHT DATA"),
                    ),
                ] {
                    fill_graphic_rect(
                        &mut pixmap,
                        rect(0.2, y, 0.6, 0.065),
                        &bg,
                        tiny_skia::Transform::identity(),
                        None,
                    );
                    if value >= 0.0001 {
                        fill_graphic_rect(
                            &mut pixmap,
                            rect(0.2, y, 0.6 * value, 0.065),
                            &paint,
                            tiny_skia::Transform::identity(),
                            None,
                        );
                    }
                    labels.push((
                        label.into(),
                        c.font_size as f32 * 0.65,
                        0.2,
                        y - 0.06,
                        false,
                    ));
                }
            }
        }
        if let Some(path) = pb.finish() {
            if c.fill
                && matches!(
                    c.graphic,
                    Graphic::Circle | Graphic::Rectangle | Graphic::Rocket
                )
            {
                pixmap.fill_path(
                    &path,
                    &bg,
                    tiny_skia::FillRule::Winding,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
            pixmap.stroke_path(
                &path,
                &paint,
                &tiny_skia::Stroke {
                    width: (c.stroke_width as f32 * wf / 1920.).max(0.5),
                    line_cap: tiny_skia::LineCap::Round,
                    ..Default::default()
                },
                tiny_skia::Transform::identity(),
                None,
            );
        }
        for (pixel, bytes) in pixels
            .pixels_mut()
            .zip(pixmap.data().as_chunks::<4>().0.iter())
        {
            let a = bytes[3];
            if a > 0 {
                *pixel = Rgba([
                    ((bytes[0] as u32 * 255) / a as u32).min(255) as u8,
                    ((bytes[1] as u32 * 255) / a as u32).min(255) as u8,
                    ((bytes[2] as u32 * 255) / a as u32).min(255) as u8,
                    a,
                ]);
            }
        }
    }
    for (text, size, x, y, center) in labels {
        draw_text(
            &mut pixels,
            &text,
            size * wf / 1920.,
            wf * x,
            hf * y,
            color,
            center,
        )?;
    }
    pixels.save(path)?;
    Ok(())
}
// The generic path rasterizer handles subpixel rectangles reliably. The specialized
// AA rectangle path in tiny-skia 0.11 asserts on narrow accent strips at thumbnail sizes.
fn fill_graphic_rect(
    pixmap: &mut tiny_skia::Pixmap,
    rect: tiny_skia::Rect,
    paint: &tiny_skia::Paint<'_>,
    transform: tiny_skia::Transform,
    mask: Option<&tiny_skia::Mask>,
) {
    pixmap.fill_path(
        &tiny_skia::PathBuilder::from_rect(rect),
        paint,
        tiny_skia::FillRule::Winding,
        transform,
        mask,
    );
}
fn parse_color(s: &str) -> [u8; 4] {
    let n = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0xffffff);
    [(n >> 16) as u8, (n >> 8) as u8, n as u8, 255]
}
fn draw_text(
    image: &mut RgbaImage,
    text: &str,
    size: f32,
    x: f32,
    y: f32,
    color: [u8; 4],
    center: bool,
) -> Result<()> {
    static FONT: std::sync::OnceLock<fontdue::Font> = std::sync::OnceLock::new();
    let font = FONT.get_or_init(|| {
        fontdue::Font::from_bytes(
            include_bytes!("../assets/fonts/DejaVuSans.ttf").as_slice(),
            fontdue::FontSettings::default(),
        )
        .expect("Bundled DejaVu font must be valid")
    });
    let lines: Vec<_> = text.lines().collect();
    let line_height = size * 1.25;
    for (i, line) in lines.iter().enumerate() {
        let width: f32 = line
            .chars()
            .map(|ch| font.metrics(ch, size).advance_width)
            .sum();
        let mut pen = if center { x - width / 2. } else { x };
        let baseline = y + (i as f32 - (lines.len() as f32 - 1.) / 2.) * line_height + size * 0.34;
        for ch in line.chars() {
            let (m, bitmap) = font.rasterize(ch, size);
            for by in 0..m.height {
                for bx in 0..m.width {
                    let px = (pen + m.xmin as f32) as i32 + bx as i32;
                    let py = (baseline - m.ymin as f32 - m.height as f32) as i32 + by as i32;
                    if px >= 0
                        && py >= 0
                        && (px as u32) < image.width()
                        && (py as u32) < image.height()
                    {
                        let alpha = bitmap[by * m.width + bx] as f32 / 255.;
                        if alpha > 0. {
                            let old = image.get_pixel_mut(px as u32, py as u32);
                            let oa = old[3] as f32 / 255.;
                            let na = alpha + oa * (1. - alpha);
                            for j in 0..3 {
                                old[j] = ((color[j] as f32 * alpha
                                    + old[j] as f32 * oa * (1. - alpha))
                                    / na) as u8;
                            }
                            old[3] = (na * 255.) as u8;
                        }
                    }
                }
            }
            pen += m.advance_width;
        }
    }
    Ok(())
}
pub fn build_graph(
    p: &Project,
    assets: &HashMap<String, Asset>,
    folder: &Path,
    origin: f64,
    duration: f64,
    mode: Mode,
) -> Result<Graph> {
    build_graph_cancel(
        p,
        assets,
        folder,
        origin,
        duration,
        mode,
        &ProcessGroup::default(),
    )
}
pub fn build_graph_cancel(
    p: &Project,
    assets: &HashMap<String, Asset>,
    folder: &Path,
    origin: f64,
    duration: f64,
    mode: Mode,
    group: &ProcessGroup,
) -> Result<Graph> {
    group.check()?;
    ensure!(
        origin.is_finite() && (0. ..=86400.).contains(&origin),
        "Invalid render origin"
    );
    p.validate()?;
    ensure!(
        duration > 0. && duration <= 30. + 1e-8 && duration.is_finite(),
        "Render windows must be greater than zero and at most 30 seconds"
    );
    fs::create_dir_all(folder)?;
    let mut sources = ProceduralSources::default();
    let samples = if mode == Mode::PreviewAudio {
        1
    } else {
        p.motion_blur_samples
    };
    let (w, h, fps) = (p.width, p.height, p.fps * samples);
    let exposure = Exposure {
        fps: p.fps,
        fraction: if samples > 1 { p.motion_shutter } else { 1. },
    };
    let mut args = vec![];
    push(
        &mut args,
        [
            "-hide_banner",
            "-y",
            "-filter_complex_threads",
            "2",
            "-threads",
            "2",
        ],
    );
    if mode == Mode::PreviewVideo {
        push(&mut args, ["-readrate", "1"]);
    }
    push(
        &mut args,
        [
            "-f",
            "lavfi",
            "-i",
            &format!("color=c=0x090c10:s={w}x{h}:r={fps}:d={duration}"),
        ],
    );
    if mode == Mode::PreviewAudio {
        push(&mut args, ["-readrate", "1"]);
    }
    push(
        &mut args,
        [
            "-f",
            "lavfi",
            "-i",
            &format!("anullsrc=r=48000:cl=stereo:d={duration}"),
        ],
    );
    let mut filters = vec![];
    if mode != Mode::PreviewAudio {
        filters.push("[0:v]format=rgba[base]".into());
    }
    let mut base = "base".to_string();
    let mut audios = vec!["[1:a]".to_string()];
    let mut index = 2;
    let mut clips: Vec<_> = p
        .clips
        .iter()
        .filter(|c| c.start < origin + duration && c.end() > origin)
        .collect();
    clips.sort_by(|a, b| b.track.cmp(&a.track).then(a.start.total_cmp(&b.start)));
    ensure!(
        clips.len() <= 24,
        "More than 24 intersecting inputs; flatten a dense composition"
    );
    for c in clips {
        if p.hidden_tracks.contains(&c.track) && c.kind != Kind::Audio {
            continue;
        }
        let a = c.asset_id.as_ref().and_then(|id| assets.get(id));
        if matches!(c.kind, Kind::Video | Kind::Audio | Kind::Image) {
            let a = a.context(format!("Missing media: {}. Relink its original.", c.name))?;
            ensure!(
                (if mode == Mode::Export {
                    a.path.is_file()
                } else {
                    a.preview_path().is_file()
                }),
                "Original offline: {}. Reconnect or relink its drive.",
                c.name
            );
            if matches!(c.kind, Kind::Video | Kind::Audio) {
                ensure!(
                    c.source_in + c.duration * c.speed
                        <= a.duration + (0.1f64).max(c.speed / p.fps as f64),
                    "Clip extends beyond its source: {}",
                    c.name
                );
            }
        }
        let has_audio = a.is_some_and(|a| a.has_audio)
            && matches!(c.kind, Kind::Video | Kind::Audio)
            && !p.muted_tracks.contains(&c.track)
            && c.volume > 0.;
        if mode == Mode::PreviewAudio && !has_audio {
            continue;
        }
        if mode == Mode::PreviewVideo && c.kind == Kind::Audio {
            continue;
        }
        let phase = (origin - c.start).max(0.);
        let start = (c.start - origin).max(0.);
        let dur = c.end().min(origin + duration) - c.start.max(origin);
        let fade = c.fade_duration.min(c.duration / 2.);
        let video_clock = format!(
            "({}-{}+{})",
            exposure.expression(&format!("(t+{start})")),
            start,
            phase
        );
        let video_t_clock = format!(
            "({}-{}+{})",
            exposure.expression(&format!("(T+{start})")),
            start,
            phase
        );
        let overlay_clock = format!("({}-{}+{})", exposure.expression("t"), start, phase);

        let procedural = c.kind == Kind::Graphic && (c.vector.is_some() || c.scene3d.is_some());
        let path = if procedural {
            let path = folder.join(format!("procedural-{index}.mkv"));
            sources.0.push(path.clone());
            crate::procedural::write_source(
                c,
                crate::procedural::Window {
                    phase,
                    duration: dur,
                    fps,
                    size: [w, h],
                    exposure,
                    start,
                },
                &path,
                group,
            )?;
            path
        } else if matches!(c.kind, Kind::Title | Kind::Graphic) {
            let path = folder.join(format!("graphic-{index}.png"));
            graphic_image(c, &path, w, h)?;
            path
        } else {
            let a = a.unwrap();
            if mode == Mode::Export {
                a.path.clone()
            } else {
                a.preview_path().to_owned()
            }
        };
        if procedural {
            push(&mut args, ["-threads", "2", "-i"]);
        } else if matches!(c.kind, Kind::Image | Kind::Title | Kind::Graphic) {
            push(
                &mut args,
                [
                    "-loop",
                    "1",
                    "-framerate",
                    &fps.to_string(),
                    "-t",
                    &dur.to_string(),
                    "-threads",
                    "2",
                    "-i",
                ],
            );
        } else {
            push(
                &mut args,
                [
                    "-ss",
                    &(c.source_in + phase * c.speed).to_string(),
                    "-t",
                    &(dur * c.speed).to_string(),
                    "-threads",
                    "2",
                    "-i",
                ],
            );
        }
        args.push(path.into_os_string());
        let idx = index;
        index += 1;
        if has_audio && mode != Mode::PreviewVideo {
            let mut speed = c.speed;
            let mut tempo = vec![];
            while speed > 2. {
                tempo.push("atempo=2".into());
                speed /= 2.;
            }
            while speed < 0.5 {
                tempo.push("atempo=0.5".into());
                speed *= 2.;
            }
            tempo.push(format!("atempo={speed}"));
            let mut af = format!(
                "[{idx}:a]asetpts=PTS-STARTPTS,{},aresample=48000,volume={},atrim=duration={dur}",
                tempo.join(","),
                c.volume
            );
            if c.transition == Transition::Fade {
                af += &format!(
                    ",volume='if(isnan(t),0,max(0,min(1,min((t+{phase})/{fade},({}-t-{phase})/{fade}))))':eval=frame",
                    c.duration
                );
            }
            af += &format!(",adelay={}:all=1[a{idx}]", (start * 1000.).round());
            filters.push(af);
            audios.push(format!("[a{idx}]"));
        }
        if mode == Mode::PreviewAudio || c.kind == Kind::Audio {
            continue;
        }
        let mut temporal = String::new();
        if c.kind == Kind::Video && mode == Mode::Export {
            if c.deflicker {
                temporal += "deflicker=size=5:mode=am,";
            }
            if c.motion_blur {
                let window = (c.speed * c.shutter).max(2.);
                let stride = (window / 16.).ceil().max(1.);
                let frames = (window / stride).round().clamp(2., 16.) as usize;
                if stride > 1. {
                    temporal += &format!("fps={},", (a.unwrap().fps / stride).max(0.00001));
                }
                temporal += &format!(
                    "tmix=frames={frames}:weights='{}',",
                    vec!["1"; frames].join(" ")
                );
            }
        }
        let mut chain = format!(
            "[{idx}:v]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black@0,setsar=1,{temporal}setpts=(PTS-STARTPTS)/{},fps={fps},trim=duration={dur},format=rgba",
            if c.kind == Kind::Video { c.speed } else { 1. }
        );
        if matches!(c.kind, Kind::Video | Kind::Image) {
            if c.brightness != 0. || c.contrast != 1. || c.saturation != 1. {
                chain += &format!(
                    ",eq=brightness={}:contrast={}:saturation={}",
                    c.brightness, c.contrast, c.saturation
                );
            }
            if c.warmth != 0. {
                chain += &format!(
                    ",colorbalance=rs={}:bs={}",
                    c.warmth * 0.25,
                    -c.warmth * 0.25
                );
            }
            if c.blur > 0. {
                chain += &format!(",gblur=sigma={}", c.blur);
            }
        }
        if c.mask == Mask::Text {
            let mask_path = folder.join(format!("text-matte-{idx}.png"));
            let matte = Clip {
                kind: Kind::Title,
                text: c.text.clone(),
                font_size: c.font_size,
                color: "#ffffff".into(),
                ..Clip::default()
            };
            graphic_image(&matte, &mask_path, w, h)?;
            let mask_index = index;
            index += 1;
            push(
                &mut args,
                [
                    "-loop",
                    "1",
                    "-framerate",
                    &fps.to_string(),
                    "-t",
                    &dur.to_string(),
                    "-threads",
                    "2",
                    "-i",
                ],
            );
            args.push(mask_path.into_os_string());
            chain += &format!(
                ",split[rgb{idx}][originalalpha{idx}];[originalalpha{idx}]alphaextract[oa{idx}];[{mask_index}:v]format=rgba,alphaextract[matte{idx}];[oa{idx}][matte{idx}]blend=all_mode=multiply[maskedalpha{idx}];[rgb{idx}][maskedalpha{idx}]alphamerge"
            );
        }
        if matches!(c.mask, Mask::Circle | Mask::Rectangle) {
            let mask = if c.mask == Mask::Circle {
                "lte(pow((X-W/2)/(W*0.28),2)+pow((Y-H/2)/(H*0.45),2),1)"
            } else {
                "between(X,W*0.2,W*0.8)*between(Y,H*0.15,H*0.85)"
            };
            chain += &format!(",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*({mask})'");
        }
        if c.reveal != Reveal::None && phase + c.reveal_offset < c.reveal_duration {
            let clock = format!(
                "({video_t_clock}+{})/{}",
                c.reveal_offset, c.reveal_duration
            );
            let progress = Easing::Smoother.expression(&clock);
            let mask = match c.reveal {
                Reveal::LeftToRight => format!("clip(W*({progress})-X,0,1)"),
                Reveal::BottomToTop => format!("clip(Y-H*(1-({progress}))+1,0,1)"),
                Reveal::Iris => {
                    format!("clip(hypot(W,H)*0.5*({progress})-hypot(X-W/2,Y-H/2)+0.5,0,1)")
                }
                Reveal::None => unreachable!(),
            };
            chain += &format!(",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*({mask})'");
        }
        // Rotate on a fixed-size input. FFmpeg configures rotate's output bounds
        // once; putting a variable-size scale before it clips later frames.
        if c.transform.rotation != 0. || c.keyframes.iter().any(|k| k.transform.rotation != 0.) {
            chain += &format!(
                ",rotate=a='({})*PI/180':ow=iw:oh=ih:c=none",
                animated(c, "rotation", &video_clock)
            );
        }
        let mut scale = animated(c, "scale", &video_clock);
        if c.transition == Transition::Zoom {
            scale = format!(
                "({scale})*(0.8+0.2*min(({video_clock})/{},1))",
                c.fade_duration
            );
        }
        // Packed RGBA after dynamic resizing isolates planar alpha filter buffers.
        // FFmpeg 7 can otherwise corrupt frame pools when animated scale and geq combine.
        if c.transform.scale != 1.
            || c.keyframes.iter().any(|k| k.transform.scale != 1.)
            || c.transition == Transition::Zoom
        {
            chain += &format!(
                ",scale=w='max(2,trunc({w}*({scale})/2)*2)':h='max(2,trunc({h}*({scale})/2)*2)':eval=frame,format=rgba"
            );
        }
        if c.transform.opacity != 1.
            || c.keyframes.iter().any(|k| k.transform.opacity != 1.)
            || c.transition == Transition::Fade
        {
            chain += &opacity_filter(c, phase, dur, fps, idx, (exposure, start));
        }
        if c.transition == Transition::Wipe {
            chain += &format!(
                ",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*lte(X,W*min(({video_t_clock})/{fade},1))'"
            );
        }
        chain += &format!(",setpts=PTS+{start}/TB[v{idx}]");
        filters.push(chain);
        let x = animated(c, "x", &overlay_clock);
        let y = animated(c, "y", &overlay_clock);
        // Overlay can retain the first input dimensions while an upstream scale
        // changes size. Compute current bounds explicitly to keep the layer's
        // anchor fixed throughout a zoom (and after a mid-animation seek).
        let mut overlay_scale = animated(c, "scale", &overlay_clock);
        if c.transition == Transition::Zoom {
            overlay_scale = format!(
                "({overlay_scale})*(0.8+0.2*min(({overlay_clock})/{},1))",
                c.fade_duration
            );
        }
        let current_w = format!("max(2,trunc({w}*({overlay_scale})/2)*2)");
        let current_h = format!("max(2,trunc({h}*({overlay_scale})/2)*2)");
        let mut xe = format!("W*({x})/100-({current_w})/2");
        if c.transition == Transition::Slide {
            xe += &format!("-W*max(1-({overlay_clock})/{fade},0)");
        }
        let next = format!("comp{idx}");
        filters.push(format!("[{base}][v{idx}]overlay=x='{xe}':y='H*({y})/100-({current_h})/2':enable='gte(t,{start})*lt(t,{})':eof_action=pass:repeatlast=0:format=auto[{next}]",start+dur));
        base = next;
    }
    if samples > 1 {
        let next = "motion_blurred";
        filters.push(format!("[{base}]tmix=frames={samples}:weights='{}',select='eq(mod(n,{samples}),{})',setpts=N/({}*TB),fps={}[{next}]",vec!["1";samples as usize].join(" "),samples-1,p.fps,p.fps));
        base = next.into();
    }
    if mode != Mode::PreviewAudio {
        filters.push(format!(
            "[{base}]format={}[outv]",
            if mode == Mode::PreviewVideo {
                "rgb24"
            } else {
                "yuv420p"
            }
        ));
    }
    if mode != Mode::PreviewVideo {
        filters.push(format!(
            "{}amix=inputs={}:duration=first:normalize=0,alimiter=limit=0.95[outa]",
            audios.join(""),
            audios.len()
        ));
    }
    let script = folder.join("filters.txt");
    fs::write(&script, filters.join(";\n"))?;
    push(&mut args, ["-filter_complex_script"]);
    args.push(script.into_os_string());
    if mode != Mode::PreviewAudio {
        push(&mut args, ["-map", "[outv]"]);
    }
    if mode != Mode::PreviewVideo {
        push(&mut args, ["-map", "[outa]"]);
    }
    push(&mut args, ["-t", &duration.to_string()]);
    Ok(Graph {
        args,
        duration,
        _sources: sources,
    })
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Job {
    pub id: String,
    pub status: String,
    pub progress: f64,
    pub error: Option<String>,
    pub output: Option<PathBuf>,
}
pub fn export(
    p: &Project,
    assets: &HashMap<String, Asset>,
    folder: &Path,
    output: &Path,
    group: &ProcessGroup,
    mut progress: impl FnMut(f64),
) -> Result<()> {
    p.validate()?;
    ensure!(
        !output.exists(),
        "Output already exists; select a new filename to preserve it"
    );
    ensure!(
        output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .is_dir(),
        "Export destination folder does not exist"
    );
    let duration = p.duration();
    ensure!(duration > 0., "Add clips before exporting");
    fs::create_dir_all(folder)?;
    ensure!(
        fs2::available_space(folder)? > 256 * 1024 * 1024,
        "Free space on the cache drive before exporting"
    );
    let mut origin = 0.;
    let mut chunks = vec![];
    while origin < duration - 1e-8 {
        group.check()?;
        let mut window = (duration - origin).min(30.);
        while p
            .clips
            .iter()
            .filter(|c| c.start < origin + window && c.end() > origin)
            .count()
            > 24
            && window > 1. / p.fps as f64
        {
            window = (window * p.fps as f64 / 2.).round().max(1.) / p.fps as f64;
        }
        let part = folder.join(format!("part-{:05}", chunks.len()));
        let graph = build_graph_cancel(p, assets, &part, origin, window, Mode::Export, group)?;
        let path = part.join("video.mp4");
        let mut cmd = graph.command();
        cmd.args([
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "20",
            "-threads",
            "2",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-nostats",
        ])
        .arg(&path);
        render_progress(
            &mut cmd,
            &part.join("render.log"),
            group,
            |v| progress((origin + window * v) / duration * 0.98),
            window,
        )?;
        chunks.push(path);
        origin += window;
    }
    group.check()?;
    let list = folder.join("concat.txt");
    fs::write(
        &list,
        chunks
            .iter()
            .map(|p| {
                format!(
                    "file '{}'\n",
                    p.canonicalize()
                        .unwrap_or_else(|_| p.clone())
                        .to_string_lossy()
                        .replace('\\', "/")
                        .replace('\'', "'\\''")
                )
            })
            .collect::<String>(),
    )?;
    let staging = folder.join("complete.mp4");
    group.run(
        command("ffmpeg")
            .args(["-v", "error", "-y", "-f", "concat", "-safe", "0", "-i"])
            .arg(&list)
            .args([
                "-c",
                "copy",
                "-t",
                &duration.to_string(),
                "-movflags",
                "+faststart",
            ])
            .arg(&staging),
        Some(&folder.join("concat.log")),
    )?;
    ensure!(
        !output.exists(),
        "Output already exists; select a new filename to protect the existing video"
    );
    let temp = output.with_extension(format!("{}.tmp", crate::model::uid()));
    let result = (|| -> Result<()> {
        fs::copy(&staging, &temp)?;
        fs::OpenOptions::new().write(true).open(&temp)?.sync_all()?;
        fs::rename(&temp, output)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    for chunk in chunks {
        let _ = fs::remove_file(chunk);
    }
    let _ = fs::remove_file(staging);
    progress(1.);
    Ok(())
}
fn render_progress(
    cmd: &mut std::process::Command,
    log: &Path,
    group: &ProcessGroup,
    mut update: impl FnMut(f64),
    duration: f64,
) -> Result<()> {
    let stderr = fs::File::create(log)?;
    let process = group.spawn(
        cmd.stdout(Stdio::piped())
            .stderr(stderr)
            .stdin(Stdio::null()),
    )?;
    let stdout = process
        .child
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .stdout
        .take()
        .context("No export progress stream")?;
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        if let Some(time) = line
            .strip_prefix("out_time_us=")
            .and_then(|s| s.parse::<f64>().ok())
        {
            update((time / 1e6 / duration).clamp(0., 0.99));
        }
        group.check()?;
    }
    let status = process
        .child
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .wait()?;
    group.check()?;
    if !status.success() {
        bail!(
            "Export failed. Inspect {}: {}",
            log.display(),
            fs::read_to_string(log)
                .unwrap_or_default()
                .chars()
                .rev()
                .take(1600)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        );
    }
    Ok(())
}
pub fn persist_job(folder: &Path, job: &Job) -> Result<()> {
    atomic_json(&folder.join("job.json"), job)
}
