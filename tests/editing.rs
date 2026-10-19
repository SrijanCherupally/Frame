use frame_studio::{
    edit,
    model::*,
    storage::{Storage, load_project},
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
#[test]
fn retiming_four_hours_to_twenty_seconds() {
    let mut c = Clip {
        duration: 14400.,
        ..image_clip("a")
    };
    c.set_speed(720., 30).unwrap();
    assert_eq!(c.duration, 20.);
    c.set_speed(0.5, 30).unwrap();
    assert_eq!(c.duration, 28800.);
    let before = c.clone();
    assert!(c.set_speed(f64::NAN, 30).is_err());
    assert_eq!(c, before);
    assert!(c.set_speed(1e12, 30).is_err());
}
#[test]
fn split_preserves_source_offset_and_animation() {
    let c = Clip {
        id: "a".into(),
        duration: 20.,
        start: 3.,
        source_in: 12.,
        speed: 720.,
        keyframes: vec![
            Keyframe {
                time: 0.,
                transform: Transform {
                    x: 10.,
                    ..Transform::default()
                },
            },
            Keyframe {
                time: 20.,
                transform: Transform {
                    x: 90.,
                    ..Transform::default()
                },
            },
        ],
        ..image_clip("a")
    };
    let mut p = Project {
        clips: vec![c],
        ..Project::default()
    };
    assert!(edit::split(&mut p, "a", 13.));
    assert_eq!(p.clips.len(), 2);
    assert_eq!(p.clips[1].source_in, 7212.);
    assert_eq!(p.clips[0].value(10.).x, 50.);
    assert_eq!(p.clips[1].value(0.).x, 50.);
    p.validate().unwrap();
}
#[test]
fn ripple_frames_preserve_animation_on_both_sides() {
    let mut p = Project {
        clips: vec![
            Clip {
                duration: 10.,
                keyframes: vec![
                    Keyframe {
                        time: 0.,
                        transform: Transform {
                            x: 0.,
                            ..Transform::default()
                        },
                    },
                    Keyframe {
                        time: 10.,
                        transform: Transform {
                            x: 100.,
                            ..Transform::default()
                        },
                    },
                ],
                ..image_clip("a")
            },
            Clip {
                start: 10.,
                duration: 5.,
                track: 4,
                ..image_clip("b")
            },
        ],
        ..Project::default()
    };
    assert!(edit::remove_range(&mut p, 3., 5.));
    assert_eq!(p.clips.len(), 3);
    assert_eq!(p.clips[0].value(3.).x, 30.);
    assert_eq!(p.clips[1].value(0.).x, 50.);
    assert_eq!(p.clips[2].start, 8.);
    p.validate().unwrap();
}
#[test]
fn reject_malformed_projects() {
    let mut p = Project {
        clips: vec![image_clip("a"), image_clip("a")],
        ..Project::default()
    };
    assert!(p.validate().is_err());
    p.clips.pop();
    p.clips[0].transform.scale = f64::INFINITY;
    assert!(p.validate().is_err());
    p.clips[0].transform.scale = 1.;
    p.clips[0].asset_id = Some("../outside".into());
    assert!(p.validate().is_err());
    p.clips[0].asset_id = Some("demo-0".into());
    p.width = 1921;
    assert!(p.validate().is_err());
}
#[test]
fn storage_atomic_recovery_snapshots_and_instance_lock() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(tmp.path().into()).unwrap();
    assert!(Storage::open(tmp.path().into()).is_err());
    let p = Project {
        clips: vec![image_clip("a")],
        ..Project::default()
    };
    s.save(&p).unwrap();
    s.save(&p).unwrap();
    std::fs::write(tmp.path().join("projects/current.json"), b"broken JSON").unwrap();
    assert_eq!(s.load().unwrap().unwrap(), p);
    let snap = s.snapshot(&p).unwrap();
    assert_eq!(load_project(&snap).unwrap(), p);
    drop(s);
    assert!(Storage::open(tmp.path().into()).is_ok());
}
#[test]
fn previous_project_json_remains_compatible() {
    let p:Project=serde_json::from_str(r##"{"version":1,"name":"Old project","width":1920,"height":1080,"fps":30,"clips":[{"id":"clip","kind":"title","track":0,"start":0,"duration":5,"x":34,"y":46,"scale":0.8,"text":"Rocket"}],"muted_tracks":[],"hidden_tracks":[]}"##).unwrap();
    p.validate().unwrap();
    assert_eq!(p.clips[0].transform.x, 34.);
    assert_eq!(p.clips[0].transform.scale, 0.8);
}
