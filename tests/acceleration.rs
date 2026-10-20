use frame_studio::{
    acceleration::{Encoder, Preferences, ThreeD},
    process::{ProcessGroup, command},
};
use std::{collections::HashMap, time::Duration};

#[test]
fn machine_preferences_are_portable_defaults_and_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        Preferences::load(dir.path()).unwrap(),
        Preferences::default()
    );
    let prefs = Preferences {
        encoder: Encoder::Nvidia,
        scene3d: ThreeD::Cpu,
    };
    prefs.save(dir.path()).unwrap();
    assert_eq!(Preferences::load(dir.path()).unwrap(), prefs);
    std::fs::write(
        dir.path().join("acceleration.json"),
        b"{\"encoder\":\"cpu\"}",
    )
    .unwrap();
    assert_eq!(Preferences::load(dir.path()).unwrap().scene3d, ThreeD::Auto);
    std::fs::write(
        dir.path().join("acceleration.json"),
        b"{\"encoder\":\"invalid\"}",
    )
    .unwrap();
    assert!(Preferences::load(dir.path()).is_err());
    let group = ProcessGroup::default();
    group.cancel();
    assert!(frame_studio::acceleration::choose(Encoder::Cpu, &group).is_err());
}

#[test]
fn cpu_selection_exports_without_hardware_and_preserves_existing_output() {
    use frame_studio::{
        model::{Clip, Graphic, Kind, Project},
        render,
    };
    let dir = tempfile::tempdir().unwrap();
    let project = Project {
        width: 320,
        height: 240,
        clips: vec![Clip {
            duration: 0.25,
            graphic: Graphic::Rectangle,
            kind: Kind::Graphic,
            ..Clip::default()
        }],
        ..Default::default()
    };
    let output = dir.path().join("output.mp4");
    let job = dir.path().join("job");
    let mut status = String::new();
    render::export_with_encoder(
        &project,
        &HashMap::new(),
        &job,
        &output,
        &ProcessGroup::default(),
        Encoder::Cpu,
        |_, s| status = s.into(),
    )
    .unwrap();
    assert!(status.contains("CPU H.264"));
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(job.join("acceleration.json")).unwrap()).unwrap();
    assert_eq!(report["encoder"], "cpu");
    let before = std::fs::read(&output).unwrap();
    assert!(
        render::export_with_encoder(
            &project,
            &HashMap::new(),
            &job,
            &output,
            &ProcessGroup::default(),
            Encoder::Cpu,
            |_, _| {}
        )
        .is_err()
    );
    assert_eq!(std::fs::read(output).unwrap(), before);
}

#[test]
fn process_timeout_stops_a_real_media_process_without_cancelling_parent() {
    let group = ProcessGroup::default();
    let error = group
        .run_timeout(
            command("ffmpeg").args([
                "-v",
                "error",
                "-re",
                "-f",
                "lavfi",
                "-i",
                "color=s=128x128:r=30",
                "-t",
                "60",
                "-f",
                "null",
                "-",
            ]),
            None,
            Duration::from_millis(300),
        )
        .unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert!(!group.cancelled());
    assert!(group.run(command("ffmpeg").arg("-version"), None).is_ok());
}

#[cfg(unix)]
#[test]
fn automatic_nvenc_failure_restarts_all_chunks_on_cpu_and_required_mode_fails() {
    use frame_studio::{
        document::{Document, Workspace},
        model::{Clip, Graphic, Kind, Project},
    };
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let project = Project {
        width: 240,
        height: 240,
        fps: 12,
        clips: vec![Clip {
            duration: 30.25,
            graphic: Graphic::Rectangle,
            kind: Kind::Graphic,
            ..Clip::default()
        }],
        ..Default::default()
    };
    let path = dir.path().join("source.frame");
    Document::capture(&project, &HashMap::new(), Workspace::default())
        .save(&path)
        .unwrap();
    let real = std::process::Command::new("which")
        .arg("ffmpeg")
        .output()
        .unwrap();
    let real = String::from_utf8(real.stdout).unwrap();
    // The first NVENC chunk succeeds via x264; the second fails. The app must
    // discard both and start at chunk zero using CPU, never concatenate mixed encoders.
    let shim = dir.path().join("ffmpeg");
    let count = dir.path().join("nvenc-count");
    let script = format!(
        r#"#!/usr/bin/env python3
import os,sys
args=sys.argv[1:]
if 'libx264' in args and '-progress' in args:
    with open({cpu_count:?},'a') as f: f.write('cpu\n')
if 'h264_nvenc' in args:
    if 'null' in args: sys.exit(0)
    counter={count:?}
    try: n=int(open(counter).read())
    except FileNotFoundError: n=0
    open(counter,'w').write(str(n+1))
    if n >= 1:
        print('Simulated device loss during second chunk',file=sys.stderr)
        sys.exit(1)
    i=args.index('-c:v')
    j=args.index('-c:a')
    args=args[:i]+['-c:v','libx264','-preset','ultrafast','-crf','20','-threads','2']+args[j:]
os.execv({real:?},[{real:?}]+args)
"#,
        count = count.to_string_lossy(),
        cpu_count = dir.path().join("cpu-count").to_string_lossy(),
        real = real.trim()
    );
    std::fs::write(&shim, script).unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    let root = dir.path().join("library");
    let output = dir.path().join("auto.mp4");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_frame"))
        .args(["--render"])
        .arg(&path)
        .arg(&output)
        .args(["--encoder", "auto"])
        .env("FRAME_DATA_DIR", &root)
        .env("FRAME_FFMPEG", &shim)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(std::fs::read_to_string(&count).unwrap(), "2");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("cpu-count"))
            .unwrap()
            .lines()
            .count(),
        2,
        "Both chunks must be re-encoded on CPU"
    );
    let job = std::fs::read_dir(root.join("jobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(job.join("acceleration.json")).unwrap()).unwrap();
    assert_eq!(report["encoder"], "cpu");
    assert!(
        report["fallback"]
            .as_str()
            .unwrap()
            .contains("second chunk")
    );
    let concat = std::fs::read_to_string(job.join("concat.txt")).unwrap();
    assert_eq!(concat.lines().count(), 2);
    let probe = ProcessGroup::default()
        .run(
            command("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-count_frames",
                    "-show_entries",
                    "stream=nb_read_frames,r_frame_rate",
                    "-of",
                    "json",
                ])
                .arg(&output),
            None,
        )
        .unwrap();
    let info: serde_json::Value = serde_json::from_slice(&probe).unwrap();
    assert_eq!(info["streams"][0]["nb_read_frames"], "363");
    assert_eq!(info["streams"][0]["r_frame_rate"], "12/1");
    let strict = dir.path().join("strict.mp4");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_frame"))
        .arg("--render")
        .arg(&path)
        .arg(&strict)
        .args(["--encoder", "nvidia"])
        .env("FRAME_DATA_DIR", dir.path().join("strict-library"))
        .env("FRAME_FFMPEG", &shim)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!strict.exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Simulated device loss"));
}
