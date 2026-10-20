//! Regenerate the editable, media-free example projects shipped with Frame.
use frame_studio::{
    document::{Document, Workspace},
    model::Project,
    presets::SCENES,
};
use std::{collections::HashMap, path::Path};
fn main() -> anyhow::Result<()> {
    std::fs::create_dir_all("examples/projects")?;
    for &(scene, name, _) in SCENES {
        let p = Project {
            name: name.into(),
            clips: scene.clips(0., 30),
            ..Project::default()
        };
        let workspace = Workspace {
            playhead: 1.,
            zoom: 70.,
            selected: p.clips.first().map(|c| c.id.clone()),
        };
        Document::capture(&p, &HashMap::new(), workspace).save(
            &Path::new("examples/projects")
                .join(format!("{}.frame", name.to_lowercase().replace(' ', "-"))),
        )?;
    }
    use frame_studio::{
        animation::Easing,
        model::{Clip, Graphic, Keyframe, Kind, Transform},
        scene3d::{Object, Pose, PoseKey, Primitive, Scene},
        vector::{Node, VectorShape},
    };
    let vector = VectorShape::default();
    let morph = VectorShape {
        nodes: vec![
            Node::corner([50., 15.]),
            Node::corner([80., 50.]),
            Node::corner([50., 85.]),
            Node::corner([20., 50.]),
        ],
        target: vec![
            Node::corner([25., 25.]),
            Node::corner([75., 25.]),
            Node::corner([75., 75.]),
            Node::corner([25., 75.]),
        ],
        closed: true,
        draw_duration: 0.,
        morph_duration: 3.,
        ..VectorShape::default()
    };
    let mut scene = Scene {
        orbit_speed: 8.,
        ..Scene::default()
    };
    for (x, color) in [(-1.6, "#56dae0"), (1.6, "#ffc675")] {
        scene.objects.push(Object {
            name: "Exploded component".into(),
            primitive: Primitive::Cube,
            color: color.into(),
            pose: Pose {
                scale: 0.3,
                ..Pose::default()
            },
            spin: [0., 15., 0.],
            keys: vec![
                PoseKey {
                    time: 0.,
                    pose: Pose {
                        scale: 0.3,
                        ..Pose::default()
                    },
                    easing: Easing::Smoother,
                },
                PoseKey {
                    time: 2.,
                    pose: Pose {
                        position: [x, 0., 0.],
                        scale: 0.3,
                        ..Pose::default()
                    },
                    easing: Easing::Linear,
                },
            ],
            ..Object::default()
        });
    }
    let motion = Clip {
        kind: Kind::Graphic,
        graphic: Graphic::Circle,
        fill: true,
        secondary_color: "#56dae0".into(),
        keyframes: vec![
            Keyframe {
                time: 0.,
                transform: Transform {
                    x: 20.,
                    scale: 0.25,
                    ..Transform::default()
                },
                easing: Easing::Smoother,
                ..Keyframe::default()
            },
            Keyframe {
                time: 3.,
                transform: Transform {
                    x: 80.,
                    scale: 0.25,
                    ..Transform::default()
                },
                ..Keyframe::default()
            },
        ],
        ..Clip::default()
    };
    for (name, clip, samples) in [
        (
            "Vector draw-on",
            Clip {
                vector: Some(vector.clone()),
                ..Clip::default()
            },
            1,
        ),
        (
            "Vector morph",
            Clip {
                vector: Some(morph),
                fill: true,
                secondary_color: "#164b65".into(),
                ..Clip::default()
            },
            2,
        ),
        (
            "3D rocket assembly",
            Clip {
                scene3d: Some(scene),
                ..Clip::default()
            },
            2,
        ),
        ("Motion blur study", motion, 4),
    ] {
        let c = Clip {
            kind: Kind::Graphic,
            track: 0,
            name: name.into(),
            duration: 5.,
            color: "#56dae0".into(),
            ..clip
        };
        let p = Project {
            name: name.into(),
            width: 1280,
            height: 720,
            motion_blur_samples: samples,
            clips: vec![c],
            ..Project::default()
        };
        p.validate()?;
        let selected = Some(p.clips[0].id.clone());
        Document::capture(
            &p,
            &HashMap::new(),
            Workspace {
                playhead: 1.,
                zoom: 100.,
                selected,
            },
        )
        .save(
            &Path::new("examples/projects")
                .join(format!("{}.frame", name.to_lowercase().replace(' ', "-"))),
        )?;
    }
    std::fs::create_dir_all("examples/vfx")?;
    std::fs::write("examples/vfx/flight-path.svg", vector.svg())?;
    std::fs::write(
        "examples/vfx/pyramid.obj",
        "# Original sample mesh; positions normalized on import\nv -1 -1 -1\nv 1 -1 -1\nv 1 -1 1\nv -1 -1 1\nv 0 1 0\nf 1 2 5\nf 2 3 5\nf 3 4 5\nf 4 1 5\nf 4 3 2 1\n",
    )?;
    let practice = Path::new("examples/vfx/tracking-practice.mp4");
    if practice.is_file() {
        let asset =
            frame_studio::media::probe(practice, &frame_studio::process::ProcessGroup::default())?;
        let video = Clip {
            kind: Kind::Video,
            name: "Track the patterned marker".into(),
            asset_id: Some(asset.id.clone()),
            duration: 3.,
            track: 1,
            ..Clip::default()
        };
        let label = Clip {
            kind: Kind::Title,
            name: "Tracking label".into(),
            text: "FOLLOW THIS TARGET".into(),
            font_size: 60,
            track: 0,
            duration: 3.,
            transform: Transform {
                x: 50.,
                y: 20.,
                ..Transform::default()
            },
            ..Clip::default()
        };
        let selected = Some(video.id.clone());
        let p = Project {
            name: "Tracking practice".into(),
            width: 1280,
            height: 720,
            clips: vec![video, label],
            ..Project::default()
        };
        p.validate()?;
        Document::capture(
            &p,
            &HashMap::from([(asset.id.clone(), asset)]),
            Workspace {
                playhead: 0.,
                zoom: 150.,
                selected,
            },
        )
        .save(Path::new("examples/vfx/tracking-practice.frame"))?;
    }
    Ok(())
}
