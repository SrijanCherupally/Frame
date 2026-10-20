//! Continuous, frame-rate-independent motion shared by editing, preview and export.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Easing {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    Smooth,
    Smoother,
    Hold,
    StepStart,
    Bezier {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
}
pub const EASINGS: &[(Easing, &str)] = &[
    (Easing::Linear, "Linear"),
    (Easing::EaseIn, "Accelerate"),
    (Easing::EaseOut, "Decelerate"),
    (Easing::Smooth, "Smooth ease in / out"),
    (Easing::Smoother, "Cinematic ease in / out"),
    (Easing::Hold, "Hold, then jump"),
    (Easing::StepStart, "Jump, then hold"),
    (
        Easing::Bezier {
            x1: 0.22,
            y1: 0.,
            x2: 0.36,
            y2: 1.,
        },
        "Custom Bézier",
    ),
];
fn bezier(t: f64, a: f64, b: f64) -> f64 {
    3. * (1. - t).powi(2) * t * a + 3. * (1. - t) * t * t * b + t.powi(3)
}
impl Easing {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bezier { .. } => "Custom Bézier",
            _ => EASINGS.iter().find(|(e, _)| *e == self).unwrap().1,
        }
    }
    pub fn sample(self, u: f64) -> f64 {
        let u = u.clamp(0., 1.);
        match self {
            Self::Linear => u,
            Self::EaseIn => u.powi(3),
            Self::EaseOut => 1. - (1. - u).powi(3),
            Self::Smooth => u * u * (3. - 2. * u),
            Self::Smoother => u.powi(3) * (u * (u * 6. - 15.) + 10.),
            Self::Hold => {
                if u < 1. {
                    0.
                } else {
                    1.
                }
            }
            Self::StepStart => {
                if u <= 0. {
                    0.
                } else {
                    1.
                }
            }
            Self::Bezier { x1, y1, x2, y2 } => {
                if u == 0. || u == 1. {
                    return u;
                }
                let (mut lo, mut hi) = (0., 1.);
                for _ in 0..24 {
                    let t = (lo + hi) * 0.5;
                    if bezier(t, x1, x2) < u {
                        lo = t;
                    } else {
                        hi = t;
                    }
                }
                bezier((lo + hi) * 0.5, y1, y2)
            }
        }
    }
    pub fn validate(self) -> Result<()> {
        if let Self::Bezier { x1, y1, x2, y2 } = self {
            ensure!(
                [x1, y1, x2, y2]
                    .iter()
                    .all(|n| n.is_finite() && (0. ..=1.).contains(n)),
                "Bézier handles must be finite and between zero and one"
            );
        }
        Ok(())
    }
    pub fn reversed(self) -> Self {
        match self {
            Self::EaseIn => Self::EaseOut,
            Self::EaseOut => Self::EaseIn,
            Self::Hold => Self::StepStart,
            Self::StepStart => Self::Hold,
            Self::Bezier { x1, y1, x2, y2 } => Self::Bezier {
                x1: 1. - x2,
                y1: 1. - y2,
                x2: 1. - x1,
                y2: 1. - y1,
            },
            other => other,
        }
    }
    /// FFmpeg evaluates this once per transform per frame, never per source pixel.
    pub fn expression(self, input: &str) -> String {
        let u = format!("clip(({input}),0,1)");
        match self {
            Self::Linear => u,
            Self::EaseIn => format!("pow({u},3)"),
            Self::EaseOut => format!("1-pow(1-({u}),3)"),
            Self::Smooth => format!("({u})*({u})*(3-2*({u}))"),
            Self::Smoother => format!("pow({u},3)*(({u})*(({u})*6-15)+10)"),
            Self::Hold => format!("gte({u},1)"),
            Self::StepStart => format!("gt({u},0)"),
            Self::Bezier { x1, y1, x2, y2 } => {
                // Bisection handles flat derivatives reliably; AVExpr registers are local
                // to this expression. Endpoint branches avoid epsilon drift at cuts.
                let mut out = format!("st(0,{u});st(1,0);st(2,1);");
                for _ in 0..24 {
                    out += &format!(
                        "st(3,(ld(1)+ld(2))/2);if(lt(3*pow(1-ld(3),2)*ld(3)*{x1}+3*(1-ld(3))*pow(ld(3),2)*{x2}+pow(ld(3),3),ld(0)),st(1,ld(3)),st(2,ld(3)));"
                    );
                }
                out += &format!(
                    "st(3,(ld(1)+ld(2))/2);if(lte(ld(0),0),0,if(gte(ld(0),1),1,3*pow(1-ld(3),2)*ld(3)*{y1}+3*(1-ld(3))*pow(ld(3),2)*{y2}+pow(ld(3),3)))"
                );
                out
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MotionPath {
    pub control1: [f64; 2],
    pub control2: [f64; 2],
}
fn mix(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
impl MotionPath {
    pub fn point(self, a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
        let (left, right) = self.split(a, b, t);
        mix(left.control2, right.control1, t)
    }
    pub fn split(self, a: [f64; 2], b: [f64; 2], t: f64) -> (Self, Self) {
        let p = mix(a, self.control1, t);
        let q = mix(self.control1, self.control2, t);
        let r = mix(self.control2, b, t);
        (
            Self {
                control1: p,
                control2: mix(p, q, t),
            },
            Self {
                control1: mix(q, r, t),
                control2: r,
            },
        )
    }
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.control1
                .iter()
                .chain(&self.control2)
                .all(|n| n.is_finite() && (-100. ..=200.).contains(n)),
            "Invalid motion path control point"
        );
        Ok(())
    }
}
pub fn default_domain() -> [f64; 2] {
    [0., 1.]
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reveal {
    #[default]
    None,
    LeftToRight,
    BottomToTop,
    Iris,
}

/// Two numeric columns with an optional time/value or x/y header. Values remain
/// embedded in the project, so a reopened scene does not need the original CSV.
pub fn parse_chart_csv(text: &str) -> Result<Vec<[f64; 2]>> {
    ensure!(text.len() <= 1024 * 1024, "Chart CSV must be at most 1 MiB");
    let mut points = vec![];
    for (i, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let columns: Vec<_> = line.split([',', '\t', ';']).map(str::trim).collect();
        ensure!(
            columns.len() == 2,
            "CSV row {} must have exactly two columns",
            i + 1
        );
        if points.is_empty()
            && matches!(
                columns[0].to_ascii_lowercase().as_str(),
                "time" | "x" | "seconds" | "timestamp"
            )
        {
            continue;
        }
        let x = columns[0]
            .parse::<f64>()
            .map_err(|_| anyhow::anyhow!("Invalid number at CSV row {}", i + 1))?;
        let y = columns[1]
            .parse::<f64>()
            .map_err(|_| anyhow::anyhow!("Invalid number at CSV row {}", i + 1))?;
        points.push([x, y]);
        ensure!(points.len() <= 2048, "Chart limit: 2,048 points");
    }
    ensure!(points.len() >= 2, "A chart needs at least two data rows");
    validate_chart(&points)?;
    Ok(points)
}
pub fn validate_chart(points: &[[f64; 2]]) -> Result<()> {
    ensure!(points.len() <= 2048, "Chart limit: 2,048 points");
    ensure!(
        points
            .iter()
            .flatten()
            .all(|n| n.is_finite() && n.abs() <= 1e12),
        "Chart values must be finite and within ±1e12"
    );
    ensure!(
        points.windows(2).all(|p| p[0][0] < p[1][0]),
        "Chart time/x values must increase strictly"
    );
    Ok(())
}

/// Sub-frame exposure clock, anchored to output-frame boundaries.
#[derive(Debug, Clone, Copy)]
pub struct Exposure {
    pub fps: u32,
    pub fraction: f64,
}
impl Default for Exposure {
    fn default() -> Self {
        Self {
            fps: 30,
            fraction: 1.,
        }
    }
}
impl Exposure {
    pub fn time(self, time: f64) -> f64 {
        if self.fraction == 1. {
            return time;
        }
        let n = time * self.fps as f64;
        let frame = (n + 1e-7).floor();
        (frame + (n - frame).max(0.) * self.fraction) / self.fps as f64
    }
    pub fn expression(self, clock: &str) -> String {
        if self.fraction == 1. {
            return clock.into();
        }
        format!(
            "((floor(({clock})*{}+0.0000001)+max(0,({clock})*{}-floor(({clock})*{}+0.0000001))*{})/{})",
            self.fps, self.fps, self.fps, self.fraction, self.fps
        )
    }
}
