//! Reusable formats, continuous motion and explanation scenes.
use crate::{
    animation::{Easing, MotionPath, Reveal},
    model::{Clip, Graphic, Keyframe, Project},
};

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
    (Graphic::Trajectory, "Flight trajectory", "FLIGHT PATH"),
    (Graphic::Orbit, "Orbital diagram", "ORBITAL MECHANICS"),
    (Graphic::Wave, "Signal waveform", "CONTROL SIGNAL"),
    (Graphic::Chart, "Data curve", "ALTITUDE / TIME"),
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
    Arc,
    Orbit,
    Pulse,
    Drift,
    Focus,
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
    (
        Motion::Arc,
        "Curved arrival",
        "A flowing Bézier motion path",
    ),
    (
        Motion::Orbit,
        "Orbital motion",
        "A continuous circular motion path",
    ),
    (Motion::Pulse, "Soft pulse", "A smooth emphasis beat"),
    (
        Motion::Drift,
        "Cinematic drift",
        "Gentle pan and push across the shot",
    ),
    (
        Motion::Focus,
        "Focus reveal",
        "A smooth scale and opacity entrance",
    ),
];
#[derive(Clone, Copy)]
pub struct MotionSettings {
    pub duration: f64,
    pub amount: f64,
    pub easing: Easing,
}
impl Default for MotionSettings {
    fn default() -> Self {
        Self {
            duration: 0.8,
            amount: 1.,
            easing: Easing::Smoother,
        }
    }
}
impl Motion {
    pub fn apply(self, c: &mut Clip, fps: u32) {
        self.apply_with(c, fps, MotionSettings::default());
    }
    pub fn apply_with(self, c: &mut Clip, fps: u32, settings: MotionSettings) {
        let base = c.transform.clone();
        let amount = settings.amount.clamp(0.05, 2.);
        let entrance = settings.duration.max(1. / fps as f64).min(c.duration);
        let ease = settings.easing;
        let mut keys = vec![];
        let mut add = |time: f64,
                       mut value: crate::model::Transform,
                       easing: Easing,
                       path: Option<MotionPath>| {
            value.x = value.x.clamp(-100., 200.);
            value.y = value.y.clamp(-100., 200.);
            value.scale = value.scale.clamp(0.05, 4.);
            value.rotation = value.rotation.clamp(-360., 360.);
            keys.push(Keyframe {
                time,
                transform: value,
                easing,
                path,
                ..Keyframe::default()
            });
        };
        match self {
            Self::ZoomIn | Self::ZoomOut | Self::Drift => {
                let mut a = base.clone();
                let mut b = base.clone();
                if self == Self::ZoomOut {
                    a.scale *= 1. + 0.15 * amount;
                } else {
                    b.scale *= 1. + 0.15 * amount;
                }
                if self == Self::Drift {
                    a.x -= 4. * amount;
                    b.x += 4. * amount;
                    a.y += 2. * amount;
                    b.y -= 2. * amount;
                }
                add(0., a, ease, None);
                add(c.duration, b, Easing::Linear, None);
            }
            Self::Float | Self::Pulse => {
                for (u, offset) in [(0., 0.), (0.25, 1.), (0.75, -1.), (1., 0.)] {
                    let mut value = base.clone();
                    if self == Self::Float {
                        value.y -= 3. * amount * offset;
                    } else {
                        value.scale *= 1. + 0.08 * amount * offset;
                    }
                    add(c.duration * u, value, Easing::Smooth, None);
                }
            }
            Self::Orbit => {
                let radius = 8. * amount;
                let k = radius * 0.5522847498;
                let points = [
                    (radius, 0.),
                    (0., radius),
                    (-radius, 0.),
                    (0., -radius),
                    (radius, 0.),
                ];
                let controls = [
                    ([radius, k], [k, radius]),
                    ([-k, radius], [-radius, k]),
                    ([-radius, -k], [-k, -radius]),
                    ([k, -radius], [radius, -k]),
                ];
                for (i, (x, y)) in points.into_iter().enumerate() {
                    let mut v = base.clone();
                    v.x += x;
                    v.y += y;
                    let path = controls.get(i).map(|(a, b)| MotionPath {
                        control1: [
                            (base.x + a[0]).clamp(-100., 200.),
                            (base.y + a[1]).clamp(-100., 200.),
                        ],
                        control2: [
                            (base.x + b[0]).clamp(-100., 200.),
                            (base.y + b[1]).clamp(-100., 200.),
                        ],
                    });
                    add(c.duration * i as f64 / 4., v, Easing::Linear, path);
                }
            }
            Self::Fade => {
                let t = entrance.min(c.duration / 2.);
                let mut invisible = base.clone();
                invisible.opacity = 0.;
                add(0., invisible.clone(), ease, None);
                add(t, base.clone(), Easing::Linear, None);
                if c.duration - t > t + 1e-9 {
                    add(c.duration - t, base.clone(), ease, None);
                } else if let Some(k) = keys.last_mut() {
                    k.easing = ease;
                }
                keys.push(Keyframe {
                    time: c.duration,
                    transform: invisible,
                    ..Keyframe::default()
                });
            }
            _ => {
                let mut from = base.clone();
                from.opacity = 0.;
                let mut path = None;
                match self {
                    Self::Rise => from.y += 18. * amount,
                    Self::SlideLeft => from.x -= 35. * amount,
                    Self::SlideRight => from.x += 35. * amount,
                    Self::Spin => from.rotation -= 45. * amount,
                    Self::Pop | Self::Focus => from.scale *= 1. - 0.4 * amount,
                    Self::Arc => {
                        from.x -= 35. * amount;
                        from.y += 15. * amount;
                        path = Some(MotionPath {
                            control1: [
                                (base.x - 30. * amount).clamp(-100., 200.),
                                (base.y - 30. * amount).clamp(-100., 200.),
                            ],
                            control2: [
                                (base.x - 10. * amount).clamp(-100., 200.),
                                (base.y - 20. * amount).clamp(-100., 200.),
                            ],
                        });
                    }
                    _ => {}
                }
                add(0., from, ease, path);
                if self == Self::Pop {
                    let mut overshoot = base.clone();
                    overshoot.scale *= 1. + 0.08 * amount;
                    add(entrance * 0.7, overshoot, Easing::Smooth, None);
                }
                add(entrance, base.clone(), Easing::Linear, None);
                if c.duration > entrance + 1e-9 {
                    add(c.duration, base.clone(), Easing::Linear, None);
                }
            }
        }
        keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        keys.dedup_by(|a, b| (a.time - b.time).abs() < 1e-10);
        c.keyframes = keys;
    }
}

#[derive(Clone, Copy)]
pub enum Scene {
    Forces,
    BuildStep,
    FlightData,
    Orbit,
    Trajectory,
    Signal,
    Comparison,
}
pub const SCENES: &[(Scene, &str, &str)] = &[
    (
        Scene::Orbit,
        "Orbital mechanics",
        "Staggered orbital diagram and explanatory label",
    ),
    (
        Scene::Trajectory,
        "Flight trajectory",
        "Revealing trajectory, telemetry and measurement",
    ),
    (
        Scene::Signal,
        "Signal and response",
        "Waveform and a smooth data-curve reveal",
    ),
    (
        Scene::Comparison,
        "Design comparison",
        "Coordinated side-by-side data cards",
    ),
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
            Self::Orbit => vec![
                graphic(Graphic::Orbit, "ORBITAL MECHANICS", 37., 48., 0.8),
                graphic(
                    Graphic::Callout,
                    "VELOCITY + GRAVITY\nA continuously falling trajectory",
                    70.,
                    40.,
                    0.65,
                ),
            ],
            Self::Trajectory => vec![
                graphic(Graphic::Trajectory, "CONTROLLED DESCENT", 40., 50., 0.85),
                graphic(Graphic::Stat, "LANDING ERROR\n0.4 m", 78., 45., 0.65),
            ],
            Self::Signal => vec![
                graphic(Graphic::Wave, "CONTROL INPUT", 50., 30., 0.8),
                graphic(Graphic::Chart, "ATTITUDE RESPONSE", 50., 72., 0.8),
            ],
            Self::Comparison => vec![
                graphic(Graphic::Stat, "DESIGN A\n124 g", 27., 45., 0.75),
                graphic(Graphic::Stat, "DESIGN B\n98 g", 73., 45., 0.75),
                graphic(
                    Graphic::Comparison,
                    "REFERENCE MASS\nOPTIMIZED MASS",
                    50.,
                    80.,
                    0.6,
                ),
            ],
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
        // Stagger entrances while keeping the scene's shared six-second end.
        for (i, c) in clips.iter_mut().enumerate() {
            let delay = (i as f64 * 0.22 * fps as f64).round() / fps as f64;
            c.start += delay;
            c.duration -= delay;
            if matches!(
                c.graphic,
                Graphic::Trajectory
                    | Graphic::Wave
                    | Graphic::Chart
                    | Graphic::Comparison
                    | Graphic::Progress
                    | Graphic::Orbit
            ) {
                c.reveal = Reveal::LeftToRight;
                c.reveal_duration = 1.6;
                Motion::Fade.apply(c, fps);
            } else {
                Motion::Rise.apply(c, fps);
            }
        }
        clips
    }
}
