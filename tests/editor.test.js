import test from "node:test";
import assert from "node:assert/strict";
import {
  newClip,
  splitClip,
  removeRange,
  keyValue,
  timecode,
  clipFrame,
} from "../frontend/src/editor.js";
const project = (clips) => ({ fps: 30, clips });

test("split retains source time at high speed and interpolates animation at the cut", () => {
  const c = newClip({
    id: "a",
    duration: 20,
    start: 3,
    source_in: 12,
    speed: 720,
    keyframes: [
      { time: 0, x: 10 },
      { time: 20, x: 90 },
    ],
  });
  const p = splitClip(project([c]), "a", 13);
  assert.equal(p.clips.length, 2);
  assert.equal(p.clips[0].duration, 10);
  assert.equal(p.clips[1].source_in, 7212);
  assert.equal(p.clips[1].start, 13);
  assert.equal(keyValue(p.clips[0], "x", 10), 50);
  assert.equal(keyValue(p.clips[1], "x", 0), 50);
});
test("remove frames preserves source offsets and ripples across tracks", () => {
  const a = newClip({ id: "a", start: 0, duration: 10, speed: 2 });
  const b = newClip({ id: "b", start: 10, duration: 5, track: 4 });
  const p = removeRange(project([a, b]), 3, 5);
  assert.equal(p.clips.length, 3);
  assert.equal(p.clips[0].duration, 3);
  assert.equal(p.clips[1].start, 3);
  assert.equal(p.clips[1].source_in, 10);
  assert.equal(p.clips[1].duration, 5);
  assert.equal(p.clips[2].start, 8);
});
test("frame deletion inside an animated clip preserves animation at resumed boundary", () => {
  const c = newClip({
    id: "a",
    duration: 10,
    keyframes: [
      { time: 0, x: 0 },
      { time: 10, x: 100 },
    ],
  });
  const p = removeRange(project([c]), 3, 5);
  assert.equal(keyValue(p.clips[0], "x", 3), 30);
  assert.equal(keyValue(p.clips[1], "x", 0), 50);
  assert.equal(keyValue(p.clips[1], "x", 5), 100);
});
test("one-frame or outside split does not create zero-length clips", () => {
  const p = project([newClip({ id: "a", start: 4, duration: 10 })]);
  assert.equal(splitClip(p, "a", 4), p);
  assert.equal(splitClip(p, "a", 3), p);
});
test("fade and zoom have defined boundaries", () => {
  const c = newClip({ transition: "fade", duration: 1, fade_duration: 0.5 });
  assert.equal(clipFrame(c, 0).opacity, 0);
  assert.equal(clipFrame(c, 0.5).opacity, 1);
  assert.equal(clipFrame(c, 1).opacity, 0);
  assert.equal(clipFrame({ ...c, transition: "zoom" }, 0).scale, 0.8);
});
test("timecode supports long source durations", () => {
  assert.equal(timecode(14400), "240:00:00");
  assert.equal(timecode(20.5), "00:20:15");
});
