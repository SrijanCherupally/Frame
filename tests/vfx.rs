use frame_studio::{
    animation::Easing,
    edit,
    model::*,
    process::ProcessGroup,
    render::{self, Mode},
    scene3d::{Object, Pose, PoseKey, Primitive, Scene, parse_obj},
    tracking::{self, GrayFrame},
    vector::{Node, VectorShape, parse_svg},
};
use std::collections::HashMap;
use tempfile::TempDir;
#[test]
fn vector_svg_morph_and_real_stroke_reveal() {
    let v =
        parse_svg("<svg viewBox=\"0 0 200 100\"><path d=\"M 20 80 C 40 0 150 0 180 70\"/></svg>")
            .unwrap();
    assert_eq!(v.nodes[0].point, [10., 80.]);
    assert_eq!(v.nodes[1].point, [90., 70.]);
    let round = parse_svg(&v.svg()).unwrap();
    assert_eq!(round.nodes, v.nodes);
    for text in [
        "<svg><path d=\"M0 0 A10 10 0 0 1 40 40\"/></svg>",
        "<svg><path d=\"M0 0 L1 1 M2 2 L3 3\"/></svg>",
        "<svg><path d=\"M0 0 C NaN 0 1 2 3 4\"/></svg>",
    ] {
        assert!(parse_svg(text).is_err());
    }
    let mut v = VectorShape::default();
    v.target = v.nodes.clone();
    v.target[1].point = [70., 30.];
    assert_eq!(v.at(0.), v.nodes);
    assert_eq!(v.at(2.), v.target);
    let draw = |t| {
        v.render(
            t,
            [320, 240],
            frame_studio::vector::Style {
                color: "#ffffff",
                secondary: "#123456",
                stroke: 20.,
                fill: false,
            },
            &ProcessGroup::default(),
        )
        .unwrap()
    };
    assert_eq!(draw(0.).pixels().filter(|p| p[3] > 0).count(), 0);
    assert!(draw(1.5).pixels().filter(|p| p[3] > 0).count() > 100);
    v.target.push(Node::corner([50., 50.]));
    assert!(v.validate().is_err());
}
#[test]
fn scene_geometry_depth_camera_animation_and_obj_validation() {
    let mesh = parse_obj("v -2 -2 0\nv 2 -2 0\nv 0 2 0\nf -3 -2 -1").unwrap();
    assert_eq!(mesh.vertices[0], [-1., -1., 0.]);
    assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
    assert!(parse_obj("v 0 0 0\nf 1 2 9").is_err());
    let mut scene = Scene::default();
    scene.objects[0].primitive = Primitive::Cube;
    scene.objects[0].keys = vec![
        PoseKey {
            time: 0.,
            pose: Pose::default(),
            easing: Easing::Smoother,
        },
        PoseKey {
            time: 2.,
            pose: Pose {
                position: [1., 0., 0.],
                ..Pose::default()
            },
            easing: Easing::Linear,
        },
    ];
    scene.validate().unwrap();
    assert!((scene.objects[0].at(1.).position[0] - 0.5).abs() < 1e-12);
    let a = scene
        .render(0., 320, 240, &ProcessGroup::default())
        .unwrap();
    let b = scene
        .render(1., 320, 240, &ProcessGroup::default())
        .unwrap();
    assert!(a.pixels().filter(|p| p[3] > 0).count() > 1000);
    assert_ne!(a, b);
    scene.objects.push(Object {
        primitive: Primitive::Sphere,
        color: "#ff0000".into(),
        pose: Pose {
            position: [0., 0., -1.],
            scale: 0.5,
            ..Pose::default()
        },
        spin: [0.; 3],
        ..Object::default()
    });
    assert!(scene.render(0., 320, 240, &ProcessGroup::default()).is_ok());
    scene.camera.distance = f64::NAN;
    assert!(scene.validate().is_err());
}
fn moving_patch(x: i32, y: i32, occluded: bool) -> GrayFrame {
    let mut pixels = vec![20; 100 * 80];
    if !occluded {
        for dy in -6_i32..=6 {
            for dx in -6_i32..=6 {
                pixels[((y + dy) * 100 + x + dx) as usize] =
                    if (dx * 13 + dy * 7).rem_euclid(11) < 5 {
                        240
                    } else {
                        70
                    };
            }
        }
    }
    GrayFrame {
        pixels,
        width: 100,
        height: 80,
    }
}
#[test]
fn tracker_follows_translation_and_stops_on_occlusion() {
    let frames: Vec<_> = (0..8)
        .map(|i| moving_patch(30 + i * 2, 35 + i, false))
        .collect();
    let track = tracking::track_frames(&frames, [30., 43.75], 6, 12, 0.7).unwrap();
    assert_eq!(track.len(), 8);
    assert!((track[7].0[0] - 44.).abs() < 0.01);
    assert!((track[7].0[1] - 52.5).abs() < 0.01);
    let mut occluded = frames;
    occluded.push(moving_patch(0, 0, true));
    assert_eq!(
        tracking::track_frames(&occluded, [30., 43.75], 6, 12, 0.7)
            .unwrap()
            .len(),
        8
    );
    assert!(tracking::track_frames(&[moving_patch(0, 0, true)], [50., 50.], 6, 12, 0.7).is_err());
}
#[test]
fn procedural_clocks_survive_cuts_retime_reverse_and_document_roundtrip() {
    let c = Clip {
        kind: Kind::Graphic,
        vector: Some(VectorShape::default()),
        duration: 4.,
        ..Clip::default()
    };
    let mut p = Project {
        motion_blur_samples: 4,
        clips: vec![c.clone()],
        ..Project::default()
    };
    p.validate().unwrap();
    let id = c.id.clone();
    assert!(edit::split(&mut p, &id, 1.3));
    assert_eq!(p.clips[1].content_offset, 1.3);
    p.clips[1].set_speed(2., 30).unwrap();
    assert_eq!(p.clips[1].content_rate, 2.);
    edit::reverse_animation(&mut p.clips[0]);
    assert_eq!(p.clips[0].content_offset, 1.3);
    assert_eq!(p.clips[0].content_rate, -1.);
    p.validate().unwrap();
    assert_eq!(
        p,
        serde_json::from_slice::<Project>(&serde_json::to_vec(&p).unwrap()).unwrap()
    );
}
fn pixels(p: &Project, folder: &std::path::Path, origin: f64, frames: u32) -> Vec<u8> {
    let graph = render::build_graph(
        p,
        &HashMap::new(),
        folder,
        origin,
        frames as f64 / p.fps as f64,
        Mode::PreviewVideo,
    )
    .unwrap();
    ProcessGroup::default()
        .run(
            graph.command().args([
                "-frames:v",
                &frames.to_string(),
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "pipe:1",
            ]),
            None,
        )
        .unwrap()
}
#[test]
fn real_procedural_sources_and_subframe_motion_blur_render() {
    let tmp = TempDir::new().unwrap();
    for (name, c) in [
        (
            "vector",
            Clip {
                vector: Some(VectorShape::default()),
                ..Clip::default()
            },
        ),
        (
            "scene",
            Clip {
                scene3d: Some(Scene::default()),
                ..Clip::default()
            },
        ),
    ] {
        let p = Project {
            width: 320,
            height: 240,
            clips: vec![Clip {
                kind: Kind::Graphic,
                duration: 2.,
                ..c
            }],
            ..Project::default()
        };
        let frame = pixels(&p, &tmp.path().join(name), 0.8, 1);
        assert_eq!(frame.len(), 320 * 240 * 3);
        assert!(frame.iter().any(|n| *n > 100));
        let mut blank = p.clone();
        blank.clips.clear();
        let background = pixels(
            &blank,
            &tmp.path().join(format!("background-{name}")),
            0.,
            1,
        );
        assert_eq!(
            &frame[..3],
            &background[..3],
            "Procedural source must preserve transparent surroundings"
        );
    }
    let c = Clip {
        kind: Kind::Graphic,
        graphic: Graphic::Rectangle,
        fill: true,
        secondary_color: "#ffffff".into(),
        duration: 1.,
        keyframes: vec![
            Keyframe {
                time: 0.,
                transform: Transform {
                    x: 15.,
                    scale: 0.2,
                    ..Transform::default()
                },
                ..Keyframe::default()
            },
            Keyframe {
                time: 1.,
                transform: Transform {
                    x: 85.,
                    scale: 0.2,
                    ..Transform::default()
                },
                ..Keyframe::default()
            },
        ],
        ..Clip::default()
    };
    let mut p = Project {
        width: 320,
        height: 240,
        clips: vec![c],
        ..Project::default()
    };
    let sharp = pixels(&p, &tmp.path().join("sharp"), 0.3, 3);
    p.motion_blur_samples = 4;
    let blur = pixels(&p, &tmp.path().join("blur"), 0.3, 3);
    assert_eq!(blur.len(), sharp.len());
    assert!(sharp.iter().zip(&blur).filter(|(a, b)| a != b).count() > 200);
    let before = ProcessGroup::default();
    before.cancel();
    assert!(
        render::build_graph_cancel(
            &p,
            &HashMap::new(),
            &tmp.path().join("cancelled"),
            0.,
            0.1,
            Mode::PreviewVideo,
            &before
        )
        .is_err()
    );
}

#[test]
fn real_video_tracking_drives_a_label_and_respects_source_retiming() {
    use frame_studio::{media, process::command, tracking::Settings};
    let tmp = TempDir::new().unwrap();
    for frame in 0..20 {
        let mut image = image::GrayImage::from_pixel(320, 240, image::Luma([20]));
        for y in -8_i32..=8 {
            for x in -8_i32..=8 {
                image.put_pixel(
                    (80 + frame * 2 + x) as u32,
                    (120 + frame + y) as u32,
                    image::Luma([if (x * 13 + y * 7).rem_euclid(11) < 5 {
                        240
                    } else {
                        70
                    }]),
                );
            }
        }
        image
            .save(tmp.path().join(format!("patch-{frame:03}.png")))
            .unwrap();
    }
    let movie = tmp.path().join("tracked.mkv");
    ProcessGroup::default()
        .run(
            command("ffmpeg")
                .args(["-v", "error", "-framerate", "10", "-i"])
                .arg(tmp.path().join("patch-%03d.png"))
                .args(["-c:v", "ffv1", "-pix_fmt", "gray", "-threads", "2"])
                .arg(&movie),
            None,
        )
        .unwrap();
    let asset = media::probe(&movie, &ProcessGroup::default()).unwrap();
    let source = Clip {
        kind: Kind::Video,
        asset_id: Some(asset.id.clone()),
        duration: 1.,
        speed: 2.,
        ..Clip::default()
    };
    let settings = Settings {
        duration: 0.9,
        seed: [25., 50.],
        patch_radius: 8,
        search_radius: 12,
        ..Settings::default()
    };
    let reference =
        tracking::reference_frame(&asset, &source, 0., &ProcessGroup::default()).unwrap();
    assert_eq!((reference.width, reference.height), (320, 240));
    let track = tracking::analyze(
        &asset,
        &source,
        &settings,
        &tmp.path().join("track.log"),
        &ProcessGroup::default(),
        |_| {},
    )
    .unwrap();
    assert!(!track.lost);
    assert_eq!(track.points.len(), 9);
    let mut target = Clip {
        kind: Kind::Title,
        duration: 1.,
        transform: Transform {
            x: 60.,
            y: 30.,
            ..Transform::default()
        },
        ..Clip::default()
    };
    let p = Project {
        width: 320,
        height: 240,
        clips: vec![source, target.clone()],
        ..Project::default()
    };
    tracking::apply(&track, &mut target, &p, 0, true).unwrap();
    assert!((target.keyframes.last().unwrap().transform.x - 70.).abs() < 0.1);
    assert!((target.keyframes.last().unwrap().transform.y - (30. + 16. / 240. * 100.)).abs() < 0.1);
    let group = ProcessGroup::default();
    group.cancel();
    assert!(
        tracking::analyze(
            &asset,
            &p.clips[0],
            &settings,
            &tmp.path().join("cancel.log"),
            &group,
            |_| {}
        )
        .is_err()
    );
}
#[test]
fn procedural_generation_cancels_and_removes_incomplete_output() {
    use frame_studio::procedural::{self, Window};
    use std::time::{Duration, Instant};
    let tmp = TempDir::new().unwrap();
    let group = ProcessGroup::default();
    let worker = group.clone();
    let output = tmp.path().join("cancel.mkv");
    let path = output.clone();
    let c = Clip {
        kind: Kind::Graphic,
        vector: Some(VectorShape::default()),
        ..Clip::default()
    };
    let handle = std::thread::spawn(move || {
        procedural::write_source(
            &c,
            Window {
                phase: 0.,
                duration: 20.,
                fps: 120,
                size: [640, 360],
                ..Default::default()
            },
            &path,
            &worker,
        )
    });
    std::thread::sleep(Duration::from_millis(30));
    let start = Instant::now();
    group.cancel();
    assert!(handle.join().unwrap().is_err());
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(!output.exists());
}

#[test]
fn procedural_blur_exports_at_project_fps_and_keeps_stationary_layers_sharp() {
    use frame_studio::process::command;
    let tmp = TempDir::new().unwrap();
    let c = Clip {
        kind: Kind::Graphic,
        scene3d: Some(Scene::default()),
        duration: 0.3,
        ..Clip::default()
    };
    let p = Project {
        width: 320,
        height: 240,
        motion_blur_samples: 4,
        clips: vec![c],
        ..Project::default()
    };
    let output = tmp.path().join("3d-blur.mp4");
    render::export(
        &p,
        &HashMap::new(),
        &tmp.path().join("export"),
        &output,
        &ProcessGroup::default(),
        |_| {},
    )
    .unwrap();
    let bytes = ProcessGroup::default()
        .run(
            command("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_entries",
                    "stream=avg_frame_rate,nb_frames:format=duration",
                    "-of",
                    "json",
                ])
                .arg(output),
            None,
        )
        .unwrap();
    let info: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(info["streams"][0]["avg_frame_rate"], "30/1");
    assert_eq!(info["streams"][0]["nb_frames"], "9");
    let mut still = p.clone();
    still.clips[0].scene3d = None;
    still.clips[0].graphic = Graphic::Rectangle;
    still.clips[0].fill = true;
    let blurred = pixels(&still, &tmp.path().join("static-blur"), 0., 1);
    still.motion_blur_samples = 1;
    let sharp = pixels(&still, &tmp.path().join("static-sharp"), 0., 1);
    assert_eq!(blurred, sharp);
}

#[test]
fn animation_shutter_matches_independent_subframe_samples() {
    use frame_studio::animation::Exposure;
    let tmp = TempDir::new().unwrap();
    let c = Clip {
        kind: Kind::Graphic,
        graphic: Graphic::Rectangle,
        fill: true,
        secondary_color: "#ffffff".into(),
        duration: 1.,
        keyframes: vec![
            Keyframe {
                time: 0.,
                transform: Transform {
                    x: 20.,
                    scale: 0.4,
                    opacity: 0.3,
                    ..Transform::default()
                },
                easing: Easing::Smoother,
                ..Keyframe::default()
            },
            Keyframe {
                time: 1.,
                transform: Transform {
                    x: 80.,
                    scale: 0.7,
                    opacity: 1.,
                    rotation: 25.,
                    ..Transform::default()
                },
                ..Keyframe::default()
            },
        ],
        ..Clip::default()
    };
    for (j, fraction) in [0.25, 0.5, 1.].into_iter().enumerate() {
        let p = Project {
            width: 320,
            height: 240,
            motion_blur_samples: 4,
            motion_shutter: fraction,
            clips: vec![c.clone()],
            ..Project::default()
        };
        let rendered = pixels(&p, &tmp.path().join(format!("shutter-{j}")), 0.3, 1);
        let mut total = vec![0_u32; rendered.len()];
        for i in 0..4 {
            let time = 0.3 + Exposure { fps: 30, fraction }.time(i as f64 / 120.);
            let mut reference = p.clone();
            reference.motion_blur_samples = 1;
            reference.clips[0].transform = c.value(time);
            reference.clips[0].keyframes.clear();
            let frame = pixels(
                &reference,
                &tmp.path().join(format!("reference-{j}-{i}")),
                0.,
                1,
            );
            for (sum, n) in total.iter_mut().zip(frame) {
                *sum += n as u32;
            }
        }
        let difference = rendered
            .iter()
            .zip(total)
            .map(|(n, sum)| (*n as f64 - sum as f64 / 4.).abs())
            .sum::<f64>()
            / rendered.len() as f64;
        assert!(
            difference < 1.,
            "Shutter {fraction} differs by {difference}"
        );
    }
    assert_eq!(
        Exposure {
            fps: 30,
            fraction: 0.5
        }
        .time(1.),
        1.
    );
}

#[test]
fn embedded_scene_mesh_and_camera_keys_survive_a_portable_frame_document() {
    use frame_studio::{
        document::{Document, Workspace},
        scene3d::{Camera, CameraKey},
    };
    let tmp = TempDir::new().unwrap();
    let mesh = parse_obj("v -1 -1 0\nv 1 -1 0\nv 0 1 0\nf 1 2 3").unwrap();
    let scene = Scene {
        objects: vec![Object {
            primitive: Primitive::Mesh,
            mesh,
            ..Object::default()
        }],
        camera_keys: vec![
            CameraKey {
                time: 0.,
                camera: Camera::default(),
                easing: Easing::Smoother,
            },
            CameraKey {
                time: 2.,
                camera: Camera {
                    yaw: 70.,
                    distance: 4.,
                    ..Camera::default()
                },
                easing: Easing::Linear,
            },
        ],
        ..Scene::default()
    };
    let p = Project {
        clips: vec![Clip {
            kind: Kind::Graphic,
            scene3d: Some(scene.clone()),
            ..Clip::default()
        }],
        motion_blur_samples: 4,
        motion_shutter: 0.25,
        ..Project::default()
    };
    let path = tmp.path().join("scene.frame");
    Document::capture(&p, &HashMap::new(), Workspace::default())
        .save(&path)
        .unwrap();
    let restored = Document::read(&path).unwrap();
    assert_eq!(restored.project, p);
    let a = scene
        .render(1., 240, 240, &ProcessGroup::default())
        .unwrap();
    let b = restored.project.clips[0]
        .scene3d
        .as_ref()
        .unwrap()
        .render(1., 240, 240, &ProcessGroup::default())
        .unwrap();
    assert_eq!(a, b);
    assert!(parse_svg("<svg viewBox = '0 0 100 100'><path id='p' data-name='line' d = 'M 0 0 L 100 100'/></svg>").is_ok());
    assert!(parse_obj("v 0 0 0\nv 1 0 0\nv 2 0 0\nf 1 2 3").is_err());
}

#[test]
fn render_graph_releases_its_generated_lossless_sources() {
    let tmp = TempDir::new().unwrap();
    let p = Project {
        width: 320,
        height: 240,
        clips: vec![Clip {
            kind: Kind::Graphic,
            vector: Some(VectorShape::default()),
            ..Clip::default()
        }],
        ..Project::default()
    };
    let graph = render::build_graph(
        &p,
        &HashMap::new(),
        tmp.path(),
        0.5,
        0.1,
        Mode::PreviewVideo,
    )
    .unwrap();
    assert!(tmp.path().join("procedural-2.mkv").exists());
    drop(graph);
    assert!(!tmp.path().join("procedural-2.mkv").exists());
}
