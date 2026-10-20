use crate::{
    media::Asset,
    model::Project,
    process::{ProcessGroup, command},
    render::{Mode, build_graph_cancel},
};
use crossbeam_channel::{Receiver, Sender, bounded};
use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(Debug)]
pub enum PreviewEvent {
    Frame {
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        time: f64,
    },
    Error(String),
    Finished,
}
pub struct Preview {
    rx: Receiver<(u64, PreviewEvent)>,
    group: ProcessGroup,
    workers: Vec<JoinHandle<()>>,
    generation: u64,
    tx: Sender<(u64, PreviewEvent)>,
}
impl Default for Preview {
    fn default() -> Self {
        let (tx, rx) = bounded(2);
        Self {
            rx,
            tx,
            group: ProcessGroup::default(),
            workers: vec![],
            generation: 0,
        }
    }
}
impl Preview {
    pub fn next_event(&self) -> Option<PreviewEvent> {
        while let Ok((generation, event)) = self.rx.try_recv() {
            if generation == self.generation {
                return Some(event);
            }
        }
        None
    }
    pub fn stop(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.group.cancel();
        while self.rx.try_recv().is_ok() {}
        self.reap();
    }
    fn reap(&mut self) {
        let mut i = 0;
        while i < self.workers.len() {
            if self.workers[i].is_finished() {
                let h = self.workers.swap_remove(i);
                let _ = h.join();
            } else {
                i += 1;
            }
        }
    }
    pub fn start(
        &mut self,
        mut p: Project,
        assets: Arc<HashMap<String, Asset>>,
        time: f64,
        playing: bool,
        root: PathBuf,
        audio: bool,
    ) {
        self.stop();
        self.group = ProcessGroup::default();
        let group = self.group.clone();
        let channel = self.tx.clone();
        let generation = self.generation;
        let tx = move |event, timeout| {
            channel
                .send_timeout((generation, event), timeout)
                .map_err(|e| match e {
                    crossbeam_channel::SendTimeoutError::Timeout((_, event)) => {
                        crossbeam_channel::SendTimeoutError::Timeout(event)
                    }
                    crossbeam_channel::SendTimeoutError::Disconnected((_, event)) => {
                        crossbeam_channel::SendTimeoutError::Disconnected(event)
                    }
                })
        };
        self.workers.push(thread::spawn(move || {
            let dir = root.join(format!("preview/{}", crate::model::uid()));
            let result = (|| -> anyhow::Result<()> {
                let ratio = p.height as f64 / p.width as f64;
                p.width = 640;
                p.height = ((640. * ratio).round() as u32 / 2 * 2).clamp(240, 1280);
                let duration = if playing {
                    (p.duration() - time).clamp(1. / p.fps as f64, 5.)
                } else {
                    1. / p.fps as f64
                };
                let graph = build_graph_cancel(
                    &p,
                    &assets,
                    &dir.join("video"),
                    time,
                    duration,
                    Mode::PreviewVideo,
                    &group,
                )?;
                let mut cmd = graph.command();
                cmd.args([
                    "-xerror", "-an", "-f", "rawvideo", "-pix_fmt", "rgb24", "-threads", "2", "pipe:1",
                ]);
                let errors = std::fs::File::create(dir.join("video.log"))?;
                let process = group.spawn(
                    cmd.stdout(Stdio::piped())
                        .stderr(errors)
                        .stdin(Stdio::null()),
                )?;
                let mut output = process
                    .child
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .stdout
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("No preview stream"))?;
                let mut audio_children = None;
                if playing
                    && audio
                    && p.clips.iter().any(|c| {
                        c.volume > 0.
                            && c.asset_id
                                .as_ref()
                                .and_then(|id| assets.get(id))
                                .is_some_and(|a| a.has_audio)
                    })
                {
                    // FFplay is an optional native audio output device; video and export never depend on it.
                    if let Ok(aud) = build_graph_cancel(
                        &p,
                        &assets,
                        &dir.join("audio"),
                        time,
                        duration,
                        Mode::PreviewAudio,
                        &group,
                    ) {
                        let mut cmd = aud.command();
                        cmd.args(["-vn", "-f", "s16le", "-ar", "48000", "-ac", "2", "pipe:1"]);
                        if let Ok(source) = group.spawn(
                            cmd.stdout(Stdio::piped())
                                .stderr(Stdio::null())
                                .stdin(Stdio::null()),
                        ) {
                            let stream = {
                                source
                                    .child
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .stdout
                                    .take()
                            };
                            if let Some(stream) = stream
                                && let Ok(sink) = group.spawn(
                                    command("ffplay")
                                        .args([
                                            "-nodisp",
                                            "-autoexit",
                                            "-loglevel",
                                            "error",
                                            "-f",
                                            "s16le",
                                            "-ar",
                                            "48000",
                                            "-ch_layout",
                                            "stereo",
                                            "-i",
                                            "pipe:0",
                                        ])
                                        .stdin(Stdio::from(stream))
                                        .stdout(Stdio::null())
                                        .stderr(Stdio::null()),
                                )
                            {
                                audio_children = Some((source, sink));
                            }
                        }
                    }
                }
                let mut i = 0;
                loop {
                    group.check()?;
                    let mut pixels = vec![0; (p.width * p.height * 3) as usize];
                    let mut filled = 0;
                    while filled < pixels.len() {
                        group.check()?;
                        match output.read(&mut pixels[filled..]) {
                            Ok(0) => break,
                            Ok(n) => filled += n,
                            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                            Err(e) => return Err(e.into()),
                        }
                    }
                    if filled == 0 { break; }
                    anyhow::ensure!(filled == pixels.len(), "Preview returned an incomplete RGB frame ({filled}/{} bytes)", pixels.len());
                    let mut event = PreviewEvent::Frame {
                        pixels,
                        width: p.width,
                        height: p.height,
                        time: time + i as f64 / p.fps as f64,
                    };
                    loop {
                        group.check()?;
                        match tx(event, Duration::from_millis(30)) {
                            Ok(()) => break,
                            Err(crossbeam_channel::SendTimeoutError::Timeout(e)) => event = e,
                            Err(_) => return Ok(()),
                        }
                    }
                    i += 1;
                }
                let code = process
                    .child
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .wait()?;
                group.check()?;
                if !code.success() {
                    anyhow::bail!("FFmpeg preview failed ({code}).\n{}", log_tail(&dir.join("video.log"), 16 * 1024));
                }
                anyhow::ensure!(i > 0, "FFmpeg produced no preview frames. Try another timeline position or relink the source.\n{}", log_tail(&dir.join("video.log"), 16 * 1024));
                drop(audio_children);
                Ok(())
            })();
            if !group.cancelled() {
                match result {
                    Ok(()) => {
                        let _ = tx(PreviewEvent::Finished, Duration::from_millis(100));
                    }
                    Err(e) => {
                        let _ = tx(
                            PreviewEvent::Error(preserve_failure(&root, &dir, &format!("{e:#}"))),
                            Duration::from_millis(100),
                        );
                    }
                }
            }
            group.cancel();
            let _ = std::fs::remove_dir_all(dir);
        }));
    }
}
impl Drop for Preview {
    fn drop(&mut self) {
        self.group.cancel();
        for h in self.workers.drain(..) {
            let _ = h.join();
        }
    }
}

/// Read only the bounded tail; FFmpeg progress logs can be much larger.
fn log_tail(path: &Path, limit: u64) -> String {
    let result = (|| -> std::io::Result<String> {
        let mut file = std::fs::File::open(path)?;
        let size = file.metadata()?.len();
        file.seek(SeekFrom::Start(size.saturating_sub(limit)))?;
        let mut bytes = Vec::new();
        file.take(limit).read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })();
    result.unwrap_or_default()
}
fn preserve_failure(root: &Path, dir: &Path, error: &str) -> String {
    let lower = error.to_lowercase();
    let png_unavailable = error.lines().any(|line| {
        let line = line.to_lowercase();
        line.contains("png")
            && (line.contains("decoder") || line.contains("decoding"))
            && (line.contains("not found")
                || line.contains("no decoder")
                || line.contains("not available"))
    });
    let summary = if png_unavailable {
        "The media tools cannot decode PNG images. Install the complete updated Frame ZIP."
    } else if lower.contains("no preview frames") {
        "No preview frames were produced. Try another timeline position or relink the source."
    } else if lower.contains("ffmpeg preview failed") {
        "The media tool could not render this preview. Open Details for the cause."
    } else {
        error
            .lines()
            .next()
            .unwrap_or("Preview could not be prepared")
    };
    let logs = root.join("diagnostics");
    let log = logs.join(format!(
        "preview-{}.log",
        dir.file_name().unwrap_or_default().to_string_lossy()
    ));
    let write = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&logs)?;
        let text = format!(
            "Frame preview failure\nFFmpeg: {}\n\n{error}\n\nFFmpeg output:\n{}\n\nFilter graph:\n{}",
            crate::process::tool("ffmpeg").display(),
            log_tail(&dir.join("video.log"), 16 * 1024),
            log_tail(&dir.join("video/filters.txt"), 32 * 1024)
        );
        // The error is bounded too; build-graph errors can include source names.
        std::fs::write(&log, text.chars().take(80 * 1024).collect::<String>())?;
        let mut old: Vec<_> = std::fs::read_dir(&logs)?
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name().to_string_lossy().starts_with("preview-")
                    && e.path().extension().is_some_and(|e| e == "log")
            })
            .collect();
        old.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        let remove = old.len().saturating_sub(5);
        for entry in old.into_iter().take(remove) {
            let _ = std::fs::remove_file(entry.path());
        }
        Ok(())
    })();
    let location = match write {
        Ok(()) => format!("Diagnostic log: {}", log.display()),
        Err(e) => format!("Could not save a diagnostic log: {e}"),
    };
    format!("Preview failed: {summary}\n\n{location}\n\n{error}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_codec_hint_does_not_confuse_other_png_inputs() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("job");
        let positive = super::preserve_failure(
            root.path(),
            &dir,
            "Decoding requested, but no decoder found for: png",
        );
        assert!(positive.contains("cannot decode PNG images"));
        let unrelated = super::preserve_failure(
            root.path(),
            &dir,
            "Input 0: original.png\nDecoding requested, but no decoder found for: hevc",
        );
        assert!(!unrelated.contains("cannot decode PNG images"));
        assert!(unrelated.contains("no decoder found for: hevc"));
    }
}
