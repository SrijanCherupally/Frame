//! Editable cubic paths, equal-topology morphing and arc-length stroke reveals.
use crate::{animation::Easing, process::ProcessGroup};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub point: [f64; 2],
    pub incoming: [f64; 2],
    pub outgoing: [f64; 2],
}
impl Node {
    pub fn corner(point: [f64; 2]) -> Self {
        Self {
            point,
            incoming: point,
            outgoing: point,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VectorShape {
    pub nodes: Vec<Node>,
    pub target: Vec<Node>,
    pub closed: bool,
    pub draw_duration: f64,
    pub morph_duration: f64,
    pub stroke_start: f64,
    pub stroke_end: f64,
}
impl Default for VectorShape {
    fn default() -> Self {
        Self {
            nodes: vec![
                Node {
                    point: [20., 75.],
                    incoming: [20., 75.],
                    outgoing: [30., 15.],
                },
                Node {
                    point: [80., 65.],
                    incoming: [65., 10.],
                    outgoing: [80., 65.],
                },
            ],
            target: vec![],
            closed: false,
            draw_duration: 1.5,
            morph_duration: 2.,
            stroke_start: 0.,
            stroke_end: 1.,
        }
    }
}
pub struct Style<'a> {
    pub color: &'a str,
    pub secondary: &'a str,
    pub stroke: f64,
    pub fill: bool,
}
impl VectorShape {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (2..=128).contains(&self.nodes.len()),
            "Vector path needs 2–128 nodes"
        );
        ensure!(
            self.target.is_empty() || self.target.len() == self.nodes.len(),
            "Morph paths must have the same node count"
        );
        for node in self.nodes.iter().chain(&self.target) {
            ensure!(
                node.point
                    .iter()
                    .chain(&node.incoming)
                    .chain(&node.outgoing)
                    .all(|v| v.is_finite() && (-100. ..=200.).contains(v)),
                "Invalid vector control point"
            );
        }
        ensure!(
            [self.draw_duration, self.morph_duration]
                .iter()
                .all(|n| n.is_finite() && (0. ..=86400.).contains(n)),
            "Invalid vector animation duration"
        );
        ensure!(
            self.stroke_start.is_finite()
                && self.stroke_end.is_finite()
                && 0. <= self.stroke_start
                && self.stroke_start <= self.stroke_end
                && self.stroke_end <= 1.,
            "Invalid stroke range"
        );
        Ok(())
    }
    pub fn at(&self, time: f64) -> Vec<Node> {
        if self.target.is_empty() {
            return self.nodes.clone();
        }
        let t = if self.morph_duration == 0. {
            1.
        } else {
            Easing::Smoother.sample(time / self.morph_duration)
        };
        let mix = |a: [f64; 2], b: [f64; 2]| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        self.nodes
            .iter()
            .zip(&self.target)
            .map(|(a, b)| Node {
                point: mix(a.point, b.point),
                incoming: mix(a.incoming, b.incoming),
                outgoing: mix(a.outgoing, b.outgoing),
            })
            .collect()
    }
    pub fn polyline(&self, time: f64) -> Vec<[f64; 2]> {
        self.points(time, 96)
    }
    fn points(&self, time: f64, steps: usize) -> Vec<[f64; 2]> {
        let nodes = self.at(time);
        let mut points = vec![nodes[0].point];
        for i in 0..nodes.len() - usize::from(!self.closed) {
            let a = &nodes[i];
            let b = &nodes[(i + 1) % nodes.len()];
            if a.outgoing == a.point && b.incoming == b.point {
                points.push(b.point);
                continue;
            }
            for j in 1..=steps {
                let t = j as f64 / steps as f64;
                let u = 1. - t;
                points.push([
                    u.powi(3) * a.point[0]
                        + 3. * u * u * t * a.outgoing[0]
                        + 3. * u * t * t * b.incoming[0]
                        + t.powi(3) * b.point[0],
                    u.powi(3) * a.point[1]
                        + 3. * u * u * t * a.outgoing[1]
                        + 3. * u * t * t * b.incoming[1]
                        + t.powi(3) * b.point[1],
                ]);
            }
        }
        points
    }
    pub fn render(
        &self,
        time: f64,
        size: [u32; 2],
        style: Style<'_>,
        group: &ProcessGroup,
    ) -> Result<image::RgbaImage> {
        let [w, h] = size;
        let Style {
            color,
            secondary,
            stroke,
            fill,
        } = style;
        group.check()?;
        ensure!(
            w > 0 && h > 0 && w <= 3840 && h <= 3840 && time.is_finite(),
            "Invalid vector canvas or time"
        );
        self.validate()?;
        let mut pix =
            tiny_skia::Pixmap::new(w, h).ok_or_else(|| anyhow::anyhow!("Invalid vector canvas"))?;
        let points: Vec<_> = self
            .points(time, (w.max(h) / 8).clamp(64, 512) as usize)
            .into_iter()
            .map(|p| [p[0] * w as f64 / 100., p[1] * h as f64 / 100.])
            .collect();
        let path = |points: &[[f64; 2]], close: bool| {
            let mut b = tiny_skia::PathBuilder::new();
            if let Some(p) = points.first() {
                b.move_to(p[0] as f32, p[1] as f32);
                for p in &points[1..] {
                    b.line_to(p[0] as f32, p[1] as f32);
                }
                if close {
                    b.close();
                }
            }
            b.finish()
        };
        let paint = |hex: &str| {
            let rgb = crate::scene3d::color(hex);
            let mut p = tiny_skia::Paint::default();
            p.set_color_rgba8(rgb[0], rgb[1], rgb[2], 255);
            p.anti_alias = true;
            p
        };
        if fill
            && self.closed
            && let Some(path) = path(&points, true)
        {
            pix.fill_path(
                &path,
                &paint(secondary),
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
        let progress = if self.draw_duration == 0. {
            1.
        } else {
            Easing::Smoother.sample(time / self.draw_duration)
        };
        let end = self.stroke_start + (self.stroke_end - self.stroke_start) * progress;
        let length: f64 = points
            .windows(2)
            .map(|p| (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1]))
            .sum();
        let (start, end) = (length * self.stroke_start, length * end);
        let mut distance = 0.;
        let mut visible = vec![];
        for pair in points.windows(2) {
            let len = (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
            if len > 1e-10 && distance + len > start && distance < end {
                let lerp = |f: f64| {
                    [
                        pair[0][0] + (pair[1][0] - pair[0][0]) * f,
                        pair[0][1] + (pair[1][1] - pair[0][1]) * f,
                    ]
                };
                if visible.is_empty() {
                    visible.push(lerp(((start - distance) / len).clamp(0., 1.)));
                }
                visible.push(lerp(((end - distance) / len).clamp(0., 1.)));
            }
            distance += len;
        }
        if let Some(path) = path(&visible, false) {
            pix.stroke_path(
                &path,
                &paint(color),
                &tiny_skia::Stroke {
                    width: (stroke as f32 * w as f32 / 1920.).max(0.5),
                    line_cap: tiny_skia::LineCap::Round,
                    line_join: tiny_skia::LineJoin::Round,
                    ..Default::default()
                },
                tiny_skia::Transform::identity(),
                None,
            );
        }
        Ok(crate::scene3d::unpremultiply(pix))
    }
    pub fn svg(&self) -> String {
        let mut d = format!("M {} {}", self.nodes[0].point[0], self.nodes[0].point[1]);
        for i in 0..self.nodes.len() - usize::from(!self.closed) {
            let a = &self.nodes[i];
            let b = &self.nodes[(i + 1) % self.nodes.len()];
            d += &format!(
                " C {} {} {} {} {} {}",
                a.outgoing[0], a.outgoing[1], b.incoming[0], b.incoming[1], b.point[0], b.point[1]
            );
        }
        if self.closed {
            d += " Z";
        }
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\"><path d=\"{d}\" fill=\"none\" stroke=\"#ffffff\" stroke-width=\"1\"/></svg>\n"
        )
    }
}
/// Import one SVG path's geometry; reject unsupported constructs instead of
/// silently producing different artwork. Styles stay editable in Frame.
pub fn parse_svg(text: &str) -> Result<VectorShape> {
    ensure!(text.len() <= 1024 * 1024, "SVG limit: 1 MiB");
    ensure!(
        !text.contains("<!DOCTYPE")
            && !text.contains("<script")
            && !text.contains("transform=")
            && !text.contains("transform ="),
        "SVG scripts, external entities and transforms are unsupported; flatten transforms first"
    );
    fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
        for (index, _) in tag.match_indices(name) {
            if index > 0 && !tag.as_bytes()[index - 1].is_ascii_whitespace() {
                continue;
            }
            let rest = tag[index + name.len()..].trim_start();
            let Some(rest) = rest.strip_prefix('=') else {
                continue;
            };
            let rest = rest.trim_start();
            let quote = rest.chars().next()?;
            if quote != '\'' && quote != '"' {
                return None;
            }
            let rest = &rest[1..];
            return Some(&rest[..rest.find(quote)?]);
        }
        None
    }
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    ensure!(
        !compact.contains("transform="),
        "Flatten SVG transforms before importing"
    );
    ensure!(
        ![
            "<rect",
            "<circle",
            "<ellipse",
            "<line",
            "<polyline",
            "<polygon",
            "<use",
            "<image",
            "<text"
        ]
        .iter()
        .any(|tag| text.contains(tag)),
        "Convert SVG objects to a single path before importing"
    );
    let path_start = text
        .find("<path")
        .ok_or_else(|| anyhow::anyhow!("SVG needs one path"))?;
    let tag = &text[path_start
        ..text[path_start..]
            .find('>')
            .map(|i| path_start + i)
            .ok_or_else(|| anyhow::anyhow!("Unclosed SVG path"))?];
    ensure!(
        !text[path_start + 5..].contains("<path"),
        "Import one path at a time"
    );
    let d = attr(tag, "d").ok_or_else(|| anyhow::anyhow!("SVG path has no d attribute"))?;
    let mut tokens = vec![];
    let bytes = d.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_whitespace() || c == ',' {
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() {
            tokens.push(c.to_string());
            i += 1;
            continue;
        }
        let start = i;
        if matches!(bytes[i], b'+' | b'-') {
            i += 1;
        }
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        if i < bytes.len() && matches!(bytes[i], b'e' | b'E') {
            i += 1;
            if i < bytes.len() && matches!(bytes[i], b'+' | b'-') {
                i += 1;
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        ensure!(i > start, "Invalid SVG path number");
        tokens.push(d[start..i].to_owned());
        ensure!(tokens.len() <= 8192, "SVG path too complex");
    }
    let mut nodes: Vec<Node> = vec![];
    let mut closed = false;
    let mut cursor = [0., 0.];
    let mut index = 0;
    let mut cmd = 'M';
    while index < tokens.len() {
        if tokens[index].len() == 1 && tokens[index].as_bytes()[0].is_ascii_alphabetic() {
            cmd = tokens[index].chars().next().unwrap();
            index += 1;
            if matches!(cmd, 'z' | 'Z') {
                closed = true;
                ensure!(index == tokens.len(), "Compound SVG paths are unsupported");
                break;
            }
        }
        let relative = cmd.is_ascii_lowercase();
        let upper = cmd.to_ascii_uppercase();
        let count = match upper {
            'M' | 'L' => 2,
            'H' | 'V' => 1,
            'C' => 6,
            'Q' => 4,
            _ => bail!("Unsupported SVG command {cmd}; convert to M/L/H/V/C/Q/Z geometry"),
        };
        ensure!(index + count <= tokens.len(), "Incomplete SVG command");
        let n: Vec<f64> = tokens[index..index + count]
            .iter()
            .map(|s| s.parse::<f64>().map_err(anyhow::Error::from))
            .collect::<Result<_>>()?;
        index += count;
        ensure!(n.iter().all(|n| n.is_finite()), "Invalid SVG coordinate");
        let xy = |a: f64, b: f64| {
            if relative {
                [cursor[0] + a, cursor[1] + b]
            } else {
                [a, b]
            }
        };
        match upper {
            'M' => {
                ensure!(nodes.is_empty(), "Compound SVG paths are unsupported");
                cursor = xy(n[0], n[1]);
                nodes.push(Node::corner(cursor));
                cmd = if relative { 'l' } else { 'L' };
            }
            'L' | 'H' | 'V' => {
                ensure!(!nodes.is_empty(), "Path must start with M");
                cursor = match upper {
                    'H' => [if relative { cursor[0] + n[0] } else { n[0] }, cursor[1]],
                    'V' => [cursor[0], if relative { cursor[1] + n[0] } else { n[0] }],
                    _ => xy(n[0], n[1]),
                };
                nodes.push(Node::corner(cursor));
            }
            'C' | 'Q' => {
                ensure!(!nodes.is_empty(), "Path must start with M");
                let (c1, c2, end) = if upper == 'C' {
                    (xy(n[0], n[1]), xy(n[2], n[3]), xy(n[4], n[5]))
                } else {
                    let q = xy(n[0], n[1]);
                    let end = xy(n[2], n[3]);
                    (
                        [
                            cursor[0] + (q[0] - cursor[0]) * 2. / 3.,
                            cursor[1] + (q[1] - cursor[1]) * 2. / 3.,
                        ],
                        [
                            end[0] + (q[0] - end[0]) * 2. / 3.,
                            end[1] + (q[1] - end[1]) * 2. / 3.,
                        ],
                        end,
                    )
                };
                nodes.last_mut().unwrap().outgoing = c1;
                nodes.push(Node {
                    point: end,
                    incoming: c2,
                    outgoing: end,
                });
                cursor = end;
            }
            _ => unreachable!(),
        }
        ensure!(nodes.len() <= 128, "Vector limit: 128 nodes");
    }
    if closed && nodes.len() > 2 && nodes.last().unwrap().point == nodes[0].point {
        let last = nodes.pop().unwrap();
        nodes[0].incoming = last.incoming;
    }
    let view = text
        .find("<svg")
        .and_then(|p| text[p..].find('>').map(|e| &text[p..p + e]))
        .and_then(|tag| attr(tag, "viewBox"));
    if let Some(view) = view {
        let v: Vec<f64> = view
            .split([',', ' ', '\t', '\n'])
            .filter(|s| !s.is_empty())
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        ensure!(
            v.len() == 4 && v.iter().all(|n| n.is_finite()) && v[2] > 0. && v[3] > 0.,
            "Invalid SVG viewBox"
        );
        for node in &mut nodes {
            for p in [&mut node.point, &mut node.incoming, &mut node.outgoing] {
                p[0] = (p[0] - v[0]) / v[2] * 100.;
                p[1] = (p[1] - v[1]) / v[3] * 100.;
            }
        }
    }
    let shape = VectorShape {
        nodes,
        closed,
        draw_duration: 0.,
        ..VectorShape::default()
    };
    shape.validate()?;
    Ok(shape)
}
