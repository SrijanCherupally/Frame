#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use eframe::egui;
use frame_studio::{
    process::{ProcessGroup, command},
    render,
    storage::{Storage, load_project},
    ui::FrameApp,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() {
    if let Err(e) = run() {
        eprintln!("Frame could not start: {e:#}");
        if std::env::args().len() == 1 {
            let _ = rfd::MessageDialog::new()
                .set_title("Frame could not start")
                .set_description(format!("{e:#}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        std::process::exit(1);
    }
}
fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--check-tools") {
        let group = ProcessGroup::default();
        for name in ["ffmpeg", "ffprobe"] {
            let bytes = group.run(command(name).arg("-version"), None)?;
            println!(
                "{}",
                String::from_utf8_lossy(&bytes)
                    .lines()
                    .next()
                    .unwrap_or("Unknown tool")
            );
        }
        return Ok(());
    }
    let storage = Storage::open(Storage::default_root())?;
    if args.first().map(String::as_str) == Some("--render") {
        anyhow::ensure!(
            args.len() == 3,
            "Usage: frame --render project.frame.json output.mp4"
        );
        let p = load_project(&PathBuf::from(&args[1]))?;
        let assets = storage.assets();
        let folder = storage.root.join("jobs").join(frame_studio::model::uid());
        render::export(
            &p,
            &assets,
            &folder,
            &PathBuf::from(&args[2]),
            &ProcessGroup::default(),
            |v| eprintln!("Export {:.0}%", v * 100.),
        )?;
        return Ok(());
    }
    let app = FrameApp::new(storage)?;
    let screenshot = if args.first().map(String::as_str) == Some("--ui-smoke") {
        anyhow::ensure!(args.len() == 2, "Usage: frame --ui-smoke screenshot.png");
        Some(PathBuf::from(&args[1]))
    } else {
        anyhow::ensure!(args.is_empty(), "Unknown command-line arguments");
        None
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Frame · Native Video Editor")
            .with_inner_size([1440., 960.])
            .with_min_inner_size([1050., 720.]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Frame",
        options,
        Box::new(move |cc| {
            FrameApp::configure(&cc.egui_ctx);
            Ok(Box::new(NativeApp {
                inner: app,
                screenshot,
                start: Instant::now(),
                requested: false,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}
struct NativeApp {
    inner: FrameApp,
    screenshot: Option<PathBuf>,
    start: Instant,
    requested: bool,
}
impl eframe::App for NativeApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.update(ctx, frame);
        if let Some(path) = &self.screenshot {
            if self.start.elapsed() > Duration::from_secs(4) && !self.requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.requested = true;
            }
            let screenshot = ctx.input(|input| {
                input.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot { image, .. } = event {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = screenshot {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                if let Err(e) = image::save_buffer(
                    path,
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                ) {
                    eprintln!("Could not write screenshot: {e}");
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.start.elapsed() > Duration::from_secs(20) {
                eprintln!("Native UI smoke test timed out");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
