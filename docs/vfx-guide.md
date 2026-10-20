# VFX Studio: tracking, vectors, 3D and animation motion blur

Frame includes native editing tools for each of these workflows. All scene geometry, vector nodes, keys and blur settings are saved in `.frame` documents. Tracking is baked to editable transform keys. Processing runs locally; no Python, Electron, cloud service or extra media-tool installation is required in the complete Windows ZIP.

## Open the tools

- **Graphics → Vector Studio** opens vector geometry and morphing.
- **Graphics → 3D Studio** opens editable 3D scenes.
- **Motion → Tracking** opens point tracking.
- **Motion → Motion blur** opens project animation-blur settings.

The four tabs share a **VFX Studio** window. Select an existing procedural layer to edit it, or create a new vector/3D layer with the button at the top. The clip-time slider seeks the timeline while editing. Layer transforms, opacity, reveal effects and ordinary Motion Studio animation can also be applied to a vector or 3D scene.

## Track a feature and attach a label

1. Import footage and add it to the timeline. Add a title or graphic on an overlapping time range.
2. Open Tracking and choose the **source video clip**.
3. Choose the start time within that clip and an analysis range of up to 20 seconds. Load the reference frame.
4. Click a small, textured feature. Corners, bolts and contrasting markings are more suitable than blank sky, uniform paint, reflections or a moving shadow.
5. Adjust patch radius, search distance and minimum confidence if necessary, then click **Analyze movement**. Progress and cancellation are available.
6. Inspect the cyan trajectory and reported minimum confidence. A lost feature stops the analysis and leaves a partial result; it does not generate invented positions through an occlusion.
7. Choose a different visual layer as the target. **Preserve label's starting offset** keeps its initial placement relative to the tracked feature. Disable it to position the target directly on the feature. Path smoothing uses neighboring samples; zero preserves the measured positions.
8. Click **Apply track to target layer**. This is undoable. Inspect the resulting keys in Motion Studio and correct them where necessary.

Try `Templates/VFX/tracking-practice.frame` from the ZIP first. The supplied synthetic footage contains a patterned square moving across a dark background; click its center near the left side of the starting picture. Keep the accompanying `tracking-practice.mp4` beside the project.

### Tracking behavior and limits

Tracking uses normalized patch correlation against the reference appearance, local motion prediction and a bounded search. Analysis decodes grayscale frames with a maximum dimension of 320 pixels and samples at up to 10 fps, with fewer than 200 results. Original full-resolution video stays linked on disk. Rotation metadata is considered when selecting analysis dimensions; clip position/scale/rotation and canvas letterboxing are considered when mapping a tracked point into the project.

This is **translation point tracking**. It is not semantic object detection, face tracking, planar/perspective tracking, optical-flow stabilization, camera solving, automatic mask tracking or rotoscoping. A feature that changes size/angle substantially, leaves the image, is occluded or resembles another nearby feature may be lost or mismatched. Confidence is a correlation score, not a probability of correctness. Shorter ranges and manual inspection are part of the workflow.

The target must overlap the tracked time range. A source that changes after analysis must be tracked again. Applying replaces the target's transform keys in the tracked interval, retains keys outside it and preserves the other transform channels at the sampled times. Curved animation in those other channels can consequently be approximated between samples. The combined result must fit the existing 200-key limit. This operation moves the target layer; it does not stabilize or modify the source video.

## Draw vector paths and reveal strokes

Create a vector layer. Its canvas shows anchors and tangent handles; it edits geometry, not a bitmap.

- Drag an anchor to move it with its handles. Click empty canvas to append a corner. Select a node to edit its coordinates.
- **Smooth handles** opens horizontal tangent handles; drag them for the desired curve. **Corner** collapses them back onto the anchor. Coincident handles do not steal anchor drags.
- Close a path to connect its last node back to its first. Choose stroke/fill colors, stroke width and optional fill.
- **Draw-on duration** animates a stroke along its approximate arc length. This is an actual path reveal, unlike Motion Studio's image wipes. A duration of zero shows the complete stroke. **Stroke start/end** trim the visible range.
- Fill is independent of the animated stroke and can be visible before the outline finishes. Use ordinary opacity animation if the whole filled object should fade in.

The curve is sampled more densely for larger output canvases, then rasterized with antialiasing. Source geometry remains editable and is rerendered at export resolution. A path supports 2–128 nodes. Coordinates and handles support the same extended −100% to 200% range as layer motion; extremely off-canvas handles may need numerical edits or resetting to recover them in the editor.

### Morph a shape

Click **Create target from base**, then select **Morph target** and move its anchors/handles. Set a morph duration. The base and target interpolate with smooth easing; the clip-time slider shows the resulting geometry. Closed shapes can change silhouette and filled area. Base/target paths must have matching node counts and order. Adding or deleting a node clears the target so the project cannot silently save an incompatible morph.

This is correspondence-based cubic-path morphing, not automatic correspondence between unrelated illustrations or a multi-path character rig. Choose matching node locations deliberately. Splitting, trimming, retiming and reversal preserve the procedural animation clock, so a cut does not restart a stroke, morph or 3D turntable.

### SVG geometry

Import a **single SVG path** with M/L/H/V/C/Q/Z geometry, in absolute or relative coordinates. Quadratic curves are converted to cubic geometry. A viewBox is normalized to the canvas coordinate system. Export creates editable cubic SVG geometry for the base path. SVG colors/styles, fonts, filters, masks, external references, groups with transforms, compound paths and arc commands are not reconstructed. Unsupported transforms/objects/commands are rejected rather than silently approximated. Convert/flatten the artwork to one compatible path first. SVG export writes to a new filename to preserve existing files.

The provided `Templates/VFX/flight-path.svg` is original sample geometry. `Vector draw-on.frame` and `Vector morph.frame` are ready-to-edit examples in Templates.

## Build and animate a 3D explanation

Create a 3D scene layer. Its right-hand shaded preview updates on a background worker while you edit the scene. It uses perspective projection, a depth buffer, directional shading and transparent surroundings, so the result can be composited over footage.

### Objects

Add cubes, spheres, cylinders or a rocket primitive. Select an object to edit its name, material color, XYZ position, XYZ rotation and uniform scale. Spin controls add continuous rotation in degrees per second. Each scene supports up to 16 objects.

For an exploded view, place components together initially, record poses, move the clip-time slider forward, separate them with XYZ positions and record more poses. **Record object pose** saves the current fields at the current content time with smooth easing. Object-key lists let you load a saved pose/time, record a replacement, delete a key or clear the animation. With keys present, changing the pose fields prepares the pose to record; it does not overwrite every existing key. Spin adds to the keyed rotation, so set it to zero when only keyed rotation is desired.

### Camera and lighting

Yaw/pitch orbit a perspective camera around the scene origin. Distance changes framing; the lens control changes vertical field of view. Record/load/delete camera poses to animate yaw, pitch, distance and lens. Continuous camera orbit is available separately. Ambient light controls the shaded/unshaded balance. Wireframe displays triangulated mesh edges; it is an alternative render style.

Edge antialiasing supersamples the scene at twice the dimensions for output up to 1920 × 1080 pixels in area, then downsamples premultiplied colors to avoid dark transparent fringes. Higher-resolution canvases render at their native resolution to bound memory. The native program monitor uses the same scene renderer and composition pipeline as export.

Try `3d-rocket-assembly.frame`: a rotating rocket and two components separate into an illustrated assembly view. It is geometry for explanation videos, not a flight/structural simulation or an editable CAD assembly.

### OBJ mesh import

Import a small polygonal OBJ into the selected object. Vertices and faces are embedded in the project; the OBJ file is not required to reopen it. Imported bounds are centered and normalized, and the object's scale/position places it in the scene. Negative OBJ indices and polygon faces are supported; polygons are triangulated as a fan, so export triangulated geometry when faces are concave.

Limits are 1 MiB per import, 2,048 vertices / 4,096 triangles per mesh, 8,192 imported triangles per scene and 100,000 imported vertices per project. OBJ texture coordinates, normals, material libraries, texture maps, rigging, animation and freeform surfaces are not imported. This is a small flat-shaded mesh workflow, not a full Blender/CAD/glTF pipeline. `Templates/VFX/pyramid.obj` is included for testing import.

## Animation motion blur

Choose **Off**, **2**, **4** or **8 samples**. Set animation shutter angle from **36° to 360°**; the default is 180°.

Frame evaluates generated layer motion, vector drawing/morphing, object animation and camera animation at sub-frame times. Complete composited samples are averaged back to the project's output frame rate. Stationary layers remain sharp. A shorter shutter reduces the spread of sampled motion; a longer shutter produces longer trails. Sampling windows start at each output-frame time. A 180° shutter samples half of a frame interval; 360° spans a full interval.

This does not invent intermediate detail in camera footage. Available decoded video frames still determine source motion; shutter angle controls generated transforms/procedural animation, not reconstruction of the original camera shutter. Clean Timelapse's source-frame blending and exposure-flicker reduction remain separate clip effects. Combining the effects increases processing cost.

Preview uses the same exposure settings at lower resolution. Two samples are inexpensive relative to eight, but can show separated echoes during extremely fast motion. More samples reduce those echoes at additional CPU and memory cost. Use short test exports to choose the amount. `Motion blur study.frame` demonstrates a moving graphic with four samples.

## Persistence, responsiveness and verification

- Vector nodes, morph targets, object meshes/materials/poses, camera keys, procedural clocks and project blur settings are included in `.frame` saves. Older project documents receive defaults and remain readable.
- Procedural sources are generated **one RGBA frame at a time**, streamed into temporary lossless FFV1 video. Full uncompressed image sequences are not held in RAM or written as thousands of PNGs. The work is bounded by the existing render windows of at most 30 seconds and intersecting-layer limits.
- Tracking, shaded-preview updates, procedural source generation, FFmpeg decoding and export run on workers. Subprocesses are owned and cancelled; incomplete procedural output is removed. Generated lossless source videos are released when their render graph finishes or fails, and preview directories are removed when their workers finish.
- 3D uses automatic DX12/Vulkan hardware rasterization with a CPU fallback; final H.264 encoding can use NVIDIA NVENC. Pixel readback, composition and temporal averaging still use the CPU. High resolution, many meshes, antialiasing and eight sub-frame samples multiply work. See the [GPU guide](gpu-guide.md) for settings, actual acceleration scope and verification limits.
- The VFX tests cover synthetic motion and occlusion, actual encoded-video tracking and retiming, target application, SVG geometry and malformed input, morph/stroke rendering, OBJ import, depth/shading, procedural clocks/JSON, actual vector/3D source rendering, shutter samples compared with independently rendered frames, stationary-layer sharpness, export fps/frame count and cancellation cleanup. Native UI tests exercise all four panels.

Further capabilities outside this implementation include planar/3D camera tracking, semantic segmentation, rigging, textured/PBR rendering, arbitrary camera translation/targets, shadows, editable multi-path SVG documents, automatic topology matching, particle/fluid systems and optical-flow video generation.
