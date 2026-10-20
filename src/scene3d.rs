//! Small, self-contained 3D scenes with perspective, depth testing and flat lighting.
use crate::{animation::Easing, process::ProcessGroup};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
pub type V3 = [f64; 3];
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pose {
    pub position: V3,
    pub rotation: V3,
    pub scale: f64,
}
impl Default for Pose {
    fn default() -> Self {
        Self {
            position: [0.; 3],
            rotation: [0.; 3],
            scale: 1.,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoseKey {
    pub time: f64,
    pub pose: Pose,
    #[serde(default)]
    pub easing: Easing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Primitive {
    Cube,
    Sphere,
    Cylinder,
    Rocket,
    Mesh,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    pub vertices: Vec<V3>,
    pub triangles: Vec<[usize; 3]>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Object {
    pub name: String,
    pub primitive: Primitive,
    pub pose: Pose,
    pub spin: V3,
    pub color: String,
    pub mesh: Mesh,
    pub keys: Vec<PoseKey>,
}
impl Default for Object {
    fn default() -> Self {
        Self {
            name: "Rocket".into(),
            primitive: Primitive::Rocket,
            pose: Pose::default(),
            spin: [0., 25., 0.],
            color: "#79baff".into(),
            mesh: Mesh::default(),
            keys: vec![],
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Camera {
    pub yaw: f64,
    pub pitch: f64,
    pub distance: f64,
    pub fov: f64,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 25.,
            pitch: 15.,
            distance: 6.,
            fov: 45.,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraKey {
    pub time: f64,
    pub camera: Camera,
    #[serde(default)]
    pub easing: Easing,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scene {
    pub objects: Vec<Object>,
    pub camera: Camera,
    pub camera_keys: Vec<CameraKey>,
    pub orbit_speed: f64,
    pub ambient: f64,
    pub wireframe: bool,
    pub antialias: bool,
}
impl Default for Scene {
    fn default() -> Self {
        Self {
            objects: vec![Object::default()],
            camera: Camera::default(),
            camera_keys: vec![],
            orbit_speed: 0.,
            ambient: 0.3,
            wireframe: false,
            antialias: true,
        }
    }
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, t: f64) -> V3 {
    [a[0] * t, a[1] * t, a[2] * t]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn mix(a: V3, b: V3, t: f64) -> V3 {
    add(a, mul(sub(b, a), t))
}
fn rotate(mut p: V3, r: V3) -> V3 {
    for (axis, angle) in r.into_iter().enumerate() {
        let (s, c) = angle.to_radians().sin_cos();
        let (i, j) = match axis {
            0 => (1, 2),
            1 => (2, 0),
            _ => (0, 1),
        };
        let (a, b) = (p[i], p[j]);
        p[i] = a * c - b * s;
        p[j] = a * s + b * c;
    }
    p
}
fn basis(rotation: V3) -> [V3; 3] {
    [
        rotate([1., 0., 0.], rotation),
        rotate([0., 1., 0.], rotation),
        rotate([0., 0., 1.], rotation),
    ]
}
fn transform(axes: [V3; 3], v: V3) -> V3 {
    add(
        add(mul(axes[0], v[0]), mul(axes[1], v[1])),
        mul(axes[2], v[2]),
    )
}
impl Pose {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.position
                .iter()
                .all(|n| n.is_finite() && n.abs() <= 10.)
                && self
                    .rotation
                    .iter()
                    .all(|n| n.is_finite() && n.abs() <= 3600.)
                && self.scale.is_finite()
                && (0.01..=5.).contains(&self.scale),
            "Invalid 3D object pose"
        );
        Ok(())
    }
}
impl Camera {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.yaw, self.pitch, self.distance, self.fov]
                .iter()
                .all(|n| n.is_finite())
                && self.yaw.abs() <= 3600.
                && self.pitch.abs() <= 89.
                && (2. ..=30.).contains(&self.distance)
                && (15. ..=100.).contains(&self.fov),
            "Invalid 3D camera"
        );
        Ok(())
    }
    pub fn view(&self, p: V3) -> V3 {
        let p = rotate(p, [0., -self.yaw, 0.]);
        let mut p = rotate(p, [-self.pitch, 0., 0.]);
        p[2] += self.distance;
        p
    }
    pub fn project(&self, p: V3, w: f64, h: f64) -> Option<[f64; 2]> {
        let p = self.view(p);
        if p[2] < 0.1 {
            return None;
        }
        let f = h / (2. * (self.fov.to_radians() / 2.).tan());
        Some([w / 2. + p[0] * f / p[2], h / 2. - p[1] * f / p[2]])
    }
}
impl Object {
    pub fn at(&self, time: f64) -> Pose {
        let mut p = self.pose.clone();
        if let Some(first) = self.keys.first() {
            p = first.pose.clone();
            for pair in self.keys.windows(2) {
                if time >= pair[0].time && time <= pair[1].time {
                    let t = pair[0]
                        .easing
                        .sample((time - pair[0].time) / (pair[1].time - pair[0].time));
                    p = Pose {
                        position: mix(pair[0].pose.position, pair[1].pose.position, t),
                        rotation: mix(pair[0].pose.rotation, pair[1].pose.rotation, t),
                        scale: pair[0].pose.scale + (pair[1].pose.scale - pair[0].pose.scale) * t,
                    };
                    break;
                }
                if time > pair[1].time {
                    p = pair[1].pose.clone();
                }
            }
        }
        p.rotation = add(p.rotation, mul(self.spin, time));
        p
    }
    pub fn world(&self, v: V3, time: f64) -> V3 {
        let p = self.at(time);
        add(rotate(mul(v, p.scale), p.rotation), p.position)
    }
    pub fn geometry(&self) -> Mesh {
        match self.primitive {
            Primitive::Mesh => self.mesh.clone(),
            Primitive::Cube => Mesh {
                vertices: vec![
                    [-0.7, -0.7, -0.7],
                    [0.7, -0.7, -0.7],
                    [0.7, 0.7, -0.7],
                    [-0.7, 0.7, -0.7],
                    [-0.7, -0.7, 0.7],
                    [0.7, -0.7, 0.7],
                    [0.7, 0.7, 0.7],
                    [-0.7, 0.7, 0.7],
                ],
                triangles: vec![
                    [0, 2, 1],
                    [0, 3, 2],
                    [4, 5, 6],
                    [4, 6, 7],
                    [0, 1, 5],
                    [0, 5, 4],
                    [3, 7, 6],
                    [3, 6, 2],
                    [0, 4, 7],
                    [0, 7, 3],
                    [1, 2, 6],
                    [1, 6, 5],
                ],
            },
            Primitive::Sphere => {
                let mut mesh = Mesh::default();
                for j in 0..=12 {
                    let a = j as f64 / 12. * std::f64::consts::PI;
                    for i in 0..=20 {
                        let b = i as f64 / 20. * std::f64::consts::TAU;
                        mesh.vertices
                            .push([a.sin() * b.cos(), a.cos(), a.sin() * b.sin()]);
                    }
                }
                for j in 0..12 {
                    for i in 0..20 {
                        let a = j * 21 + i;
                        mesh.triangles
                            .extend([[a, a + 1, a + 21], [a + 1, a + 22, a + 21]]);
                    }
                }
                mesh
            }
            Primitive::Cylinder | Primitive::Rocket => {
                let rocket = self.primitive == Primitive::Rocket;
                let mut mesh = Mesh::default();
                let rings = if rocket {
                    vec![(-1.1, 0.32), (0.65, 0.32), (1.3, 0.0)]
                } else {
                    vec![(-0.8, 0.65), (0.8, 0.65)]
                };
                for &(y, r) in &rings {
                    for i in 0..24 {
                        let a = i as f64 / 24. * std::f64::consts::TAU;
                        mesh.vertices.push([r * a.cos(), y, r * a.sin()]);
                    }
                }
                for j in 0..rings.len() - 1 {
                    for i in 0..24 {
                        let a = j * 24 + i;
                        let b = j * 24 + (i + 1) % 24;
                        mesh.triangles.extend([[a, b, a + 24], [b, b + 24, a + 24]]);
                    }
                }
                let center = mesh.vertices.len();
                mesh.vertices.push([0., rings[0].0, 0.]);
                for i in 0..24 {
                    mesh.triangles.push([center, i, (i + 1) % 24]);
                }
                if rocket {
                    for i in 0..4 {
                        let a = i as f64 * std::f64::consts::FRAC_PI_2;
                        let k = mesh.vertices.len();
                        mesh.vertices.extend([
                            [0.25 * a.cos(), -0.25, 0.25 * a.sin()],
                            [0.75 * a.cos(), -1.2, 0.75 * a.sin()],
                            [0.25 * a.cos(), -1.0, 0.25 * a.sin()],
                        ]);
                        mesh.triangles.push([k, k + 1, k + 2]);
                    }
                } else {
                    let top = mesh.vertices.len();
                    mesh.vertices.push([0., 0.8, 0.]);
                    for i in 0..24 {
                        mesh.triangles.push([top, 24 + i, 24 + (i + 1) % 24]);
                    }
                }
                mesh
            }
        }
    }
}
impl Scene {
    pub fn camera_at(&self, time: f64) -> Camera {
        let mut c = self.camera.clone();
        if let Some(first) = self.camera_keys.first() {
            c = first.camera.clone();
            for pair in self.camera_keys.windows(2) {
                if time >= pair[0].time && time <= pair[1].time {
                    let t = pair[0]
                        .easing
                        .sample((time - pair[0].time) / (pair[1].time - pair[0].time));
                    let a = &pair[0].camera;
                    let b = &pair[1].camera;
                    c = Camera {
                        yaw: a.yaw + (b.yaw - a.yaw) * t,
                        pitch: a.pitch + (b.pitch - a.pitch) * t,
                        distance: a.distance + (b.distance - a.distance) * t,
                        fov: a.fov + (b.fov - a.fov) * t,
                    };
                    break;
                }
                if time > pair[1].time {
                    c = pair[1].camera.clone();
                }
            }
        }
        c.yaw += self.orbit_speed * time;
        c
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.objects.is_empty() && self.objects.len() <= 16,
            "3D scene needs 1–16 objects"
        );
        self.camera.validate()?;
        ensure!(
            self.orbit_speed.is_finite()
                && self.orbit_speed.abs() <= 90.
                && self.ambient.is_finite()
                && (0. ..=1.).contains(&self.ambient),
            "Invalid 3D lighting/orbit"
        );
        fn keys(times: impl Iterator<Item = f64>) -> Result<()> {
            let t: Vec<_> = times.collect();
            ensure!(
                t.len() <= 100
                    && t.iter()
                        .all(|t| t.is_finite() && (0. ..=86400.).contains(t))
                    && t.windows(2).all(|p| p[0] < p[1]),
                "Invalid 3D key times"
            );
            Ok(())
        }
        keys(self.camera_keys.iter().map(|k| k.time))?;
        for k in &self.camera_keys {
            k.camera.validate()?;
            k.easing.validate()?;
        }
        ensure!(
            self.objects
                .iter()
                .map(|o| o.mesh.triangles.len())
                .sum::<usize>()
                <= 8192,
            "Scene mesh budget: 8,192 triangles"
        );
        for o in &self.objects {
            ensure!(o.name.len() <= 200, "3D object name too long");
            o.pose.validate()?;
            ensure!(
                o.spin.iter().all(|n| n.is_finite() && n.abs() <= 360.),
                "Invalid 3D spin"
            );
            ensure!(valid_color(&o.color), "Invalid 3D material");
            keys(o.keys.iter().map(|k| k.time))?;
            for k in &o.keys {
                k.pose.validate()?;
                k.easing.validate()?;
            }
            ensure!(
                o.mesh.vertices.len() <= 2048 && o.mesh.triangles.len() <= 4096,
                "Mesh limit: 2,048 vertices / 4,096 triangles"
            );
            ensure!(
                o.mesh
                    .vertices
                    .iter()
                    .flatten()
                    .all(|n| n.is_finite() && n.abs() <= 10.),
                "Invalid mesh vertex"
            );
            ensure!(
                o.mesh
                    .triangles
                    .iter()
                    .flatten()
                    .all(|i| *i < o.mesh.vertices.len()),
                "Invalid mesh face"
            );
            if o.primitive == Primitive::Mesh {
                ensure!(
                    o.mesh.vertices.len() >= 3 && !o.mesh.triangles.is_empty(),
                    "Mesh is empty"
                );
            }
        }
        Ok(())
    }
    pub fn render(
        &self,
        time: f64,
        w: u32,
        h: u32,
        group: &ProcessGroup,
    ) -> Result<image::RgbaImage> {
        ensure!(
            w > 0 && h > 0 && w <= 3840 && h <= 3840 && time.is_finite(),
            "Invalid 3D render dimensions"
        );
        if self.antialias && w as u64 * h as u64 <= 1920 * 1080 {
            // Render at double resolution, resize premultiplied colors, then
            // unpremultiply so transparent edges do not acquire dark fringes.
            let mut high = self.render_inner(time, w * 2, h * 2, group)?;
            for p in high.pixels_mut() {
                for i in 0..3 {
                    p[i] = ((p[i] as u32 * p[3] as u32) / 255) as u8;
                }
            }
            let mut small =
                image::imageops::resize(&high, w, h, image::imageops::FilterType::Triangle);
            for p in small.pixels_mut() {
                if p[3] > 0 {
                    for i in 0..3 {
                        p[i] = ((p[i] as u32 * 255) / p[3] as u32).min(255) as u8;
                    }
                }
            }
            return Ok(small);
        }
        self.render_inner(time, w, h, group)
    }
    fn render_inner(
        &self,
        time: f64,
        w: u32,
        h: u32,
        group: &ProcessGroup,
    ) -> Result<image::RgbaImage> {
        self.validate()?;
        group.check()?;
        let mut image = image::RgbaImage::new(w, h);
        let mut depth = vec![f64::INFINITY; w as usize * h as usize];
        let camera = self.camera_at(time);
        let focal = h as f64 / (2. * (camera.fov.to_radians() / 2.).tan());
        let camera_axes = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
            .map(|v| sub(camera.view(v), [0., 0., camera.distance]));
        for object in &self.objects {
            group.check()?;
            let geometry = object.geometry();
            let pose = object.at(time);
            let axes = basis(pose.rotation);
            let world: Vec<_> = geometry
                .vertices
                .iter()
                .map(|v| add(transform(axes, mul(*v, pose.scale)), pose.position))
                .collect();
            let view: Vec<_> = world
                .iter()
                .map(|v| add(transform(camera_axes, *v), [0., 0., camera.distance]))
                .collect();
            let material = color(&object.color);
            for (face_index, face) in geometry.triangles.iter().enumerate() {
                if face_index % 64 == 0 {
                    group.check()?;
                }
                let a = world[face[0]];
                let b = world[face[1]];
                let c = world[face[2]];
                let normal = cross(sub(b, a), sub(c, a));
                let len = dot(normal, normal).sqrt();
                if len < 1e-12 {
                    continue;
                }
                let light = (dot(mul(normal, 1. / len), [-0.4, 0.5, -0.7681145748]).abs()
                    * (1. - self.ambient)
                    + self.ambient)
                    .clamp(0., 1.);
                let rgb = [
                    (material[0] as f64 * light) as u8,
                    (material[1] as f64 * light) as u8,
                    (material[2] as f64 * light) as u8,
                    255,
                ];
                let input = [view[face[0]], view[face[1]], view[face[2]]];
                let mut polygon = vec![];
                for i in 0..3 {
                    let p = input[i];
                    let q = input[(i + 1) % 3];
                    if p[2] >= 0.1 {
                        polygon.push(p);
                    }
                    if (p[2] >= 0.1) != (q[2] >= 0.1) {
                        polygon.push(mix(p, q, (0.1 - p[2]) / (q[2] - p[2])));
                    }
                }
                for i in 1..polygon.len().saturating_sub(1) {
                    let v = [polygon[0], polygon[i], polygon[i + 1]].map(|p| {
                        [
                            w as f64 / 2. + p[0] * focal / p[2],
                            h as f64 / 2. - p[1] * focal / p[2],
                            p[2],
                        ]
                    });
                    raster_triangle(&mut image, &mut depth, v, rgb, self.wireframe);
                }
            }
        }
        Ok(image)
    }
}
fn raster_triangle(
    image: &mut image::RgbaImage,
    depth: &mut [f64],
    p: [V3; 3],
    color: [u8; 4],
    wire: bool,
) {
    let edge =
        |a: V3, b: V3, x: f64, y: f64| (x - a[0]) * (b[1] - a[1]) - (y - a[1]) * (b[0] - a[0]);
    let area = edge(p[0], p[1], p[2][0], p[2][1]);
    if area.abs() < 1e-12 {
        return;
    }
    let w = image.width();
    let h = image.height();
    let xmin = p
        .iter()
        .map(|p| p[0])
        .fold(f64::INFINITY, f64::min)
        .floor()
        .clamp(0., w as f64) as u32;
    let xmax = p
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .clamp(0., w as f64) as u32;
    let ymin = p
        .iter()
        .map(|p| p[1])
        .fold(f64::INFINITY, f64::min)
        .floor()
        .clamp(0., h as f64) as u32;
    let ymax = p
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .clamp(0., h as f64) as u32;
    for y in ymin..ymax {
        for x in xmin..xmax {
            let a = edge(p[1], p[2], x as f64 + 0.5, y as f64 + 0.5) / area;
            let b = edge(p[2], p[0], x as f64 + 0.5, y as f64 + 0.5) / area;
            let c = 1. - a - b;
            if a >= 0. && b >= 0. && c >= 0. {
                if wire && a.min(b).min(c) > 0.025 {
                    continue;
                }
                let z = 1. / (a / p[0][2] + b / p[1][2] + c / p[2][2]);
                let i = (y * w + x) as usize;
                if z < depth[i] {
                    depth[i] = z;
                    image.put_pixel(x, y, image::Rgba(color));
                }
            }
        }
    }
}
pub fn color(s: &str) -> [u8; 3] {
    let n = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0xffffff);
    [(n >> 16) as u8, (n >> 8) as u8, n as u8]
}
pub fn valid_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn unpremultiply(pix: tiny_skia::Pixmap) -> image::RgbaImage {
    let (w, h) = (pix.width(), pix.height());
    let mut bytes = pix.take();
    for p in bytes.as_chunks_mut::<4>().0 {
        if p[3] > 0 {
            for i in 0..3 {
                p[i] = ((p[i] as u32 * 255) / p[3] as u32).min(255) as u8;
            }
        }
    }
    image::RgbaImage::from_raw(w, h, bytes).unwrap()
}
/// Import polygonal OBJ geometry, resolving negative indices and normalizing bounds.
pub fn parse_obj(text: &str) -> Result<Mesh> {
    ensure!(text.len() <= 1024 * 1024, "OBJ limit: 1 MiB");
    let mut mesh = Mesh::default();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut p = line.split_whitespace();
        match p.next() {
            Some("v") => {
                let v: Vec<f64> = p.map(str::parse).collect::<std::result::Result<_, _>>()?;
                ensure!(
                    v.len() >= 3 && v[..3].iter().all(|n| n.is_finite() && n.abs() < 1e12),
                    "Invalid OBJ vertex"
                );
                mesh.vertices.push([v[0], v[1], v[2]]);
                ensure!(mesh.vertices.len() <= 2048, "OBJ vertex limit: 2,048");
            }
            Some("f") => {
                let indices: Vec<usize> = p
                    .map(|s| -> Result<usize> {
                        let n = s.split('/').next().unwrap_or("").parse::<isize>()?;
                        let i = if n < 0 {
                            mesh.vertices.len() as isize + n
                        } else {
                            n - 1
                        };
                        ensure!(
                            i >= 0 && (i as usize) < mesh.vertices.len(),
                            "OBJ face references an unavailable vertex"
                        );
                        Ok(i as usize)
                    })
                    .collect::<Result<_>>()?;
                ensure!(
                    (3..=64).contains(&indices.len()),
                    "OBJ face needs 3–64 vertices"
                );
                for i in 1..indices.len() - 1 {
                    mesh.triangles
                        .push([indices[0], indices[i], indices[i + 1]]);
                }
                ensure!(mesh.triangles.len() <= 4096, "OBJ triangle limit: 4,096");
            }
            Some("curv" | "surf") => {
                bail!("OBJ curves/surfaces are unsupported; export triangulated mesh geometry")
            }
            _ => {}
        }
    }
    ensure!(
        mesh.vertices.len() >= 3 && !mesh.triangles.is_empty(),
        "OBJ contains no faces"
    );
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for v in &mesh.vertices {
        for i in 0..3 {
            min[i] = min[i].min(v[i]);
            max[i] = max[i].max(v[i]);
        }
    }
    let scale = (0..3).map(|i| max[i] - min[i]).fold(0., f64::max);
    ensure!(scale > 1e-12, "OBJ has zero-size bounds");
    for v in &mut mesh.vertices {
        for i in 0..3 {
            v[i] = (v[i] - (min[i] + max[i]) * 0.5) / scale * 2.;
        }
    }
    ensure!(
        mesh.triangles.iter().any(|f| {
            let n = cross(
                sub(mesh.vertices[f[1]], mesh.vertices[f[0]]),
                sub(mesh.vertices[f[2]], mesh.vertices[f[0]]),
            );
            dot(n, n) > 1e-16
        }),
        "OBJ contains only degenerate faces"
    );
    Ok(mesh)
}
