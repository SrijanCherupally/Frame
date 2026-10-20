//! Cancellable translation tracking using normalized patch correlation.
//! Stops on low confidence instead of inventing motion through an occlusion.
use crate::{
    media::Asset,
    model::{Clip, Keyframe, Kind, Project},
    process::{ProcessGroup, command},
};
use anyhow::{Context, Result, ensure};
use std::{io::Read, path::Path, process::Stdio};
#[derive(Debug, Clone)]
pub struct GrayFrame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone)]
pub struct TrackPoint {
    pub time: f64,
    pub position: [f64; 2],
    pub confidence: f64,
}
#[derive(Debug, Clone)]
pub struct Track {
    pub source: Clip,
    pub width: u32,
    pub height: u32,
    pub points: Vec<TrackPoint>,
    pub lost: bool,
}
#[derive(Debug, Clone)]
pub struct Settings {
    pub start: f64,
    pub duration: f64,
    pub seed: [f64; 2],
    pub patch_radius: u32,
    pub search_radius: u32,
    pub min_confidence: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            start: 0.,
            duration: 5.,
            seed: [50., 50.],
            patch_radius: 8,
            search_radius: 16,
            min_confidence: 0.65,
        }
    }
}
fn dimensions(asset: &Asset, group: &ProcessGroup) -> Result<(u32, u32, u32, u32)> {
    let bytes = group.run(
        command("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=width,height:stream_side_data=rotation",
                "-of",
                "json",
            ])
            .arg(&asset.path),
        None,
    )?;
    let info: serde_json::Value = serde_json::from_slice(&bytes)?;
    let stream = &info["streams"][0];
    let mut width = stream["width"].as_u64().unwrap_or(asset.width as u64) as u32;
    let mut height = stream["height"].as_u64().unwrap_or(asset.height as u64) as u32;
    let rotation = stream["side_data_list"]
        .as_array()
        .and_then(|list| list.iter().find_map(|v| v["rotation"].as_f64()))
        .unwrap_or(0.);
    if ((rotation.abs() % 180.) - 90.).abs() < 0.01 {
        std::mem::swap(&mut width, &mut height);
    }
    ensure!(width > 0 && height > 0, "Source dimensions are unavailable");
    let scale = 320. / width.max(height) as f64;
    Ok((
        ((width as f64 * scale).round() as u32 / 2 * 2).max(16),
        ((height as f64 * scale).round() as u32 / 2 * 2).max(16),
        width,
        height,
    ))
}
pub fn reference_frame(
    asset: &Asset,
    c: &Clip,
    start: f64,
    group: &ProcessGroup,
) -> Result<GrayFrame> {
    let (w, h, _, _) = dimensions(asset, group)?;
    ensure!(
        start.is_finite() && start >= 0. && start < c.duration,
        "Reference time is outside this clip"
    );
    let pixels = group.run(
        command("ffmpeg")
            .args([
                "-v",
                "error",
                "-nostdin",
                "-ss",
                &(c.source_in + start * c.speed).to_string(),
                "-threads",
                "2",
                "-i",
            ])
            .arg(&asset.path)
            .args([
                "-vf",
                &format!("scale={w}:{h},format=gray"),
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "gray",
                "pipe:1",
            ]),
        None,
    )?;
    ensure!(
        pixels.len() == (w * h) as usize,
        "Could not decode tracking reference frame"
    );
    Ok(GrayFrame {
        pixels,
        width: w,
        height: h,
    })
}
struct Patch {
    values: Vec<f64>,
    energy: f64,
    r: i32,
}
impl Patch {
    fn new(frame: &GrayFrame, position: [i32; 2], r: i32) -> Result<Self> {
        let values = patch_values(frame, position, r)
            .context("Tracking point is too close to the picture edge")?;
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let values: Vec<_> = values.into_iter().map(|n| n - mean).collect();
        let energy = values.iter().map(|n| n * n).sum::<f64>().sqrt();
        ensure!(
            energy > (values.len() as f64).sqrt() * 4.,
            "Choose a textured feature, not a flat area"
        );
        Ok(Self { values, energy, r })
    }
    fn score(&self, frame: &GrayFrame, p: [i32; 2]) -> Option<f64> {
        if p[0] - self.r < 0
            || p[1] - self.r < 0
            || p[0] + self.r >= frame.width as i32
            || p[1] + self.r >= frame.height as i32
        {
            return None;
        }
        let mut sum = 0.;
        let mut sum2 = 0.;
        let mut correlation = 0.;
        let mut i = 0;
        for y in p[1] - self.r..=p[1] + self.r {
            for x in p[0] - self.r..=p[0] + self.r {
                let n = frame.pixels[(y as u32 * frame.width + x as u32) as usize] as f64;
                sum += n;
                sum2 += n * n;
                correlation += n * self.values[i];
                i += 1;
            }
        }
        let energy = (sum2 - sum * sum / self.values.len() as f64).max(0.).sqrt();
        if energy < 1e-8 {
            None
        } else {
            Some(correlation / (self.energy * energy))
        }
    }
}
fn patch_values(frame: &GrayFrame, p: [i32; 2], r: i32) -> Option<Vec<f64>> {
    if p[0] - r < 0
        || p[1] - r < 0
        || p[0] + r >= frame.width as i32
        || p[1] + r >= frame.height as i32
    {
        return None;
    }
    let mut v = vec![];
    for y in p[1] - r..=p[1] + r {
        for x in p[0] - r..=p[0] + r {
            v.push(frame.pixels[(y as u32 * frame.width + x as u32) as usize] as f64);
        }
    }
    Some(v)
}
/// Core matcher is exposed for deterministic fixtures independent of a codec.
pub fn track_frames(
    frames: &[GrayFrame],
    seed: [f64; 2],
    radius: u32,
    search: u32,
    threshold: f64,
) -> Result<Vec<([f64; 2], f64)>> {
    ensure!(!frames.is_empty(), "No frames to track");
    let first = &frames[0];
    let mut matcher = Matcher::new(first, seed, radius, search, threshold)?;
    let mut out = vec![(seed, 1.)];
    for frame in &frames[1..] {
        if let Some(p) = matcher.next(frame)? {
            out.push(p);
        } else {
            break;
        }
    }
    Ok(out)
}
struct Matcher {
    patch: Patch,
    position: [i32; 2],
    velocity: [i32; 2],
    search: i32,
    threshold: f64,
    width: u32,
    height: u32,
}
impl Matcher {
    fn new(
        first: &GrayFrame,
        seed: [f64; 2],
        radius: u32,
        search: u32,
        threshold: f64,
    ) -> Result<Self> {
        ensure!(
            (3..=20).contains(&radius)
                && (2..=40).contains(&search)
                && threshold.is_finite()
                && (0.1..=0.99).contains(&threshold)
                && seed
                    .iter()
                    .all(|n| n.is_finite() && (0. ..=100.).contains(n)),
            "Invalid tracking settings"
        );
        ensure!(
            first.width > 0
                && first.height > 0
                && first.pixels.len() == (first.width * first.height) as usize,
            "Invalid tracking frame"
        );
        let position = [
            (seed[0] / 100. * first.width as f64).round() as i32,
            (seed[1] / 100. * first.height as f64).round() as i32,
        ];
        let patch = Patch::new(first, position, radius as i32)?;
        Ok(Self {
            patch,
            position,
            velocity: [0, 0],
            search: search as i32,
            threshold,
            width: first.width,
            height: first.height,
        })
    }
    fn next(&mut self, frame: &GrayFrame) -> Result<Option<([f64; 2], f64)>> {
        ensure!(
            frame.width == self.width
                && frame.height == self.height
                && frame.pixels.len() == (self.width * self.height) as usize,
            "Tracking frame size changed"
        );
        let predicted = [
            self.position[0] + self.velocity[0],
            self.position[1] + self.velocity[1],
        ];
        let mut best = (-2., self.position);
        for y in (predicted[1] - self.search..=predicted[1] + self.search).step_by(2) {
            for x in (predicted[0] - self.search..=predicted[0] + self.search).step_by(2) {
                if let Some(score) = self.patch.score(frame, [x, y])
                    && score > best.0
                {
                    best = (score, [x, y]);
                }
            }
        }
        let center = best.1;
        for y in center[1] - 2..=center[1] + 2 {
            for x in center[0] - 2..=center[0] + 2 {
                if let Some(score) = self.patch.score(frame, [x, y])
                    && score > best.0
                {
                    best = (score, [x, y]);
                }
            }
        }
        if best.0 < self.threshold {
            return Ok(None);
        }
        self.velocity = [
            (best.1[0] - self.position[0]).clamp(-self.search, self.search),
            (best.1[1] - self.position[1]).clamp(-self.search, self.search),
        ];
        self.position = best.1;
        Ok(Some((
            [
                best.1[0] as f64 / self.width as f64 * 100.,
                best.1[1] as f64 / self.height as f64 * 100.,
            ],
            best.0,
        )))
    }
}
pub fn analyze(
    asset: &Asset,
    source: &Clip,
    settings: &Settings,
    log: &Path,
    group: &ProcessGroup,
    mut progress: impl FnMut(f64),
) -> Result<Track> {
    ensure!(source.kind == Kind::Video, "Tracking needs a video clip");
    ensure!(
        settings.start.is_finite()
            && settings.duration.is_finite()
            && settings.start >= 0.
            && settings.duration > 0.
            && settings.duration <= 20.
            && settings.start + settings.duration <= source.duration + 1e-8,
        "Track a range of at most 20 seconds inside the source clip"
    );
    let (w, h, source_width, source_height) = dimensions(asset, group)?;
    let fps = 10_f64.min(198. / settings.duration);
    let count = (settings.duration * fps - 1e-8).ceil() as usize;
    ensure!(
        count >= 2,
        "Choose a tracking range of at least two analysis samples (about 0.2 seconds)"
    );
    let child = group.spawn(
        command("ffmpeg")
            .args([
                "-v",
                "error",
                "-nostdin",
                "-ss",
                &(source.source_in + settings.start * source.speed).to_string(),
                "-t",
                &(settings.duration * source.speed).to_string(),
                "-threads",
                "2",
                "-i",
            ])
            .arg(&asset.path)
            .args([
                "-vf",
                &format!(
                    "setpts=(PTS-STARTPTS)/{},fps={fps},scale={w}:{h},format=gray",
                    source.speed
                ),
                "-frames:v",
                &count.to_string(),
                "-f",
                "rawvideo",
                "-pix_fmt",
                "gray",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(log)?)
            .stdin(Stdio::null()),
    )?;
    let result = (|| -> Result<Track> {
        let mut stream = child
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stdout
            .take()
            .context("No tracking decoder stream")?;
        let mut frame = GrayFrame {
            width: w,
            height: h,
            pixels: vec![0; (w * h) as usize],
        };
        stream
            .read_exact(&mut frame.pixels)
            .context("Could not decode tracking frame")?;
        let mut matcher = Matcher::new(
            &frame,
            settings.seed,
            settings.patch_radius,
            settings.search_radius,
            settings.min_confidence,
        )?;
        let mut points = vec![TrackPoint {
            time: source.start + settings.start,
            position: settings.seed,
            confidence: 1.,
        }];
        let mut lost = false;
        for i in 1..count {
            group.check()?;
            stream
                .read_exact(&mut frame.pixels)
                .context("Source ended before the tracking range completed")?;
            if let Some((position, confidence)) = matcher.next(&frame)? {
                points.push(TrackPoint {
                    time: source.start + settings.start + i as f64 / fps,
                    position,
                    confidence,
                });
            } else {
                lost = true;
                break;
            }
            progress(i as f64 / count as f64);
        }
        group.check()?;
        ensure!(
            points.len() >= 2,
            "Feature was lost immediately; choose a clearer feature or a smaller time range"
        );
        Ok(Track {
            source: source.clone(),
            width: source_width,
            height: source_height,
            points,
            lost,
        })
    })();
    {
        let mut p = child.child.lock().unwrap_or_else(|e| e.into_inner());
        let _ = p.kill();
        let _ = p.wait();
    }
    result
}
fn canvas_position(track: &Track, p: &TrackPoint, project: &Project) -> [f64; 2] {
    let scale = (project.width as f64 / track.width as f64)
        .min(project.height as f64 / track.height as f64);
    let t = track.source.value(p.time - track.source.start);
    let x = (p.position[0] / 100. - 0.5) * track.width as f64 * scale * t.scale;
    let y = (p.position[1] / 100. - 0.5) * track.height as f64 * scale * t.scale;
    let (sin, cos) = t.rotation.to_radians().sin_cos();
    [
        t.x + (x * cos - y * sin) / project.width as f64 * 100.,
        t.y + (x * sin + y * cos) / project.height as f64 * 100.,
    ]
}
pub fn apply(
    track: &Track,
    target: &mut Clip,
    project: &Project,
    smoothing: usize,
    preserve_offset: bool,
) -> Result<()> {
    ensure!(
        target.is_visual() && track.points.len() >= 2 && smoothing <= 4,
        "Invalid tracking target or smoothing"
    );
    ensure!(
        track.source.mask == crate::model::Mask::None,
        "Apply tracking from an unmasked source clip"
    );
    let positions: Vec<_> = track
        .points
        .iter()
        .map(|p| canvas_position(track, p, project))
        .collect();
    let smoothed: Vec<[f64; 2]> = (0..positions.len())
        .map(|i| {
            let lo = i.saturating_sub(smoothing);
            let hi = (i + smoothing + 1).min(positions.len());
            let n = (hi - lo) as f64;
            [
                positions[lo..hi].iter().map(|p| p[0]).sum::<f64>() / n,
                positions[lo..hi].iter().map(|p| p[1]).sum::<f64>() / n,
            ]
        })
        .collect();
    let first = track
        .points
        .iter()
        .position(|p| (0. ..=target.duration).contains(&(p.time - target.start)))
        .context("Target and track do not overlap")?;
    let initial = target.value(track.points[first].time - target.start);
    let offset = if preserve_offset {
        [
            initial.x - smoothed[first][0],
            initial.y - smoothed[first][1],
        ]
    } else {
        [0., 0.]
    };
    let mut generated = vec![];
    for (i, p) in track.points.iter().enumerate() {
        let local = p.time - target.start;
        if !(0. ..=target.duration).contains(&local) {
            continue;
        }
        let x = smoothed[i][0] + offset[0];
        let y = smoothed[i][1] + offset[1];
        ensure!(
            (-100. ..=200.).contains(&x) && (-100. ..=200.).contains(&y),
            "Tracked layer would leave the supported position range"
        );
        let mut value = target.value(local);
        value.x = x;
        value.y = y;
        generated.push(Keyframe {
            time: local,
            transform: value,
            ..Keyframe::default()
        });
    }
    ensure!(
        generated.len() >= 2,
        "Target and tracked time ranges do not overlap"
    );
    let first = generated[0].time;
    let last = generated.last().unwrap().time;
    let mut keys: Vec<_> = target
        .keyframes
        .iter()
        .filter(|k| k.time < first || k.time > last)
        .cloned()
        .collect();
    keys.extend(generated);
    ensure!(
        keys.len() <= 200,
        "Tracking plus existing keys exceeds the 200-key limit"
    );
    keys.sort_by(|a, b| a.time.total_cmp(&b.time));
    target.keyframes = keys;
    Ok(())
}
