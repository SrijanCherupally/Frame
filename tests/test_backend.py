import hashlib
import json
import os
import subprocess
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

# Isolate every test run from the editor's real library and saved project.
TEMP = tempfile.TemporaryDirectory(prefix="frame-test-")
os.environ["FRAME_DATA_DIR"] = TEMP.name
from fastapi.testclient import TestClient

from backend.app import DATA, app
from backend.models import Clip, Keyframe, Project
from backend.render import render_project


class BackendTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.client = TestClient(app)
        cls.client.__enter__()
        cls.source = Path(TEMP.name) / "fixture.mp4"
        subprocess.run(
            [
                "ffmpeg",
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=30:duration=4",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=4",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-threads",
                "2",
                "-c:a",
                "aac",
                "-shortest",
                str(cls.source),
            ],
            check=True,
        )
        with cls.source.open("rb") as f:
            response = cls.client.post(
                "/api/media", files={"file": ("fixture.mp4", f, "video/mp4")}
            )
        assert response.status_code == 200, response.text
        cls.media = response.json()["id"]
        cls.wait_media(cls.media)

    @classmethod
    def tearDownClass(cls):
        cls.client.__exit__(None, None, None)
        TEMP.cleanup()

    @classmethod
    def wait_media(cls, mid):
        until = time.monotonic() + 30
        while time.monotonic() < until:
            a = next(a for a in cls.client.get("/api/media").json() if a["id"] == mid)
            if a["status"] == "error":
                return a
            if a["status"] == "ready":
                return a
            time.sleep(0.1)
        raise AssertionError("Media processing did not finish")

    def wait_job(self, jid, timeout=60):
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            job = self.client.get("/api/exports/" + jid).json()
            if job["status"] not in ["queued", "rendering"]:
                return job
            time.sleep(0.1)
        self.fail("Export did not finish")

    def render(self, clips, **settings):
        p = Project(width=320, height=240, clips=clips, **settings)
        r = self.client.post("/api/exports", json=p.model_dump())
        self.assertEqual(r.status_code, 200, r.text)
        job = self.wait_job(r.json()["id"])
        self.assertEqual(job["status"], "done", job)
        file = DATA / "jobs" / job["id"] / "frame-export.mp4"
        result = subprocess.run(
            [
                "ffprobe",
                "-v",
                "error",
                "-show_format",
                "-show_streams",
                "-of",
                "json",
                str(file),
            ],
            capture_output=True,
            check=True,
        )
        return file, json.loads(result.stdout)

    def test_01_streaming_media_and_native_link(self):
        assets = self.client.get("/api/media").json()
        a = next(a for a in assets if a["id"] == self.media)
        self.assertEqual(a["kind"], "video")
        self.assertTrue(a["has_audio"])
        self.assertNotIn("path", a)
        r = self.client.get(
            f"/api/media/{self.media}/preview", headers={"Range": "bytes=0-127"}
        )
        self.assertEqual(r.status_code, 206)
        self.assertEqual(len(r.content), 128)
        r = self.client.post("/api/media/link", json={"path": str(self.source)})
        self.assertEqual(r.status_code, 403)
        with patch.dict(os.environ, {"FRAME_DESKTOP_TOKEN": "test-only-token"}):
            r = self.client.post(
                "/api/media/link",
                json={"path": str(self.source)},
                headers={"X-Frame-Token": "test-only-token"},
            )
        self.assertEqual(r.status_code, 200, r.text)
        linked = r.json()
        self.assertEqual(self.wait_media(linked["id"])["status"], "ready")
        metadata = json.loads((DATA / "media" / linked["id"] / "meta.json").read_text())
        self.assertEqual(Path(metadata["path"]), self.source)
        self.assertFalse(any((DATA / "media" / linked["id"]).glob("original*")))

    def test_02_bad_import_is_recoverable(self):
        r = self.client.post(
            "/api/media",
            files={"file": ("broken.mp4", b"not a real video", "video/mp4")},
        )
        a = self.wait_media(r.json()["id"])
        self.assertEqual(a["status"], "error")
        self.assertIn("decoded", a["error"])
        self.assertEqual(self.client.get("/api/health").status_code, 200)

    def test_03_project_validation_and_recovery(self):
        p = Project(
            name="Recovery test", clips=[Clip(id="a", kind="image", asset_id="demo-0")]
        ).model_dump()
        for _ in range(2):
            self.assertEqual(self.client.put("/api/project", json=p).status_code, 200)
        self.assertEqual(
            self.client.get("/api/project").json()["name"], "Recovery test"
        )
        (DATA / "projects" / "current.json").write_text("{interrupted write")
        self.assertEqual(
            self.client.get("/api/project").json()["name"], "Recovery test"
        )
        self.client.put("/api/project", json=p)
        snapshot = self.client.post("/api/snapshots", json=p).json()
        self.assertEqual(
            self.client.get("/api/snapshots/" + snapshot["id"]).json()["name"],
            "Recovery test",
        )
        self.assertEqual(len(self.client.get("/api/snapshots").json()), 1)
        bad = {**p, "clips": [p["clips"][0], p["clips"][0]]}
        self.assertEqual(self.client.put("/api/project", json=bad).status_code, 422)
        r = self.client.put(
            "/api/project", json=p, headers={"Origin": "https://untrusted.example"}
        )
        self.assertEqual(r.status_code, 403)
        missing = Project(
            clips=[Clip(id="bad", asset_id="does-not-exist")]
        ).model_dump()
        self.assertEqual(
            self.client.post("/api/exports", json=missing).status_code, 400
        )

    def test_04_composite_animation_masks_and_every_transition(self):
        clips = []
        for i, transition in enumerate(["none", "fade", "slide", "zoom", "wipe"]):
            clips.append(
                Clip(
                    id=str(i),
                    kind="video",
                    asset_id=self.media,
                    start=i * 0.4,
                    duration=0.4,
                    source_in=0.2,
                    transition=transition,
                    fade_duration=0.1,
                    mask="circle" if i == 2 else "rectangle" if i == 3 else "none",
                    brightness=0.03,
                    contrast=1.1,
                    saturation=0.8,
                    warmth=0.2,
                    blur=0.5,
                    keyframes=[
                        Keyframe(time=0, x=40, scale=0.8),
                        Keyframe(time=0.4, x=60, scale=1, rotation=5),
                    ],
                )
            )
        clips += [
            Clip(
                id="title",
                kind="title",
                track=0,
                duration=2,
                text="Rocket 'flight': 100%\nA:B, C\\D",
                font_size=50,
                transition="fade",
            ),
            Clip(
                id="arrow",
                kind="graphic",
                graphic="arrow",
                track=0,
                start=1,
                duration=0.5,
                color="#c6e99d",
            ),
        ]
        _, probe = self.render(clips)
        self.assertAlmostEqual(float(probe["format"]["duration"]), 2, delta=0.1)
        self.assertTrue(any(s["codec_type"] == "audio" for s in probe["streams"]))

    def test_05_clean_timelapse_and_slowdown_have_correct_durations(self):
        clean, _ = self.render(
            [
                Clip(
                    id="fast",
                    asset_id=self.media,
                    duration=0.5,
                    speed=8,
                    motion_blur=True,
                    deflicker=True,
                )
            ]
        )
        raw, _ = self.render(
            [Clip(id="raw", asset_id=self.media, duration=0.5, speed=8)]
        )

        def frame(path):
            return subprocess.run(
                [
                    "ffmpeg",
                    "-v",
                    "error",
                    "-ss",
                    "0.25",
                    "-i",
                    str(path),
                    "-frames:v",
                    "1",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgb24",
                    "-",
                ],
                capture_output=True,
                check=True,
            ).stdout

        a, b = frame(clean), frame(raw)
        self.assertEqual(len(a), 320 * 240 * 3)
        self.assertGreater(
            sum(abs(x - y) for x, y in zip(a, b, strict=False)) / len(a),
            0.5,
            "Blending must affect actual pixels",
        )
        _, probe = self.render(
            [Clip(id="slow", asset_id=self.media, duration=2, speed=0.25)]
        )
        self.assertAlmostEqual(float(probe["format"]["duration"]), 2, delta=0.1)

    def test_06_four_hours_to_twenty_seconds(self):
        # A sparse-frame 4-hour source tests real long timestamps without a multi-GB fixture.
        path = Path(TEMP.name) / "four-hours.mp4"
        subprocess.run(
            [
                "ffmpeg",
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=320x240:r=1/60:d=14400",
                "-c:v",
                "libx264",
                "-threads",
                "2",
                "-preset",
                "ultrafast",
                str(path),
            ],
            check=True,
        )
        assets = {"long": {"path": str(path), "has_audio": False, "fps": 1 / 60}}
        p = Project(
            width=320,
            height=240,
            clips=[
                Clip(
                    id="long", asset_id="long", speed=720, duration=20, motion_blur=True
                )
            ],
        )
        out = render_project(
            p,
            assets,
            Path(TEMP.name) / "long-export",
            lambda _: None,
            group="long-test",
        )
        probe = json.loads(
            subprocess.run(
                [
                    "ffprobe",
                    "-v",
                    "error",
                    "-show_format",
                    "-show_streams",
                    "-of",
                    "json",
                    str(out),
                ],
                capture_output=True,
                check=True,
            ).stdout
        )
        self.assertAlmostEqual(float(probe["format"]["duration"]), 20, delta=0.1)
        video = next(s for s in probe["streams"] if s["codec_type"] == "video")
        self.assertEqual(int(video["nb_frames"]), 600)

    def test_07_cancellation_protects_originals(self):
        before = hashlib.sha256(self.source.read_bytes()).hexdigest()
        p = Project(
            width=1920,
            height=1080,
            clips=[Clip(id="long", kind="image", asset_id="demo-0", duration=300)],
        ).model_dump()
        r = self.client.post("/api/exports", json=p)
        self.assertEqual(r.status_code, 200, r.text)
        jid = r.json()["id"]
        self.assertEqual(
            self.client.post("/api/exports/" + jid + "/cancel").status_code, 200
        )
        job = self.wait_job(jid)
        self.assertEqual(job["status"], "cancelled", job)
        self.assertEqual(
            self.client.get("/api/exports/" + jid + "/download").status_code, 409
        )
        self.assertEqual(before, hashlib.sha256(self.source.read_bytes()).hexdigest())

    def test_08_chunked_timeline_retains_content_after_boundary(self):
        clips = [
            Clip(
                id="background",
                asset_id="demo-0",
                kind="image",
                duration=31,
                transition="fade",
                keyframes=[Keyframe(time=0, opacity=0.8), Keyframe(time=31, opacity=1)],
            ),
            Clip(
                id="late",
                kind="title",
                track=0,
                start=29,
                duration=2,
                text="After the boundary",
                transition="slide",
                font_size=100,
            ),
        ]
        out, probe = self.render(clips)
        self.assertAlmostEqual(float(probe["format"]["duration"]), 31, delta=0.1)
        frame = subprocess.run(
            [
                "ffmpeg",
                "-v",
                "error",
                "-ss",
                "30.5",
                "-i",
                str(out),
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-threads",
                "1",
                "-",
            ],
            capture_output=True,
            check=True,
        ).stdout
        self.assertEqual(len(frame), 320 * 240 * 3)
        self.assertGreater(
            sum(frame) / len(frame),
            15,
            "The second render window must contain the image, not a black gap",
        )

    def test_09_progress_failure_releases_render_process(self):
        from backend.runtime import PROCESSES

        p = Project(
            width=320,
            height=240,
            clips=[Clip(id="still", kind="image", asset_id="demo-0", duration=2)],
        )
        media = json.loads((DATA / "media" / "demo-0" / "meta.json").read_text())

        def fail(_progress):
            raise OSError("Simulated unavailable job-state disk")

        with self.assertRaises(OSError):
            render_project(
                p,
                {"demo-0": media},
                Path(TEMP.name) / "failure-export",
                fail,
                group="test-failure",
            )
        self.assertNotIn("test-failure", PROCESSES.children.values())

    def test_10_disk_full_export_reports_actionable_error(self):
        from collections import namedtuple

        Usage = namedtuple("Usage", "total used free")
        p = Project(
            clips=[Clip(id="still", kind="image", asset_id="demo-0")]
        ).model_dump()
        with patch("backend.app.shutil.disk_usage", return_value=Usage(100, 100, 0)):
            response = self.client.post("/api/exports", json=p)
        self.assertEqual(response.status_code, 507)
        self.assertIn("space", response.json()["detail"])


if __name__ == "__main__":
    unittest.main()
