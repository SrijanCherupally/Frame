# Building polished engineering videos in Frame

Reference: **I Built a Self Landing Satellite — I Build Stuff**, supplied by the user, corresponding to `https://www.youtube.com/watch?v=7yVFZn87TkY`.

## What was inspected

The uploaded 144p copy is 15:28.38 long, 256 × 144, approximately 29.97 fps. Review covered 62 overview samples across its full duration, 244 automatically detected visual-change candidates, and half-second samples of the 00:24–00:38 title sequence. Candidate frames include fast camera/hand movements and are **not** an exact cut count. Timestamps below are approximate viewing locations.

This is a visual breakdown. The soundtrack was not auditioned or transcribed here, and the small reference cannot establish the original color settings, shutter angle, interpolation algorithm, or whether particular blurred frames came from the camera or post-production. The reference media is not bundled with Frame.

## Observed techniques and the capabilities they require

| Reference section | Visible technique | Required editing capabilities | Frame implementation |
| --- | --- | --- | --- |
| 00:00–00:31 | Outdoor presentation and parachute footage establish the object and objective before the workshop sequence. | Precise trims, a strong opening selection, cutaways, source audio control. | Existing timeline and split/trim tools; **Detach audio** adds independent audio timing. |
| Approximately 00:31–00:35 | Large two-line **GUIDED LANDING** lettering contains moving footage against black, then the scene resolves to full footage. | Text alpha mattes, large text, layers with matching source timing, fades. | **Mask → Text**, editable multiline text and 12–800 px font size. Animate opacity or overlap a fade into an unmasked copy for the reveal to full footage. |
| Approximately 01:04–01:08 | Images of other projects appear over workshop footage with attribution near the bottom. | Picture-in-picture scale/position, layered titles, independent duration, softened background. | Existing transforms/titles/color softness, plus continuous eased entrances and motion paths. Add attribution as a separate title layer. |
| 01:16–01:31 | Rapid close-up wiring and soldering shots compress multiple operations into a short sequence. Some cuts are around a second apart, with much shorter changes in the fastest section. | Frame-accurate selection, ripple deletion, source retiming and repeatable shot layout. | Existing split/trim/Remove frames and unrestricted practical retiming; smooth transform presets add deliberate reframing. A whole long clip at one high speed does not reproduce this editorial rhythm. |
| 01:42–02:41 | Assembly, servo and mechanism demonstrations alternate details and wider views. | Cutaway layering, freeze/hold-like explanations, labels and directional graphics. | Existing clips, titles and diagrams; new reveal controls and curved arrivals support annotation. Full automatic subject tracking is not implemented. |
| 02:45–03:14; 03:41–03:44; 06:12–06:32 | CAD and printer views explain parts and construction. Several CAD views are filmed on a laptop screen. | Import recorded CAD footage/stills, inspect details with pans/zooms, add explanation overlays. | Footage import, **cinematic drift**, slow pushes/pulls and diagram layers. Frame does not import editable CAD meshes or replace CAD/3D rendering software. |
| 03:14–04:09 | Orange parts progress from laid-out pieces to a working mechanism through short assembly shots. | Action matching, consistent framing, careful shot endpoints and occasional retiming. | Existing cutting and source timing, plus reusable animation copied between clips. Spatial/easing curves survive cuts and retiming. |
| 04:36–05:38 | Close electronics views, screen details and testing alternate to communicate setup and debugging. | Readable screen inserts, punch-ins, callouts, arrows and independent audio editing. | Continuous eased transforms, editable callouts and text, source-audio detachment. Screen capture itself remains external. |
| Approximately 06:40–07:00 | Wider workshop activity contrasts with the close construction details. | Long-take compression, a stable camera, consistent exposure, controlled motion trails. | Existing Clean Timelapse blends bounded source samples before retiming and can reduce exposure flicker. The reference resolution does not establish its exact retiming/blur settings. |
| 07:43–08:00 | Very short assembly inserts, close-ups and angle changes make the final fit-out feel active. | Efficient trimming, duplication, undo, different shot scales and deliberate cadence. | Existing timeline editing; clip animations can now be copied, fitted to duration and reversed. |
| Approximately 08:38–08:59 | A monochrome surveillance-style insert has recording/timecode graphics and visible subtitles. | Desaturation, layered graphics, timed text, potentially reusable overlay assets. | Monochrome color preset, titles and imported overlays are available. Automatic captions, editable subtitle tracks, dynamic timecode generators and a complete surveillance-effect template remain missing. |
| 09:27–09:59 | Website/product inserts and close-ups appear within the build narrative; a colored line is visible along the bottom in this portion. | Full-frame inserts, graphic overlays and deliberate section timing. | Existing import/graphics/layer timing; progress and line-style graphics can be revealed. This review does not infer how the reference's line was generated. |
| 10:10–10:45 | Aircraft, landscape and bus clips form a travel bridge; location text appears over footage. | Short montage cuts, a location title and visual continuity across different shots. | Existing timeline and titles; **rise**, **focus reveal** and **cinematic drift** offer controlled motion. |
| 10:58–12:27 | Preparation and launch coverage alternates people, hardware, wide views and sky footage. | Long source handling, precise source ranges, independent natural sound and clear narrative ordering. | Disk-linked originals/proxies and independent audio clips; no automatic multicam synchronization or stabilization yet. |
| Approximately 12:40–13:37 | Onboard flight footage carries telemetry readouts and plotted data near the top. The review cannot determine which overlays were baked into the recording. | Layered chart footage or data-driven plotting with synchronized time and readable labels. | **Data curve** graphics accept embedded two-column CSV data and a reveal animation. Existing recordings with baked-in telemetry can be imported. Multiple synchronized telemetry channels, units/legend editing, time-linked graph cursors and GPS maps are future work. |
| 13:41–15:28 | People reviewing screens, recorded flight views, outdoor testing, plotting screens and closing shots alternate. | PIP/overlays, result comparisons, natural shot lengths and an ending that resolves the build. | Existing footage/title layers plus comparison cards, CSV charts, reveal effects and reusable motion. |

## Motion Studio

Select a visual clip, choose **Motion**, then **Open Motion Studio**. The inspector's Animation card provides the same entry point.

- **Keyframes & channels:** select a key, edit its exact time, X/Y, scale, rotation or opacity; add a key at the playhead or create start/end keys. Keys remain editable project data.
- **Timing curve:** choose linear, accelerate, decelerate, smooth, cinematic smooth, hold/jump or custom cubic Bézier. Drag the gold timing handles for custom curves. Easing belongs to the segment **after** the selected key; the final key has no outgoing segment.
- **Spatial motion path:** enable a curved segment, then drag its anchors and two control handles. Timing easing changes speed along that path. These controls edit 2D layer motion; they are not a 3D camera or automatic object tracker.
- **Reuse:** Copy animation and Paste & fit transfer transforms, paths, easing and reveal settings to another layer, rescaling time to its duration. Paste replaces the receiving layer's positions too. Reverse motion reverses the transform animation; it does not reverse source footage or reverse a reveal effect.
- **Reveal:** choose left-to-right, bottom-to-top or iris, then set its duration. These use a smooth, zero-velocity entrance and appear in preview and export. They reveal the existing picture; they do **not** trace arbitrary vector strokes or morph shapes.
- **Preview from start:** plays the selected layer from its timeline start so its entrance can be judged against surrounding clips.

Fourteen motion presets include soft reveal, rise, left/right arrival, pop, push/pull, floating, turn, curved arrival, orbit, pulse, drift and focus reveal. Entrance duration and intensity are adjustable before applying. Slow moves/loops span the clip's duration. Orbit maintains a continuous path; float and pulse are one smooth cycle, not infinitely repeating procedural effects.

Presets store at most a few meaningful keys, rather than approximating all motion with fixed-rate samples. Smooth and cinematic curves have zero endpoint velocity; cinematic easing also settles acceleration at its endpoints. Custom timing uses monotone handles restricted to 0–1, so it cannot accidentally overshoot opacity or produce negative scales. For overshoot, use the pop preset or add an explicit intermediate key.

Key insertion, split, ripple deletion, left trim, retime, reverse and fitted paste preserve curve sections and Bézier geometry. Curved paths can change apparent speed as curvature changes; automatic constant-distance path traversal and a velocity graph are not implemented.

## Diagrams and CSV charts

The library now has **18 graphic types** and **seven explanation scenes**, including orbital mechanics, flight trajectory, signal/response, design comparison, rocket forces, build chapter and flight report. Scenes use staggered entrances and coordinated timing. They are editable timelines, not locked video templates.

For telemetry-style charts:

1. Add **Graphics → Data curve**.
2. In Text & appearance, import a CSV with two columns. An optional `time,value` or `x,y` header is accepted; commas, tabs or semicolons can separate the columns.
3. Use strictly increasing time/X values and finite numbers. Each chart supports up to 2,048 points and imports at most 1 MiB; all charts in a project share a 100,000-point limit. Exporting a suitable sample from a telemetry tool is preferable to importing millions of raw samples.
4. Frame scales the chart to the data range and embeds the points in the `.frame` document. Empty charts show an **illustrated example**, not measured telemetry.
5. Choose a left-to-right reveal in Motion Studio. To reveal approximately with data time, align the chart clip to the footage and choose an appropriate reveal duration. The current reveal sweeps the complete graphic, including its axes; it is not a precise per-sample telemetry clock.

The ZIP includes `Templates/Data/illustrative-flight-profile.csv` for practicing import ([source example](../examples/data/illustrative-flight-profile.csv)); its altitude values are invented.

The sample wave, trajectory and orbital diagram are illustrations. Frame does not simulate a flight, solve control equations or validate telemetry. Import real measurements for factual charts, and label simulations, units and comparisons clearly.

## A practical recipe for a video with this pacing

1. **Build the story first.** Choose the opening result/problem, then arrange design, first build, failed test, revision, final test and evidence of the result. Keep a rough assembly before polishing animation.
2. **Collect useful coverage.** For each operation, record a wide shot, a close detail and the visible result. Hold the camera still for some long operations. Capture clean, consistent light; global blur cannot manufacture the reference's selective focus.
3. **Make montages from selected actions.** Trim to the useful hand movement or mechanical change, vary the shot scale, and remove dead time. Use high-speed clips for long repetitive work, mixed with normal-speed moments that make the process understandable.
4. **Treat audio as an independent edit.** Detach audio when an explanation or natural sound should continue across a picture cut. Keep voice readable, lower music underneath it and place mechanical sounds deliberately. These are production recommendations; the reference's exact mix was not analyzed. EQ, noise cleanup and automatic ducking are not yet in Frame.
5. **Explain one relationship at a time.** Bring in the object, reveal the arrow/path, then introduce the label about 0.15–0.3 seconds later. Start with the staggered scenes, modify their content and place them over a quiet shot or a simple background.
6. **Choose motion with purpose.** Use a 0.4–0.9 second eased arrival for labels, a gentle 5–15% push over several seconds for a detail, and a restrained pop for emphasis. Avoid making every object bounce, orbit or rotate simultaneously. Inspect the path and timing curve when a move feels abrupt.
7. **Keep diagrams legible.** Use a small set of colors, generous negative space and consistent label positions. Leave important material away from picture edges and possible player captions.
8. **Make the title.** Duplicate the relevant footage source, set Mask → Text, use large two-line lettering and align the source ranges. Transition to an unmasked layer below it at the intended moment.
9. **Support the result.** Combine the flight recording with a measured-data chart or a separately rendered telemetry overlay. Keep the time relationship and units honest. Pause on an important result long enough to understand it.
10. **Review at delivery settings.** 60 fps can help generated motion when desired; it does not create authentic intermediate detail in 30 fps camera footage. At 24/30 fps, use slower moves and short test renders. Do not rely on a stressed proxy preview alone to judge an export's smoothness.

## Quality and remaining work

Motion rendering tests compare full streamed frames and mid-segment seeks against the same model samples for all easing modes and curved paths. Additional tests cover actual reveal exports, text mattes, CSV-rendered pixel differences, dense key tracks, legacy JSON, round trips, malformed input, editing invariance and native Motion Studio drawing.

The rendering update also fixes two concrete zoom/rotation problems: overlay positioning used stale dimensions during variable-size scaling, and rotating after a dynamic scale could clip later frames to the initial bounds. Rotation now receives a fixed canvas and positioning uses the current scaled dimensions explicitly.

VFX Studio now extends these tools with confidence-based point tracking, editable cubic vectors and stroke drawing, equal-topology morphing, perspective 3D scenes and OBJ geometry, keyed camera/object poses, and sub-frame animation motion blur. See the [VFX guide](vfx-guide.md) for supported workflows and exact limits.

Frame still lacks a complete After Effects/Resolve Fusion workflow: semantic/planar/camera tracking, arbitrary mask tracking, full multi-path SVG editing, automatic morph topology matching, nested/parented compositions, rigging, textured/PBR 3D scenes, GPU composition, optical flow, audio automation/EQ, subtitle tracks, stabilization and synchronized telemetry channels remain outside this implementation. Clean Timelapse source blending and generated-animation motion blur are separate effects.
