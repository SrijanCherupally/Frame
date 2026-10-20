//! Machine-local choices: projects remain portable across GPU vendors.
use crate::process::{ProcessGroup, command};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Encoder {
    #[default]
    Auto,
    Nvidia,
    Cpu,
}
impl Encoder {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatic · prefer NVIDIA",
            Self::Nvidia => "NVIDIA NVENC · required",
            Self::Cpu => "CPU · H.264 software",
        }
    }
    pub fn args(self) -> &'static [&'static str] {
        match self {
            Self::Nvidia => &[
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p5",
                "-tune",
                "hq",
                "-rc",
                "vbr",
                "-cq",
                "20",
                "-b:v",
                "0",
                "-profile:v",
                "high",
                "-pix_fmt",
                "yuv420p",
            ],
            _ => &[
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-threads", "2",
                "-pix_fmt", "yuv420p",
            ],
        }
    }
    pub fn active_label(self) -> &'static str {
        if self == Self::Nvidia {
            "NVIDIA NVENC"
        } else {
            "CPU H.264"
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThreeD {
    #[default]
    Auto,
    Cpu,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub encoder: Encoder,
    pub scene3d: ThreeD,
}
impl Preferences {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("acceleration.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
    pub fn save(&self, root: &Path) -> Result<()> {
        crate::storage::atomic_json(&root.join("acceleration.json"), self)
    }
    pub fn apply(self) {
        crate::gpu3d::set_enabled(self.scene3d == ThreeD::Auto);
    }
}
/// Encode actual frames: `-encoders` alone does not establish working hardware.
/// Runs before expensive procedural source generation and remains cancellable.
pub fn probe_nvenc(group: &ProcessGroup) -> Result<()> {
    group.run_timeout(
        command("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=128x128:r=30",
                "-frames:v",
                "2",
                "-an",
            ])
            .args(Encoder::Nvidia.args())
            .args(["-f", "null", "-"]),
        None,
        std::time::Duration::from_secs(15),
    )?;
    Ok(())
}
pub fn choose(requested: Encoder, group: &ProcessGroup) -> Result<(Encoder, Option<String>)> {
    group.check()?;
    if requested == Encoder::Cpu {
        return Ok((Encoder::Cpu, None));
    }
    match probe_nvenc(group) {
        Ok(()) => Ok((Encoder::Nvidia, None)),
        Err(e) => {
            group.check()?;
            if requested == Encoder::Nvidia {
                anyhow::bail!(
                    "NVIDIA NVENC is unavailable. Install a current NVIDIA driver and use the complete Frame ZIP with NVENC-enabled FFmpeg, or choose Automatic/CPU. {e}"
                );
            }
            Ok((
                Encoder::Cpu,
                Some(format!("NVIDIA probe failed; using CPU: {e}")),
            ))
        }
    }
}
pub fn diagnostics(group: &ProcessGroup) -> Result<String> {
    let nvenc = match probe_nvenc(group) {
        Ok(()) => "NVIDIA NVENC: real test encode passed".to_owned(),
        Err(e) => {
            group.check()?;
            format!("NVIDIA NVENC unavailable: {e}")
        }
    };
    group.check()?;
    Ok(format!(
        "{nvenc}\n3D: {}",
        crate::gpu3d::diagnostics(group)?
    ))
}
