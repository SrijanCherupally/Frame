use frame_studio::{
    document::{Document, Workspace},
    media::Asset,
    model::*,
    presets::{self, Motion},
    storage::{Storage, load_project},
};
use std::{collections::HashMap, fs};
use tempfile::TempDir;

#[test]
fn portable_document_restores_timeline_and_media_in_a_fresh_library_after_move() {
    let temp = TempDir::new().unwrap();
    let bundle = temp.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    let original = bundle.join("source.mov");
    fs::write(&original, b"footage is not embedded").unwrap();
    let asset = Asset {
        id: "camera-1".into(),
        path: original,
        name: "Camera".into(),
        duration: 30.,
        ..Asset::default()
    };
    let mut clip = Clip {
        asset_id: Some(asset.id.clone()),
        kind: Kind::Video,
        source_in: 4.,
        duration: 3.,
        speed: 2.,
        ..Clip::default()
    };
    Motion::Pop.apply(&mut clip, 30);
    let p = Project {
        name: "Rocket build".into(),
        clips: vec![clip.clone()],
        width: 1080,
        height: 1920,
        fps: 30,
        muted_tracks: vec![4],
        ..Project::default()
    };
    let path = bundle.join("rocket.frame");
    let doc = Document::capture(
        &p,
        &HashMap::from([(asset.id.clone(), asset)]),
        Workspace {
            playhead: 1.5,
            zoom: 0.1,
            selected: Some(clip.id),
        },
    );
    doc.save(&path).unwrap();
    let json = fs::read_to_string(&path).unwrap();
    assert!(json.contains("source.mov"));
    assert!(!json.contains(bundle.to_str().unwrap()));
    assert!(json.len() < 40000);
    let moved = temp.path().join("moved");
    fs::rename(bundle, &moved).unwrap();
    let mut reopened = Document::read(&moved.join("rocket.frame")).unwrap();
    assert_eq!(reopened.project, p);
    assert_eq!(reopened.workspace.playhead, 1.5);
    assert_eq!(reopened.workspace.zoom, 0.1);
    let storage = Storage::open(temp.path().join("fresh-library")).unwrap();
    let restored = reopened.restore(&storage, &HashMap::new()).unwrap();
    assert_eq!(restored[0].path, moved.join("source.mov"));
    assert!(restored[0].error.is_none());
    assert_eq!(storage.assets()["camera-1"].path, moved.join("source.mov"));
    assert_eq!(load_project(&moved.join("rocket.frame")).unwrap(), p);
}
#[test]
fn collisions_preserve_existing_media_and_offline_originals_can_be_relinked() {
    let temp = TempDir::new().unwrap();
    let storage = Storage::open(temp.path().join("library")).unwrap();
    let local = Asset {
        id: "same-id".into(),
        path: temp.path().join("my-video.mp4"),
        ..Asset::default()
    };
    let incoming = Asset {
        id: local.id.clone(),
        path: temp.path().join("offline.mp4"),
        ..Asset::default()
    };
    let project = Project {
        clips: vec![Clip {
            asset_id: Some(local.id.clone()),
            ..Clip::default()
        }],
        ..Project::default()
    };
    let existing = HashMap::from([(local.id.clone(), local.clone())]);
    let mut doc = Document::capture(
        &project,
        &HashMap::from([(incoming.id.clone(), incoming)]),
        Workspace::default(),
    );
    let restored = doc.restore(&storage, &existing).unwrap();
    assert_ne!(restored[0].id, local.id);
    assert_eq!(
        doc.project.clips[0].asset_id.as_deref(),
        Some(restored[0].id.as_str())
    );
    assert_eq!(restored[0].status, "error");
    assert!(restored[0].error.is_some());
    assert_eq!(existing["same-id"].path, local.path);
}
#[test]
fn malformed_documents_are_rejected_before_import() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("invalid.frame");
    for json in [
        r#"{}"#,
        r#"{"format":"unrecognized","version":1,"project":{},"media":[]}"#,
        r#"{"format":"frame-project","version":900,"project":{},"media":[]}"#,
    ] {
        fs::write(&path, json).unwrap();
        assert!(Document::read(&path).is_err());
    }
    let mut doc = Document::capture(&Project::default(), &HashMap::new(), Workspace::default());
    doc.media = vec![Asset {
        id: "../escape".into(),
        ..Asset::default()
    }];
    assert!(
        doc.restore(
            &Storage::open(temp.path().join("library")).unwrap(),
            &HashMap::new()
        )
        .is_err()
    );
}
#[test]
fn every_format_and_scene_is_valid_and_motion_survives_cuts() {
    for format in presets::FORMATS {
        let mut p = Project::default();
        format.apply(&mut p).unwrap();
        p.validate().unwrap();
    }
    let mut short = Project {
        fps: 60,
        clips: vec![Clip {
            kind: Kind::Title,
            duration: 1. / 60.,
            ..Clip::default()
        }],
        ..Project::default()
    };
    let before = short.clone();
    assert!(presets::FORMATS[0].apply(&mut short).is_err());
    assert_eq!(short, before);
    for &(scene, _, _) in presets::SCENES {
        Project {
            clips: scene.clips(4., 30),
            ..Project::default()
        }
        .validate()
        .unwrap();
    }
    for &(motion, _, _) in presets::MOTIONS {
        let mut c = Clip {
            kind: Kind::Graphic,
            ..Clip::default()
        };
        motion.apply(&mut c, 30);
        let mut p = Project {
            clips: vec![c.clone()],
            ..Project::default()
        };
        p.validate().unwrap();
        assert!(frame_studio::edit::split(&mut p, &c.id, 1.3));
        for t in [0., 0.2, 0.8, 1.3, 2., 4.5] {
            let actual = if t <= 1.3 {
                p.clips[0].value(t)
            } else {
                p.clips[1].value(t - 1.3)
            };
            let expected = c.value(t);
            assert!(
                (actual.x - expected.x).abs() < 1e-7
                    && (actual.y - expected.y).abs() < 1e-7
                    && (actual.scale - expected.scale).abs() < 1e-7
                    && (actual.opacity - expected.opacity).abs() < 1e-7
            );
        }
    }
}

#[test]
fn repeated_opens_with_stale_library_snapshots_do_not_overwrite_media() {
    let temp = TempDir::new().unwrap();
    let storage = Storage::open(temp.path().join("library")).unwrap();
    let project = Project {
        clips: vec![Clip {
            asset_id: Some("camera".into()),
            ..Clip::default()
        }],
        ..Project::default()
    };
    let mut ids = vec![];
    for filename in ["first.mov", "second.mov"] {
        let asset = Asset {
            id: "camera".into(),
            path: temp.path().join(filename),
            ..Asset::default()
        };
        let mut doc = Document::capture(
            &project,
            &HashMap::from([(asset.id.clone(), asset)]),
            Workspace::default(),
        );
        let restored = doc.restore(&storage, &HashMap::new()).unwrap();
        ids.push(restored[0].id.clone());
        let saved = temp.path().join(format!("{filename}.frame"));
        doc.save(&saved).unwrap();
        let read = Document::read(&saved).unwrap();
        assert_eq!(
            read.project.clips[0].asset_id.as_deref(),
            Some(read.media[0].id.as_str())
        );
    }
    assert_ne!(ids[0], ids[1]);
    assert!(storage.assets()[&ids[0]].path.ends_with("first.mov"));
    assert!(storage.assets()[&ids[1]].path.ends_with("second.mov"));
}

#[test]
fn motion_entrances_finish_promptly_even_on_four_hour_clips() {
    for &(motion, _, _) in presets::MOTIONS {
        let mut c = Clip {
            kind: Kind::Graphic,
            duration: 14400.,
            ..Clip::default()
        };
        motion.apply(&mut c, 60);
        Project {
            fps: 60,
            clips: vec![c.clone()],
            ..Project::default()
        }
        .validate()
        .unwrap();
        assert!(c.keyframes.len() <= 200);
        if !matches!(motion, Motion::ZoomIn | Motion::ZoomOut | Motion::Float) {
            assert!(
                (c.value(0.8).opacity - 1.).abs() < 1e-8,
                "{motion:?} should be visible after its entrance"
            );
            assert!((c.value(2.).y - 50.).abs() < 1e-8);
        }
    }
}
