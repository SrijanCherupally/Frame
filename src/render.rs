use crate::{
    media::Asset,
    model::{Clip, Graphic, Kind, Mask, Project, Transform, Transition},
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
pub struct Graph {
    pub args: Vec<OsString>,
    pub duration: f64,
}
impl Graph {
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
fn animated(c: &Clip, p: &str, clock: &str) -> String {
    let mut keys = c.keyframes.clone();
    keys.sort_by(|a, b| a.time.total_cmp(&b.time));
    if keys.is_empty() {
        return prop(&c.transform, p).to_string();
    }
    let mut out = prop(&keys.last().unwrap().transform, p).to_string();
    for pair in keys.windows(2).rev() {
        let (a, b) = (&pair[0], &pair[1]);
        let (v0, v1) = (prop(&a.transform, p), prop(&b.transform, p));
        out = format!(
            "if(lt({clock},{}),{v0}+({})*({clock}-{})/{},{out})",
            b.time,
            v1 - v0,
            a.time,
            b.time - a.time
        );
    }
    format!(
        "if(lt({clock},{}),{},{out})",
        keys[0].time,
        prop(&keys[0].transform, p)
    )
}
pub fn graphic_image(c: &Clip, path: &Path, w: u32, h: u32) -> Result<()> {
    let mut pixels = RgbaImage::new(w, h);
    let color = parse_color(&c.color);
    if c.kind == Kind::Title {
        draw_text(
            &mut pixels,
            &c.text,
            c.font_size as f32 * w as f32 / 1920.,
            w as f32 / 2.,
            h as f32 / 2.,
            color,
            true,
        )?;
    } else {
        let mut pixmap = tiny_skia::Pixmap::new(w, h).context("Graphic canvas too large")?;
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        paint.anti_alias = true;
        let mut pb = tiny_skia::PathBuilder::new();
        let (wf, hf) = (w as f32, h as f32);
        match c.graphic {
            Graphic::Arrow => {
                pb.move_to(wf * 0.3, hf * 0.5);
                pb.line_to(wf * 0.7, hf * 0.5);
                pb.move_to(wf * 0.62, hf * 0.4);
                pb.line_to(wf * 0.7, hf * 0.5);
                pb.line_to(wf * 0.62, hf * 0.6);
            }
            Graphic::Circle => {
                if let Some(rect) =
                    tiny_skia::Rect::from_xywh(wf * 0.35, hf * 0.23, wf * 0.3, hf * 0.54)
                {
                    pb.push_oval(rect);
                }
            }
            Graphic::Rectangle => {
                pb.push_rect(
                    tiny_skia::Rect::from_xywh(wf * 0.25, hf * 0.25, wf * 0.5, hf * 0.5).unwrap(),
                );
            }
            Graphic::LowerThird => {
                let r =
                    tiny_skia::Rect::from_xywh(wf * 0.06, hf * 0.72, wf * 0.59, hf * 0.18).unwrap();
                let mut bg = tiny_skia::Paint::default();
                bg.set_color_rgba8(14, 21, 24, 225);
                pixmap.fill_rect(r, &bg, tiny_skia::Transform::identity(), None);
                pixmap.fill_rect(
                    tiny_skia::Rect::from_xywh(wf * 0.06, hf * 0.72, wf * 0.006, hf * 0.18)
                        .unwrap(),
                    &paint,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        }
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(
                &path,
                &paint,
                &tiny_skia::Stroke {
                    width: (wf / 180.).max(3.),
                    ..Default::default()
                },
                tiny_skia::Transform::identity(),
                None,
            );
        }
        for (x, y, p) in pixels.enumerate_pixels_mut() {
            let at = ((y * w + x) * 4) as usize;
            let a = pixmap.data()[at + 3];
            if a > 0 {
                *p = Rgba([
                    ((pixmap.data()[at] as u32 * 255) / a as u32).min(255) as u8,
                    ((pixmap.data()[at + 1] as u32 * 255) / a as u32).min(255) as u8,
                    ((pixmap.data()[at + 2] as u32 * 255) / a as u32).min(255) as u8,
                    a,
                ]);
            }
        }
        if c.graphic == Graphic::LowerThird {
            draw_text(
                &mut pixels,
                &c.text.chars().take(80).collect::<String>(),
                40. * wf / 1920.,
                wf * 0.085,
                hf * 0.81,
                color,
                false,
            )?;
        }
    }
    pixels.save(path)?;
    Ok(())
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
    let font = fontdue::Font::from_bytes(
        include_bytes!("../assets/fonts/DejaVuSans.ttf").as_slice(),
        fontdue::FontSettings::default(),
    )
    .map_err(|e| anyhow::anyhow!(e))?;
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
    p.validate()?;
    ensure!(
        duration > 0. && duration.is_finite(),
        "Add clips before rendering"
    );
    fs::create_dir_all(folder)?;
    let (w, h, fps) = (p.width, p.height, p.fps);
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
        let path = if matches!(c.kind, Kind::Title | Kind::Graphic) {
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
        if matches!(c.kind, Kind::Image | Kind::Title | Kind::Graphic) {
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
        if c.mask != Mask::None {
            let mask = if c.mask == Mask::Circle {
                "lte(pow((X-W/2)/(W*0.28),2)+pow((Y-H/2)/(H*0.45),2),1)"
            } else {
                "between(X,W*0.2,W*0.8)*between(Y,H*0.15,H*0.85)"
            };
            chain += &format!(",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*({mask})'");
        }
        let mut scale = animated(c, "scale", &format!("(t+{phase})"));
        if c.transition == Transition::Zoom {
            scale = format!("({scale})*(0.8+0.2*min((t+{phase})/{},1))", c.fade_duration);
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
        if c.transform.rotation != 0. || c.keyframes.iter().any(|k| k.transform.rotation != 0.) {
            chain += &format!(
                ",rotate=a='({})*PI/180':ow=iw:oh=ih:c=none",
                animated(c, "rotation", &format!("(t+{phase})"))
            );
        }
        if c.transform.opacity != 1. || c.keyframes.iter().any(|k| k.transform.opacity != 1.) {
            chain += &format!(
                ",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*({})'",
                animated(c, "opacity", &format!("(T+{phase})"))
            );
        }
        if c.transition == Transition::Fade {
            if phase == 0. {
                chain += &format!(
                    ",fade=t=in:d={fade}:alpha=1,fade=t=out:st={}:d={fade}:alpha=1",
                    c.duration - fade
                );
            } else if phase >= fade && c.duration - phase >= fade {
                chain += &format!(
                    ",fade=t=out:st={}:d={fade}:alpha=1",
                    c.duration - phase - fade
                );
            } else {
                chain += &format!(
                    ",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*max(0,min(1,min((T+{phase})/{fade},({}-T-{phase})/{fade})))'",
                    c.duration
                );
            }
        }
        if c.transition == Transition::Wipe {
            chain += &format!(
                ",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*lte(X,W*min((T+{phase})/{fade},1))'"
            );
        }
        chain += &format!(",setpts=PTS+{start}/TB[v{idx}]");
        filters.push(chain);
        let x = animated(c, "x", &format!("(t-{start}+{phase})"));
        let y = animated(c, "y", &format!("(t-{start}+{phase})"));
        let mut xe = format!("W*({x})/100-w/2");
        if c.transition == Transition::Slide {
            xe += &format!("-W*max(1-(t-{start}+{phase})/{fade},0)");
        }
        let next = format!("comp{idx}");
        filters.push(format!("[{base}][v{idx}]overlay=x='{xe}':y='H*({y})/100-h/2':enable='gte(t,{start})*lt(t,{})':eof_action=pass:repeatlast=0:format=auto[{next}]",start+dur));
        base = next;
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
    Ok(Graph { args, duration })
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
        let graph = build_graph(p, assets, &part, origin, window, Mode::Export)?;
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
