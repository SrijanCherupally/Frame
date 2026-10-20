use frame_studio::{
    animation::{EASINGS, Easing, MotionPath, Reveal, parse_chart_csv},
    edit,
    model::*,
    presets,
};
fn animated() -> Clip {
    Clip {
        kind: Kind::Graphic,
        duration: 4.,
        reveal: Reveal::LeftToRight,
        reveal_duration: 2.,
        keyframes: vec![
            Keyframe {
                time: 0.,
                easing: Easing::Bezier {
                    x1: 0.22,
                    y1: 0.,
                    x2: 0.36,
                    y2: 1.,
                },
                path: Some(MotionPath {
                    control1: [10., 90.],
                    control2: [80., 5.],
                }),
                transform: Transform {
                    x: 20.,
                    y: 30.,
                    scale: 0.6,
                    opacity: 0.2,
                    ..Transform::default()
                },
                ..Keyframe::default()
            },
            Keyframe {
                time: 4.,
                transform: Transform {
                    x: 80.,
                    y: 65.,
                    scale: 1.3,
                    opacity: 1.,
                    rotation: 70.,
                },
                ..Keyframe::default()
            },
        ],
        ..Clip::default()
    }
}
fn close(a: &Transform, b: &Transform) {
    for (a, b) in [
        (a.x, b.x),
        (a.y, b.y),
        (a.scale, b.scale),
        (a.rotation, b.rotation),
        (a.opacity, b.opacity),
    ] {
        assert!((a - b).abs() < 2e-5, "{a} != {b}");
    }
}
#[test]
fn easing_has_exact_endpoints_and_smooth_curves_settle_without_a_velocity_jump() {
    for &(e, _) in EASINGS {
        assert_eq!(e.sample(0.), 0.);
        assert_eq!(e.sample(1.), 1.);
        let samples: Vec<_> = (0..=1000).map(|i| e.sample(i as f64 / 1000.)).collect();
        assert!(samples.windows(2).all(|p| p[0] <= p[1]));
        for i in 1..1000 {
            let u = i as f64 / 1000.;
            assert!((e.reversed().sample(1. - u) - (1. - e.sample(u))).abs() < 2e-6);
        }
    }
    let h = 1e-4;
    for e in [Easing::Smooth, Easing::Smoother] {
        assert!(e.sample(h) / h < 0.001);
        assert!((1. - e.sample(1. - h)) / h < 0.001);
    }
}
#[test]
fn inserting_and_cutting_preserves_continuous_easing_and_curved_paths() {
    let original = animated();
    let mut inserted = original.clone();
    edit::add_keyframe(&mut inserted, 1.3, 30);
    for i in 0..=400 {
        let t = i as f64 / 100.;
        close(&original.value(t), &inserted.value(t));
    }
    let mut project = Project {
        clips: vec![inserted],
        ..Project::default()
    };
    let id = project.clips[0].id.clone();
    assert!(edit::split(&mut project, &id, 2.1));
    project.validate().unwrap();
    for c in &project.clips {
        for i in 0..=100 {
            let local = c.duration * i as f64 / 100.;
            close(&original.value(c.start + local), &c.value(local));
        }
    }
    assert_eq!(project.clips[1].reveal_offset, 2.1);
}
#[test]
fn ripple_trim_retime_reverse_and_paste_preserve_motion() {
    let original = animated();
    let mut p = Project {
        clips: vec![original.clone()],
        ..Project::default()
    };
    assert!(edit::remove_range(&mut p, 1., 2.));
    p.validate().unwrap();
    for i in 0..=100 {
        let t = i as f64 / 100.;
        close(&original.value(t), &p.clips[0].value(t));
        close(&original.value(t + 2.), &p.clips[1].value(t));
    }
    let trimmed = Clip {
        duration: 2.5,
        keyframes: edit::slice_keyframes(&original, 0.7, 3.2),
        ..original.clone()
    };
    for i in 0..=100 {
        let t = 2.5 * i as f64 / 100.;
        close(&original.value(0.7 + t), &trimmed.value(t));
    }
    let mut fast = original.clone();
    fast.set_speed(2., 60).unwrap();
    assert_eq!(fast.reveal_duration, 1.);
    for i in 0..=100 {
        let t = i as f64 / 50.;
        close(&original.value(t * 2.), &fast.value(t));
    }
    let mut reversed = original.clone();
    edit::reverse_animation(&mut reversed);
    for i in 0..=100 {
        let t = i as f64 / 25.;
        close(&original.value(4. - t), &reversed.value(t));
    }
    let mut target = Clip {
        duration: 8.,
        ..original.clone()
    };
    edit::paste_animation(&original, &mut target);
    for i in 0..=100 {
        let t = i as f64 / 25.;
        close(&original.value(t), &target.value(t * 2.));
    }
}
#[test]
fn slicing_dense_animation_stays_within_the_key_limit_and_preserves_holds() {
    let c = Clip {
        kind: Kind::Graphic,
        duration: 10.,
        keyframes: (0..200)
            .map(|i| Keyframe {
                time: 1. + i as f64 / 30.,
                transform: Transform {
                    x: i as f64 / 2.,
                    ..Transform::default()
                },
                ..Keyframe::default()
            })
            .collect(),
        ..Clip::default()
    };
    for (from, to) in [
        (0., 10.),
        (0., 2.5),
        (2.5, 10.),
        (2.5, 4.5),
        (0., 0.5),
        (9., 10.),
    ] {
        let slice = Clip {
            duration: to - from,
            keyframes: edit::slice_keyframes(&c, from, to),
            ..c.clone()
        };
        assert!(slice.keyframes.len() <= 200);
        for i in 0..=100 {
            let t = (to - from) * i as f64 / 100.;
            close(&c.value(from + t), &slice.value(t));
        }
    }
}
#[test]
fn presets_are_sparse_and_new_animation_data_round_trips() {
    for &(motion, _, _) in presets::MOTIONS {
        let mut c = animated();
        motion.apply(&mut c, 60);
        assert!(c.keyframes.len() <= 6);
        let p = Project {
            fps: 60,
            clips: vec![c],
            ..Project::default()
        };
        p.validate().unwrap();
        let json = serde_json::to_vec(&p).unwrap();
        assert_eq!(p, serde_json::from_slice::<Project>(&json).unwrap());
    }
    let key: Keyframe = serde_json::from_str(r#"{"time":1,"x":20,"y":30}"#).unwrap();
    assert_eq!(key.easing, Easing::Linear);
    assert_eq!(key.domain, [0., 1.]);
}
#[test]
fn invalid_curves_paths_and_csv_are_rejected() {
    let mut p = Project {
        clips: vec![animated()],
        ..Project::default()
    };
    p.clips[0].keyframes[0].domain = [0.5, 0.5];
    assert!(p.validate().is_err());
    p.clips[0] = animated();
    p.clips[0].keyframes[0].path.as_mut().unwrap().control1[0] = f64::NAN;
    assert!(p.validate().is_err());
    p.clips[0] = animated();
    p.clips[0].keyframes[0].easing = Easing::Bezier {
        x1: 1.1,
        y1: 0.,
        x2: 0.,
        y2: 1.,
    };
    assert!(p.validate().is_err());
    assert_eq!(
        parse_chart_csv("time,value\n0,12\n1,17\n2,10").unwrap(),
        vec![[0., 12.], [1., 17.], [2., 10.]]
    );
    for bad in [
        "time,value\n0,NaN\n1,2",
        "0,1\n0,2",
        "1,2\n0,3",
        "time,value\n1,2",
        "0,1,2\n1,2,3",
        "0,inf\n1,3",
    ] {
        assert!(parse_chart_csv(bad).is_err(), "{bad}");
    }
}

#[test]
fn detached_audio_preserves_source_timing_without_doubling_the_sound() {
    let mut p = Project {
        clips: vec![Clip {
            kind: Kind::Video,
            asset_id: Some("camera-1".into()),
            start: 2.,
            duration: 4.,
            speed: 2.,
            source_in: 7.,
            volume: 0.75,
            transition: Transition::Fade,
            ..Clip::default()
        }],
        ..Project::default()
    };
    let id = p.clips[0].id.clone();
    let detached = edit::detach_audio(&mut p, &id).unwrap();
    p.validate().unwrap();
    let a = p.clips.iter().find(|c| c.id == detached).unwrap();
    assert_eq!(
        (a.start, a.duration, a.speed, a.source_in, a.volume),
        (2., 4., 2., 7., 0.75)
    );
    assert_eq!(a.kind, Kind::Audio);
    assert_eq!(a.transition, Transition::Fade);
    assert_eq!(p.clips[0].volume, 0.);
    assert!(edit::detach_audio(&mut p, &id).is_none());
}
