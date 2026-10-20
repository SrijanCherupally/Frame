# GPU acceleration, including RTX 4070 laptops

Frame can use two independent GPU engines: NVIDIA NVENC for H.264 export encoding, and DX12/Vulkan hardware rasterization for 3D layers. Both default to Automatic. Download the **complete updated Windows ZIP**, extract it and keep its media tools beside Frame.exe. Updating only Frame.exe in an old folder does not upgrade FFmpeg to include NVENC.

[View the export controls](screenshots/gpu-export.png).

## Enable and check

1. Install a current NVIDIA driver. A current Studio Driver is suitable for editing. Frame's bundled Video Codec SDK 12.1 headers require Windows driver 531.61 or newer. No CUDA toolkit, SDK, Rust, Python or separate FFmpeg installation is needed to run the app.
2. Open **Settings** or **Export video** and find **Hardware acceleration**.
3. Keep export at **Automatic · prefer NVIDIA** and 3D at **Automatic GPU**.
4. Click **Check GPU & encoder**. This runs on a background worker, actually encodes two frames through NVENC and reports the selected 3D adapter/backend. Merely listing an encoder does not establish that its driver works.
5. Export a short section first. The export status reports **NVIDIA NVENC** or **CPU H.264**. The 3D status identifies the selected device or explains its CPU fallback.

The RTX 4070 Laptop GPU supports NVENC H.264 and the required 3D APIs. On a hybrid laptop, Frame ranks NVIDIA adapters first, then discrete, integrated and virtual GPUs, and attempts the next adapter if device creation fails. Software adapters are excluded from automatic GPU 3D selection. Windows uses DX12 or Vulkan; Linux requires a Vulkan driver for GPU 3D. The UI continues using OpenGL. If Windows power policy hides/disables the dedicated GPU, select Frame.exe under **Windows Settings → System → Display → Graphics → High performance**. Plugging in the laptop and selecting its performance mode can avoid power-limited performance.

## Export choices

- **Automatic** tries a real NVENC encode before preparing expensive procedural sources. If unavailable it exports with libx264. If NVENC fails during a later chunk, Frame discards the encoded chunks and restarts the entire export on CPU. This avoids joining incompatible hardware/software H.264 streams. Cancellation never triggers a retry.
- **NVIDIA NVENC · required** reports a failure if the GPU/driver/encoder is unavailable or fails; it does not silently change encoders. Use it when confirming GPU export on your laptop.
- **CPU** selects the existing software H.264 encoder without probing NVENC.

Output remains MP4/H.264/AAC for compatibility with YouTube and common players. NVENC uses the P5/HQ preset, VBR constant-quality target 20 and 4:2:0 pixels. CPU uses libx264 veryfast/CRF 20. These quality numbers are codec-specific; they do not promise identical output size or visual quality. Export is split into bounded windows, then stream-copied together without a second encode.

The encoder test has a 15-second timeout. Selection/fallback details are saved in the export job's `acceleration.json`, beside its render logs, under the cache path shown in Settings. Device loss or exhausted encoder sessions can still interrupt GPU work. Close other recording/encoding applications or select CPU when troubleshooting.

## What 3D runs on the GPU

The renderer submits triangles to a real graphics pipeline with perspective projection, near-plane clipping, a depth attachment, flat material colors, transparent output and wireframes. Object/camera animation and mesh preparation remain in Rust on the CPU. Existing supersampling and premultiplied edge downsampling are preserved. Preview and exported procedural sources use the same renderer and animation clock, including shutter sub-samples.

One shared GPU device, reusable vertex buffer and a single current offscreen color/depth/readback target bound resource use. Changing output dimensions replaces the target. Workers serialize access to this renderer and check cancellation while waiting/readback. A failed initialization, lost device, validation error or readback timeout disables that GPU renderer for the session and rerenders through the software implementation. Restart Frame to retry the GPU after fixing its driver. Choose **3D · CPU** to bypass it directly.

The renderer reads RGBA pixels back to system memory because the existing composition/export graph consumes software frames. It is not a zero-copy pipeline. Low-detail or tiny previews can be dominated by transfer/setup costs, and eight motion-blur samples multiply rendering work. GPU acceleration does not remove mesh limits or introduce textured/PBR shading, shadows or rigging.

## What still runs on the CPU

FFmpeg decoding, timeline compositing, masks, color filters, vector drawing, timelapse blending, sub-frame averaging, audio processing, proxy generation and lossless intermediate encoding remain CPU-based. NVENC accelerates the **final compression stage**, not the complete effects graph. CPU/disk-heavy projects may see limited export improvement even with a fast GPU; no fixed speedup or real-time 4K guarantee is claimed. GPU video decoding, CUDA effects and direct GPU-to-NVENC sharing are not implemented.

Choices are saved on this machine separately from `.frame` projects, so projects remain portable. GPU utilization may appear under **Video Encode** for NVENC and **3D/Graphics** for rendering, rather than the default Task Manager graph. Brief GPU bursts or CPU utilization during compositing do not establish that NVENC is inactive; inspect the app's encoder status.

## Command line and verification

```sh
frame --check-gpu
frame --render project.frame output.mp4 --encoder auto
frame --render project.frame output.mp4 --encoder nvidia
frame --render project.frame output.mp4 --encoder cpu
```

Without `--encoder`, rendering uses the saved machine preference. `--check-gpu` diagnoses hardware availability independently of saved choices. Custom `FRAME_FFMPEG` installations must include `h264_nvenc` for NVIDIA export. The complete ZIP already includes that encoder and all media license/corresponding-source materials; NVIDIA's proprietary runtime is supplied by your installed driver, not redistributed in the ZIP.

Validation covers actual software exports, protecting existing output, cancellation/timeouts, preferences, simulated mid-export device loss across multiple chunks, strict NVIDIA errors, and a Vulkan shader/reference pixel comparison for depth, clipping, wireframes, readback padding and reuse. The shader check uses Mesa's software Vulkan adapter in this cloud environment; it verifies graphics commands but is not an RTX hardware benchmark. Windows/RTX runtime performance still needs verification on a real laptop.
