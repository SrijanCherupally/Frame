import json
import logging
import os
import secrets
import shutil
import threading
import uuid
from concurrent.futures import ThreadPoolExecutor
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Annotated

from fastapi import FastAPI, File, HTTPException, Request, UploadFile
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, Field

from .demo import FONT, make_art
from .models import Project
from .render import render_project
from .runtime import PROCESSES, Cancelled

ROOT = Path(__file__).resolve().parent.parent
DATA = Path(os.environ.get("FRAME_DATA_DIR", ROOT / ".frame-data")).resolve()
DATA.mkdir(parents=True, exist_ok=True)
for directory in ["media", "jobs", "projects", "snapshots"]:
    (DATA / directory).mkdir(exist_ok=True)
POOL = ThreadPoolExecutor(max_workers=1, thread_name_prefix="media")
RENDER_POOL = ThreadPoolExecutor(max_workers=1, thread_name_prefix="export")
LOCK = threading.RLock()
JOBS = {}
LOG = logging.getLogger("frame")


def atomic_json(path, value):
    tmp = path.with_suffix(".tmp")
    with tmp.open("w") as f:
        json.dump(value, f, allow_nan=False)
        f.flush()
        os.fsync(f.fileno())
    tmp.replace(path)


def all_assets():
    result = {}
    for file in (DATA / "media").glob("*/meta.json"):
        try:
            a = json.loads(file.read_text())
            result[a["id"]] = a
        except (OSError, ValueError):
            LOG.warning("Unreadable asset metadata: %s", file.name)
    return result


def public(a):
    return {k: v for k, v in a.items() if k not in ("path", "proxy_path")}


def media_record(media_id):
    # IDs never become arbitrary filesystem paths.
    if not media_id.replace("-", "").isalnum():
        raise HTTPException(404, "Media not found")
    path = DATA / "media" / media_id / "meta.json"
    if not path.exists():
        raise HTTPException(404, "Media not found")
    return json.loads(path.read_text())


def process_media(asset):
    directory = DATA / "media" / asset["id"]
    try:
        result = PROCESSES.run(
            [
                "ffprobe",
                "-v",
                "error",
                "-show_format",
                "-show_streams",
                "-of",
                "json",
                asset["path"],
            ],
            group="media-" + asset["id"],
            timeout=60,
        )
        probe = json.loads(result)
        streams = probe["streams"]
        video = next((s for s in streams if s["codec_type"] == "video"), None)
        audio = next((s for s in streams if s["codec_type"] == "audio"), None)
        if not video and not audio:
            raise ValueError("No decodable video or audio stream found")
        image = Path(asset["path"]).suffix.lower() in [
            ".png",
            ".jpg",
            ".jpeg",
            ".webp",
            ".bmp",
            ".tiff",
            ".tif",
        ]
        asset.update(
            kind="image" if image else "video" if video else "audio",
            duration=float(probe.get("format", {}).get("duration", 5)),
            has_audio=bool(audio),
            width=video.get("width", 0) if video else 0,
            height=video.get("height", 0) if video else 0,
        )
        if video:
            thumb = directory / "thumb.jpg"
            PROCESSES.run(
                [
                    "ffmpeg",
                    "-v",
                    "error",
                    "-y",
                    "-threads",
                    "2",
                    "-i",
                    asset["path"],
                    "-frames:v",
                    "1",
                    "-vf",
                    "scale=480:-2",
                    "-threads",
                    "2",
                    str(thumb),
                ],
                group="media-" + asset["id"],
                timeout=60,
            )
        if video and not image:
            asset["status"] = "proxying"
            with LOCK:
                atomic_json(directory / "meta.json", asset)
            proxy = directory / "proxy.mp4"
            cmd = [
                "ffmpeg",
                "-v",
                "error",
                "-y",
                "-threads",
                "2",
                "-i",
                asset["path"],
                "-map",
                "0:v:0",
                "-map",
                "0:a:0?",
                "-vf",
                "scale=960:540:force_original_aspect_ratio=decrease:force_divisible_by=2",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "27",
                "-threads",
                "2",
                "-c:a",
                "aac",
                "-ac",
                "2",
                "-movflags",
                "+faststart",
                str(proxy),
            ]
            PROCESSES.run(cmd, group="media-" + asset["id"])
            asset["proxy_path"] = str(proxy)
        if video:
            try:
                num, den = map(float, video.get("avg_frame_rate", "30/1").split("/"))
                asset["fps"] = num / den if den else 30
            except (ValueError, ZeroDivisionError):
                asset["fps"] = 30
        if audio and not video:
            proxy = directory / "proxy.m4a"
            PROCESSES.run(
                [
                    "ffmpeg",
                    "-v",
                    "error",
                    "-y",
                    "-i",
                    asset["path"],
                    "-vn",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "128k",
                    "-threads",
                    "2",
                    str(proxy),
                ],
                group="media-" + asset["id"],
            )
            asset["proxy_path"] = str(proxy)
        asset["status"] = "ready"
    except Cancelled:
        asset.update(status="processing")
    except Exception:
        asset.update(
            status="error",
            error="This file could not be decoded or its preview could not be generated. Try converting it to MP4/H.264 or WAV.",
        )
        LOG.exception("Media processing failed for %s", asset["id"])
    with LOCK:
        atomic_json(directory / "meta.json", asset)


def seed():
    for i, name in enumerate(["Launch day", "The build", "Into the sky"]):
        directory = DATA / "media" / f"demo-{i}"
        directory.mkdir(exist_ok=True)
        if (directory / "meta.json").exists():
            continue
        path = directory / "original.png"
        make_art(path, i)
        shutil.copyfile(path, directory / "thumb.jpg")
        atomic_json(
            directory / "meta.json",
            {
                "id": f"demo-{i}",
                "name": name + ".png",
                "kind": "image",
                "duration": 6,
                "has_audio": False,
                "width": 1280,
                "height": 720,
                "path": str(path),
                "status": "ready",
                "size": path.stat().st_size,
                "demo": True,
            },
        )
    # Mark interrupted work explicitly instead of leaving infinite spinners after restart.
    for a in all_assets().values():
        if a["status"] in ("processing", "proxying"):
            POOL.submit(process_media, a)


@asynccontextmanager
async def lifespan(app):
    seed()
    for path in (DATA / "jobs").glob("*/job.json"):
        try:
            job = json.loads(path.read_text())
            if job["status"] in ("queued", "rendering"):
                job.update(
                    status="interrupted",
                    error="The app closed during this export. Start a new export.",
                )
                atomic_json(path, job)
            JOBS[job["id"]] = job
        except (OSError, ValueError, KeyError):
            LOG.warning("Unreadable export record")
    try:
        yield
    finally:
        PROCESSES.stop_all()
        POOL.shutdown(wait=True, cancel_futures=True)
        RENDER_POOL.shutdown(wait=True, cancel_futures=True)


app = FastAPI(title="Frame Studio", lifespan=lifespan)


@app.middleware("http")
async def local_mutations(request: Request, call_next):
    # Prevent a random website from using the local editor API through a browser.
    origin = request.headers.get("origin")
    if request.method not in ("GET", "HEAD", "OPTIONS") and origin:
        from urllib.parse import urlparse

        allowed_origins = set(
            os.environ.get(
                "FRAME_ALLOWED_ORIGINS", "http://127.0.0.1:5173,http://localhost:5173"
            ).split(",")
        )
        if (
            urlparse(origin).netloc != request.headers.get("host")
            and origin not in allowed_origins
        ):
            from fastapi.responses import JSONResponse

            return JSONResponse(
                {"detail": "Cross-origin writes are not allowed"}, status_code=403
            )
    return await call_next(request)


@app.get("/api/font")
def font():
    return FileResponse(FONT, media_type="font/ttf")


@app.get("/api/health")
def health():
    return {
        "status": "ok",
        "ffmpeg": bool(shutil.which("ffmpeg")),
        "disk_free": shutil.disk_usage(DATA).free,
    }


@app.get("/api/media")
def list_media():
    return [public(a) for a in all_assets().values()]


@app.post("/api/media")
async def upload(file: Annotated[UploadFile, File()]):
    mid = uuid.uuid4().hex
    directory = DATA / "media" / mid
    directory.mkdir()
    suffix = Path(file.filename or "").suffix.lower()[:12]
    if not suffix.replace(".", "").isalnum():
        suffix = ".bin"
    path = directory / f"original{suffix}"
    size = 0
    try:
        with path.open("wb") as out:
            while chunk := await file.read(8 * 1024 * 1024):
                if shutil.disk_usage(DATA).free < len(chunk) + 256 * 1024 * 1024:
                    raise HTTPException(
                        507,
                        "Not enough disk space. Use a larger FRAME_DATA_DIR volume.",
                    )
                out.write(chunk)
                size += len(chunk)
        asset = {
            "id": mid,
            "name": (file.filename or "Media")[:300],
            "path": str(path),
            "size": size,
            "status": "processing",
            "kind": "video",
            "duration": 0,
            "has_audio": False,
        }
        with LOCK:
            atomic_json(directory / "meta.json", asset)
        POOL.submit(process_media, asset)
        return public(asset)
    except Exception:
        shutil.rmtree(directory, ignore_errors=True)
        raise
    finally:
        await file.close()


class LocalMedia(BaseModel):
    path: str = Field(max_length=10000)


def desktop_only(request):
    token = os.environ.get("FRAME_DESKTOP_TOKEN")
    if not token or not secrets.compare_digest(
        request.headers.get("X-Frame-Token", ""), token
    ):
        raise HTTPException(
            403, "Local file linking is available through the desktop file picker only"
        )


def local_asset(path, mid=None):
    source = Path(path).expanduser().resolve()
    if not source.is_file():
        raise HTTPException(
            400, "File not found. Reconnect the media drive and try again."
        )
    mid = mid or uuid.uuid4().hex
    directory = DATA / "media" / mid
    directory.mkdir(exist_ok=True)
    # Link the original by path. Only derived proxies live in the cache; no original is copied.
    asset = {
        "id": mid,
        "name": source.name[:300],
        "path": str(source),
        "size": source.stat().st_size,
        "status": "processing",
        "kind": "video",
        "duration": 0,
        "has_audio": False,
        "linked": True,
    }
    with LOCK:
        atomic_json(directory / "meta.json", asset)
    POOL.submit(process_media, asset)
    return public(asset)


@app.post("/api/media/link")
def link_media(media: LocalMedia, request: Request):
    desktop_only(request)
    return local_asset(media.path)


@app.post("/api/media/{mid}/relink")
def relink_media(mid: str, media: LocalMedia, request: Request):
    desktop_only(request)
    old = media_record(mid)
    if old["status"] in ("processing", "proxying"):
        raise HTTPException(409, "Wait for media processing before relinking")
    return local_asset(media.path, mid)


@app.get("/api/media/{mid}/{variant}")
def get_media(mid: str, variant: str):
    a = media_record(mid)
    directory = DATA / "media" / mid
    if variant == "thumb":
        path = directory / "thumb.jpg"
    elif variant == "preview":
        path = Path(a.get("proxy_path") or a["path"])
    elif variant == "original":
        path = Path(a["path"])
    else:
        raise HTTPException(404, "Unknown media variant")
    if not path.exists():
        raise HTTPException(404, "Preview is not ready")
    return FileResponse(path)


@app.put("/api/project")
def save_project(project: Project):
    with LOCK:
        current = DATA / "projects" / "current.json"
        if current.exists():
            try:
                atomic_json(
                    current.with_name("previous.json"), json.loads(current.read_text())
                )
            except (ValueError, OSError):
                LOG.warning("Previous project could not be backed up")
        atomic_json(current, project.model_dump())
    return {"saved": True}


@app.get("/api/project")
def get_project():
    path = DATA / "projects" / "current.json"
    if not path.exists():
        return None
    try:
        return Project.model_validate_json(path.read_text()).model_dump()
    except (ValueError, OSError):
        previous = path.with_name("previous.json")
        if previous.exists():
            try:
                return Project.model_validate_json(previous.read_text()).model_dump()
            except (ValueError, OSError):
                pass
        raise HTTPException(
            500,
            "Project file is unreadable. Restore a .frame.json backup; media originals have not been changed.",
        ) from None


@app.post("/api/snapshots")
def create_snapshot(project: Project):
    sid = uuid.uuid4().hex
    with LOCK:
        atomic_json(DATA / "snapshots" / f"{sid}.json", project.model_dump())
    return {"id": sid, "name": project.name}


@app.get("/api/snapshots")
def snapshots():
    result = []
    for file in sorted(
        (DATA / "snapshots").glob("*.json"),
        key=lambda p: p.stat().st_mtime,
        reverse=True,
    ):
        try:
            p = json.loads(file.read_text())
            result.append(
                {
                    "id": file.stem,
                    "name": p["name"],
                    "clips": len(p["clips"]),
                    "modified": file.stat().st_mtime,
                }
            )
        except (OSError, ValueError, KeyError):
            continue
    return result


@app.get("/api/snapshots/{sid}")
def get_snapshot(sid: str):
    if len(sid) != 32 or not sid.isalnum():
        raise HTTPException(404, "Snapshot not found")
    path = DATA / "snapshots" / f"{sid}.json"
    if not path.exists():
        raise HTTPException(404, "Snapshot not found")
    return Project.model_validate_json(path.read_text()).model_dump()


@app.post("/api/exports")
def export(project: Project):
    if not project.clips:
        raise HTTPException(400, "Add clips before exporting")
    if shutil.disk_usage(DATA).free < 256 * 1024 * 1024:
        raise HTTPException(
            507, "Free more space on the media cache volume before exporting."
        )
    # One export at a time avoids exhausting memory on many overlapping 4K layers.
    with LOCK:
        if any(j["status"] in ("queued", "rendering") for j in JOBS.values()):
            raise HTTPException(409, "An export is already running")
        assets = all_assets()
        for c in project.clips:
            if c.asset_id and c.asset_id not in assets:
                raise HTTPException(400, f"Missing media: {c.name}")
            if c.asset_id and not Path(assets[c.asset_id]["path"]).is_file():
                raise HTTPException(
                    400,
                    f"Original file is offline: {c.name}. Reconnect the drive or relink the media.",
                )
            if c.asset_id and assets[c.asset_id]["status"] not in ("ready", "proxying"):
                raise HTTPException(400, f"Media is not ready: {c.name}")
            if c.asset_id and c.kind in ("video", "audio"):
                available = assets[c.asset_id].get("duration", 0)
                if c.source_in + c.duration * c.speed > available + max(
                    0.1, c.speed / project.fps
                ):
                    raise HTTPException(
                        400,
                        f"{c.name} extends beyond its original media. Trim the clip or relink the correct source.",
                    )
        jid = uuid.uuid4().hex
        JOBS[jid] = {"id": jid, "status": "queued", "progress": 0}
    directory = DATA / "jobs" / jid
    try:
        directory.mkdir(exist_ok=True)
    except OSError as error:
        JOBS[jid].update(status="error", error="Could not create export folder")
        raise HTTPException(
            507, "Could not create export folder. Check disk space and permissions."
        ) from error

    def update(**fields):
        with LOCK:
            JOBS[jid].update(**fields)
            atomic_json(directory / "job.json", JOBS[jid])

    try:
        update()
    except OSError as error:
        JOBS[jid].update(status="error", error="Could not save export state")
        raise HTTPException(
            507, "Could not save export state. Check disk space and permissions."
        ) from error

    def run():
        try:
            update(status="rendering")
            render_project(
                project, assets, directory, lambda p: update(progress=p), group=jid
            )
            update(status="done", progress=1)
        except Cancelled:
            update(
                status="cancelled",
                error="Export cancelled. Your project and originals are unchanged.",
            )
        except Exception:
            LOG.exception("Export failed: %s", jid)
            update(
                status="error",
                error="Export failed. Check the engine log and jobs/"
                + jid
                + "/ in your media cache folder for render logs.",
            )

    RENDER_POOL.submit(run)
    return JOBS[jid]


@app.get("/api/exports/{jid}")
def job_status(jid: str):
    if jid not in JOBS:
        raise HTTPException(404, "Export not found; the server may have restarted")
    return JOBS[jid]


@app.post("/api/exports/{jid}/cancel")
def cancel_export(jid: str):
    job = job_status(jid)
    if job["status"] in ("queued", "rendering"):
        PROCESSES.cancel(jid)
    return {"cancel_requested": True}


@app.get("/api/exports/{jid}/download")
def download(jid: str):
    j = job_status(jid)
    if j["status"] != "done":
        raise HTTPException(409, "Export is not finished")
    return FileResponse(
        DATA / "jobs" / jid / "frame-export.mp4",
        filename="frame-export.mp4",
        media_type="video/mp4",
    )


if (ROOT / "dist").exists():
    app.mount("/", StaticFiles(directory=ROOT / "dist", html=True), name="frontend")
