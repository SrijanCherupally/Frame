# Frame — native Rust video editor

Frame is now a compiled **Rust desktop application**. It uses **egui/eframe** for a native GPU-rendered interface and **FFmpeg** for decoding, filtering, audio mixing, and export. There is no Electron, browser engine, JavaScript, npm, Node.js, Python service, HTTP server, or web build step in the application.

[Workspace preview](docs/screenshots/workspace.png) · [Format templates](docs/screenshots/formats.png) · [Explanation graphics](docs/screenshots/graphics.png) · [Themes](docs/screenshots/themes.png) · [Graphite](docs/screenshots/graphite.png) · [Light theme](docs/screenshots/dawn.png) · [Rendered rocket scene](docs/screenshots/explanation.png)

The workspace follows familiar professional editing conventions: project assets at the left, the program monitor in the center, effect controls at the right, and the layered timeline below. **Edit, Color, Audio, and Motion** workspace buttons open their relevant tools. Raised cards, contrasting panel depths, selection indicators, and consistent track colors make the hierarchy easier to scan.

Choose **Theme** in the toolbar for **Midnight, Graphite, Amethyst, Forest, or Dawn**. Graphite uses neutral charcoal and warm gold; Dawn provides a light option. Theme choices persist between launches and affect only the interface, never exported footage or graphics.

The previous Electron implementation is preserved in Git history at `2a3d1b4`. The current source replaces it rather than wrapping it.

## Download the Windows app

**[Download Frame for Windows (.zip)](https://github.com/SrijanCherupally/Frame/raw/refs/heads/main/downloads/Frame-Windows-x64.zip)**

1. Download the ZIP and choose **Extract All**.
2. Open the extracted folder and double-click **Frame.exe**.

The ZIP includes **FFmpeg, FFprobe, and FFplay** beside the app. No separate media download, first-launch internet connection, Rust, npm, or administrator setup is required. Keep the extracted folder together. It also contains templates, license notices, and the complete corresponding source and build recipe for the bundled media tools.

**[Download only Frame.exe](https://github.com/SrijanCherupally/Frame/raw/refs/heads/main/downloads/Frame.exe)** is an application-only update for an existing complete folder. Use the ZIP for a new installation. Windows x64; unsigned, cross-compiled build. Windows runtime testing remains outstanding. [Download checksums](downloads/SHA256SUMS.txt).

The instructions below are for building Frame from source; you can skip them when using the download.

## Build and run

Install [Rust](https://rustup.rs/), a system C linker, and FFmpeg 7 or later. `rust-toolchain.toml` pins the compiler; Cargo downloads verified dependency versions from `Cargo.lock`.

```sh
cargo run --locked --release
```

Or build once and launch the executable directly:

```sh
cargo build --locked --release
# Linux:
./target/release/frame
# Windows PowerShell:
.\target\release\frame.exe
```

On Windows, the standard Rust MSVC toolchain needs Visual Studio C++ Build Tools with the Windows SDK. On Linux this build uses X11 and OpenGL; a working X11 session and the normal system X11/GL libraries are required. Wayland users need XWayland. A Windows build is a normal `.exe`; users of the compiled application do not need Rust or any package manager.

Place `ffmpeg` and `ffprobe` beside the application executable or install them on PATH. Add `ffplay` for native audio preview. FFplay is optional for video preview and export; exported audio is mixed by FFmpeg regardless. Custom binary locations can be set with `FRAME_FFMPEG`, `FRAME_FFPROBE`, and `FRAME_FFPLAY`.

The UI draws through the GPU. Video effects, proxy generation, and export currently use CPU-based FFmpeg processing. Rust removes the browser runtime and scripting-language coordination layers, but it does not make codecs, disk I/O, or every effect arbitrarily fast. GPU decoding/encoding and hundred-gigabyte project benchmarks are still future work.

## Windows application and packaging

Building on Windows produces `target/release/frame.exe`. For a portable app folder and ZIP:

```powershell
.\scripts\package-windows.ps1
```

The script reuses the bundled media tools, licenses, and source archive from the checked-in Windows ZIP, replaces the application with a fresh native Windows build, writes SHA-256 hashes, and creates an offline portable ZIP. Existing output folders and archives are preserved. To use a different FFmpeg build, provide all three executables and its license/corresponding-source materials:

```powershell
.\scripts\package-windows.ps1 `
  -FfmpegDir C:\Tools\ffmpeg\bin `
  -FfmpegLicenseDir C:\Tools\ffmpeg\distribution-materials
```

The bundled tools are FFmpeg 7.1.5 with static x264 and SDL2, built from pinned, unmodified upstream sources. `Sources/MediaTools-source.tar.gz` in the ZIP includes those sources and [the build recipe](scripts/build-bundled-media.sh); `Media-Licenses` includes their notices. [Build provenance and source details](docs/bundled-media.txt). This GPL media build supports local-file processing and H.264/AAC export; network protocols, capture devices, and hardware acceleration are disabled. Source builds of Frame can use other compatible FFmpeg installations.

[Windows CI](.github/workflows/windows.yml) builds the native app, tests editing, exercises the bundled media tools, and uploads a complete offline ZIP. Its Windows runtime checks must pass before treating that platform as verified. Code signing, installers, and automatic updates are not implemented.

## Animation and explanation videos

**Motion Studio** adds editable timing curves and 2D Bézier motion paths, fourteen configurable motion presets, animation copy/paste with duration fitting, reversal, and smooth wipe/iris reveals. Easing and paths survive cuts, trims, ripple deletion and retiming. Generated motion is evaluated continuously at the project's output frame rate.

The graphics library now includes eighteen types and seven staggered explanation scenes. Data curves accept a two-column CSV and embed measured values in `.frame` projects. Text mattes place footage inside large multiline lettering. **Detach audio** creates a separately editable source-audio clip for J/L cuts.

See the **[timestamped reference breakdown and practical production guide](docs/animation-guide.md)** for the uploaded self-landing-satellite video, feature mappings, instructions, and precise limits. [Motion Studio screenshot](docs/screenshots/animation.png).

## Tracking, vectors, 3D and animation blur

**VFX Studio** adds real point tracking with confidence/loss handling; editable cubic vector paths, stroke drawing, SVG geometry import/export and matched-node morphing; perspective 3D scenes with primitives, small OBJ meshes, object/camera keys and shaded previews; and 2/4/8-sample animation motion blur with 36–360° shutter control.

Open **Graphics → Vector Studio / 3D Studio**, or **Motion → Tracking / Motion blur**. The Windows package includes editable VFX example projects, original SVG/OBJ geometry and a tracking-practice video/project. All procedural geometry and animation data live in `.frame` documents.

Read the **[VFX workflows, supported formats, performance details and limits](docs/vfx-guide.md)**. [3D scene editor](docs/screenshots/3d.png) · [Vector editor](docs/screenshots/vectors.png) · [Tracking](docs/screenshots/tracking.png).

## Editing workflow

The app opens an original illustrated rocket project so its controls can be tried immediately. Demo images are sample artwork rather than camera footage.

- **Import:** select files with the native file picker or drop original files into the window. Frame links to their filesystem paths and leaves originals in place. Probing and preview generation run on background workers. Multiple files can be probed while the proxy worker prepares an earlier file.
- **Arrange:** add a library item with `+`, double-click it, or drag it to a timeline track. Drag clips to move; drag their edges to trim. Properties also provides timing and track assignment. Footage, B-roll, titles/graphics, voiceover, and music have five editable tracks.
- **Cut:** `S` splits at the playhead. Arrow keys step one project frame; Shift + arrows step ten. Remove Frames deletes a selected time interval across tracks and closes the gap. Undo and redo cover editing and property changes.
- **Retime:** use Properties → Timing to enter a positive speed multiplier or target duration. **Four hours into 20 seconds is 720×.** Speed changes preserve the selected source range and rescale animation times. Practical limits are at least one output frame and a 24-hour timeline, rather than a small preset multiplier cap.
- **Explain:** browse **Graphics** for 18 editable explanation elements, including callouts, measurements, axes, grids, step numbers, telemetry cards, progress indicators, equations, comparison bars, trajectories, orbital diagrams, waves and data curves. Customize text, font size, colors, line weight, shape fill, and data values in Properties. Start with one of seven ready-made explanation scenes.
- **Animate:** select a visual layer and open **Motion** for fourteen presets with entrance timing, intensity and easing controls. Open **Motion Studio** to edit key times, transform channels, timing curves and spatial path handles; copy, fit, reverse or clear motion; and configure smooth reveals. Applying a preset replaces the clip's transform animation and supports undo.
- **Finish:** adjust brightness, contrast, saturation, temperature, blur, ellipse/rectangle/text masks, and fade/slide/zoom/wipe entrances. Fade also affects the clip's ending and audio. Overlap layers to reveal footage beneath an animated clip.
- **Export:** choose a resolution, select a new `.mp4` filename, and render H.264/AAC from original media. Progress and cancellation are available. Existing output files are rejected to protect previous renders.

Space plays/pauses, Delete removes a selected clip, Ctrl/Cmd + Z undoes, Ctrl/Cmd + Shift + Z redoes, and Ctrl/Cmd + S saves. Choose **New project** in the project menu to browse 12 format templates with pixel size and fps together: Full HD, UHD/4K, cinematic 24 fps, 60 fps action, Shorts/Reels, square, 4:5 portrait, PAL, and lightweight HD. Click the format badge in the top bar to change an existing project. Settings offers custom even dimensions from 240–3840 pixels and frame rates up to 60 fps. A format change that would make an existing clip shorter than one frame is rejected without modifying the project.

## Clean Timelapse

Select a video, open **Color**, and choose **8×**, **32×**, or **720×** in Clean Timelapse. Presets enable motion blending and exposure flicker reduction, and mute source audio. All three settings can be changed independently. The blending strength controls the sampled source-time exposure window.

Export blends neighboring source samples **before retiming**, rather than simply blurring the already sped-up result. At most 16 sampled frames are retained per active temporal layer. High speeds spread those samples across the exposure window to avoid retaining hundreds of full-resolution source frames. Flicker reduction uses nearby frames to smooth brightness changes.

This is exposure-style blending, not optical flow. Fast movement can ghost; low source frame rates, camera shake, or poor lighting cannot be repaired automatically. Use a fixed camera and consistent lighting, lower the strength if moving details smear, and render a short test first. Very slow playback repeats/interpolates existing presentation timestamps; optical-flow generation of new motion detail is not implemented.

Temporal blending and deflicker are export effects. Native preview uses the same FFmpeg composition pipeline for the other implemented transforms, masks, titles, and color controls, with low-resolution media and no temporal processing. Audio playback uses a companion FFplay process and needs an available audio device. Preview restarts after seeks and at short bounded window boundaries; frame-perfect uninterrupted A/V playback is not yet guaranteed.

## Storage, responsiveness, and recovery

The original file size is not read into Rust UI memory. Project objects contain metadata and paths. UI previews use a two-frame bounded channel at a 640-pixel canvas width, with stale-generation frames discarded after a seek. Video proxies are at most 960×540. The media browser virtualizes visible rows and limits its thumbnail texture cache to 64 entries. Codec work, export, media probing, and imported-project media restoration run outside the UI thread. Animated opacity is evaluated once per output frame and applied with FFmpeg runtime alpha commands, avoiding a keyframe expression evaluation for every pixel. Constant animated properties are reduced to constants; motion presets omit redundant straight-line keyframes.

There is one probing worker, one proxy worker, and one export at a time. FFmpeg decoder/filter/encoder parallelism is bounded. Long exports render timeline windows of at most 30 seconds; dense windows shrink to avoid opening every project source at once. No more than 24 intersecting clip inputs are admitted per render window. Completed chunks are joined without a second video encode. Temporary chunks are removed after a successful export.

These choices keep RAM use separate from total original-file bytes. They do **not** establish smooth performance for hundreds of gigabytes of high-bitrate camera footage. Proxy generation still decodes the source. Simultaneous 4K layers and temporal effects remain expensive. Use sufficiently large, fast storage; allow room for proxies and roughly three times the compressed export size while the final file is assembled and copied. Completed jobs are retained until manually cleaned while Frame is closed.

The library, projects, snapshots, and jobs live in the platform's per-user local data directory. Set `FRAME_DATA_DIR` to an absolute path on another drive to choose a different media cache. A filesystem lock prevents two app instances from editing the same library. Original files must remain available for export; cached proxies can be used for preview while an original is offline. Reconnect a disconnected drive or relink the original. The application currently limits projects to 2,000 clips and 200 transform keyframes per clip.

Project JSON is saved with create-new temporary files, a data flush, and atomic replacement. A previous valid save is retained for recovery. A project snapshot is saved before creating/opening/loading a different project; snapshots can also be saved/restored from the project menu. Pending edits are saved when closing. If a close-time save fails, the app stays open so a backup can be made. Use **Save .frame** in the toolbar or **Save project as .frame** in the project menu to create a reusable project document.

Subprocesses have explicit ownership and are killed/reaped on cancellation and shutdown. An interrupted proxy job is retried when the library reopens. Job state and diagnostic FFmpeg logs live in `jobs/<id>/`; interrupted exports must be started again. Partial-export resumption and automatic cache eviction are not implemented.

The v1 `.frame.json` project format and existing media metadata are readable by the Rust version. To reuse this checkout's earlier library, set `FRAME_DATA_DIR` to the existing absolute `.frame-data` path. No automatic move or deletion of original footage occurs. Legacy `.frame.json` backups contain only media IDs, so a different library may require relinking their originals. New `.frame` documents include media references to restore those links automatically.

## Save and reopen a `.frame` project

**Save .frame** writes the complete editable project: canvas/fps, clip order and timing, source ranges, speed, track visibility/mutes, color, masks, transitions, graphics parameters, transform keyframes, playhead, timeline zoom, selected clip, and a manifest of the referenced original media. Exact floating-point JSON round trips preserve keyframe values. Only media used by that project is included in its manifest.

Open it from the project menu, drop the `.frame` file onto the window, or launch `frame my-project.frame`. Frame first snapshots the current project. It restores the document and rebuilds proxies in the background when needed. Imported media IDs that conflict with an unrelated local asset are remapped without overwriting that asset. Offline originals remain in the library with a **Retry / relink** action.

The document **does not embed footage or caches**. Keep the original files available. Sources under the saved project's folder use relative references: move that folder and its media together to preserve links. Sources elsewhere use their original paths; moving to another drive or computer may require relinking them. This keeps a project small even when its footage spans hundreds of gigabytes. A `.frame` file is a versioned JSON document, capped at 32 MB; old `.frame.json` files remain readable.

Media-free example projects for all seven explanation scenes are included in [examples/projects](examples/projects): orbital mechanics, flight trajectory, signal/response, design comparison, rocket forces, a build chapter, and a flight report. They open as fully editable timelines and need no imported footage. The Windows ZIP includes them in its **Templates** folder. Regenerate them with `cargo run --locked --example create_templates`.

## Validation

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --all-targets -- --test-threads=2
cargo build --locked --release
```

The tests include:

- VFX geometry/morphs, encoded-video point tracking and loss handling, procedural cancellation, sub-frame exposure pixel comparisons, 3D/vector rendering, project round trips and output frame counts.

- Continuous easing and Bézier paths preserved through key insertion, split, ripple deletion, trim, retime, reversal, fitted paste and JSON round trips; malformed curves/CSV; independent audio detachment.
- Real rendered animation frames and seeks compared to model samples, reveals in preview/export, footage-filled text mattes, data-driven chart pixels and dense key tracks.

- Headless native UI drawing, keyboard splitting/undo, snapshots, saving pending edits at shutdown, all four workspace modes, and five persistent themes without project mutations.
- Pure Rust retiming, source offsets, animation-preserving split/ripple edits, malformed project validation, atomic recovery, and library locking.
- Fresh-library `.frame` restoration after moving its folder, offline sources, ID collisions, workspace recovery, exact keyframe round trips, all format/motion presets, and all explanation scenes.
- Visible transparent renders of every graphic (including small canvases, thin lines and nearly zero values), real FFmpeg exports/previews of animated scenes, and pixel comparisons proving animated opacity matches static opacity and mid-animation seeks.
- Real FFmpeg probing, disk-linked originals and decodable proxies, composited audio/video, masks, keyframes, all implemented transitions, motion-blended pixel differences, slow motion, cancellation, missing sources, output preservation, streamed native preview frames, and non-silent preview PCM audio.
- A real four-hour sparse-frame file rendered to 20 seconds at 720×. This validates long timestamps and output duration, **not** high-bitrate four-hour performance.

Inspect tools without opening a window:

```sh
./target/release/frame --check-tools
```

Render a saved project against the selected media library:

```sh
./target/release/frame --render project.frame new-output.mp4
```

For a native-window smoke test in a graphical session:

```sh
./target/release/frame --ui-smoke screenshot.png
./target/release/frame --ui-smoke graphics.png graphics
./target/release/frame --ui-smoke formats.png formats
./target/release/frame --ui-smoke themes.png themes
./target/release/frame --ui-smoke animation.png animation
./target/release/frame --ui-smoke vectors.png vectors
./target/release/frame --ui-smoke 3d.png 3d
./target/release/frame --ui-smoke graphite.png graphite
./target/release/frame --ui-smoke dawn.png dawn
```

This launches the actual OpenGL native window, requests a framebuffer screenshot after four seconds, and closes. In Linux CI, run it under an X11 virtual display. Unit UI tests need no browser or graphical server. Real media tests need FFmpeg/FFprobe.

## Current limits

Frame is an early working editor, not a Resolve/Premiere replacement. Still missing: optical flow, variable speed ramps, stabilization, planar/camera/semantic tracking, arbitrary/Bezier masks, multicam, nested sequences, RAW/log/HDR color management, LUTs, scopes, audio waveforms/EQ/noise reduction, transcription/captions, nested/parented motion compositions, full SVG documents, textured/rigged 3D scenes, synchronized telemetry channels, plugins, collaboration, an MCP server, hardware video acceleration, signed installers, and large-project performance qualification. Input support depends on the installed FFmpeg build; not every codec/container or protected format is supported. Output is MP4/H.264/AAC.

## Source layout

- `src/ui.rs`: native workspace, inspector, timeline, dialogs, asynchronous job coordination.
- `src/animation.rs`, `src/ui/animation.rs`: easing, Bézier paths, CSV validation and native Motion Studio.
- `src/vector.rs`, `src/scene3d.rs`, `src/procedural.rs`: vector geometry/morphing, perspective scenes and bounded source rendering.
- `src/tracking.rs`, `src/ui/effects.rs`: confidence-based point tracking and native VFX Studio.
- `src/ui/theme.rs`: centralized contrast palette, semantic accents, surfaces and raised cards.
- `src/model.rs`, `src/edit.rs`: validated project format and pure edit operations.
- `src/media.rs`: disk-linked media probing, thumbnail/proxy preparation, embedded demo assets.
- `src/preview.rs`: cancellable bounded native frame streaming and optional audio output.
- `src/render.rs`: FFmpeg graphs, keyframes, generated graphics, temporal effects, chunked export.
- `src/process.rs`: tool discovery, subprocess groups, cancellation, bounded diagnostic capture.
- `src/storage.rs`, `src/document.rs`: atomic saves, recovery, portable project documents, media restoration, exclusive library lock.
- `src/presets.rs`, `src/ui/tools.rs`: video formats, motion presets, explanation scenes, and native galleries.
- `tests/editing.rs`, `tests/native.rs`: pure editing and actual media regression tests.
- `scripts/package-windows.ps1`: portable Windows app packaging.

Original demo artwork and DejaVu Sans are embedded in the executable. The font's license is under `assets/fonts/`.
