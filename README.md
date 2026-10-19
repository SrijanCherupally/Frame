# Frame Studio

A local desktop video editor for build montages, rocket launches, and explanation graphics. This is a working early version, built with Electron, React, Python, and FFmpeg. It does not yet replace Resolve or Premiere, and it has not been benchmarked with a hundred-gigabyte library.

## Run the desktop app

Requirements: **Node.js 22.12+**, **Python 3.12+**, and **FFmpeg 7+** with `ffmpeg` and `ffprobe` on PATH. FFmpeg must include libx264, AAC, and the standard video filters. A normal graphical desktop session is required.

```sh
npm ci
python -m pip install -r backend/requirements.txt
npm run desktop
```

`npm run desktop` builds the interface, opens an application window, and starts its local engine automatically. No hosted website, account, cloud upload, or API key is needed. This repository currently runs from source; signed macOS/Windows installers and bundled Python/FFmpeg runtimes are not provided. The desktop smoke test has been run on Linux; Windows and macOS remain unverified.

If Python is not available as `python3` on macOS/Linux or `python` on Windows, set `FRAME_PYTHON` to its executable path. A virtual environment can be used:

```sh
python -m venv .venv
# macOS/Linux:
. .venv/bin/activate
# Windows PowerShell: .venv\Scripts\Activate.ps1
python -m pip install -r backend/requirements.txt
# macOS/Linux:
FRAME_PYTHON="$PWD/.venv/bin/python" npm run desktop
```

On Linux, Electron requires the distribution's normal sandbox/user-namespace support and GUI libraries. Do not disable the sandbox for everyday use. The automated container smoke test uses Chromium's headless platform and disables its sandbox inside the already isolated test container.

## Edit a project

The application starts with an original illustrated rocket demo. These images are sample artwork, not camera footage.

- **Import media:** use the native file picker. Frame links to originals in place; it does not copy them into the project. Thumbnails and low-resolution video/audio preview files are generated on disk. Files become editable once probing and thumbnail generation finish; unfamiliar video codecs may need their preview conversion to finish before playback works.
- **Arrange:** add a media item with its `+` button, or drag it onto a track. Drag clips to move and their left/right edges to trim. Track assignment is also available in Properties. The five tracks support layered footage, titles, graphics, voiceover, and music.
- **Cut precisely:** place the playhead and press `S`. Arrow keys step one frame; Shift + arrows step ten. Remove Frames deletes a time range across tracks and closes the gap. Undo/redo covers timeline and property edits.
- **Retime:** select video or audio and use Properties → Timing. Enter any positive speed multiplier that results in at least one output frame and no more than 24 hours, or enter the desired output duration in seconds. **Four hours → 20 seconds = 720×.** Changing speed preserves the selected source range and rescales its keyframe times.
- **Make a clean timelapse:** select a video, open Color, and choose an 8×, 32×, or 720× Clean Timelapse preset. Presets enable source-frame motion blending, exposure flicker reduction, and mute source audio. You can change these independently, adjust blending strength, and restore source volume in Audio. A preset is unavailable if the source is too short to produce a full output frame.
- **Explain:** add titles, subtitles, arrows, ellipses, focus rectangles, and lower thirds. Edit their text, color, size, position, scale, rotation, opacity, and track. Add transform keyframes, move the playhead, then change a transform to animate. Interpolation is linear.
- **Finish:** choose exposure/brightness, contrast, saturation, warmth, blur, an ellipse/rectangle mask, or fade, slide, zoom, and wipe entrance effects. Fade also fades the clip's end and audio. Layer overlapping clips for a reveal of the footage below.
- **Export:** render H.264/AAC MP4 from originals at 360p, 720p, 1080p, 4K, square, or vertical resolution. Export progress and cancellation are supported. Changing export dimensions changes composition layout; set your project canvas before arranging titles.

Space plays/pauses, Delete removes the selected clip, Ctrl/Cmd + Z undoes, Ctrl/Cmd + Shift + Z redoes, and Ctrl/Cmd + S saves. The Help button lists shortcuts.

## Clean Timelapse, honestly

Simple frame dropping often makes a sped-up workbench clip look nervous. Frame's export pipeline scales decoded frames to the export canvas, optionally reduces brightness flicker, averages frames across an adjustable source-time exposure window, and then retimes the result. It keeps at most **16 sampled temporal frames per active video layer**; very high speeds spread those samples over the exposure window rather than retaining hundreds of full-resolution frames.

This is an exposure-style approximation, not optical flow or a simulation of every missing frame. It can produce ghosting with fast objects or camera motion. It cannot repair a shaky camera or guarantee a cinematic result from poor footage. Use a locked camera and consistent lighting; choose lower blending strength when fine moving parts look smeared. Render a short test before a long export.

Motion blending and flicker reduction are **export effects**, not real-time preview effects. Browser preview approximates FFmpeg color and typography. Extreme retiming uses seeking in preview; audio is pitch-preserved in export, but preview playback follows browser media playback behavior. Optical-flow slow motion and speed ramps are not implemented.

## Large footage and resource use

Original file byte size is not loaded into JavaScript memory. Native imports are references to file paths, and preview playback uses ranged HTTP file reads over the local loopback interface. Preview video is at most 960×540. Active media elements are created on demand; deleted clips release their elements. A paused preview redraws only when its state or media changes. There is one media preparation worker and one export worker, with bounded FFmpeg decoder/filter threads.

Long exports render in timeline windows of at most 30 seconds, shrinking dense windows to avoid opening all project sources simultaneously. Identically encoded windows are concatenated without a second video encode. Keyframes and transition clocks retain their original clip timing across window boundaries. Temporary chunk videos are deleted after successful assembly. Very dense compositions are limited to 24 intersecting clip inputs per render window; flatten complex compositions before export.

These choices make large originals practical **in principle**, but are not proof of smooth hundred-gigabyte projects. Proxy generation must decode the source, can take significant time, and consumes disk. High-resolution compositing, temporal blending, and multiple simultaneous layers still use substantial memory/CPU. Export is CPU-based; GPU decode/encode acceleration and hardware-specific performance tuning are not implemented. The current cloud machine has approximately 30 GB of free storage and is unsuitable for storing hundreds of gigabytes itself.

Keep large originals on a fast local SSD or attached media volume. Use `FRAME_DATA_DIR` to put the preview cache, project database, and renders on a sufficiently large drive. Original files must remain available at their linked paths; reconnect a disconnected drive or use **Relink original** on the media item. Project JSON files do not embed footage. Relink a compatible source; edits beyond its duration are rejected during export. Cache cleanup is currently manual while the application is closed; do not remove `media/*/meta.json` or originals. Completed exports and snapshots are retained until you remove them.

Current project limits are 2,000 clips, 200 transform keyframes per clip, five tracks, up to 3840×3840 pixels and 60 fps, and a 24-hour timeline. Speed has no preset-based multiplier cap; source availability and output frame duration remain the practical limits.

## Saving and recovery

The desktop app stores its library under Electron's per-user application data directory (`frame-studio/media-library`); `FRAME_DATA_DIR` overrides that location. The web development mode defaults to `.frame-data/` in this checkout. The two are intentionally separate libraries.

Project changes auto-save after a short debounce. Writes use a temporary file, flush/fsync, and atomic replacement. A previous-save copy is retained and used if the current JSON is unreadable. The desktop app flushes pending project edits before quitting and offers to stay open if saving fails. A single-instance lock prevents two app windows from writing the same desktop project.

Before New Project, Open Project, or Load Demo replaces your work, Frame saves an immutable project snapshot. You can also save a snapshot explicitly and restore snapshots from the project menu. **Download project backup** creates a portable `.frame.json` file. It contains references and edits, not media; cross-machine media-library migration is not implemented.

Original media is never overwritten by Frame's processors. Failed decoding is reported per media item. Export states persist on disk; an app restart marks unfinished exports as interrupted. Running child processes are tracked and terminated on app shutdown. Proxy work interrupted by shutdown is retried at startup. Restarted exports must be started again; partial export resumption is not implemented.

The local engine binds only to loopback. Native linking/relinking requires a per-launch random token held in the desktop main process, and only paths selected through the native file picker or obtained from genuine OS-dropped files reach it. The renderer has Node integration disabled, context isolation, a sandbox, navigation restrictions, and a content security policy. This is a single-user local application, not a multi-user server; do not expose the engine to a network.

## Development and verification

For interface development only, start the engine and Vite in two terminals:

```sh
npm run server
npm run dev
```

The native desktop workflow is the primary application. Development's file-upload fallback copies files to the cache and is intended for small test fixtures; use native linking for large footage.

```sh
python -m pip install -r backend/requirements-dev.txt
npm test                 # pure timeline/animation regression tests
npm run test:backend     # isolated real FFmpeg + API integration tests
npm run build
npm run test:e2e         # native Electron integration test; needs a GUI
npm run format:check
```

In a Linux CI container without a display:

```sh
FRAME_TEST_HEADLESS=1 npm run test:e2e
```

`tests/browser.mjs` launches the actual Electron app and local Python engine, exercises native IPC/file linking (stubbing only the OS picker), checks the rendered canvas, edits the timeline, retimes footage, applies a clean timelapse preset, edits titles, reloads saved work, and renders/downloads an MP4. Tests use temporary libraries and do not modify your projects. Backend tests cover malformed media, range reads, access restrictions, project recovery, snapshots, compositing, every implemented transition, keyframes, masks, motion-blended pixel differences, slow motion, long-timestamp 720× retiming, export cancellation, and content across render-window boundaries. The four-hour fixture uses sparse frames to test real long timestamps; it is not a four-hour high-bitrate footage performance benchmark.

Electron installation in a proxied environment may require Node's `NODE_USE_ENV_PROXY=1`. To put its verified binary download cache in a writable location, set `XDG_CACHE_HOME` or `electron_config_cache`. Do not skip checksum verification. If lazy installation needs to be retried, run `node node_modules/electron/install.js` with the same proxy/cache environment.

## Not yet implemented

This release has a real editing/export workflow, but does not have professional-editor feature parity. Not implemented: optical flow, motion tracking, stabilization, arbitrary Bezier/rotoscoping masks, multicam, nested sequences, variable speed ramps, GPU effects, RAW/log/HDR color management, LUT import, scopes, chroma key, audio waveforms/EQ/noise reduction, captions/transcription, plugins, collaborative projects, automatic media migration, native installers, or an MCP server. Input support depends on the installed FFmpeg build; camera RAW, DRM-protected material, and every codec/container combination are not guaranteed. Output is currently MP4/H.264/AAC only.

## Code layout

- `desktop/`: application lifecycle, native picker IPC, engine startup/shutdown.
- `frontend/src/editor.js`: pure edit operations and animation evaluation.
- `frontend/src/Preview.jsx`: on-demand media playback and canvas compositing.
- `frontend/src/main.jsx`: workspace, timeline, inspector, project and export interactions.
- `backend/app.py`: validated local API, disk-backed library, snapshots, background jobs.
- `backend/render.py`: original-media FFmpeg composition and bounded render windows.
- `backend/runtime.py`: subprocess ownership, cancellation, shutdown.
- `backend/models.py`: validated project/clip format.
- `tests/`: timeline, FFmpeg/API, and native application regression coverage.

Demo artwork is generated locally in `backend/demo.py`. DejaVu Sans is bundled with its license under `assets/fonts/` for offline, consistent title rendering.
