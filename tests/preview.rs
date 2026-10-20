use frame_studio::{
    media,
    model::{Clip, Graphic, Kind, Project},
    preview::{Preview, PreviewEvent},
    process::{ProcessGroup, command},
};
use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn wait(preview: &Preview) -> Vec<PreviewEvent> {
    let mut events = vec![];
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(event) = preview.next_event() {
            let done = matches!(event, PreviewEvent::Error(_) | PreviewEvent::Finished);
            events.push(event);
            if done {
                return events;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("Preview did not complete: {events:?}");
}
fn image_project() -> Project {
    Project {
        clips: vec![Clip {
            asset_id: Some("demo-0".into()),
            kind: Kind::Image,
            duration: 1.,
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn assets(root: &Path) -> Arc<HashMap<String, media::Asset>> {
    Arc::new(
        media::seed(root)
            .unwrap()
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect(),
    )
}
#[test]
fn preview_composites_real_png_footage_titles_and_graphics() {
    let dir = tempfile::tempdir().unwrap();
    let mut assets = assets(dir.path());
    let path = dir.path().join("footage.mp4");
    ProcessGroup::default()
        .run(
            command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=blue:s=640x360:r=30:d=1",
                    "-c:v",
                    "libx264",
                    "-threads",
                    "2",
                ])
                .arg(&path),
            None,
        )
        .unwrap();
    let video = media::probe(&path, &ProcessGroup::default()).unwrap();
    let video_id = video.id.clone();
    Arc::make_mut(&mut assets).insert(video.id.clone(), video);
    let p = Project {
        clips: vec![
            Clip {
                asset_id: Some(video_id),
                kind: Kind::Video,
                track: 2,
                duration: 1.,
                ..Default::default()
            },
            Clip {
                asset_id: Some("demo-0".into()),
                kind: Kind::Image,
                track: 1,
                duration: 1.,
                transform: frame_studio::model::Transform {
                    opacity: 0.5,
                    ..Default::default()
                },
                ..Default::default()
            },
            Clip {
                kind: Kind::Title,
                text: "PNG preview".into(),
                track: 0,
                duration: 1.,
                ..Default::default()
            },
            Clip {
                kind: Kind::Graphic,
                graphic: Graphic::Rectangle,
                track: 0,
                duration: 1.,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let mut preview = Preview::default();
    preview.start(p, assets, 0.2, false, dir.path().into(), false);
    let events = wait(&preview);
    assert!(
        matches!(events.last(), Some(PreviewEvent::Finished)),
        "{events:?}"
    );
    assert!(events.iter().any(|e|matches!(e,PreviewEvent::Frame { pixels,width:640,height:360,.. } if pixels.len() == 640*360*3 && pixels.iter().any(|b|*b > 80))));
}
#[test]
fn preview_failure_keeps_diagnostics_after_cleanup_and_recovers_on_retry() {
    let dir = tempfile::tempdir().unwrap();
    let mut assets = assets(dir.path());
    let source = assets["demo-0"].path.clone();
    let image = std::fs::read(&source).unwrap();
    std::fs::write(&source, b"not a valid image").unwrap();
    let mut preview = Preview::default();
    preview.start(
        image_project(),
        assets.clone(),
        0.2,
        false,
        dir.path().into(),
        false,
    );
    let events = wait(&preview);
    let Some(PreviewEvent::Error(error)) = events.last() else {
        panic!("Missing preview error: {events:?}");
    };
    assert!(error.contains("Diagnostic log:"), "{error}");
    assert!(error.contains("FFmpeg preview failed"), "{error}");
    let log = std::fs::read_dir(dir.path().join("diagnostics"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let details = std::fs::read_to_string(&log).unwrap();
    assert!(details.contains("FFmpeg output:"));
    assert!(details.contains("Filter graph:"));
    assert!(details.contains("overlay"));
    assert!(details.contains("original.png"));
    // A repaired source must preview successfully without restarting the app.
    std::fs::write(&source, image).unwrap();
    Arc::make_mut(&mut assets).get_mut("demo-0").unwrap().status = "ready".into();
    preview.start(
        image_project(),
        assets,
        0.2,
        false,
        dir.path().into(),
        false,
    );
    assert!(
        wait(&preview)
            .iter()
            .any(|e| matches!(e, PreviewEvent::Frame { .. }))
    );
    drop(preview);
    assert_eq!(
        std::fs::read_dir(dir.path().join("preview"))
            .unwrap()
            .count(),
        0
    );
    assert!(
        log.is_file(),
        "Successful retry must retain the diagnostic log"
    );
}
#[test]
fn diagnostic_retention_is_bounded_and_cancelled_generations_are_silent() {
    let dir = tempfile::tempdir().unwrap();
    let assets = assets(dir.path());
    std::fs::write(&assets["demo-0"].path, b"broken PNG").unwrap();
    let mut preview = Preview::default();
    for _ in 0..7 {
        preview.start(
            image_project(),
            assets.clone(),
            0.2,
            false,
            dir.path().into(),
            false,
        );
        assert!(matches!(
            wait(&preview).last(),
            Some(PreviewEvent::Error(_))
        ));
    }
    drop(preview);
    let logs = std::fs::read_dir(dir.path().join("diagnostics")).unwrap();
    let logs: Vec<_> = logs.map(Result::unwrap).collect();
    assert_eq!(logs.len(), 5);
    assert!(
        logs.iter()
            .all(|e| e.metadata().unwrap().len() < 320 * 1024)
    );
    let mut preview = Preview::default();
    preview.start(image_project(), assets, 0.2, true, dir.path().into(), false);
    preview.stop();
    drop(preview);
    assert_eq!(
        std::fs::read_dir(dir.path().join("diagnostics"))
            .unwrap()
            .count(),
        5
    );
}
