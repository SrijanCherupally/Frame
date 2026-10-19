export const TRACKS = [
  { id: 0, name: "Titles & graphics", type: "visual", color: "#bd9bef" },
  { id: 1, name: "Main footage", type: "visual", color: "#83bba8" },
  { id: 2, name: "B-roll", type: "visual", color: "#719dbd" },
  { id: 3, name: "Voiceover", type: "audio", color: "#deaa78" },
  { id: 4, name: "Music & sound", type: "audio", color: "#a197d0" },
];
export const uid = () => crypto.randomUUID();
export const clamp = (n, lo, hi) => Math.min(hi, Math.max(lo, n));
export const endTime = (p) =>
  Math.max(0, ...p.clips.map((c) => c.start + c.duration));
export function timecode(n, fps = 30) {
  n = Math.max(0, n || 0);
  const s = Math.floor(n);
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}:${String(Math.floor((n - s) * fps + 0.0001)).padStart(2, "0")}`;
}
export const defaults = {
  source_in: 0,
  speed: 1,
  volume: 1,
  x: 50,
  y: 50,
  scale: 1,
  opacity: 1,
  rotation: 0,
  brightness: 0,
  contrast: 1,
  saturation: 1,
  warmth: 0,
  blur: 0,
  motion_blur: false,
  shutter: 0.5,
  deflicker: false,
  mask: "none",
  transition: "none",
  fade_duration: 0.5,
  font_size: 64,
  color: "#ffffff",
  graphic: "arrow",
  text: "Your story starts here",
  keyframes: [],
};
export function newClip(fields) {
  return {
    ...defaults,
    id: uid(),
    duration: 5,
    start: 0,
    track: 1,
    kind: "video",
    name: "Clip",
    ...fields,
  };
}
export function demoProject() {
  return {
    version: 1,
    name: "A little closer to space",
    width: 1920,
    height: 1080,
    fps: 30,
    muted_tracks: [],
    hidden_tracks: [],
    clips: [
      newClip({
        id: "launch",
        asset_id: "demo-0",
        kind: "image",
        name: "01 · Launch day",
        start: 0,
        duration: 8,
        transition: "fade",
      }),
      newClip({
        id: "build",
        asset_id: "demo-1",
        kind: "image",
        name: "02 · From the workbench",
        start: 8,
        duration: 7,
        transition: "fade",
      }),
      newClip({
        id: "flight",
        asset_id: "demo-2",
        kind: "image",
        name: "03 · Into the sky",
        start: 15,
        duration: 9,
        transition: "fade",
      }),
      newClip({
        id: "title",
        kind: "title",
        name: "A LITTLE CLOSER TO SPACE",
        text: "A LITTLE CLOSER\nTO SPACE",
        font_size: 108,
        start: 0,
        duration: 6,
        track: 0,
        x: 34,
        y: 44,
        scale: 0.8,
        transition: "fade",
        keyframes: [
          { time: 0, x: 34, y: 46, scale: 0.8, opacity: 1, rotation: 0 },
          { time: 6, x: 34, y: 42, scale: 0.84, opacity: 1, rotation: 0 },
        ],
      }),
      newClip({
        id: "subtitle",
        kind: "title",
        name: "THE ROCKET DIARIES / EP. 001",
        text: "THE ROCKET DIARIES   /   EP. 001",
        font_size: 25,
        start: 0,
        duration: 6,
        track: 0,
        x: 34,
        y: 61,
        color: "#b2ccbd",
        transition: "fade",
      }),
      newClip({
        id: "build-title",
        kind: "graphic",
        graphic: "lower-third",
        name: "Build annotation",
        text: "01  /  FROM THE WORKBENCH",
        start: 9,
        duration: 5,
        track: 0,
        color: "#c6e99d",
        transition: "slide",
      }),
    ],
  };
}
export function splitClip(project, id, time) {
  const c = project.clips.find((c) => c.id === id);
  if (
    !c ||
    time - c.start < 1 / project.fps ||
    c.start + c.duration - time < 1 / project.fps
  )
    return project;
  const offset = time - c.start;
  const sample = (t) =>
    Object.fromEntries(
      ["x", "y", "scale", "opacity", "rotation"].map((p) => [
        p,
        keyValue(c, p, t),
      ]),
    );
  const leftKeys = c.keyframes.length
    ? [
        ...c.keyframes.filter((k) => k.time < offset),
        { time: offset, ...sample(offset) },
      ]
    : [];
  const rightKeys = c.keyframes.length
    ? [
        { time: 0, ...sample(offset) },
        ...c.keyframes
          .filter((k) => k.time > offset)
          .map((k) => ({ ...k, time: k.time - offset })),
      ]
    : [];
  return {
    ...project,
    clips: project.clips.flatMap((x) =>
      x.id !== id
        ? [x]
        : [
            { ...c, duration: offset, keyframes: leftKeys },
            {
              ...c,
              id: uid(),
              start: time,
              source_in: c.source_in + offset * c.speed,
              duration: c.duration - offset,
              keyframes: rightKeys,
            },
          ],
    ),
  };
}
export function removeRange(project, start, end) {
  if (end <= start) return project;
  const delta = end - start;
  return {
    ...project,
    clips: project.clips.flatMap((c) => {
      const ce = c.start + c.duration;
      if (ce <= start) return [c];
      if (c.start >= end) return [{ ...c, start: c.start - delta }];
      const pieces = [];
      if (c.start < start)
        pieces.push({
          ...c,
          duration: start - c.start,
          keyframes: c.keyframes.length
            ? [
                ...c.keyframes.filter((k) => k.time < start - c.start),
                {
                  time: start - c.start,
                  ...Object.fromEntries(
                    ["x", "y", "scale", "opacity", "rotation"].map((p) => [
                      p,
                      keyValue(c, p, start - c.start),
                    ]),
                  ),
                },
              ]
            : [],
        });
      if (ce > end) {
        const offset = end - c.start;
        pieces.push({
          ...c,
          id: uid(),
          start: Math.max(start, c.start - delta),
          duration: ce - end,
          source_in: c.source_in + offset * c.speed,
          keyframes: c.keyframes.length
            ? [
                {
                  time: 0,
                  ...Object.fromEntries(
                    ["x", "y", "scale", "opacity", "rotation"].map((p) => [
                      p,
                      keyValue(c, p, offset),
                    ]),
                  ),
                },
                ...c.keyframes
                  .filter((k) => k.time > offset)
                  .map((k) => ({ ...k, time: k.time - offset })),
              ]
            : [],
        });
      }
      return pieces;
    }),
  };
}
export function keyValue(c, prop, t) {
  const keys = [...c.keyframes].sort((a, b) => a.time - b.time);
  if (!keys.length) return c[prop];
  if (t <= keys[0].time) return keys[0][prop];
  for (let i = 1; i < keys.length; i++)
    if (t <= keys[i].time) {
      const a = keys[i - 1],
        b = keys[i];
      return a[prop] + ((b[prop] - a[prop]) * (t - a.time)) / (b.time - a.time);
    }
  return keys.at(-1)[prop];
}
export function clipFrame(c, local) {
  const v = Object.fromEntries(
    ["x", "y", "scale", "opacity", "rotation"].map((p) => [
      p,
      keyValue(c, p, local),
    ]),
  );
  const fade = Math.min(c.fade_duration, c.duration / 2);
  if (c.transition === "fade")
    v.opacity *= clamp(
      Math.min(local / fade, (c.duration - local) / fade),
      0,
      1,
    );
  if (c.transition === "slide") v.x -= 100 * Math.max(1 - local / fade, 0);
  if (c.transition === "zoom")
    v.scale *= 0.8 + 0.2 * Math.min(local / c.fade_duration, 1);
  return v;
}
