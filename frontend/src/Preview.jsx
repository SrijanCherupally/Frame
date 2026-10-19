import React, { useRef, useEffect, useState } from "react";
import { clipFrame, clamp } from "./editor";

export default function Preview({ project, assets, time, playing, onError }) {
  const canvas = useRef(null),
    pool = useRef(new Map()),
    state = useRef({ project, assets, time, playing, onError });
  state.current = { project, assets, time, playing, onError };
  useEffect(() => {
    const resources = pool.current;
    let raf,
      dirty = true,
      lastProject,
      lastAssets,
      lastTime,
      lastPlaying;
    document.fonts.load('16px "Frame Sans"').then(() => {
      dirty = true;
    });
    const draw = () => {
      const { project: p, assets, time, playing, onError } = state.current;
      if (
        !dirty &&
        lastProject === p &&
        lastAssets === assets &&
        lastTime === time &&
        lastPlaying === playing
      ) {
        raf = requestAnimationFrame(draw);
        return;
      }
      dirty = false;
      lastProject = p;
      lastAssets = assets;
      lastTime = time;
      lastPlaying = playing;
      const cv = canvas.current;
      if (!cv) return;
      // Preview stays at 960px while exports render from originals at project resolution.
      const w = 960,
        h = Math.round((960 * p.height) / p.width);
      if (cv.width !== w || cv.height !== h) {
        cv.width = w;
        cv.height = h;
      }
      const ctx = cv.getContext("2d");
      ctx.fillStyle = "#090c10";
      ctx.fillRect(0, 0, w, h);
      const live = new Set();
      for (const c of [...p.clips].sort(
        (a, b) => b.track - a.track || a.start - b.start,
      )) {
        const active =
          time >= c.start &&
          time < c.start + c.duration &&
          !p.hidden_tracks.includes(c.track);
        if (!active) continue;
        const a = assets.find((a) => a.id === c.asset_id);
        let media;
        if (a) {
          const key = c.id;
          const expected =
            `/api/media/${a.id}/preview` +
            (a.status === "ready" ? "?ready=1" : "");
          if (
            resources.has(key) &&
            resources.get(key).dataset?.status !== a.status
          ) {
            const old = resources.get(key);
            if (old.pause) old.pause();
            old.src = "";
            resources.delete(key);
          }
          if (!resources.has(key)) {
            let el;
            if (a.kind === "image") {
              el = new Image();
              el.src = expected;
            } else {
              el = document.createElement(
                a.kind === "audio" ? "audio" : "video",
              );
              el.preload = "auto";
              el.src = expected;
              el.playsInline = true;
            }
            el.addEventListener("load", () => {
              dirty = true;
            });
            el.addEventListener("loadeddata", () => {
              dirty = true;
            });
            el.addEventListener("seeked", () => {
              dirty = true;
            });
            el.addEventListener("error", () => {
              dirty = true;
            });
            el.dataset.status = a.status;
            resources.set(key, el);
          }
          media = resources.get(key);
          live.add(key);
          if (a.kind !== "image") {
            const target = c.source_in + (time - c.start) * c.speed;
            media.playbackRate = clamp(c.speed, 0.0625, 16);
            const fade =
              c.transition === "fade"
                ? clamp(
                    Math.min(
                      (time - c.start) / c.fade_duration,
                      (c.start + c.duration - time) / c.fade_duration,
                    ),
                    0,
                    1,
                  )
                : 1;
            media.volume = clamp(c.volume * fade, 0, 1);
            media.muted = p.muted_tracks.includes(c.track);
            if (
              media.readyState >= 1 &&
              Math.abs(media.currentTime - target) > (playing ? 0.25 : 0.025) &&
              !media.seeking
            )
              media.currentTime = target;
            if (playing && media.paused && c.speed <= 16)
              media.play().catch(() => {});
            if (!playing && !media.paused) media.pause();
          }
        }
        if (c.kind === "audio") continue;
        const local = time - c.start,
          v = clipFrame(c, local);
        ctx.save();
        ctx.globalAlpha = clamp(v.opacity, 0, 1);
        if (c.transition === "wipe") {
          ctx.beginPath();
          ctx.rect(
            0,
            0,
            w * Math.min(local / Math.min(c.fade_duration, c.duration / 2), 1),
            h,
          );
          ctx.clip();
        }
        ctx.translate((w * v.x) / 100, (h * v.y) / 100);
        ctx.rotate((v.rotation * Math.PI) / 180);
        ctx.scale(v.scale, v.scale);
        if (c.mask !== "none") {
          ctx.beginPath();
          if (c.mask === "circle")
            ctx.ellipse(0, 0, w * 0.28, h * 0.45, 0, 0, Math.PI * 2);
          else ctx.rect(-w * 0.3, -h * 0.35, w * 0.6, h * 0.7);
          ctx.clip();
        }
        if (media && c.kind !== "audio") {
          const mw = media.videoWidth || media.naturalWidth,
            mh = media.videoHeight || media.naturalHeight;
          if (mw && mh) {
            ctx.filter = `brightness(${Math.max(0, 1 + c.brightness)}) contrast(${c.contrast}) saturate(${c.saturation}) blur(${(c.blur * w) / p.width}px)`;
            const ratio = Math.min(w / mw, h / mh),
              dw = mw * ratio,
              dh = mh * ratio;
            ctx.drawImage(media, -dw / 2, -dh / 2, dw, dh);
            ctx.filter = "none";
            if (c.warmth) {
              ctx.globalCompositeOperation = "soft-light";
              ctx.globalAlpha = Math.abs(c.warmth) * 0.3 * v.opacity;
              ctx.fillStyle = c.warmth > 0 ? "#ff9d45" : "#438bff";
              ctx.fillRect(-dw / 2, -dh / 2, dw, dh);
              ctx.globalCompositeOperation = "source-over";
            }
          }
        } else if (c.kind === "title") {
          const size = (c.font_size * w) / 1920;
          ctx.font = `${size}px "Frame Sans", Arial, sans-serif`;
          ctx.fillStyle = c.color;
          ctx.textAlign = "center";
          ctx.textBaseline = "middle";
          const lines = c.text.split("\n"),
            lineHeight = size + (10 * w) / p.width;
          lines.forEach((line, i) =>
            ctx.fillText(line, 0, (i - (lines.length - 1) / 2) * lineHeight),
          );
        } else if (c.kind === "graphic") {
          ctx.strokeStyle = c.color;
          ctx.fillStyle = c.color;
          ctx.lineWidth = Math.max(3, w / 180);
          ctx.lineCap = "round";
          if (c.graphic === "arrow") {
            ctx.beginPath();
            ctx.moveTo(-w * 0.2, 0);
            ctx.lineTo(w * 0.2, 0);
            ctx.moveTo(w * 0.12, -h * 0.1);
            ctx.lineTo(w * 0.2, 0);
            ctx.lineTo(w * 0.12, h * 0.1);
            ctx.stroke();
          } else if (c.graphic === "circle") {
            ctx.beginPath();
            ctx.ellipse(0, 0, w * 0.15, h * 0.27, 0, 0, Math.PI * 2);
            ctx.stroke();
          } else if (c.graphic === "rectangle")
            ctx.strokeRect(-w * 0.25, -h * 0.25, w * 0.5, h * 0.5);
          else {
            ctx.fillStyle = "#0e1518e1";
            ctx.fillRect(-w * 0.44, h * 0.22, w * 0.59, h * 0.18);
            ctx.fillStyle = c.color;
            ctx.fillRect(-w * 0.44, h * 0.22, w * 0.006, h * 0.18);
            ctx.font = `${(40 * w) / 1920}px "Frame Sans",Arial`;
            ctx.fillText(c.text.slice(0, 80), -w * 0.415, h * 0.31);
          }
        }
        ctx.restore();
      }
      for (const [id, el] of resources) {
        if (!live.has(id) && el.pause) el.pause();
        if (!p.clips.some((c) => c.id === id)) {
          el.src = "";
          if (el.load) el.load();
          resources.delete(id);
        }
      }
      raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(raf);
      for (const el of resources.values()) {
        if (el.pause) el.pause();
        el.src = "";
      }
      resources.clear();
    };
  }, []);
  return (
    <canvas
      ref={canvas}
      className="preview-canvas"
      aria-label="Video preview"
    />
  );
}
