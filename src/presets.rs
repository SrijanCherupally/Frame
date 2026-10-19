//! Reusable canvas and motion presets. Motion is baked into ordinary editable keyframes.
use crate::model::{Clip, Graphic, Keyframe, Project};

pub struct FormatPreset {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub description: &'static str,
}
pub const FORMATS: &[FormatPreset] = &[
    FormatPreset {
        name: "YouTube · Full HD",
        width: 1920,
        height: 1080,
        fps: 30,
        description: "Everyday videos and build montages",
    },
    FormatPreset {
        name: "YouTube · Full HD 60",
        width: 1920,
        height: 1080,
        fps: 60,
        description: "Smooth action and rocket launches",
    },
    FormatPreset {
        name: "YouTube · 4K",
        width: 3840,
        height: 2160,
        fps: 30,
        description: "Detailed builds on a large canvas",
    },
    FormatPreset {
        name: "YouTube · 4K 60",
        width: 3840,
        height: 2160,
        fps: 60,
        description: "High resolution action",
    },
    FormatPreset {
        name: "Cinematic · Full HD",
        width: 1920,
        height: 1080,
        fps: 24,
        description: "Film cadence for storytelling",
    },
    FormatPreset {
        name: "Cinematic · 4K",
        width: 3840,
        height: 2160,
        fps: 24,
        description: "Film cadence in UHD",
    },
    FormatPreset {
        name: "Shorts / Reels",
        width: 1080,
        height: 1920,
        fps: 30,
        description: "Vertical videos for phones",
    },
    FormatPreset {
        name: "Shorts / Reels · 60",
        width: 1080,
        height: 1920,
        fps: 60,
        description: "Vertical action footage",
    },
    FormatPreset {
        name: "Social · Square",
        width: 1080,
        height: 1080,
        fps: 30,
        description: "Square posts and explainers",
    },
    FormatPreset {
        name: "Social · Portrait",
        width: 1080,
        height: 1350,
        fps: 30,
        description: "4:5 portrait posts",
    },
    FormatPreset {
        name: "HD · Lightweight",
        width: 1280,
        height: 720,
        fps: 30,
        description: "Smaller exports and quick drafts",
    },
    FormatPreset {
        name: "PAL · Full HD",
        width: 1920,
        height: 1080,
        fps: 25,
        description: "25 fps camera workflows",
    },
];
impl FormatPreset {
    pub fn apply(&self, project: &mut Project) -> anyhow::Result<()> {
        let mut next = project.clone();
        next.width = self.width;
        next.height = self.height;
        next.fps = self.fps;
        next.validate()?;
        *project = next;
        Ok(())
    }
}
pub const GRAPHICS: &[(Graphic, &str, &str)] = &[
    (Graphic::Arrow, "Direction arrow", "THRUST"),
    (Graphic::Circle, "Highlight ellipse", "Focus"),
    (Graphic::Rectangle, "Focus frame", "Focus"),
    (
        Graphic::LowerThird,
        "Lower third",
        "FLIGHT TEST 01\nSolid motor development",
    ),
    (
        Graphic::Callout,
        "Leader callout",
        "CARBON FIBER\nLight. Strong. Flight ready.",
    ),
    (Graphic::Dimension, "Measurement", "AIRFRAME  /  1.2 m"),
    (Graphic::Axes, "Coordinate axes", "VELOCITY / TIME"),
    (Graphic::Grid, "Engineering grid", "Engineering grid"),
    (Graphic::Step, "Numbered step", "01\nPREPARE THE AIRFRAME"),
    (Graphic::Stat, "Telemetry card", "APOGEE\n1,250 m"),
    (Graphic::Progress, "Progress indicator", "BUILD PROGRESS"),
    (Graphic::Rocket, "Rocket diagram", "Rocket schematic"),
    (
        Graphic::Equation,
        "Equation card",
        "F = m × a\nForce = mass × acceleration",
    ),
    (
        Graphic::Comparison,
        "Comparison bars",
        "SIMULATION\nFLIGHT DATA",
    ),
];
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    Fade,
    Rise,
    SlideLeft,
    SlideRight,
    Pop,
    ZoomIn,
    ZoomOut,
    Float,
    Spin,
}
pub const MOTIONS: &[(Motion, &str, &str)] = &[
    (Motion::Fade, "Soft reveal", "Fade in, hold, fade out"),
    (Motion::Rise, "Rise & settle", "Smooth upward entrance"),
    (
        Motion::SlideLeft,
        "Slide from left",
        "Bring labels into the scene",
    ),
    (
        Motion::SlideRight,
        "Slide from right",
        "An opposing entrance",
    ),
    (
        Motion::Pop,
        "Pop emphasis",
        "A small overshoot for emphasis",
    ),
    (Motion::ZoomIn, "Slow push in", "A gentle documentary zoom"),
    (
        Motion::ZoomOut,
        "Slow pull back",
        "Reveal the wider picture",
    ),
    (
        Motion::Float,
        "Floating label",
        "A subtle up and down motion",
    ),
    (Motion::Spin, "Turn & reveal", "A rotating entrance"),
];
impl Motion {
    pub fn apply(self, c: &mut Clip, fps: u32) {
        let base = c.transform.clone();
        // Sample only actual frame times. These remain ordinary linear keyframes,
        // preserving their path under splitting, trimming, retiming, and export.
        let frames = (c.duration * fps as f64).floor().max(1.) as u64;
        let mut frame_times = std::collections::BTreeSet::from([0, frames]);
        if matches!(self, Self::ZoomIn | Self::ZoomOut | Self::Float) {
            let samples = frames.min(48);
            frame_times.extend((0..=samples).map(|i| i * frames / samples));
        } else {
            let entrance_frames = ((c.duration * 0.25).min(0.8) * fps as f64).ceil().max(1.) as u64;
            let entrance_frames = entrance_frames.min(frames);
            frame_times.extend(0..=entrance_frames);
            if self == Self::Fade {
                frame_times.extend((0..=entrance_frames).map(|i| frames - i));
            }
        }
        c.keyframes.clear();
        for frame in frame_times {
            let t = frame as f64 / fps as f64;
            let u = t / c.duration;
            let entrance = (t / (c.duration * 0.25).min(0.8)).min(1.);
            let ease = 1. - (1. - entrance).powi(3);
            let mut v = base.clone();
            match self {
                Self::Fade => {
                    v.opacity *= entrance
                        .min(((c.duration - t) / (c.duration * 0.25).min(0.8)).clamp(0., 1.))
                }
                Self::Rise => {
                    v.y += 18. * (1. - ease);
                    v.opacity *= ease;
                }
                Self::SlideLeft => {
                    v.x -= 35. * (1. - ease);
                    v.opacity *= ease;
                }
                Self::SlideRight => {
                    v.x += 35. * (1. - ease);
                    v.opacity *= ease;
                }
                Self::Pop => {
                    let x = entrance - 1.;
                    let overshoot = 1. + 2.70158 * x.powi(3) + 1.70158 * x.powi(2);
                    v.scale *= 0.6 + 0.4 * overshoot;
                    v.opacity *= ease;
                }
                Self::ZoomIn => v.scale *= 1. + u * 0.15,
                Self::ZoomOut => v.scale *= 1.15 - u * 0.15,
                Self::Float => v.y -= 3. * (u * std::f64::consts::TAU).sin(),
                Self::Spin => {
                    v.rotation -= 45. * (1. - ease);
                    v.opacity *= ease;
                }
            }
            v.x = v.x.clamp(-100., 200.);
            v.y = v.y.clamp(-100., 200.);
            v.scale = v.scale.clamp(0.05, 4.);
            v.rotation = v.rotation.clamp(-360., 360.);
            c.keyframes.push(Keyframe {
                time: t,
                transform: v,
            });
            // Remove a middle sample when the same straight segment already expresses it.
            while c.keyframes.len() >= 3 {
                let n = c.keyframes.len();
                let (a, b, d) = (
                    &c.keyframes[n - 3],
                    &c.keyframes[n - 2],
                    &c.keyframes[n - 1],
                );
                let f = (b.time - a.time) / (d.time - a.time);
                let straight = [
                    (a.transform.x, b.transform.x, d.transform.x),
                    (a.transform.y, b.transform.y, d.transform.y),
                    (a.transform.scale, b.transform.scale, d.transform.scale),
                    (
                        a.transform.rotation,
                        b.transform.rotation,
                        d.transform.rotation,
                    ),
                    (
                        a.transform.opacity,
                        b.transform.opacity,
                        d.transform.opacity,
                    ),
                ]
                .iter()
                .all(|(a, b, d)| (a + (d - a) * f - b).abs() < 1e-10);
                if !straight {
                    break;
                }
                c.keyframes.remove(n - 2);
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum Scene {
    Forces,
    BuildStep,
    FlightData,
}
pub const SCENES: &[(Scene, &str, &str)] = &[
    (
        Scene::Forces,
        "Rocket forces",
        "Rocket schematic, thrust arrow and labels",
    ),
    (
        Scene::BuildStep,
        "Build chapter",
        "An animated chapter title and progress bar",
    ),
    (
        Scene::FlightData,
        "Flight report",
        "An apogee card with comparison bars",
    ),
];
impl Scene {
    pub fn clips(self, start: f64, fps: u32) -> Vec<Clip> {
        use crate::model::{Kind, Transform};
        let graphic = |shape, text: &str, x, y, scale| Clip {
            kind: Kind::Graphic,
            graphic: shape,
            text: text.into(),
            name: text.lines().next().unwrap_or("Diagram").into(),
            start,
            duration: 6.,
            track: 0,
            color: "#c6e99d".into(),
            font_size: 48,
            transform: Transform {
                x,
                y,
                scale,
                ..Transform::default()
            },
            ..Clip::default()
        };
        let mut clips = match self {
            Self::Forces => {
                let rocket = graphic(Graphic::Rocket, "Rocket airframe", 40., 50., 0.9);
                let mut arrow = graphic(Graphic::Arrow, "Thrust vector", 63., 50., 0.55);
                arrow.transform.rotation = -90.;
                let mut label = graphic(Graphic::Arrow, "THRUST\nF = m × a", 78., 50., 0.8);
                label.kind = Kind::Title;
                vec![rocket, arrow, label]
            }
            Self::BuildStep => vec![
                graphic(Graphic::Step, "01\nAIRFRAME ASSEMBLY", 50., 45., 1.),
                graphic(Graphic::Progress, "BUILD PROGRESS", 50., 76., 0.8),
            ],
            Self::FlightData => vec![
                graphic(Graphic::Stat, "APOGEE\n1,250 m", 26., 50., 0.8),
                graphic(
                    Graphic::Comparison,
                    "PREDICTED ALTITUDE\nMEASURED ALTITUDE",
                    69.,
                    50.,
                    0.7,
                ),
            ],
        };
        for c in &mut clips {
            Motion::Rise.apply(c, fps);
        }
        clips
    }
}
