"""FFmpeg renderer: bounded workers, disk originals, explicit numeric filter expressions."""

import itertools
import math
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from .demo import FONT
from .runtime import PROCESSES, Cancelled


def animated(clip, prop, clock="t"):
    points = sorted({k.time: getattr(k, prop) for k in clip.keyframes}.items())
    if not points:
        return str(getattr(clip, prop))
    expr = str(points[-1][1])
    for (t0, v0), (t1, v1) in reversed(list(itertools.pairwise(points))):
        expr = f"if(lt({clock},{t1}),{v0}+({v1 - v0})*({clock}-{t0})/{t1 - t0},{expr})"
    return f"if(lt({clock},{points[0][0]}),{points[0][1]},{expr})"


def graphic_image(clip, path, w, h):
    im = Image.new("RGBA", (w, h))
    d = ImageDraw.Draw(im)
    col = clip.color
    if clip.kind == "title":
        font = ImageFont.truetype(FONT, max(12, round(clip.font_size * w / 1920)))
        box = d.multiline_textbbox(
            (0, 0), clip.text, font=font, spacing=10, align="center"
        )
        d.multiline_text(
            (w / 2 - (box[2] - box[0]) / 2, h / 2 - (box[3] - box[1]) / 2 - box[1]),
            clip.text,
            font=font,
            fill=col,
            spacing=10,
            align="center",
            stroke_width=0,
        )
    elif clip.graphic == "arrow":
        d.line((w * 0.3, h * 0.5, w * 0.7, h * 0.5), fill=col, width=max(3, w // 180))
        d.line(
            (w * 0.62, h * 0.4, w * 0.7, h * 0.5, w * 0.62, h * 0.6),
            fill=col,
            width=max(3, w // 180),
        )
    elif clip.graphic == "circle":
        d.ellipse(
            (w * 0.35, h * 0.23, w * 0.65, h * 0.77),
            outline=col,
            width=max(3, w // 180),
        )
    elif clip.graphic == "rectangle":
        d.rectangle(
            (w * 0.25, h * 0.25, w * 0.75, h * 0.75),
            outline=col,
            width=max(3, w // 180),
        )
    else:
        d.rectangle((w * 0.06, h * 0.72, w * 0.65, h * 0.9), fill=(14, 21, 24, 225))
        d.rectangle((w * 0.06, h * 0.72, w * 0.066, h * 0.9), fill=col)
        font = ImageFont.truetype(FONT, max(12, round(40 * w / 1920)))
        d.text((w * 0.085, h * 0.77), clip.text[:80], font=font, fill=col)
    im.save(path)


def render_window(
    project, assets, folder, update, group="export", origin=0, window_duration=None
):
    folder = Path(folder)
    folder.mkdir(parents=True, exist_ok=True)
    duration = (
        window_duration
        if window_duration is not None
        else max((c.start + c.duration for c in project.clips), default=0)
    )
    if not duration:
        raise ValueError("Add clips before exporting")
    w, h, fps = project.width, project.height, project.fps
    cmd = [
        "ffmpeg",
        "-hide_banner",
        "-y",
        "-filter_complex_threads",
        "2",
        "-threads",
        "2",
        "-f",
        "lavfi",
        "-i",
        f"color=c=0x090c10:s={w}x{h}:r={fps}:d={duration}",
        "-f",
        "lavfi",
        "-i",
        f"anullsrc=r=48000:cl=stereo:d={duration}",
    ]
    filters = ["[0:v]format=rgba[base]"]
    base = "base"
    audios = ["[1:a]"]
    index = 2
    for clip in sorted(project.clips, key=lambda c: (-c.track, c.start)):
        if clip.start + clip.duration <= origin or clip.start >= origin + duration:
            continue
        phase = max(0, origin - clip.start)
        local_start = max(0, clip.start - origin)
        local_duration = min(clip.start + clip.duration, origin + duration) - max(
            clip.start, origin
        )
        if clip.track in project.hidden_tracks and clip.kind != "audio":
            continue
        a = assets.get(clip.asset_id) if clip.asset_id else None
        if clip.kind in ("video", "audio", "image") and not a:
            raise ValueError(
                f"Missing media for {clip.name}; relink or import the file"
            )
        if clip.kind in ("title", "graphic"):
            path = folder / f"graphic-{index}.png"
            graphic_image(clip, path, w, h)
            cmd += [
                "-loop",
                "1",
                "-framerate",
                str(fps),
                "-t",
                str(local_duration),
                "-threads",
                "2",
                "-i",
                str(path),
            ]
        elif clip.kind == "image":
            cmd += [
                "-loop",
                "1",
                "-framerate",
                str(fps),
                "-t",
                str(local_duration),
                "-threads",
                "2",
                "-i",
                a["path"],
            ]
        else:
            cmd += [
                "-ss",
                str(clip.source_in + phase * clip.speed),
                "-t",
                str(local_duration * clip.speed),
                "-threads",
                "2",
                "-i",
                a["path"],
            ]
        idx = index
        index += 1
        if (
            a
            and a.get("has_audio")
            and clip.kind in ("video", "audio")
            and clip.track not in project.muted_tracks
            and clip.volume > 0
        ):
            speed = clip.speed
            tempo = []
            while speed > 2:
                tempo.append("atempo=2")
                speed /= 2
            while speed < 0.5:
                tempo.append("atempo=0.5")
                speed *= 2
            tempo.append(f"atempo={speed}")
            fade = min(clip.fade_duration, clip.duration / 2)
            af = (
                f"[{idx}:a]asetpts=PTS-STARTPTS,"
                + ",".join(tempo)
                + f",aresample=48000,volume={clip.volume},atrim=duration={local_duration}"
            )
            if clip.transition == "fade":
                af += f",volume='if(isnan(t),0,max(0,min(1,min((t+{phase})/{fade},({clip.duration}-t-{phase})/{fade}))))':eval=frame"
            af += f",adelay={round(local_start * 1000)}:all=1[a{idx}]"
            filters.append(af)
            audios.append(f"[a{idx}]")
        if clip.kind == "audio":
            continue
        dur = local_duration
        # Scale before temporal effects; at most 16 sampled frames are retained.
        # Large speedups sample the exposure window rather than buffer hundreds of originals.
        temporal = ""
        if clip.kind == "video":
            if clip.deflicker:
                temporal += "deflicker=size=5:mode=am,"
            if clip.motion_blur:
                window = max(2, clip.speed * clip.shutter)
                stride = max(1, math.ceil(window / 16))
                frames = max(2, min(16, round(window / stride)))
                if stride > 1:
                    temporal += f"fps={max(0.00001, a.get('fps', 30) / stride)},"
                temporal += (
                    f"tmix=frames={frames}:weights='" + " ".join(["1"] * frames) + "',"
                )
        chain = f"[{idx}:v]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black@0,setsar=1,{temporal}setpts=(PTS-STARTPTS)/{clip.speed if clip.kind == 'video' else 1},fps={fps},trim=duration={dur},format=rgba"
        if clip.kind in ("video", "image"):
            if clip.brightness or clip.contrast != 1 or clip.saturation != 1:
                chain += f",eq=brightness={clip.brightness}:contrast={clip.contrast}:saturation={clip.saturation}"
            if clip.warmth:
                chain += (
                    f",colorbalance=rs={clip.warmth * 0.25}:bs={-clip.warmth * 0.25}"
                )
            if clip.blur:
                chain += f",gblur=sigma={clip.blur}"

        if clip.mask != "none":
            mask = (
                "lte(pow((X-W/2)/(W*.28),2)+pow((Y-H/2)/(H*.45),2),1)"
                if clip.mask == "circle"
                else "between(X,W*.2,W*.8)*between(Y,H*.15,H*.85)"
            )
            chain += f",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*({mask})'"
        scale = animated(clip, "scale", f"(t+{phase})")
        if clip.transition == "zoom":
            scale = f"({scale})*(0.8+0.2*min((t+{phase})/{clip.fade_duration},1))"
        if (
            clip.scale != 1
            or any(k.scale != 1 for k in clip.keyframes)
            or clip.transition == "zoom"
        ):
            chain += f",scale=w='max(2,trunc({w}*({scale})/2)*2)':h='max(2,trunc({h}*({scale})/2)*2)':eval=frame"
        rotation = animated(clip, "rotation", f"(t+{phase})")
        if clip.rotation or any(k.rotation for k in clip.keyframes):
            chain += f",rotate=a='({rotation})*PI/180':ow=iw:oh=ih:c=none"
        # geq time uses T rather than t; keyframe opacity is sampled per output frame.
        opacity = animated(clip, "opacity", f"(T+{phase})")
        if clip.opacity != 1 or any(k.opacity != 1 for k in clip.keyframes):
            chain += f",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*({opacity})'"
        fade = min(clip.fade_duration, clip.duration / 2)
        if clip.transition == "fade":
            if phase == 0:
                chain += f",fade=t=in:d={fade}:alpha=1,fade=t=out:st={clip.duration - fade}:d={fade}:alpha=1"
            elif phase >= fade and clip.duration - phase >= fade:
                chain += (
                    f",fade=t=out:st={clip.duration - phase - fade}:d={fade}:alpha=1"
                )
            else:
                chain += f",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*max(0,min(1,min((T+{phase})/{fade},({clip.duration}-T-{phase})/{fade})))'"
        if clip.transition == "wipe":
            chain += f",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*lte(X,W*min((T+{phase})/{fade},1))'"
        chain += f",setpts=PTS+{local_start}/TB[v{idx}]"
        filters.append(chain)
        x = animated(clip, "x", f"(t-{local_start}+{phase})")
        y = animated(clip, "y", f"(t-{local_start}+{phase})")
        xe = f"W*({x})/100-w/2"
        ye = f"H*({y})/100-h/2"
        if clip.transition == "slide":
            xe += f"-W*max(1-(t-{local_start}+{phase})/{fade},0)"
        out = f"comp{idx}"
        filters.append(
            f"[{base}][v{idx}]overlay=x='{xe}':y='{ye}':enable='gte(t,{local_start})*lt(t,{local_start + dur})':eof_action=pass:repeatlast=0:format=auto[{out}]"
        )
        base = out
    filters.append(f"[{base}]format=yuv420p[outv]")
    filters.append(
        "".join(audios)
        + f"amix=inputs={len(audios)}:duration=first:normalize=0,alimiter=limit=0.95[outa]"
    )
    script = folder / "filters.txt"
    script.write_text(";\n".join(filters))
    output = folder / "frame-export.mp4"
    cmd += [
        "-filter_complex_script",
        str(script),
        "-map",
        "[outv]",
        "-map",
        "[outa]",
        "-t",
        str(duration),
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "20",
        "-threads",
        "2",
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        "-movflags",
        "+faststart",
        "-progress",
        "pipe:1",
        "-nostats",
        str(output),
    ]
    log = folder / "render.log"
    with log.open("w") as err:
        proc = PROCESSES.start(
            cmd, group, stdout=subprocess.PIPE, stderr=err, text=True
        )
        try:
            for line in proc.stdout:
                if line.startswith("out_time_us="):
                    try:
                        progress = int(line.split("=")[1]) / 1e6 / duration
                    except ValueError:
                        continue
                    update(min(0.99, progress))
            code = proc.wait()
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
            proc.stdout.close()
            PROCESSES.release(proc)
        if group in PROCESSES.cancelled or PROCESSES.stopping:
            raise Cancelled("Export cancelled")
        if code:
            raise RuntimeError(log.read_text()[-3000:])
    update(1)
    return output


def render_project(project, assets, folder, update, group="export"):
    """Render bounded timeline windows so long projects do not open every file at once.

    A window owns only intersecting clips. Chunk outputs use identical encoding settings,
    then concatenate without re-encoding. The original timeline clock drives animation.
    """
    duration = max((c.start + c.duration for c in project.clips), default=0)
    if duration <= 0:
        raise ValueError("Add clips before exporting")
    folder = Path(folder)
    folder.mkdir(parents=True, exist_ok=True)
    # Keep small exports in a single render. Large projects bound the number of open inputs.
    if duration <= 30 and len(project.clips) <= 24:
        return render_window(project, assets, folder, update, group)
    chunks = []
    origin = 0
    while origin < duration - 1e-8:
        window = min(30, duration - origin)
        active = [
            c
            for c in project.clips
            if c.start < origin + window and c.start + c.duration > origin
        ]
        # Dense timelines use smaller windows to avoid opening hundreds of unrelated clips.
        while len(active) > 24 and window > 1 / project.fps:
            window = max(1 / project.fps, round(window * project.fps / 2) / project.fps)
            active = [
                c
                for c in project.clips
                if c.start < origin + window and c.start + c.duration > origin
            ]
        if len(active) > 24:
            raise ValueError(
                "More than 24 simultaneous layers. Flatten a composition before export."
            )
        index = len(chunks)
        part = folder / f"part-{index:05d}"
        output = render_window(
            project,
            assets,
            part,
            lambda p, origin=origin, window=window: update(
                (origin + window * p) / duration * 0.98
            ),
            group,
            origin,
            window,
        )
        chunks.append(output)
        origin += window
    # Generated paths contain no user-controlled text. Normalize for FFmpeg concat syntax.
    listing = folder / "concat.txt"
    listing.write_text(
        "".join(
            "file '" + str(p.resolve()).replace("'", "'\\''") + "'\n" for p in chunks
        )
    )
    result = folder / "frame-export.mp4"
    PROCESSES.run(
        [
            "ffmpeg",
            "-v",
            "error",
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            str(listing),
            "-c",
            "copy",
            "-t",
            str(duration),
            "-movflags",
            "+faststart",
            str(result),
        ],
        group=group,
    )
    for output in chunks:
        output.unlink(missing_ok=True)
    update(1)
    return result
