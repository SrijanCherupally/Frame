//! Offscreen hardware rasterization for procedural 3D layers. A single reusable
//! device/target bounds VRAM use; CPU fallback is handled by Scene::render.
use crate::{process::ProcessGroup, scene3d::Scene};
use anyhow::{Context, Result, ensure};
use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 4],
    pub color: [f32; 4],
    pub barycentric: [f32; 3],
}
const SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) @interpolate(flat) color: vec4f,
    @location(1) @interpolate(linear) barycentric: vec3f,
};
@vertex fn vs(@location(0) position: vec4f, @location(1) color: vec4f,
             @location(2) barycentric: vec3f) -> VertexOutput {
    return VertexOutput(position, color, barycentric);
}
@fragment fn fs(v: VertexOutput) -> @location(0) vec4f {
    if v.color.a > 0.5 && min(min(v.barycentric.x, v.barycentric.y), v.barycentric.z) > 0.025 {
        discard;
    }
    return vec4f(v.color.rgb, 1.0);
}
"#;
static ENABLED: AtomicBool = AtomicBool::new(true);
#[derive(Default)]
struct State {
    renderer: Option<Renderer>,
    unavailable: Option<String>,
}
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
fn lock(group: &ProcessGroup) -> Result<std::sync::MutexGuard<'static, State>> {
    let mutex = STATE.get_or_init(|| Mutex::new(State::default()));
    loop {
        group.check()?;
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(e)) => return Ok(e.into_inner()),
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}
pub fn status() -> String {
    if !ENABLED.load(Ordering::Relaxed) {
        return "CPU selected for 3D".into();
    }
    let Some(state) = STATE.get().and_then(|m| m.try_lock().ok()) else {
        return "Automatic · GPU checked on first 3D frame".into();
    };
    if let Some(r) = &state.renderer {
        format!("GPU · {} ({:?})", r.info.name, r.info.backend)
    } else if let Some(reason) = &state.unavailable {
        format!("CPU fallback · {reason}")
    } else {
        "Automatic · GPU not checked yet".into()
    }
}
fn initialize(state: &mut State) {
    if state.renderer.is_some() || state.unavailable.is_some() {
        return;
    }
    match std::panic::catch_unwind(|| Renderer::new(false)) {
        Ok(Ok(renderer)) => state.renderer = Some(renderer),
        Ok(Err(e)) => state.unavailable = Some(format!("{e:#}")),
        Err(_) => state.unavailable = Some("Graphics driver initialization failed".into()),
    }
}
pub fn diagnostics(group: &ProcessGroup) -> Result<String> {
    if !ENABLED.load(Ordering::Relaxed) {
        return Ok(status());
    }
    let mut state = lock(group)?;
    initialize(&mut state);
    drop(state);
    group.check()?;
    Ok(status())
}
pub(crate) fn render(
    scene: &Scene,
    time: f64,
    w: u32,
    h: u32,
    group: &ProcessGroup,
) -> Result<Option<image::RgbaImage>> {
    group.check()?;
    if !ENABLED.load(Ordering::Relaxed) {
        return Ok(None);
    }
    let mut state = lock(group)?;
    initialize(&mut state);
    let Some(renderer) = &mut state.renderer else {
        group.check()?;
        return Ok(None);
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        renderer.render(scene, time, w, h, group)
    }));
    group.check()?;
    match result {
        Ok(Ok(image)) => Ok(Some(image)),
        error => {
            state.unavailable = Some(match error {
                Ok(Err(e)) => format!("GPU render failed: {e:#}"),
                _ => "GPU driver error during render".into(),
            });
            state.renderer = None;
            Ok(None)
        }
    }
}
struct Target {
    size: [u32; 2],
    color: wgpu::Texture,
    depth: wgpu::TextureView,
    readback: wgpu::Buffer,
    stride: u32,
}
pub(crate) struct Renderer {
    info: wgpu::AdapterInfo,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    capacity: u64,
    target: Option<Target>,
    error: Arc<Mutex<Option<String>>>,
}
impl Renderer {
    /// Software adapters are only enabled by the explicit GPU verification test.
    pub(crate) fn new(allow_software: bool) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12 | wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let mut adapters =
            instance.enumerate_adapters(wgpu::Backends::DX12 | wgpu::Backends::VULKAN);
        adapters.retain(|a| allow_software || a.get_info().device_type != wgpu::DeviceType::Cpu);
        adapters.sort_by_key(|a| {
            let i = a.get_info();
            (
                i.vendor != 0x10de,
                match i.device_type {
                    wgpu::DeviceType::DiscreteGpu => 0,
                    wgpu::DeviceType::IntegratedGpu => 1,
                    wgpu::DeviceType::VirtualGpu => 2,
                    _ => 3,
                },
            )
        });
        let mut errors = vec![];
        for adapter in adapters {
            match Self::from_adapter(adapter) {
                Ok(renderer) => return Ok(renderer),
                Err(e) => errors.push(e.to_string()),
            }
        }
        anyhow::bail!(
            "No usable hardware DX12/Vulkan adapter. {}",
            errors.join("; ")
        )
    }
    fn from_adapter(adapter: wgpu::Adapter) -> Result<Self> {
        let info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Frame offscreen 3D"),
            required_limits: wgpu::Limits {
                max_texture_dimension_2d: 7680,
                ..wgpu::Limits::downlevel_defaults()
            },
            ..Default::default()
        }))
        .context("Could not open GPU device")?;
        let error = Arc::new(Mutex::new(None));
        let errors = error.clone();
        device.on_uncaptured_error(Arc::new(move |e| {
            *errors.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("{e}"));
        }));
        let errors = error.clone();
        device.set_device_lost_callback(move |reason, message| {
            *errors.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(format!("Device lost ({reason:?}): {message}"));
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Frame shaded 3D / wireframe"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x3];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Frame 3D depth-tested triangles"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                }],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Frame reusable triangle buffer"),
            size: 4096,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if let Some(e) = error.lock().unwrap_or_else(|e| e.into_inner()).take() {
            anyhow::bail!("{e}");
        }
        Ok(Self {
            info,
            device,
            queue,
            pipeline,
            vertices,
            capacity: 4096,
            target: None,
            error,
        })
    }
    fn target(&mut self, w: u32, h: u32) {
        if self.target.as_ref().is_some_and(|t| t.size == [w, h]) {
            return;
        }
        // Release the previous full-resolution target before allocating its replacement.
        self.target = None;
        let texture = |format, usage| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Frame offscreen target"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = texture(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
        .create_view(&Default::default());
        let stride = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Frame reusable pixel readback"),
            size: stride as u64 * h as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        self.target = Some(Target {
            size: [w, h],
            color,
            depth,
            readback,
            stride,
        });
    }
    pub(crate) fn render(
        &mut self,
        scene: &Scene,
        time: f64,
        w: u32,
        h: u32,
        group: &ProcessGroup,
    ) -> Result<image::RgbaImage> {
        group.check()?;
        ensure!(
            w > 0 && h > 0 && w <= 7680 && h <= 7680,
            "Invalid GPU target"
        );
        if let Some(e) = self.error.lock().unwrap_or_else(|e| e.into_inner()).take() {
            anyhow::bail!("{e}");
        }
        let vertices = scene.gpu_vertices(time, w, h, group)?;
        let bytes = bytemuck::cast_slice(&vertices);
        if bytes.len() as u64 > self.capacity {
            self.capacity = (bytes.len() as u64).next_power_of_two();
            self.vertices = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Frame triangle buffer"),
                size: self.capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !bytes.is_empty() {
            self.queue.write_buffer(&self.vertices, 0, bytes);
        }
        self.target(w, h);
        let target = self.target.as_ref().unwrap();
        let view = target.color.create_view(&Default::default());
        let mut commands = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Frame shaded scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        commands.copy_texture_to_buffer(
            target.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &target.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(target.stride),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([commands.finish()]);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        target
            .readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
        let wait = (|| -> Result<()> {
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                group.check()?;
                self.device.poll(wgpu::PollType::Poll)?;
                match rx.try_recv() {
                    Ok(result) => {
                        result.context("GPU readback failed")?;
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        anyhow::bail!("GPU readback disconnected")
                    }
                    Err(_) => {
                        ensure!(Instant::now() < deadline, "GPU readback timed out");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            }
            if let Some(e) = self.error.lock().unwrap_or_else(|e| e.into_inner()).take() {
                anyhow::bail!("{e}");
            }
            Ok(())
        })();
        if let Err(e) = wait {
            target.readback.unmap();
            return Err(e);
        }
        let data = target.readback.slice(..).get_mapped_range();
        let mut output = image::RgbaImage::new(w, h);
        for (source, dest) in data
            .chunks_exact(target.stride as usize)
            .zip(output.as_mut().chunks_exact_mut(w as usize * 4))
        {
            dest.copy_from_slice(&source[..w as usize * 4]);
        }
        drop(data);
        target.readback.unmap();
        group.check()?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene3d::{Camera, Object, Pose, Primitive};
    #[test]
    #[ignore = "Requires a working Vulkan/DX12 adapter; explicitly run on hardware or Mesa lavapipe"]
    fn gpu_raster_matches_cpu_depth_clipping_wireframe_and_readback() {
        let mut gpu = Renderer::new(true).expect("GPU test adapter must be available");
        eprintln!(
            "Testing real GPU pipeline on {} ({:?})",
            gpu.info.name, gpu.info.backend
        );
        let group = ProcessGroup::default();
        let mut scene = Scene {
            antialias: false,
            camera: Camera {
                yaw: 0.,
                pitch: 0.,
                distance: 3.,
                ..Default::default()
            },
            ambient: 1.,
            objects: vec![
                Object {
                    primitive: Primitive::Cube,
                    color: "#ff3020".into(),
                    spin: [0.; 3],
                    ..Default::default()
                },
                Object {
                    primitive: Primitive::Cube,
                    color: "#2080ff".into(),
                    pose: Pose {
                        position: [0.35, 0., 1.],
                        ..Default::default()
                    },
                    spin: [0.; 3],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        for (w, h, wire, near, time) in [
            (319, 181, false, false, 0.),
            (320, 240, true, false, 0.),
            (257, 193, false, true, 0.),
            (301, 211, false, false, 1.),
        ] {
            scene.wireframe = wire;
            scene.objects[0].pose.position[2] = if near { -2.7 } else { 0. };
            scene.orbit_speed = if time > 0. { 17. } else { 0. };
            scene.ambient = if time > 0. { 0.35 } else { 1. };
            let actual = gpu.render(&scene, time, w, h, &group).unwrap();
            let expected = scene.render_cpu(time, w, h, &group).unwrap();
            assert_eq!(actual.get_pixel(0, 0)[3], expected.get_pixel(0, 0)[3]);
            let changed = actual
                .pixels()
                .zip(expected.pixels())
                .filter(|(a, b)| {
                    a.0.iter()
                        .zip(b.0)
                        .any(|(a, b)| (*a as i16 - b as i16).abs() > 2)
                })
                .count();
            assert!(
                changed < (w * h) as usize / 40,
                "CPU/GPU mismatch: {changed} pixels at {w}x{h}, wire={wire}, near={near}"
            );
            assert!(actual.pixels().any(|p| p[3] > 0));
        }
        let cancelled = ProcessGroup::default();
        cancelled.cancel();
        assert!(gpu.render(&scene, 0., 320, 240, &cancelled).is_err());
        // Cancellation must not leave the shared target mapped or unusable.
        assert!(gpu.render(&scene, 0., 320, 240, &group).is_ok());
    }
}
