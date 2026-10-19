use crate::{
    model::{Kind, uid},
    process::{ProcessGroup, command},
    storage::atomic_json,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub path: PathBuf,
    pub proxy_path: Option<PathBuf>,
    pub duration: f64,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
    pub status: String,
    pub size: u64,
    pub demo: bool,
    pub error: Option<String>,
}
impl Default for Asset {
    fn default() -> Self {
        Self {
            id: uid(),
            name: "Media".into(),
            kind: Kind::Video,
            path: PathBuf::new(),
            proxy_path: None,
            duration: 0.,
            fps: 30.,
            width: 0,
            height: 0,
            has_audio: false,
            status: "processing".into(),
            size: 0,
            demo: false,
            error: None,
        }
    }
}
impl Asset {
    pub fn preview_path(&self) -> &Path {
        self.proxy_path
            .as_deref()
            .filter(|p| p.is_file())
            .unwrap_or(&self.path)
    }
    pub fn ready(&self) -> bool {
        matches!(self.status.as_str(), "ready" | "proxying")
    }
}
pub fn probe(source: &Path, group: &ProcessGroup) -> Result<Asset> {
    let source = source
        .canonicalize()
        .context("Original file is unavailable; reconnect its drive or relink it")?;
    let bytes = group.run(
        command("ffprobe")
            .args([
                "-v",
                "error",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
            ])
            .arg(&source),
        None,
    )?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let streams = value["streams"]
        .as_array()
        .context("No media streams found")?;
    let video = streams.iter().find(|v| v["codec_type"] == "video");
    let audio = streams.iter().any(|v| v["codec_type"] == "audio");
    ensure!(
        video.is_some() || audio,
        "No playable video or audio stream"
    );
    let ext = source
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let image = matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff"
    );
    let duration = value["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(if image { 5. } else { 0. });
    ensure!(
        image || (duration > 0. && duration <= 86400.),
        "Media must have a finite duration up to 24 hours"
    );
    let fps = video
        .and_then(|v| v["avg_frame_rate"].as_str())
        .and_then(|s| s.split_once('/'))
        .and_then(|(a, b)| Some(a.parse::<f64>().ok()? / b.parse::<f64>().ok()?))
        .filter(|n| n.is_finite() && *n > 0.)
        .unwrap_or(30.);
    Ok(Asset {
        name: source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        size: fs::metadata(&source)?.len(),
        path: source,
        kind: if image {
            Kind::Image
        } else if video.is_some() {
            Kind::Video
        } else {
            Kind::Audio
        },
        duration,
        fps,
        width: video.and_then(|v| v["width"].as_u64()).unwrap_or(0) as u32,
        height: video.and_then(|v| v["height"].as_u64()).unwrap_or(0) as u32,
        has_audio: audio,
        ..Asset::default()
    })
}
pub fn prepare(
    mut a: Asset,
    root: &Path,
    group: &ProcessGroup,
    mut update: impl FnMut(Asset),
) -> Result<Asset> {
    let dir = root.join("media").join(&a.id);
    fs::create_dir_all(&dir)?;
    if a.kind != Kind::Audio {
        group.run(
            command("ffmpeg")
                .args(["-v", "error", "-y", "-threads", "2", "-i"])
                .arg(&a.path)
                .args(["-frames:v", "1", "-vf", "scale=480:-2", "-threads", "2"])
                .arg(dir.join("thumb.jpg")),
            Some(&dir.join("prepare.log")),
        )?;
    }
    a.status = if a.kind == Kind::Image {
        "ready"
    } else {
        "proxying"
    }
    .into();
    atomic_json(&dir.join("meta.json"), &a)?;
    update(a.clone());
    if a.kind == Kind::Video {
        let proxy = dir.join("proxy.mp4");
        group.run(
            command("ffmpeg")
                .args(["-v", "error", "-y", "-threads", "2", "-i"])
                .arg(&a.path)
                .args([
                    "-map",
                    "0:v:0",
                    "-map",
                    "0:a:0?",
                    "-vf",
                    "scale=960:540:force_original_aspect_ratio=decrease:force_divisible_by=2",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "veryfast",
                    "-crf",
                    "27",
                    "-threads",
                    "2",
                    "-c:a",
                    "aac",
                    "-ac",
                    "2",
                    "-movflags",
                    "+faststart",
                ])
                .arg(&proxy),
            Some(&dir.join("prepare.log")),
        )?;
        a.proxy_path = Some(proxy);
    } else if a.kind == Kind::Audio {
        let proxy = dir.join("proxy.m4a");
        group.run(
            command("ffmpeg")
                .args(["-v", "error", "-y", "-i"])
                .arg(&a.path)
                .args(["-vn", "-c:a", "aac", "-b:a", "128k", "-threads", "2"])
                .arg(&proxy),
            Some(&dir.join("prepare.log")),
        )?;
        a.proxy_path = Some(proxy);
    }
    a.status = "ready".into();
    a.error = None;
    atomic_json(&dir.join("meta.json"), &a)?;
    Ok(a)
}
pub fn seed(root: &Path) -> Result<Vec<Asset>> {
    let mut assets = vec![];
    for (id, name, data) in [
        (
            "demo-0",
            "Launch day",
            include_bytes!("../assets/demo/launch.png").as_slice(),
        ),
        (
            "demo-1",
            "The build",
            include_bytes!("../assets/demo/build.png").as_slice(),
        ),
        (
            "demo-2",
            "Into the sky",
            include_bytes!("../assets/demo/flight.png").as_slice(),
        ),
    ] {
        let dir = root.join("media").join(id);
        fs::create_dir_all(&dir)?;
        let path = dir.join("original.png");
        if !path.exists() {
            fs::write(&path, data)?;
        }
        let thumb = dir.join("thumb.jpg");
        if !thumb.exists() {
            image::load_from_memory(data)?
                .thumbnail(480, 270)
                .to_rgb8()
                .save(&thumb)?;
        }
        let a = Asset {
            id: id.into(),
            name: format!("{name}.png"),
            path,
            kind: Kind::Image,
            duration: 6.,
            fps: 30.,
            width: 1280,
            height: 720,
            status: "ready".into(),
            size: data.len() as u64,
            demo: true,
            ..Asset::default()
        };
        if !dir.join("meta.json").exists() {
            atomic_json(&dir.join("meta.json"), &a)?;
        }
        assets.push(a);
    }
    Ok(assets)
}
