//! Bounded, cancellable procedural sources. Stream one RGBA frame at a time into
//! a lossless temporary video; never retain an image sequence in RAM or on disk.
use crate::{
    model::Clip,
    process::{ProcessGroup, command},
};
use anyhow::{Context, Result, ensure};
use std::{io::Write, path::Path, process::Stdio};
pub fn image(
    c: &Clip,
    local: f64,
    w: u32,
    h: u32,
    group: &ProcessGroup,
) -> Result<image::RgbaImage> {
    ensure!(
        local.is_finite() && w > 0 && h > 0 && w <= 3840 && h <= 3840,
        "Invalid procedural render request"
    );
    let time = c.content_offset + local * c.content_rate;
    if let Some(vector) = &c.vector {
        vector.render(
            time,
            [w, h],
            crate::vector::Style {
                color: &c.color,
                secondary: &c.secondary_color,
                stroke: c.stroke_width,
                fill: c.fill,
            },
            group,
        )
    } else if let Some(scene) = &c.scene3d {
        scene.render(time, w, h, group)
    } else {
        anyhow::bail!("Layer has no procedural content")
    }
}
#[derive(Default)]
pub struct Window {
    pub phase: f64,
    pub duration: f64,
    pub fps: u32,
    pub size: [u32; 2],
    pub exposure: crate::animation::Exposure,
    pub start: f64,
}
pub fn write_source(c: &Clip, window: Window, path: &Path, group: &ProcessGroup) -> Result<()> {
    let Window {
        phase,
        duration,
        fps,
        size: [w, h],
        exposure,
        start,
    } = window;
    group.check()?;
    let log = path.with_extension("log");
    let child = group.spawn(
        command("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                &format!("{w}x{h}"),
                "-framerate",
                &fps.to_string(),
                "-i",
                "pipe:0",
                "-an",
                "-c:v",
                "ffv1",
                "-level",
                "3",
                "-pix_fmt",
                "bgra",
                "-threads",
                "2",
            ])
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&log)?),
    )?;
    let result = (|| -> Result<()> {
        let mut input = child
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stdin
            .take()
            .context("No procedural encoder input")?;
        let frames = (duration * fps as f64 - 1e-8).ceil().max(1.) as u32;
        for frame in 0..frames {
            group.check()?;
            let pixels = image(
                c,
                phase + exposure.time(start + frame as f64 / fps as f64) - start,
                w,
                h,
                group,
            )?;
            input
                .write_all(pixels.as_raw())
                .context("Procedural encoder stopped")?;
        }
        drop(input);
        // Poll instead of holding the process lock while waiting: cancellation must
        // be able to kill an encoder even when it has stopped consuming input.
        loop {
            group.check()?;
            if let Some(status) = child
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .try_wait()?
            {
                ensure!(
                    status.success(),
                    "Procedural encoding failed: {}",
                    std::fs::read_to_string(&log).unwrap_or_default()
                );
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok(())
    })();
    if result.is_err() {
        let mut p = child.child.lock().unwrap_or_else(|e| e.into_inner());
        let _ = p.kill();
        let _ = p.wait();
        let _ = std::fs::remove_file(path);
    }
    result
}
