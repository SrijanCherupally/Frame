use frame_studio::{
    animation::{EASINGS, Easing, MotionPath, Reveal},
    edit, media,
    model::*,
    process::ProcessGroup,
    render::{self, Mode},
};
use std::{collections::HashMap, path::Path};
use tempfile::TempDir;
fn raw(p: &Project, folder: &Path, origin: f64, frames: u32) -> Vec<u8> {
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
fn mean_difference(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(a, b)| (*a as f64 - *b as f64).abs())
        .sum::<f64>()
        / a.len() as f64
}
fn shape() -> Clip {
    Clip {
        kind: Kind::Graphic,
        graphic: Graphic::Rectangle,
        fill: true,
        color: "#ffffff".into(),
        secondary_color: "#ffffff".into(),
        duration: 1.,
        ..Clip::default()
    }
}
#[test]
fn rendered_easing_and_paths_match_model_samples_and_mid_segment_seeks() {
    let tmp = TempDir::new().unwrap();
    let size = 320 * 240 * 3;
    for (index, &(easing, _)) in EASINGS.iter().enumerate() {
        let c = Clip {
            keyframes: vec![
                Keyframe {
                    time: 0.,
                    easing,
                    path: Some(MotionPath {
                        control1: [10., 80.],
                        control2: [75., 15.],
                    }),
                    transform: Transform {
                        x: 25.,
                        y: 35.,
                        scale: 0.4,
                        opacity: 0.2,
                        ..Transform::default()
                    },
                    ..Keyframe::default()
                },
                Keyframe {
                    time: 1.,
                    transform: Transform {
                        x: 75.,
                        y: 65.,
                        scale: 0.7,
                        opacity: 0.9,
                        rotation: 20.,
                    },
                    ..Keyframe::default()
                },
            ],
            ..shape()
        };
        let p = Project {
            width: 320,
            height: 240,
            clips: vec![c.clone()],
            ..Project::default()
        };
        let frames = raw(&p, &tmp.path().join(format!("full-{index}")), 0., 10);
        assert_eq!(frames.len(), size * 10);
        let seek = raw(&p, &tmp.path().join(format!("seek-{index}")), 0.3, 1);
        let mut reference = p.clone();
        reference.clips[0].transform = c.value(0.3);
        reference.clips[0].keyframes.clear();
        let still = raw(
            &reference,
            &tmp.path().join(format!("still-{index}")),
            0.,
            1,
        );
        let difference = mean_difference(&frames[9 * size..10 * size], &still);
        assert!(
            difference < 1.,
            "{easing:?} stream differs from model: {difference}"
        );
        assert!(
            mean_difference(&seek, &still) < 1.,
            "{easing:?} seek differs from model"
        );
    }
}
#[test]
fn reveals_animate_in_preview_and_export_and_survive_cuts() {
    let tmp = TempDir::new().unwrap();
    let size = 320 * 240 * 3;
    for (i, reveal) in [Reveal::LeftToRight, Reveal::BottomToTop, Reveal::Iris]
        .into_iter()
        .enumerate()
    {
        let c = Clip {
            reveal,
            reveal_duration: 1.,
            ..shape()
        };
        let p = Project {
            width: 320,
            height: 240,
            clips: vec![c.clone()],
            ..Project::default()
        };
        let frames = raw(&p, &tmp.path().join(format!("reveal-{i}")), 0., 30);
        assert_eq!(frames.len(), size * 30);
        let energy = |f: &[u8]| f.iter().filter(|n| **n > 100).count();
        assert!(energy(&frames[..size]) < 10);
        assert!(energy(&frames[29 * size..]) > 10000);
        let seek = raw(&p, &tmp.path().join(format!("reveal-seek-{i}")), 0.5, 1);
        assert!(mean_difference(&frames[15 * size..16 * size], &seek) < 0.1);
        let mut cut = p.clone();
        assert!(edit::split(&mut cut, &c.id, 0.4));
        let after = raw(&cut, &tmp.path().join(format!("reveal-cut-{i}")), 0.5, 1);
        assert!(mean_difference(&seek, &after) < 0.1);
        render::export(
            &p,
            &HashMap::new(),
            &tmp.path().join(format!("export-{i}")),
            &tmp.path().join(format!("reveal-{i}.mp4")),
            &ProcessGroup::default(),
            |_| {},
        )
        .unwrap();
    }
}
#[test]
fn footage_can_fill_a_text_matte_and_charts_use_imported_values() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("source.png");
    image::RgbaImage::from_pixel(320, 240, image::Rgba([240, 80, 30, 255]))
        .save(&path)
        .unwrap();
    let a = media::probe(&path, &ProcessGroup::default()).unwrap();
    let c = Clip {
        kind: Kind::Image,
        asset_id: Some(a.id.clone()),
        mask: Mask::Text,
        text: "FLIGHT\nTEST".into(),
        font_size: 300,
        duration: 0.5,
        ..Clip::default()
    };
    let p = Project {
        width: 320,
        height: 240,
        clips: vec![c],
        ..Project::default()
    };
    let assets = HashMap::from([(a.id.clone(), a)]);
    let graph = render::build_graph(
        &p,
        &assets,
        &tmp.path().join("matte"),
        0.,
        0.1,
        Mode::PreviewVideo,
    )
    .unwrap();
    let pixels = ProcessGroup::default()
        .run(
            graph.command().args([
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "pipe:1",
            ]),
            None,
        )
        .unwrap();
    let visible = pixels
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|p| p[0] > 150)
        .count();
    assert!(
        visible > 500 && visible < 320 * 240 / 2,
        "Text matte must expose letters and hide surroundings: {visible}"
    );
    assert!(pixels[..3].iter().all(|b| *b < 40));
    let mut chart = Clip {
        graphic: Graphic::Chart,
        chart_data: vec![[0., 0.], [1., 1.], [2., 0.]],
        ..shape()
    };
    render::graphic_image(&chart, &tmp.path().join("chart-a.png"), 640, 360).unwrap();
    chart.chart_data = vec![[0., 1.], [1., 0.], [2., 1.]];
    render::graphic_image(&chart, &tmp.path().join("chart-b.png"), 640, 360).unwrap();
    let a = image::open(tmp.path().join("chart-a.png"))
        .unwrap()
        .to_rgba8();
    let b = image::open(tmp.path().join("chart-b.png"))
        .unwrap()
        .to_rgba8();
    assert!(a.pixels().zip(b.pixels()).filter(|(a, b)| a != b).count() > 1000);
}
#[test]
fn dense_animated_tracks_do_not_exceed_ffmpeg_expression_recursion_limits() {
    let tmp = TempDir::new().unwrap();
    let c = Clip {
        keyframes: (0..200)
            .map(|i| Keyframe {
                time: i as f64 / 199.,
                transform: Transform {
                    x: 20. + i as f64 / 4.,
                    scale: 0.5,
                    ..Transform::default()
                },
                easing: Easing::Smooth,
                ..Keyframe::default()
            })
            .collect(),
        ..shape()
    };
    let p = Project {
        width: 320,
        height: 240,
        clips: vec![c],
        ..Project::default()
    };
    let pixels = raw(&p, &tmp.path().join("dense"), 0.5, 1);
    assert_eq!(pixels.len(), 320 * 240 * 3);
}
