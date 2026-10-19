use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub fn uid() -> String {
    uuid::Uuid::new_v4().to_string()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Video,
    Audio,
    Image,
    Title,
    Graphic,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transition {
    None,
    Fade,
    Slide,
    Zoom,
    Wipe,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mask {
    None,
    Circle,
    Rectangle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Graphic {
    Arrow,
    Circle,
    Rectangle,
    LowerThird,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 50.,
            y: 50.,
            scale: 1.,
            rotation: 0.,
            opacity: 1.,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub time: f64,
    #[serde(flatten)]
    pub transform: Transform,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    pub id: String,
    pub asset_id: Option<String>,
    pub kind: Kind,
    pub name: String,
    pub track: usize,
    pub start: f64,
    pub duration: f64,
    pub source_in: f64,
    pub speed: f64,
    pub volume: f64,
    #[serde(flatten)]
    pub transform: Transform,
    pub brightness: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub warmth: f64,
    pub blur: f64,
    pub motion_blur: bool,
    pub shutter: f64,
    pub deflicker: bool,
    pub mask: Mask,
    pub transition: Transition,
    pub fade_duration: f64,
    pub text: String,
    pub font_size: u32,
    pub color: String,
    pub graphic: Graphic,
    pub keyframes: Vec<Keyframe>,
}
impl Default for Clip {
    fn default() -> Self {
        Self {
            id: uid(),
            asset_id: None,
            kind: Kind::Video,
            name: "Clip".into(),
            track: 1,
            start: 0.,
            duration: 5.,
            source_in: 0.,
            speed: 1.,
            volume: 1.,
            transform: Transform::default(),
            brightness: 0.,
            contrast: 1.,
            saturation: 1.,
            warmth: 0.,
            blur: 0.,
            motion_blur: false,
            shutter: 0.5,
            deflicker: false,
            mask: Mask::None,
            transition: Transition::None,
            fade_duration: 0.5,
            text: "Your story starts here".into(),
            font_size: 64,
            color: "#ffffff".into(),
            graphic: Graphic::Arrow,
            keyframes: vec![],
        }
    }
}
impl Clip {
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }
    pub fn is_visual(&self) -> bool {
        self.kind != Kind::Audio
    }
    pub fn value(&self, time: f64) -> Transform {
        let mut keys: Vec<_> = self.keyframes.iter().collect();
        keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        if keys.is_empty() {
            return self.transform.clone();
        }
        if time <= keys[0].time {
            return keys[0].transform.clone();
        }
        for pair in keys.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if time <= b.time {
                let f = (time - a.time) / (b.time - a.time);
                return interpolate(&a.transform, &b.transform, f);
            }
        }
        keys.last().unwrap().transform.clone()
    }
    pub fn set_speed(&mut self, speed: f64, fps: u32) -> Result<()> {
        ensure!(
            speed.is_finite() && speed > 0.,
            "Speed must be a positive finite number"
        );
        let ratio = self.speed / speed;
        let duration = self.duration * ratio;
        ensure!(
            duration >= 1. / fps as f64 - 1e-8 && self.start + duration <= 86400.,
            "Output must last at least one frame and fit a 24-hour timeline"
        );
        self.duration = duration;
        self.speed = speed;
        for k in &mut self.keyframes {
            k.time *= ratio;
        }
        Ok(())
    }
}
fn interpolate(a: &Transform, b: &Transform, f: f64) -> Transform {
    Transform {
        x: a.x + (b.x - a.x) * f,
        y: a.y + (b.y - a.y) * f,
        scale: a.scale + (b.scale - a.scale) * f,
        rotation: a.rotation + (b.rotation - a.rotation) * f,
        opacity: a.opacity + (b.opacity - a.opacity) * f,
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub clips: Vec<Clip>,
    pub muted_tracks: Vec<usize>,
    pub hidden_tracks: Vec<usize>,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            version: 1,
            name: "Untitled project".into(),
            width: 1920,
            height: 1080,
            fps: 30,
            clips: vec![],
            muted_tracks: vec![],
            hidden_tracks: vec![],
        }
    }
}
impl Project {
    pub fn duration(&self) -> f64 {
        self.clips.iter().map(Clip::end).fold(0., f64::max)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported project version");
        ensure!(self.name.len() <= 800, "Project name too long");
        ensure!(
            (240..=3840).contains(&self.width)
                && (240..=3840).contains(&self.height)
                && self.width.is_multiple_of(2)
                && self.height.is_multiple_of(2),
            "Canvas must have even dimensions between 240 and 3840"
        );
        ensure!(
            (12..=60).contains(&self.fps),
            "Frame rate must be 12–60 fps"
        );
        ensure!(self.clips.len() <= 2000, "Project limit: 2,000 clips");
        let mut ids = HashSet::new();
        for c in &self.clips {
            ensure!(
                !c.id.is_empty() && c.id.len() <= 100 && ids.insert(&c.id),
                "Clip IDs must be unique and nonempty"
            );
            ensure!(c.track < 5, "Invalid track");
            for (name, n, lo, hi) in [
                ("Start", c.start, 0., 86400.),
                ("Duration", c.duration, 1. / self.fps as f64 - 1e-8, 86400.),
                ("Source in", c.source_in, 0., 86400.),
                ("Volume", c.volume, 0., 2.),
                ("Brightness", c.brightness, -1., 1.),
                ("Contrast", c.contrast, 0., 3.),
                ("Saturation", c.saturation, 0., 3.),
                ("Warmth", c.warmth, -1., 1.),
                ("Blur", c.blur, 0., 20.),
                ("Shutter", c.shutter, 0.1, 1.),
                ("Fade duration", c.fade_duration, 0.05, 5.),
            ] {
                ensure!(n.is_finite() && n >= lo && n <= hi, "{name} out of range");
            }
            ensure!(c.speed.is_finite() && c.speed > 0., "Invalid speed");
            ensure!(c.end() <= 86400. + 1e-8, "Timeline limit: 24 hours");
            validate_transform(&c.transform)?;
            ensure!(
                c.text.len() <= 12000 && c.name.len() <= 1200,
                "Text too long"
            );
            ensure!((12..=200).contains(&c.font_size), "Invalid font size");
            ensure!(
                c.color.len() == 7
                    && c.color.starts_with('#')
                    && c.color[1..].bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid color"
            );
            if matches!(c.kind, Kind::Video | Kind::Audio | Kind::Image) {
                ensure!(
                    c.asset_id.as_ref().is_some_and(|s| safe_id(s)),
                    "Missing or invalid media ID"
                );
            }
            ensure!(c.keyframes.len() <= 200, "Keyframe limit: 200");
            let mut times = HashSet::new();
            for k in &c.keyframes {
                ensure!(
                    k.time.is_finite()
                        && (0.0..=86400.0).contains(&k.time)
                        && times.insert(if k.time == 0. { 0 } else { k.time.to_bits() }),
                    "Invalid or duplicate keyframe time"
                );
                validate_transform(&k.transform)?;
            }
        }
        ensure!(
            self.muted_tracks
                .iter()
                .chain(&self.hidden_tracks)
                .all(|t| *t < 5),
            "Invalid track ID"
        );
        Ok(())
    }
}
pub fn validate_transform(t: &Transform) -> Result<()> {
    for (n, lo, hi) in [
        (t.x, -100., 200.),
        (t.y, -100., 200.),
        (t.scale, 0.05, 4.),
        (t.rotation, -360., 360.),
        (t.opacity, 0., 1.),
    ] {
        ensure!(n.is_finite() && n >= lo && n <= hi, "Invalid transform");
    }
    Ok(())
}
pub fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 100 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
pub fn timecode(t: f64, fps: u32) -> String {
    let t = t.max(0.);
    let s = t.floor() as u64;
    format!(
        "{:02}:{:02}:{:02}",
        s / 60,
        s % 60,
        ((t - t.floor()) * fps as f64 + 1e-5).floor() as u32
    )
}
pub const TRACKS: [&str; 5] = [
    "Titles & graphics",
    "Main footage",
    "B-roll",
    "Voiceover",
    "Music & sound",
];
