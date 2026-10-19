use frame_studio::{
    media::{self, Asset},
    model::*,
    preview::{Preview, PreviewEvent},
    process::{ProcessGroup, command},
    render::{self, Mode},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn image_clip(id: &str) -> Clip {
    Clip {
        id: id.into(),
        asset_id: Some("demo-0".into()),
        kind: Kind::Image,
        ..Clip::default()
    }
}
fn probe_output(path: &Path) -> serde_json::Value {
    let bytes = ProcessGroup::default()
        .run(
            command("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-show_format",
                    "-show_streams",
                    "-of",
                    "json",
                ])
                .arg(path),
            None,
        )
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn fixture(root: &Path) -> Asset {
    let path = root.join("fixture.mp4");
    let group = ProcessGroup::default();
    group
        .run(
            command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=s=320x240:r=30:d=4",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=4",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-threads",
                    "2",
                    "-c:a",
                    "aac",
                    "-shortest",
                ])
                .arg(&path),
            None,
        )
        .unwrap();
    media::probe(&path, &group).unwrap()
}
fn render(p: &Project, a: &HashMap<String, Asset>, root: &Path, name: &str) -> PathBuf {
    let out = root.join(format!("{name}.mp4"));
    render::export(
        p,
        a,
        &root.join(name),
        &out,
        &ProcessGroup::default(),
        |_| {},
    )
    .unwrap();
    out
}
#[test]
fn originals_link_without_copy_and_proxy_is_decodable() {
    let tmp = TempDir::new().unwrap();
    let a = fixture(tmp.path());
    let original = a.path.clone();
    let mut updates = vec![];
    let a = media::prepare(a, tmp.path(), &ProcessGroup::default(), |a| updates.push(a)).unwrap();
    assert_eq!(a.path, original);
    assert_eq!(a.status, "ready");
    assert!(a.proxy_path.as_ref().unwrap().is_file());
    assert!(updates.iter().any(|a| a.status == "proxying"));
    assert!(
        !tmp.path()
            .join("media")
            .join(&a.id)
            .join("original.mp4")
            .exists()
    );
    let metadata = probe_output(a.preview_path());
    assert!(
        metadata["streams"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["codec_type"] == "audio")
    );
}
#[test]
fn render_transitions_masks_keyframes_and_audio() {
    let tmp = TempDir::new().unwrap();
    let a = fixture(tmp.path());
    let mut clips = vec![];
    for (i, t) in [
        Transition::None,
        Transition::Fade,
        Transition::Slide,
        Transition::Zoom,
        Transition::Wipe,
    ]
    .into_iter()
    .enumerate()
    {
        clips.push(Clip {
            id: format!("clip{i}"),
            kind: Kind::Video,
            asset_id: Some(a.id.clone()),
            start: i as f64 * 0.4,
            duration: 0.4,
            transition: t,
            fade_duration: 0.1,
            mask: if i == 2 {
                Mask::Circle
            } else if i == 3 {
                Mask::Rectangle
            } else {
                Mask::None
            },
            brightness: 0.03,
            contrast: 1.1,
            saturation: 0.8,
            warmth: 0.2,
            keyframes: vec![
                Keyframe {
                    time: 0.,
                    transform: Transform {
                        x: 40.,
                        scale: 0.8,
                        ..Transform::default()
                    },
                },
                Keyframe {
                    time: 0.4,
                    transform: Transform {
                        x: 60.,
                        ..Transform::default()
                    },
                },
            ],
            ..Clip::default()
        });
    }
    clips.push(Clip {
        kind: Kind::Title,
        track: 0,
        duration: 2.,
        text: "Rocket 'flight': 100%\nA:B, C\\D".into(),
        transition: Transition::Fade,
        ..Clip::default()
    });
    clips.push(Clip {
        kind: Kind::Graphic,
        track: 0,
        start: 1.,
        duration: 0.5,
        graphic: Graphic::Arrow,
        ..Clip::default()
    });
    let p = Project {
        width: 320,
        height: 240,
        clips,
        ..Project::default()
    };
    let assets = HashMap::from([(a.id.clone(), a)]);
    let out = render(&p, &assets, tmp.path(), "composite");
    let info = probe_output(&out);
    assert!(
        (info["format"]["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            - 2.)
            .abs()
            < 0.1
    );
    assert!(
        info["streams"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["codec_type"] == "audio")
    );
}
#[test]
fn temporal_blending_changes_actual_export_pixels_and_slowdown_duration() {
    let tmp = TempDir::new().unwrap();
    let a = fixture(tmp.path());
    let c = Clip {
        kind: Kind::Video,
        asset_id: Some(a.id.clone()),
        duration: 0.5,
        speed: 8.,
        motion_blur: true,
        deflicker: true,
        ..Clip::default()
    };
    let mut p = Project {
        width: 320,
        height: 240,
        clips: vec![c],
        ..Project::default()
    };
    let assets = HashMap::from([(a.id.clone(), a)]);
    let clean = render(&p, &assets, tmp.path(), "clean");
    p.clips[0].motion_blur = false;
    p.clips[0].deflicker = false;
    let raw = render(&p, &assets, tmp.path(), "raw");
    let frame = |path: &Path| {
        ProcessGroup::default()
            .run(
                command("ffmpeg")
                    .args(["-v", "error", "-ss", "0.25", "-i"])
                    .arg(path)
                    .args([
                        "-frames:v",
                        "1",
                        "-f",
                        "rawvideo",
                        "-pix_fmt",
                        "rgb24",
                        "-threads",
                        "1",
                        "-",
                    ]),
                None,
            )
            .unwrap()
    };
    let a = frame(&clean);
    let b = frame(&raw);
    assert_eq!(a.len(), 320 * 240 * 3);
    let difference = a
        .iter()
        .zip(&b)
        .map(|(a, b)| a.abs_diff(*b) as f64)
        .sum::<f64>()
        / a.len() as f64;
    assert!(difference > 0.5, "Motion blending must alter real pixels");
    p.clips[0].duration = 2.;
    p.clips[0].speed = 0.25;
    let slow = render(&p, &assets, tmp.path(), "slow");
    assert!(
        (probe_output(&slow)["format"]["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            - 2.)
            .abs()
            < 0.1
    );
}
#[test]
fn real_four_hour_timestamps_render_to_twenty_seconds() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("four-hours.mp4");
    let group = ProcessGroup::default();
    group
        .run(
            command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=blue:s=320x240:r=1/60:d=14400",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-threads",
                    "2",
                ])
                .arg(&file),
            None,
        )
        .unwrap();
    let a = media::probe(&file, &group).unwrap();
    let p = Project {
        width: 320,
        height: 240,
        clips: vec![Clip {
            kind: Kind::Video,
            asset_id: Some(a.id.clone()),
            speed: 720.,
            duration: 20.,
            motion_blur: true,
            ..Clip::default()
        }],
        ..Project::default()
    };
    let assets = HashMap::from([(a.id.clone(), a)]);
    let out = render(&p, &assets, tmp.path(), "long");
    let info = probe_output(&out);
    assert!(
        (info["format"]["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            - 20.)
            .abs()
            < 0.1
    );
    assert_eq!(info["streams"][0]["nb_frames"], "600");
}
#[test]
fn chunked_timeline_retains_content_after_boundary() {
    let tmp = TempDir::new().unwrap();
    let assets = media::seed(tmp.path())
        .unwrap()
        .into_iter()
        .map(|a| (a.id.clone(), a))
        .collect();
    let p = Project {
        width: 320,
        height: 240,
        clips: vec![
            Clip {
                duration: 31.,
                transition: Transition::Fade,
                ..image_clip("back")
            },
            Clip {
                kind: Kind::Title,
                track: 0,
                start: 29.,
                duration: 2.,
                text: "After the boundary".into(),
                transition: Transition::Slide,
                ..Clip::default()
            },
        ],
        ..Project::default()
    };
    let out = render(&p, &assets, tmp.path(), "chunked");
    let bytes = ProcessGroup::default()
        .run(
            command("ffmpeg")
                .args(["-v", "error", "-ss", "30.5", "-i"])
                .arg(out)
                .args([
                    "-frames:v",
                    "1",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgb24",
                    "-threads",
                    "1",
                    "-",
                ]),
            None,
        )
        .unwrap();
    assert_eq!(bytes.len(), 320 * 240 * 3);
    assert!(bytes.iter().map(|b| *b as f64).sum::<f64>() / bytes.len() as f64 > 15.);
}
#[test]
fn cancellation_stops_processing_and_preserves_source() {
    let tmp = TempDir::new().unwrap();
    let assets = media::seed(tmp.path())
        .unwrap()
        .into_iter()
        .map(|a| (a.id.clone(), a))
        .collect::<HashMap<_, _>>();
    let original = std::fs::read(&assets["demo-0"].path).unwrap();
    let p = Project {
        clips: vec![Clip {
            duration: 300.,
            ..image_clip("a")
        }],
        ..Project::default()
    };
    let group = ProcessGroup::default();
    let cancel = group.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        cancel.cancel();
    });
    let out = tmp.path().join("cancelled.mp4");
    let start = Instant::now();
    assert!(render::export(&p, &assets, &tmp.path().join("job"), &out, &group, |_| {}).is_err());
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(!out.exists());
    assert_eq!(std::fs::read(&assets["demo-0"].path).unwrap(), original);
}
#[test]
fn export_rejects_offline_sources_and_keeps_existing_output() {
    let tmp = TempDir::new().unwrap();
    let assets = media::seed(tmp.path())
        .unwrap()
        .into_iter()
        .map(|a| (a.id.clone(), a))
        .collect::<HashMap<_, _>>();
    let p = Project {
        width: 320,
        height: 240,
        clips: vec![Clip {
            duration: 0.2,
            ..image_clip("a")
        }],
        ..Project::default()
    };
    let out = tmp.path().join("existing.mp4");
    std::fs::write(&out, b"keep this file").unwrap();
    assert!(
        render::export(
            &p,
            &assets,
            &tmp.path().join("job"),
            &out,
            &ProcessGroup::default(),
            |_| {}
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&out).unwrap(), b"keep this file");
    std::fs::remove_file(&assets["demo-0"].path).unwrap();
    assert!(
        render::build_graph(
            &p,
            &assets,
            &tmp.path().join("missing"),
            0.,
            0.2,
            Mode::Export
        )
        .is_err()
    );
}
#[test]
fn native_preview_streams_real_frames_and_cancels_cleanly() {
    let tmp = TempDir::new().unwrap();
    let assets = Arc::new(
        media::seed(tmp.path())
            .unwrap()
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect(),
    );
    let p = Project {
        clips: vec![Clip {
            duration: 1.,
            ..image_clip("a")
        }],
        ..Project::default()
    };
    let mut preview = Preview::default();
    preview.start(p, assets, 0.2, false, tmp.path().into(), false);
    let until = Instant::now() + Duration::from_secs(10);
    let mut got_frame = false;
    while Instant::now() < until {
        if let Some(event) = preview.next_event() {
            match event {
                PreviewEvent::Frame {
                    pixels,
                    width,
                    height,
                    ..
                } => {
                    assert_eq!(pixels.len(), (width * height * 3) as usize);
                    assert!(pixels.iter().any(|b| *b > 50));
                    got_frame = true;
                    break;
                }
                PreviewEvent::Error(e) => panic!("Preview error: {e}"),
                _ => {}
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    preview.stop();
    assert!(got_frame);
}

#[test]
fn preview_audio_graph_outputs_non_silent_pcm() {
    let temp = TempDir::new().unwrap();
    let asset = fixture(temp.path());
    let p = Project {
        width: 320,
        height: 240,
        clips: vec![Clip {
            kind: Kind::Video,
            asset_id: Some(asset.id.clone()),
            duration: 1.,
            speed: 2.,
            transition: Transition::Fade,
            ..Clip::default()
        }],
        ..Project::default()
    };
    let assets = HashMap::from([(asset.id.clone(), asset)]);
    let graph = render::build_graph(
        &p,
        &assets,
        &temp.path().join("audio"),
        0.,
        0.5,
        Mode::PreviewAudio,
    )
    .unwrap();
    let mut command = graph.command();
    command.args(["-vn", "-f", "s16le", "-ar", "48000", "-ac", "2", "pipe:1"]);
    let pcm = ProcessGroup::default().run(&mut command, None).unwrap();
    assert!(pcm.len() > 48000);
    assert!(
        pcm.as_chunks::<2>()
            .0
            .iter()
            .any(|s| i16::from_le_bytes([s[0], s[1]]).abs() > 100)
    );
}
