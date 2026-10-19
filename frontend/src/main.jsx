import React, { useState, useRef, useEffect, useCallback } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  ArrowDownToLine,
  ArrowLeft,
  ArrowRight,
  AudioLines,
  Check,
  CheckCheck,
  ChevronDown,
  ChevronRight,
  Circle,
  Clapperboard,
  Clock3,
  Copy,
  Diamond,
  Download,
  Eye,
  EyeOff,
  FileJson,
  Film,
  FolderOpen,
  Fullscreen,
  Gauge,
  Grip,
  HelpCircle,
  Image as ImageIcon,
  Layers,
  Loader2,
  Magnet,
  Maximize2,
  MoreHorizontal,
  MousePointer2,
  Move,
  Music2,
  Pause,
  Play,
  Plus,
  Redo2,
  RotateCcw,
  Save,
  Scissors,
  Search,
  Settings2,
  Shapes,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
  Sparkles,
  Square,
  Trash2,
  Type,
  Undo2,
  Upload,
  Volume2,
  VolumeX,
  WandSparkles,
  X,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import Preview from "./Preview";
import {
  TRACKS,
  uid,
  clamp,
  endTime,
  timecode,
  defaults,
  newClip,
  demoProject,
  splitClip,
  removeRange,
  keyValue,
} from "./editor";
import "./style.css";

const IconButton = ({
  icon: Icon,
  title,
  active,
  className = "",
  ...props
}) => (
  <button
    className={`icon-button ${active ? "active" : ""} ${className}`}
    title={title}
    aria-label={title}
    {...props}
  >
    <Icon size={16} />
  </button>
);
const bytes = (n) =>
  n > 1e9
    ? `${(n / 1e9).toFixed(1)} GB`
    : n > 1e6
      ? `${(n / 1e6).toFixed(1)} MB`
      : `${Math.round(n / 1024)} KB`;
const TABS = [
  ["media", FolderOpen, "Your media"],
  ["audio", AudioLines, "Audio"],
  ["text", Type, "Text"],
  ["graphics", Shapes, "Graphics"],
  ["transitions", Layers, "Transitions"],
  ["color", SlidersHorizontal, "Color & effects"],
];
async function api(url, options = {}) {
  const r = await fetch("/api" + url, options);
  if (!r.ok) {
    let e;
    try {
      e = await r.json();
    } catch {}
    throw new Error(
      typeof e?.detail === "string" ? e.detail : `Request failed (${r.status})`,
    );
  }
  return r.json();
}
const jsonOptions = (data) => ({
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(data),
});
function downloadJSON(p) {
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(p, null, 2)], { type: "application/json" }),
  );
  const a = document.createElement("a");
  a.href = url;
  a.download = p.name.replace(/[^a-z0-9 -]/gi, "") + ".frame.json";
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 5000);
}

const desktop = window.frameDesktop;
function App() {
  const [project, setProject] = useState(demoProject),
    [assets, setAssets] = useState([]),
    [time, setTime] = useState(2.5),
    [playing, setPlaying] = useState(false),
    [selected, setSelected] = useState("launch");
  const [tab, setTab] = useState("media"),
    [inspector, setInspector] = useState("video"),
    [search, setSearch] = useState(""),
    [toast, setToast] = useState(""),
    [ready, setReady] = useState(false),
    [saved, setSaved] = useState("Loading project…");
  const [zoom, setZoom] = useState(32),
    [snap, setSnap] = useState(true),
    [uploading, setUploading] = useState(null),
    [modal, setModal] = useState(null),
    [projectMenu, setProjectMenu] = useState(false),
    [job, setJob] = useState(null),
    [exportSize, setExportSize] = useState("1920x1080");
  const [range, setRange] = useState({ start: 0, end: 1 }),
    [historyVersion, setHistoryVersion] = useState(0),
    [snapshots, setSnapshots] = useState([]);
  const history = useRef({ past: [], future: [] }),
    fileInput = useRef(),
    projectInput = useRef(),
    timeline = useRef(),
    ruler = useRef(),
    projectRef = useRef(project),
    timeRef = useRef(time),
    loaded = useRef(false);
  projectRef.current = project;
  timeRef.current = time;
  const duration = endTime(project),
    clip = project.clips.find((c) => c.id === selected);
  const notify = useCallback((s) => setToast(s), []);
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(""), 5000);
    return () => clearTimeout(t);
  }, [toast]);
  const remember = useCallback(() => {
    history.current.past.push(projectRef.current);
    if (history.current.past.length > 100) history.current.past.shift();
    history.current.future = [];
    setHistoryVersion((x) => x + 1);
  }, []);
  const change = useCallback(
    (fn, record = true) => {
      if (record) remember();
      setProject((p) => (typeof fn === "function" ? fn(p) : fn));
    },
    [remember],
  );
  const patch = (id, fields, record = true) =>
    change(
      (p) => ({
        ...p,
        clips: p.clips.map((c) => (c.id === id ? { ...c, ...fields } : c)),
      }),
      record,
    );
  const undo = useCallback(() => {
    const h = history.current;
    if (!h.past.length) return;
    h.future.push(projectRef.current);
    setProject(h.past.pop());
    setHistoryVersion((x) => x + 1);
  }, []);
  const redo = useCallback(() => {
    const h = history.current;
    if (!h.future.length) return;
    h.past.push(projectRef.current);
    setProject(h.future.pop());
    setHistoryVersion((x) => x + 1);
  }, []);
  useEffect(() => {
    Promise.all([api("/media"), api("/project")])
      .then(([a, p]) => {
        setAssets(a);
        if (p) {
          setProject({
            ...p,
            clips: p.clips.map((c) => ({ ...defaults, ...c })),
          });
          setSelected(p.clips[0]?.id);
          setTime(0);
        }
        setReady(true);
        setSaved("All changes saved");
        loaded.current = true;
      })
      .catch((e) => {
        notify(
          "Backend unavailable. Start the Frame server to import and export media.",
        );
        setSaved("Offline");
      });
  }, []);
  const processing = assets.some((a) =>
    ["processing", "proxying"].includes(a.status),
  );
  useEffect(() => {
    const t = setInterval(
      () =>
        api("/media")
          .then(setAssets)
          .catch(() => {}),
      processing ? 600 : 10000,
    );
    return () => clearInterval(t);
  }, [processing]);
  useEffect(() => {
    if (!loaded.current) return;
    setSaved("Saving…");
    const timer = setTimeout(
      () =>
        api("/project", { method: "PUT", ...jsonOptions(project) })
          .then(() => setSaved("All changes saved"))
          .catch(() => setSaved("Save failed · download a backup")),
      700,
    );
    return () => clearTimeout(timer);
  }, [project, ready]);
  useEffect(() => {
    if (!playing) return;
    let raf,
      last = performance.now();
    const run = (now) => {
      const delta = (now - last) / 1000;
      last = now;
      const next = timeRef.current + delta;
      if (next >= endTime(projectRef.current)) {
        setPlaying(false);
        setTime(endTime(projectRef.current));
        return;
      }
      timeRef.current = next;
      setTime(next);
      raf = requestAnimationFrame(run);
    };
    raf = requestAnimationFrame(run);
    return () => cancelAnimationFrame(raf);
  }, [playing]);
  useEffect(() => {
    if (
      !job ||
      ["done", "error", "cancelled", "interrupted"].includes(job.status)
    )
      return;
    const timer = setInterval(
      () =>
        api("/exports/" + job.id)
          .then(setJob)
          .catch((e) => {
            setJob((j) => ({ ...j, status: "error", error: e.message }));
          }),
      800,
    );
    return () => clearInterval(timer);
  }, [job?.id, job?.status]);
  useEffect(() => {
    window.frameFlushProject = async () => {
      if (!loaded.current) return true;
      try {
        await api("/project", {
          method: "PUT",
          ...jsonOptions(projectRef.current),
        });
        return true;
      } catch {
        return false;
      }
    };
    return () => {
      delete window.frameFlushProject;
    };
  }, []);

  function togglePlay() {
    if (!projectRef.current.clips.length) return;
    if (timeRef.current >= endTime(projectRef.current)) setTime(0);
    setPlaying((x) => !x);
  }
  function split() {
    const p = splitClip(projectRef.current, selected, timeRef.current);
    if (p === projectRef.current)
      notify("Place the playhead inside the selected clip to split.");
    else change(p);
  }
  function deleteClip() {
    if (!selected) return;
    change((p) => ({ ...p, clips: p.clips.filter((c) => c.id !== selected) }));
    setSelected(null);
  }
  function duplicate() {
    if (!clip) return;
    const c = { ...clip, id: uid(), start: clip.start + clip.duration };
    change((p) => ({ ...p, clips: [...p.clips, c] }));
    setSelected(c.id);
  }
  const saveNow = () =>
    api("/project", { method: "PUT", ...jsonOptions(projectRef.current) })
      .then(() => notify("Project saved on this machine"))
      .catch((e) => notify(e.message));
  useEffect(() => {
    const handler = (e) => {
      if (
        ["INPUT", "TEXTAREA", "SELECT"].includes(e.target.tagName) ||
        e.target.isContentEditable
      )
        return;
      if (e.code === "Space") {
        e.preventDefault();
        togglePlay();
      }
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        deleteClip();
      }
      if (e.key.toLowerCase() === "s" && !e.ctrlKey && !e.metaKey) split();
      if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
        e.preventDefault();
        setPlaying(false);
        setTime((t) =>
          clamp(
            t +
              ((e.key === "ArrowLeft" ? -1 : 1) * (e.shiftKey ? 10 : 1)) /
                projectRef.current.fps,
            0,
            endTime(projectRef.current),
          ),
        );
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
        e.preventDefault();
        e.shiftKey ? redo() : undo();
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        saveNow();
      }
      if (e.key === "Escape") {
        setModal(null);
        setProjectMenu(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  });
  async function pickMedia() {
    if (!desktop) {
      fileInput.current.click();
      return;
    }
    try {
      const added = await desktop.importMedia();
      setAssets((a) => [...a, ...added]);
      if (added.length)
        notify(
          `${added.length} files linked. Originals stay on your drive; previews are preparing.`,
        );
    } catch (e) {
      notify(e.message);
    }
  }
  async function relink(id) {
    try {
      const a = await desktop.relinkMedia(id);
      if (a) {
        setAssets((xs) => xs.map((x) => (x.id === id ? a : x)));
        notify("Media relinked. Preparing a fresh preview.");
      }
    } catch (e) {
      notify(e.message);
    }
  }
  async function dropFiles(files) {
    if (!desktop) return uploadFiles(files);
    try {
      const added = await desktop.importDroppedMedia(files);
      setAssets((a) => [...a, ...added]);
      if (added.length)
        notify(`${added.length} originals linked without copying.`);
    } catch (e) {
      notify(e.message);
    }
  }
  async function uploadFiles(files) {
    for (let i = 0; i < files.length; i++) {
      const file = files[i];
      setUploading({
        name: file.name,
        progress: 0,
        index: i + 1,
        total: files.length,
      });
      try {
        const result = await new Promise((resolve, reject) => {
          const xhr = new XMLHttpRequest();
          xhr.open("POST", "/api/media");
          xhr.upload.onprogress = (e) => {
            if (e.lengthComputable)
              setUploading({
                name: file.name,
                progress: e.loaded / e.total,
                index: i + 1,
                total: files.length,
              });
          };
          xhr.onload = () => {
            if (xhr.status < 300) resolve(JSON.parse(xhr.responseText));
            else {
              let message = "Import failed";
              try {
                message = JSON.parse(xhr.responseText).detail;
              } catch {}
              reject(new Error(message));
            }
          };
          xhr.onerror = () => reject(new Error("Upload connection lost"));
          const form = new FormData();
          form.append("file", file);
          xhr.send(form);
        });
        setAssets((a) => [...a, result]);
      } catch (e) {
        notify(e.message);
      }
    }
    setUploading(null);
    notify("Upload finished. Preview copies are prepared in the background.");
  }
  function addAsset(a, at) {
    if (!["ready", "proxying"].includes(a.status)) {
      notify(a.status === "error" ? a.error : "This file is still processing.");
      return;
    }
    const track = a.kind === "audio" ? 4 : 1,
      start =
        at ??
        Math.max(
          0,
          ...project.clips
            .filter((c) => c.track === track)
            .map((c) => c.start + c.duration),
        );
    const c = newClip({
      asset_id: a.id,
      kind: a.kind,
      name: a.name,
      duration: a.kind === "image" ? 5 : a.duration,
      start,
      track,
    });
    change((p) => ({ ...p, clips: [...p.clips, c] }));
    setSelected(c.id);
    setTime(start);
    notify("Added to timeline");
  }
  async function preserve() {
    await api("/snapshots", {
      method: "POST",
      ...jsonOptions(projectRef.current),
    });
  }
  async function switchProject(p) {
    try {
      await preserve();
      change(p);
      setSelected(p.clips[0]?.id ?? null);
      setTime(0);
      setPlaying(false);
      setProjectMenu(false);
      setModal(null);
    } catch (e) {
      notify("Could not preserve your current project: " + e.message);
    }
  }
  async function showSnapshots() {
    try {
      setSnapshots(await api("/snapshots"));
      setModal("snapshots");
      setProjectMenu(false);
    } catch (e) {
      notify(e.message);
    }
  }
  function addText(style) {
    const c = newClip({
      kind: "title",
      track: 0,
      name: style.name,
      text: style.text,
      font_size: style.size,
      color: style.color || "#ffffff",
      start: time,
      duration: 5,
      y: style.y ?? 50,
      transition: "fade",
    });
    change((p) => ({ ...p, clips: [...p.clips, c] }));
    setSelected(c.id);
    setInspector("video");
  }
  function addGraphic(type) {
    const c = newClip({
      kind: "graphic",
      track: 0,
      graphic: type,
      name:
        type === "lower-third"
          ? "Lower third"
          : `${type[0].toUpperCase() + type.slice(1)} annotation`,
      text: "YOUR ANNOTATION",
      color: "#c6e99d",
      start: time,
      duration: 5,
      transition: "fade",
    });
    change((p) => ({ ...p, clips: [...p.clips, c] }));
    setSelected(c.id);
  }
  function apply(fields) {
    if (!clip || clip.kind === "audio") {
      notify("Select a visual clip on the timeline first.");
      return;
    }
    patch(clip.id, fields);
  }
  function setTrack(key, id) {
    change((p) => ({
      ...p,
      [key]: p[key].includes(id)
        ? p[key].filter((x) => x !== id)
        : [...p[key], id],
    }));
  }
  function scrub(e) {
    const rect = ruler.current.getBoundingClientRect();
    setPlaying(false);
    setTime(clamp((e.clientX - rect.left) / zoom, 0, Math.max(duration, 30)));
  }
  function startDrag(e, c, mode = "move") {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    setSelected(c.id);
    setPlaying(false);
    remember();
    const origin = e.clientX;
    const move = (ev) => {
      let delta = (ev.clientX - origin) / zoom;
      let next = {};
      if (mode === "move") {
        let start = Math.max(0, c.start + delta);
        if (snap) {
          const points = [
            0,
            time,
            ...project.clips
              .filter((x) => x.id !== c.id && x.track === c.track)
              .flatMap((x) => [x.start, x.start + x.duration]),
          ];
          for (const point of points) {
            if (Math.abs(start - point) < 7 / zoom) {
              start = point;
              break;
            }
            if (Math.abs(start + c.duration - point) < 7 / zoom) {
              start = point - c.duration;
              break;
            }
          }
        }
        next = {
          start: Math.max(0, Math.round(start * project.fps) / project.fps),
        };
      } else if (mode === "end") {
        const a = assets.find((a) => a.id === c.asset_id),
          max =
            a && ["video", "audio"].includes(c.kind)
              ? Math.max(1 / project.fps, (a.duration - c.source_in) / c.speed)
              : 86400;
        next = { duration: clamp(c.duration + delta, 1 / project.fps, max) };
      } else {
        delta = clamp(
          delta,
          -Math.min(c.start, c.source_in / c.speed),
          c.duration - 1 / project.fps,
        );
        if (["image", "title", "graphic"].includes(c.kind))
          delta = clamp(
            (ev.clientX - origin) / zoom,
            -c.start,
            c.duration - 1 / project.fps,
          );
        next = {
          start: c.start + delta,
          duration: c.duration - delta,
          source_in: Math.max(0, c.source_in + delta * c.speed),
          keyframes: c.keyframes
            .map((k) => ({ ...k, time: Math.max(0, k.time - delta) }))
            .filter(
              (k, i, all) => i === all.findIndex((x) => x.time === k.time),
            ),
        };
      }
      setProject((p) => ({
        ...p,
        clips: p.clips.map((x) => (x.id === c.id ? { ...x, ...next } : x)),
      }));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
  function addKey() {
    if (!clip) return;
    const local = clamp(time - clip.start, 0, clip.duration);
    const k = {
      time: Math.round(local * project.fps) / project.fps,
      ...Object.fromEntries(
        ["x", "y", "scale", "opacity", "rotation"].map((p) => [
          p,
          keyValue(clip, p, local),
        ]),
      ),
    };
    patch(clip.id, {
      keyframes: [
        ...clip.keyframes.filter(
          (x) => Math.abs(x.time - k.time) > 0.5 / project.fps,
        ),
        k,
      ].sort((a, b) => a.time - b.time),
    });
    notify("Transform keyframe added at " + timecode(local, project.fps));
  }
  function setSpeed(speed) {
    if (!clip || !Number.isFinite(speed) || speed <= 0) return;
    const duration = (clip.duration * clip.speed) / speed;
    if (duration > 86400 || duration < 1 / project.fps) {
      notify("Choose a duration between one frame and 24 hours.");
      return;
    }
    patch(clip.id, {
      speed,
      duration,
      keyframes: clip.keyframes.map((k) => ({
        ...k,
        time: (k.time * clip.speed) / speed,
      })),
    });
  }
  function transform(prop, value) {
    if (!clip) return;
    if (clip.keyframes.length) {
      const local =
        Math.round(clamp(time - clip.start, 0, clip.duration) * project.fps) /
        project.fps;
      const values = Object.fromEntries(
        ["x", "y", "scale", "opacity", "rotation"].map((p) => [
          p,
          keyValue(clip, p, local),
        ]),
      );
      patch(clip.id, {
        keyframes: [
          ...clip.keyframes.filter(
            (k) => Math.abs(k.time - local) > 0.5 / project.fps,
          ),
          { time: local, ...values, [prop]: value },
        ].sort((a, b) => a.time - b.time),
      });
    } else patch(clip.id, { [prop]: value });
  }
  async function startExport() {
    try {
      setPlaying(false);
      const [width, height] = exportSize.split("x").map(Number);
      const j = await api("/exports", {
        method: "POST",
        ...jsonOptions({ ...project, width, height }),
      });
      setJob(j);
    } catch (e) {
      notify(e.message);
    }
  }
  const shownAssets = assets.filter(
    (a) =>
      (tab !== "audio" || a.kind === "audio") &&
      a.name.toLowerCase().includes(search.toLowerCase()),
  );
  const fitTimeline = () => {
    const width = timeline.current?.clientWidth || 900;
    setZoom(clamp((width - 50) / Math.max(duration, 10), 8, 150));
  };
  const readProject = async (file) => {
    if (!file) return;
    try {
      const p = JSON.parse(await file.text());
      await preserve();
      await api("/project", { method: "PUT", ...jsonOptions(p) });
      const canonical = await api("/project");
      change({
        ...canonical,
        clips: canonical.clips.map((c) => ({ ...defaults, ...c })),
      });
      setSelected(null);
      setTime(0);
      notify(
        "Project opened. Media IDs refer to this machine’s media library.",
      );
    } catch (e) {
      notify("Could not open project: " + e.message);
    }
  };
  return (
    <div className="app">
      <header className="topbar">
        <a
          className="brand"
          href="#"
          onClick={(e) => e.preventDefault()}
          aria-label="Frame home"
        >
          <span className="brand-mark">
            <span />
            <span />
            <span />
          </span>
          frame<span className="beta">BETA</span>
        </a>
        <div className="project-name">
          <span className="project-divider" />
          <button
            className="project-button"
            onClick={() => setProjectMenu((x) => !x)}
          >
            <span className="project-dot" />
            {project.name}
            <ChevronDown size={13} />
          </button>
          {projectMenu && (
            <div className="dropdown">
              <button
                onClick={() => {
                  switchProject({
                    ...demoProject(),
                    name: "Untitled project",
                    clips: [],
                  });
                }}
              >
                New project
              </button>
              <button
                onClick={() => {
                  projectInput.current.click();
                  setProjectMenu(false);
                }}
              >
                Open .frame.json
              </button>
              <button
                onClick={() => {
                  downloadJSON(project);
                  setProjectMenu(false);
                }}
              >
                Download project backup
              </button>
              <button
                onClick={() => {
                  switchProject(demoProject());
                }}
              >
                Load rocket demo
              </button>
              <button onClick={showSnapshots}>
                Restore a project snapshot
              </button>
              <button
                onClick={() =>
                  preserve()
                    .then(() => notify("Project snapshot saved"))
                    .catch((e) => notify(e.message))
                }
              >
                Save a project snapshot
              </button>
            </div>
          )}
        </div>
        <span className="save-state">
          <CheckCheck size={14} />
          {saved}
        </span>
        <div className="top-actions">
          <IconButton
            icon={HelpCircle}
            title="Keyboard shortcuts and help"
            onClick={() => setModal("help")}
          />
          <span className="avatar">SC</span>
          <button
            className="export-button"
            onClick={() => {
              setModal("export");
              setExportSize(`${project.width}x${project.height}`);
            }}
          >
            <Upload size={15} />
            Export video
            <ChevronDown size={13} />
          </button>
        </div>
      </header>
      <main className="workspace">
        <nav className="rail">
          {TABS.map(([id, Icon, label]) => (
            <button
              key={id}
              className={tab === id ? "selected" : ""}
              onClick={() => {
                setTab(id);
                setSearch("");
              }}
              title={label}
            >
              <Icon size={21} />
              <span>
                {id === "media"
                  ? "Media"
                  : id === "graphics"
                    ? "Graphics"
                    : id === "transitions"
                      ? "Transitions"
                      : id === "color"
                        ? "Effects"
                        : label}
              </span>
            </button>
          ))}
          <button
            className="rail-bottom"
            onClick={() => setModal("settings")}
            title="Project settings"
          >
            <Settings2 size={20} />
            <span>Settings</span>
          </button>
        </nav>
        <aside className="library">
          <div className="panel-heading">
            <h2>{TABS.find((t) => t[0] === tab)?.[2]}</h2>
            <span className="mini-badge">
              {["media", "audio"].includes(tab) ? shownAssets.length : ""}
            </span>
          </div>
          {["media", "audio"].includes(tab) ? (
            <>
              <button className="import-button" onClick={() => pickMedia()}>
                <Plus size={17} />
                Import media <span>＋</span>
              </button>
              <div className="search">
                <Search size={14} />
                <input
                  aria-label="Search media"
                  placeholder="Search your media"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                />
              </div>
              <div className="library-label">
                <span>
                  {tab === "audio" ? "AUDIO LIBRARY" : "PROJECT MEDIA"}
                </span>
                <span>{shownAssets.length} files</span>
              </div>
              <div
                className="asset-grid"
                onDragOver={(e) => e.preventDefault()}
                onDrop={(e) => {
                  e.preventDefault();
                  if (e.dataTransfer.files.length)
                    dropFiles([...e.dataTransfer.files]);
                }}
              >
                {shownAssets.map((a) => (
                  <div
                    key={a.id}
                    className={`asset ${a.status === "error" ? "asset-error" : ""}`}
                    draggable={["ready", "proxying"].includes(a.status)}
                    onDragStart={(e) =>
                      e.dataTransfer.setData("application/frame-asset", a.id)
                    }
                    onDoubleClick={() => addAsset(a)}
                  >
                    <div className="asset-thumb">
                      {a.kind === "audio" ? (
                        <div className="audio-art">
                          <AudioLines size={30} />
                        </div>
                      ) : ["ready", "proxying"].includes(a.status) ? (
                        <img src={`/api/media/${a.id}/thumb`} alt={a.name} />
                      ) : (
                        <div className="processing">
                          {a.status === "error" ? (
                            <X size={22} />
                          ) : (
                            <Loader2 size={22} className="spin" />
                          )}
                        </div>
                      )}
                      <span className="asset-type">
                        {a.kind === "image" ? (
                          <ImageIcon size={11} />
                        ) : a.kind === "audio" ? (
                          <Music2 size={11} />
                        ) : (
                          <Film size={11} />
                        )}
                      </span>
                      <span className="asset-duration">
                        {a.kind === "image"
                          ? "IMAGE"
                          : a.duration
                            ? timecode(a.duration).slice(0, 5)
                            : a.status.toUpperCase()}
                      </span>
                      <button
                        className="asset-add"
                        disabled={!["ready", "proxying"].includes(a.status)}
                        aria-label={`Add ${a.name} to timeline`}
                        onClick={() => addAsset(a)}
                      >
                        <Plus size={15} />
                      </button>
                    </div>
                    <div className="asset-name" title={a.name}>
                      {a.name}
                    </div>
                    <div className="asset-meta">
                      {a.linked && (
                        <span title="Original linked on disk">↗ </span>
                      )}
                      {a.demo
                        ? "Sample illustration"
                        : a.status === "ready"
                          ? `${a.width ? `${a.width} × ${a.height} · ` : ""}${bytes(a.size)}`
                          : a.status === "error"
                            ? "Import failed"
                            : "Preparing preview…"}
                    </div>
                    {desktop && !a.demo && (
                      <button
                        className="relink-button"
                        onClick={() => relink(a.id)}
                      >
                        Relink original
                      </button>
                    )}
                  </div>
                ))}
                {!shownAssets.length && (
                  <div className="empty-library">
                    <FolderOpen size={28} />
                    <p>
                      {tab === "audio"
                        ? "Bring your story to life with sound."
                        : "Your footage belongs here."}
                    </p>
                    <button onClick={() => pickMedia()}>Import files</button>
                  </div>
                )}
              </div>
              {uploading && (
                <div className="upload-status">
                  <Loader2 size={14} className="spin" />
                  <span>
                    Uploading {uploading.index}/{uploading.total} ·{" "}
                    {Math.round(uploading.progress * 100)}%
                  </span>
                  <progress value={uploading.progress} max="1" />
                </div>
              )}
              <div className="library-tip">
                <div className="tip-icon">
                  <Sparkles size={15} />
                </div>
                <div>
                  <strong>Big ideas. Lighter editing.</strong>
                  <p>
                    Preview copies keep your originals untouched. Drag media to
                    the timeline to start.
                  </p>
                </div>
              </div>
            </>
          ) : tab === "text" ? (
            <div className="presets">
              <p className="panel-description">
                A few words. A whole new story.
              </p>
              {[
                {
                  name: "Statement",
                  text: "YOUR NEXT\nGREAT STORY",
                  size: 108,
                },
                { name: "Section title", text: "THE BUILD", size: 76 },
                {
                  name: "Subtitle",
                  text: "A small step toward something bigger.",
                  size: 34,
                  y: 84,
                },
                {
                  name: "Technical label",
                  text: "APOGEE  /  1,250 M",
                  size: 40,
                  color: "#c6e99d",
                },
              ].map((t) => (
                <button
                  className="text-preset"
                  key={t.name}
                  onClick={() => addText(t)}
                >
                  <span
                    style={{ fontSize: t.size > 80 ? 23 : 17, color: t.color }}
                  >
                    {t.text}
                  </span>
                  <small>
                    {t.name}
                    <Plus size={14} />
                  </small>
                </button>
              ))}
            </div>
          ) : tab === "graphics" ? (
            <div className="presets">
              <p className="panel-description">
                Make the complicated feel simple.
              </p>
              {[
                ["arrow", "Direction arrow", ArrowRight],
                ["circle", "Highlight circle", Circle],
                ["rectangle", "Focus frame", Square],
                ["lower-third", "Lower third", Type],
              ].map(([id, name, Icon]) => (
                <button
                  className="graphic-preset"
                  key={id}
                  onClick={() => addGraphic(id)}
                >
                  <Icon size={30} />
                  <span>{name}</span>
                  <Plus size={15} />
                </button>
              ))}
            </div>
          ) : tab === "transitions" ? (
            <div className="presets">
              <p className="panel-description">
                Select a clip, then choose how it enters.
              </p>
              {["none", "fade", "slide", "zoom", "wipe"].map((t) => (
                <button
                  key={t}
                  className={`transition-preset ${clip?.transition === t ? "chosen" : ""}`}
                  onClick={() => apply({ transition: t })}
                >
                  <span className={`transition-art ${t}`}>
                    <i />
                    <i />
                  </span>
                  <span>
                    {t === "none"
                      ? "Clean cut"
                      : t === "fade"
                        ? "Fade in & out"
                        : t === "slide"
                          ? "Slide from left"
                          : t === "zoom"
                            ? "Gentle zoom"
                            : "Wipe reveal"}
                  </span>
                  {clip?.transition === t ? (
                    <Check size={15} />
                  ) : (
                    <Plus size={15} />
                  )}
                </button>
              ))}
            </div>
          ) : (
            <div className="presets">
              <p className="panel-description">
                Find the feeling of your film.
              </p>
              {[
                {
                  name: "Original",
                  brightness: 0,
                  contrast: 1,
                  saturation: 1,
                  warmth: 0,
                },
                {
                  name: "Golden hour",
                  brightness: 0.03,
                  contrast: 1.1,
                  saturation: 1.15,
                  warmth: 0.5,
                },
                {
                  name: "Cinematic cool",
                  brightness: -0.04,
                  contrast: 1.2,
                  saturation: 0.8,
                  warmth: -0.45,
                },
                {
                  name: "Documentary",
                  brightness: 0.02,
                  contrast: 1.15,
                  saturation: 0.85,
                  warmth: 0.1,
                },
                {
                  name: "Monochrome",
                  brightness: 0,
                  contrast: 1.2,
                  saturation: 0,
                  warmth: 0,
                },
              ].map((p, i) => (
                <button
                  className="color-preset"
                  key={p.name}
                  onClick={() => {
                    const { name, ...values } = p;
                    apply(values);
                  }}
                >
                  <img
                    src="/api/media/demo-0/thumb"
                    alt=""
                    style={{
                      filter: `brightness(${1 + p.brightness}) contrast(${p.contrast}) saturate(${p.saturation}) sepia(${Math.max(0, p.warmth)})`,
                    }}
                  />
                  <span>{p.name}</span>
                  <Plus size={14} />
                </button>
              ))}
              <button
                className={`motion-preset ${clip?.motion_blur ? "chosen" : ""}`}
                onClick={() => apply({ motion_blur: !clip?.motion_blur })}
              >
                <Activity size={20} />
                <span>
                  Timelapse motion blend
                  <small>Exposure-style blend on export</small>
                </span>
                <Check
                  size={15}
                  style={{ opacity: clip?.motion_blur ? 1 : 0 }}
                />
              </button>
            </div>
          )}
        </aside>
        <section className="viewer">
          <div className="viewer-heading">
            <div>
              <span className="live-dot" />
              Preview
              <span className="viewer-dimension">
                {project.width} × {project.height}
              </span>
            </div>
            <button
              className="plain-button"
              onClick={() => setModal("settings")}
            >
              {project.width === project.height
                ? "1:1"
                : project.width > project.height
                  ? "16:9"
                  : "9:16"}
              <ChevronDown size={12} />
            </button>
          </div>
          <div className="preview-area">
            <div
              className="canvas-wrap"
              style={{ aspectRatio: `${project.width}/${project.height}` }}
            >
              <Preview
                project={project}
                assets={assets}
                time={time}
                playing={playing}
              />
              <span className="preview-label">
                {assets.every((a) => a.demo) ? "SAMPLE PROJECT" : "PREVIEW"}
              </span>
            </div>
          </div>
          <div className="viewer-controls">
            <span className="preview-time">
              {timecode(time, project.fps)}
              <span> / {timecode(duration, project.fps)}</span>
            </span>
            <div className="transport">
              <IconButton
                icon={SkipBack}
                title="Go to beginning"
                onClick={() => {
                  setTime(0);
                  setPlaying(false);
                }}
              />
              <IconButton
                icon={ArrowLeft}
                title="Previous frame (←)"
                onClick={() => {
                  setPlaying(false);
                  setTime((t) => Math.max(0, t - 1 / project.fps));
                }}
              />
              <button
                className="play-button"
                aria-label={playing ? "Pause" : "Play"}
                onClick={togglePlay}
              >
                {playing ? (
                  <Pause size={17} fill="currentColor" />
                ) : (
                  <Play size={17} fill="currentColor" />
                )}
              </button>
              <IconButton
                icon={ArrowRight}
                title="Next frame (→)"
                onClick={() => {
                  setPlaying(false);
                  setTime((t) => Math.min(duration, t + 1 / project.fps));
                }}
              />
              <IconButton
                icon={SkipForward}
                title="Go to end"
                onClick={() => {
                  setPlaying(false);
                  setTime(duration);
                }}
              />
            </div>
            <div className="viewer-end">
              <span className="quality">540p preview</span>
              <IconButton
                icon={Maximize2}
                title="Fullscreen preview"
                onClick={() =>
                  document
                    .querySelector(".canvas-wrap")
                    .requestFullscreen?.()
                    .catch(() =>
                      notify("Fullscreen is unavailable in this browser"),
                    )
                }
              />
            </div>
          </div>
          <div className="viewer-footer">
            <span>
              <span className="tiny-square" />
              Originals preserved
            </span>
            <span>Made for the story you want to tell.</span>
          </div>
        </section>
        <aside className="inspector">
          <div className="inspector-tabs">
            {["video", "audio", "color"].map((t) => (
              <button
                key={t}
                onClick={() => setInspector(t)}
                className={inspector === t ? "selected" : ""}
              >
                {t === "video"
                  ? "Properties"
                  : t === "audio"
                    ? "Audio"
                    : "Color"}
              </button>
            ))}
          </div>
          {!clip ? (
            <div className="inspector-empty">
              <MousePointer2 size={26} />
              <h3>Make it yours</h3>
              <p>Select a clip on the timeline to adjust its properties.</p>
            </div>
          ) : (
            <>
              <div className="selected-clip">
                <span className="clip-icon">
                  {clip.kind === "title" ? (
                    <Type size={16} />
                  ) : clip.kind === "audio" ? (
                    <Music2 size={16} />
                  ) : clip.kind === "graphic" ? (
                    <Shapes size={16} />
                  ) : (
                    <Film size={16} />
                  )}
                </span>
                <div>
                  <strong>{clip.name}</strong>
                  <small>
                    {clip.kind === "title"
                      ? "TEXT LAYER"
                      : clip.kind === "graphic"
                        ? "GRAPHIC LAYER"
                        : clip.kind.toUpperCase() + " CLIP"}
                  </small>
                </div>
                <IconButton
                  icon={Copy}
                  title="Duplicate clip"
                  onClick={duplicate}
                />
              </div>
              <div className="inspector-scroll">
                {inspector === "video" ? (
                  <>
                    {["title", "graphic"].includes(clip.kind) && (
                      <section className="property-section">
                        <h3>
                          {clip.kind === "title" ? "Text" : "Annotation"}
                          <Type size={13} />
                        </h3>
                        {(clip.kind === "title" ||
                          clip.graphic === "lower-third") && (
                          <textarea
                            aria-label="Text content"
                            value={clip.text}
                            onChange={(e) =>
                              patch(clip.id, {
                                text: e.target.value,
                                name:
                                  e.target.value.split("\n")[0].slice(0, 70) ||
                                  "Text",
                              })
                            }
                          />
                        )}
                        <div className="property-row">
                          <label>Color</label>
                          <input
                            type="color"
                            aria-label="Text or graphic color"
                            value={clip.color}
                            onChange={(e) =>
                              patch(clip.id, { color: e.target.value })
                            }
                          />
                          {clip.kind === "title" && (
                            <input
                              className="number-input"
                              aria-label="Font size"
                              type="number"
                              min="12"
                              max="200"
                              value={clip.font_size}
                              onChange={(e) =>
                                patch(clip.id, {
                                  font_size: clamp(
                                    Number(e.target.value),
                                    12,
                                    200,
                                  ),
                                })
                              }
                            />
                          )}
                        </div>
                      </section>
                    )}
                    {clip.kind !== "audio" && (
                      <section className="property-section">
                        <h3>
                          Transform
                          <IconButton
                            icon={RotateCcw}
                            title="Reset transform and keyframes"
                            onClick={() =>
                              patch(clip.id, {
                                x: 50,
                                y: 50,
                                scale: 1,
                                rotation: 0,
                                opacity: 1,
                                keyframes: [],
                              })
                            }
                          />
                        </h3>
                        <div className="position-inputs">
                          <label>
                            <span>Position X</span>
                            <div>
                              <input
                                aria-label="Position X"
                                type="number"
                                value={
                                  Math.round(
                                    keyValue(clip, "x", time - clip.start) * 10,
                                  ) / 10
                                }
                                onChange={(e) =>
                                  transform(
                                    "x",
                                    clamp(+e.target.value, -100, 200),
                                  )
                                }
                              />
                              <span>%</span>
                            </div>
                          </label>
                          <label>
                            <span>Position Y</span>
                            <div>
                              <input
                                aria-label="Position Y"
                                type="number"
                                value={
                                  Math.round(
                                    keyValue(clip, "y", time - clip.start) * 10,
                                  ) / 10
                                }
                                onChange={(e) =>
                                  transform(
                                    "y",
                                    clamp(+e.target.value, -100, 200),
                                  )
                                }
                              />
                              <span>%</span>
                            </div>
                          </label>
                        </div>
                        <Slider
                          label="Scale"
                          value={
                            keyValue(clip, "scale", time - clip.start) * 100
                          }
                          min={5}
                          max={400}
                          unit="%"
                          onChange={(n) => transform("scale", n / 100)}
                        />
                        <Slider
                          label="Rotation"
                          value={keyValue(clip, "rotation", time - clip.start)}
                          min={-180}
                          max={180}
                          unit="°"
                          onChange={(n) => transform("rotation", n)}
                        />
                        <Slider
                          label="Opacity"
                          value={
                            keyValue(clip, "opacity", time - clip.start) * 100
                          }
                          min={0}
                          max={100}
                          unit="%"
                          onChange={(n) => transform("opacity", n / 100)}
                        />
                      </section>
                    )}
                    <section className="property-section">
                      <h3>
                        Timing
                        <Clock3 size={13} />
                      </h3>
                      <div className="property-row">
                        <label>Track</label>
                        <select
                          aria-label="Clip track"
                          value={clip.track}
                          onChange={(e) =>
                            patch(clip.id, { track: +e.target.value })
                          }
                        >
                          {TRACKS.map((t) => (
                            <option key={t.id} value={t.id}>
                              {t.name}
                            </option>
                          ))}
                        </select>
                      </div>
                      <div className="position-inputs">
                        <label>
                          <span>Start (s)</span>
                          <input
                            aria-label="Clip start"
                            type="number"
                            min="0"
                            step={1 / project.fps}
                            value={+clip.start.toFixed(3)}
                            onChange={(e) =>
                              patch(clip.id, {
                                start: clamp(+e.target.value, 0, 86400),
                              })
                            }
                          />
                        </label>
                        <label>
                          <span>Duration (s)</span>
                          <input
                            aria-label="Clip duration"
                            type="number"
                            min={1 / project.fps}
                            step={1 / project.fps}
                            value={+clip.duration.toFixed(3)}
                            onChange={(e) => {
                              let max = 86400;
                              const a = assets.find(
                                (a) => a.id === clip.asset_id,
                              );
                              if (a && ["video", "audio"].includes(clip.kind))
                                max =
                                  (a.duration - clip.source_in) / clip.speed;
                              patch(clip.id, {
                                duration: clamp(
                                  +e.target.value,
                                  1 / project.fps,
                                  max,
                                ),
                              });
                            }}
                          />
                        </label>
                      </div>
                      {["video", "audio"].includes(clip.kind) && (
                        <>
                          <div className="speed-editor">
                            <label>
                              Speed multiplier
                              <div className="unit-input">
                                <input
                                  aria-label="Speed multiplier"
                                  type="number"
                                  min="0.000001"
                                  step="any"
                                  value={+clip.speed.toPrecision(8)}
                                  onChange={(e) =>
                                    setSpeed(Number(e.target.value))
                                  }
                                />
                                <span>×</span>
                              </div>
                            </label>
                            <label>
                              Target duration
                              <div className="unit-input">
                                <input
                                  aria-label="Target duration seconds"
                                  type="number"
                                  min={1 / project.fps}
                                  step="any"
                                  value={+clip.duration.toFixed(4)}
                                  onChange={(e) => {
                                    const seconds = Number(e.target.value);
                                    if (seconds > 0)
                                      setSpeed(
                                        (clip.duration * clip.speed) / seconds,
                                      );
                                  }}
                                />
                                <span>sec</span>
                              </div>
                            </label>
                            <small>
                              {clip.speed >= 1
                                ? `${+clip.speed.toFixed(2)}× faster`
                                : `${+(1 / clip.speed).toFixed(2)}× slower`}{" "}
                              · {timecode(clip.duration, project.fps)} output
                            </small>
                          </div>
                          <div className="speed-presets">
                            {[0.25, 0.5, 1, 2, 8, 16].map((s) => (
                              <button
                                key={s}
                                className={clip.speed === s ? "chosen" : ""}
                                onClick={() => setSpeed(s)}
                              >
                                {s}×
                              </button>
                            ))}
                          </div>
                        </>
                      )}
                    </section>
                    {clip.kind !== "audio" && (
                      <>
                        <section className="property-section">
                          <h3>
                            Animation
                            <button className="text-button" onClick={addKey}>
                              <Plus size={12} />
                              Keyframe
                            </button>
                          </h3>
                          <p className="hint">
                            Add a keyframe, move the playhead, then change the
                            transform.
                          </p>
                          <div className="keyframe-list">
                            {clip.keyframes.length ? (
                              clip.keyframes.map((k, i) => (
                                <div key={i}>
                                  <button
                                    onClick={() => {
                                      setPlaying(false);
                                      setTime(clip.start + k.time);
                                    }}
                                  >
                                    <Diamond size={11} fill="currentColor" />
                                    {timecode(k.time, project.fps)}
                                  </button>
                                  <IconButton
                                    icon={X}
                                    title={`Remove keyframe ${i + 1}`}
                                    onClick={() =>
                                      patch(clip.id, {
                                        keyframes: clip.keyframes.filter(
                                          (_, j) => j !== i,
                                        ),
                                      })
                                    }
                                  />
                                </div>
                              ))
                            ) : (
                              <span className="no-keyframes">
                                No keyframes yet
                              </span>
                            )}
                          </div>
                        </section>
                        <section className="property-section">
                          <h3>
                            Mask & transition
                            <Layers size={13} />
                          </h3>
                          <div className="property-row">
                            <label>Mask</label>
                            <select
                              aria-label="Mask"
                              value={clip.mask}
                              onChange={(e) =>
                                patch(clip.id, { mask: e.target.value })
                              }
                            >
                              <option value="none">None</option>
                              <option value="circle">Ellipse</option>
                              <option value="rectangle">Rectangle</option>
                            </select>
                          </div>
                          <div className="property-row">
                            <label>Transition</label>
                            <select
                              aria-label="Transition"
                              value={clip.transition}
                              onChange={(e) =>
                                patch(clip.id, { transition: e.target.value })
                              }
                            >
                              {["none", "fade", "slide", "zoom", "wipe"].map(
                                (t) => (
                                  <option key={t}>{t}</option>
                                ),
                              )}
                            </select>
                          </div>
                          {clip.transition !== "none" && (
                            <Slider
                              label="Transition length"
                              value={clip.fade_duration}
                              min={0.05}
                              max={5}
                              step={0.05}
                              unit="s"
                              onChange={(n) =>
                                patch(clip.id, { fade_duration: n })
                              }
                            />
                          )}
                        </section>
                      </>
                    )}
                  </>
                ) : inspector === "audio" ? (
                  <>
                    <section className="property-section">
                      <h3>
                        Audio mix
                        <Volume2 size={14} />
                      </h3>
                      {["audio", "video"].includes(clip.kind) ? (
                        <>
                          <Slider
                            label="Volume"
                            value={clip.volume * 100}
                            min={0}
                            max={200}
                            unit="%"
                            onChange={(n) =>
                              patch(clip.id, { volume: n / 100 })
                            }
                          />
                          <button
                            className="secondary-button"
                            onClick={() =>
                              patch(clip.id, { volume: clip.volume ? 0 : 1 })
                            }
                          >
                            {clip.volume ? "Mute clip" : "Unmute clip"}
                          </button>
                          <p className="hint">
                            For audio fades, choose Fade in & out under
                            transitions. Track mute also applies to export. Gain
                            above 100% applies on export; browser preview caps
                            at 100%.
                          </p>
                        </>
                      ) : (
                        <p className="hint">
                          This layer has no audio. Import an audio file or
                          select a video clip with sound.
                        </p>
                      )}
                    </section>
                  </>
                ) : (
                  <section className="property-section">
                    <h3>
                      Color adjustments
                      <IconButton
                        icon={RotateCcw}
                        title="Reset color"
                        onClick={() =>
                          patch(clip.id, {
                            brightness: 0,
                            contrast: 1,
                            saturation: 1,
                            warmth: 0,
                            blur: 0,
                            motion_blur: false,
                          })
                        }
                      />
                    </h3>
                    {[
                      ["Exposure", "brightness", -100, 100],
                      ["Contrast", "contrast", 0, 300],
                      ["Saturation", "saturation", 0, 300],
                      ["Temperature", "warmth", -100, 100],
                      ["Softness", "blur", 0, 20],
                    ].map(([label, p, min, max]) => (
                      <Slider
                        key={p}
                        label={label}
                        value={clip[p] * (p === "blur" ? 1 : 100)}
                        min={min}
                        max={max}
                        onChange={(n) =>
                          patch(clip.id, { [p]: n / (p === "blur" ? 1 : 100) })
                        }
                      />
                    ))}
                    {clip.kind === "video" && (
                      <div className="timelapse-card">
                        <Sparkles size={18} />
                        <h4>Clean Timelapse</h4>
                        <p>Blend the motion. Keep the detail.</p>
                        <p>
                          4 hours → 20 seconds? Use 720×, or enter your target
                          duration in Properties.
                        </p>
                        <div className="speed-presets">
                          {[8, 32, 720].map((s) => (
                            <button
                              key={s}
                              disabled={
                                (clip.duration * clip.speed) / s <
                                1 / project.fps
                              }
                              title={
                                (clip.duration * clip.speed) / s <
                                1 / project.fps
                                  ? "This source is too short for a full output frame at this speed"
                                  : ""
                              }
                              onClick={() =>
                                patch(clip.id, {
                                  speed: s,
                                  duration: (clip.duration * clip.speed) / s,
                                  motion_blur: true,
                                  deflicker: true,
                                  volume: 0,
                                  shutter: 0.5,
                                  keyframes: clip.keyframes.map((k) => ({
                                    ...k,
                                    time: (k.time * clip.speed) / s,
                                  })),
                                })
                              }
                            >
                              {s}× clean
                            </button>
                          ))}
                        </div>
                        <Slider
                          label="Motion blend strength"
                          value={(clip.shutter ?? 0.5) * 100}
                          min={10}
                          max={100}
                          unit="%"
                          onChange={(n) => patch(clip.id, { shutter: n / 100 })}
                        />
                        <label className="toggle-row">
                          <span>
                            Reduce flicker
                            <small>
                              Balances exposure across nearby frames
                            </small>
                          </span>
                          <input
                            type="checkbox"
                            checked={clip.deflicker ?? false}
                            onChange={(e) =>
                              patch(clip.id, { deflicker: e.target.checked })
                            }
                          />
                        </label>
                      </div>
                    )}
                    <label className="toggle-row">
                      <span>
                        Motion blending
                        <small>Source-frame blending on export</small>
                      </span>
                      <input
                        type="checkbox"
                        checked={clip.motion_blur}
                        onChange={(e) =>
                          patch(clip.id, { motion_blur: e.target.checked })
                        }
                      />
                    </label>
                    <p className="hint">
                      Color and font rendering in the browser are approximate.
                      Export a short segment to check your final look.
                    </p>
                  </section>
                )}
              </div>
            </>
          )}
        </aside>
      </main>
      <section className="timeline-section">
        <div className="timeline-toolbar">
          <div className="toolbar-left">
            <IconButton
              icon={Undo2}
              title="Undo (⌘Z)"
              disabled={!history.current.past.length}
              onClick={undo}
            />
            <IconButton
              icon={Redo2}
              title="Redo (⌘⇧Z)"
              disabled={!history.current.future.length}
              onClick={redo}
            />
            <span className="toolbar-divider" />
            <IconButton
              icon={MousePointer2}
              title="Selection tool · drag clips to move, edges to trim"
              active
            />
            <IconButton
              icon={Scissors}
              title="Split at playhead (S)"
              onClick={split}
            />
            <IconButton
              icon={Copy}
              title="Duplicate selected clip"
              onClick={duplicate}
            />
            <IconButton
              icon={Trash2}
              title="Delete selected clip (Delete)"
              onClick={deleteClip}
            />
            <span className="toolbar-divider" />
            <IconButton
              icon={Magnet}
              title="Toggle snapping"
              active={snap}
              onClick={() => setSnap((x) => !x)}
            />
            <button
              className="range-button"
              onClick={() => {
                setRange({
                  start: Math.floor(time * project.fps) / project.fps,
                  end: (Math.floor(time * project.fps) + 1) / project.fps,
                });
                setModal("range");
              }}
            >
              Remove frames
            </button>
          </div>
          <div className="toolbar-right">
            <span className="timeline-hint">Drag to move · edges to trim</span>
            <IconButton
              icon={ZoomOut}
              title="Zoom timeline out"
              onClick={() => setZoom((z) => Math.max(8, z / 1.3))}
            />
            <input
              aria-label="Timeline zoom"
              type="range"
              min="8"
              max="150"
              value={zoom}
              onChange={(e) => setZoom(+e.target.value)}
            />
            <IconButton
              icon={ZoomIn}
              title="Zoom timeline in"
              onClick={() => setZoom((z) => Math.min(150, z * 1.3))}
            />
            <IconButton
              icon={Fullscreen}
              title="Fit timeline"
              onClick={fitTimeline}
            />
          </div>
        </div>
        <div className="timeline-body">
          <div className="track-headers">
            <div className="tracks-ruler">
              <Layers size={13} />
              <span>{project.fps} FPS</span>
            </div>
            {TRACKS.map((t) => (
              <div className="track-header" key={t.id}>
                <div className="track-name">
                  <span style={{ background: t.color }} />
                  {t.name}
                </div>
                <div className="track-buttons">
                  <span className="track-number">
                    {t.type === "audio" ? "A" : "V"}
                    {t.id + 1}
                  </span>
                  {t.type === "audio" ? (
                    <IconButton
                      icon={
                        project.muted_tracks.includes(t.id) ? VolumeX : Volume2
                      }
                      title={`${project.muted_tracks.includes(t.id) ? "Unmute" : "Mute"} ${t.name}`}
                      active={project.muted_tracks.includes(t.id)}
                      onClick={() => setTrack("muted_tracks", t.id)}
                    />
                  ) : (
                    <IconButton
                      icon={project.hidden_tracks.includes(t.id) ? EyeOff : Eye}
                      title={`${project.hidden_tracks.includes(t.id) ? "Show" : "Hide"} ${t.name}`}
                      active={project.hidden_tracks.includes(t.id)}
                      onClick={() => setTrack("hidden_tracks", t.id)}
                    />
                  )}
                </div>
              </div>
            ))}
          </div>
          <div className="timeline-scroll" ref={timeline}>
            <div
              className="timeline-content"
              style={{ width: Math.max(960, (duration + 8) * zoom) }}
            >
              <div
                className="ruler"
                ref={ruler}
                onPointerDown={(e) => {
                  scrub(e);
                  e.currentTarget.setPointerCapture(e.pointerId);
                }}
                onPointerMove={(e) => {
                  if (e.buttons === 1) scrub(e);
                }}
              >
                {Array.from(
                  {
                    length:
                      Math.ceil(
                        Math.max(30, duration + 8) /
                          (zoom < 20 ? 10 : zoom < 45 ? 5 : 2),
                      ) + 1,
                  },
                  (_, i) => {
                    const n = i * (zoom < 20 ? 10 : zoom < 45 ? 5 : 2);
                    return (
                      <span key={i} style={{ left: n * zoom }}>
                        {timecode(n).slice(0, 5)}
                      </span>
                    );
                  },
                )}
              </div>
              {TRACKS.map((t) => (
                <div
                  className={`timeline-track ${project.hidden_tracks.includes(t.id) || project.muted_tracks.includes(t.id) ? "disabled-track" : ""}`}
                  key={t.id}
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={(e) => {
                    e.preventDefault();
                    const id = e.dataTransfer.getData(
                        "application/frame-asset",
                      ),
                      a = assets.find((a) => a.id === id);
                    if (a) {
                      const rect = ruler.current.getBoundingClientRect();
                      const start = Math.max(0, (e.clientX - rect.left) / zoom);
                      const c = newClip({
                        asset_id: a.id,
                        name: a.name,
                        kind: a.kind,
                        track: t.id,
                        start,
                        duration: a.kind === "image" ? 5 : a.duration,
                      });
                      if (!["ready", "proxying"].includes(a.status)) {
                        notify("Wait for preview processing to finish");
                        return;
                      }
                      change((p) => ({ ...p, clips: [...p.clips, c] }));
                      setSelected(c.id);
                    }
                  }}
                  onPointerDown={(e) => {
                    if (e.target === e.currentTarget) scrub(e);
                  }}
                >
                  {project.clips
                    .filter((c) => c.track === t.id)
                    .map((c) => {
                      const a = assets.find((a) => a.id === c.asset_id);
                      const peers = project.clips.filter(
                        (x) =>
                          x.track === c.track &&
                          x.start < c.start + c.duration &&
                          x.start + x.duration > c.start,
                      );
                      const lane = peers.findIndex((x) => x.id === c.id);
                      return (
                        <div
                          key={c.id}
                          role="button"
                          tabIndex={0}
                          aria-label={`Timeline clip ${c.name}`}
                          onFocus={() => setSelected(c.id)}
                          className={`timeline-clip ${c.kind} ${selected === c.id ? "selected" : ""}`}
                          style={{
                            left: c.start * zoom,
                            width: Math.max(5, c.duration * zoom),
                            "--clip-color": t.color,
                            ...(peers.length > 1
                              ? {
                                  height: Math.max(8, 36 / peers.length - 1),
                                  top: 4 + (lane * 36) / peers.length,
                                }
                              : {}),
                          }}
                          onPointerDown={(e) => startDrag(e, c)}
                        >
                          <div
                            className="trim-handle left"
                            onPointerDown={(e) => startDrag(e, c, "start")}
                          />
                          {a && c.kind !== "audio" && (
                            <div
                              className="clip-filmstrip"
                              style={{
                                backgroundImage: `url(/api/media/${a.id}/thumb)`,
                              }}
                            />
                          )}
                          <div className="clip-label">
                            {c.kind === "title" ? (
                              <Type size={11} />
                            ) : c.kind === "graphic" ? (
                              <Shapes size={11} />
                            ) : c.kind === "audio" ? (
                              <Music2 size={11} />
                            ) : (
                              <Film size={11} />
                            )}
                            <span>{c.name}</span>
                            {c.speed !== 1 && <b>{c.speed}×</b>}
                          </div>
                          {c.kind === "audio" && (
                            <div className="audio-clip-fill">
                              <AudioLines size={28} />
                              <span>
                                Audio ·{" "}
                                {c.volume === 0
                                  ? "muted"
                                  : Math.round(c.volume * 100) + "%"}
                              </span>
                            </div>
                          )}
                          {c.keyframes.map((k, i) => (
                            <Diamond
                              className="timeline-key"
                              key={i}
                              size={8}
                              fill="currentColor"
                              style={{ left: k.time * zoom }}
                            />
                          ))}
                          {c.transition !== "none" && (
                            <span
                              className="clip-fade"
                              style={{
                                width:
                                  Math.min(c.fade_duration, c.duration / 2) *
                                  zoom,
                              }}
                            />
                          )}
                          <div
                            className="trim-handle right"
                            onPointerDown={(e) => startDrag(e, c, "end")}
                          />
                        </div>
                      );
                    })}
                </div>
              ))}
              <div
                className="playhead"
                style={{ left: time * zoom }}
                onPointerDown={(e) => {
                  e.stopPropagation();
                  e.currentTarget.setPointerCapture(e.pointerId);
                }}
                onPointerMove={(e) => {
                  if (e.buttons === 1) scrub(e);
                }}
              >
                <span />
                <i />
              </div>
              {!project.clips.length && (
                <div className="timeline-empty">
                  <Film size={24} />
                  <p>Your story starts here. Drag in your first clip.</p>
                </div>
              )}
            </div>
          </div>
        </div>
        <footer className="statusbar">
          <span>
            <span className="status-dot" />{" "}
            {ready
              ? desktop
                ? "Desktop engine connected"
                : "Local engine connected"
              : "Connecting to engine"}
            <span className="status-separator">/</span>
            {project.clips.length} clips
            <span className="status-separator">/</span>
            {timecode(duration, project.fps)} total
          </span>
          <span>
            <span className="keycap">Space</span> Play / pause{" "}
            <span className="keycap">S</span> Split{" "}
            <span className="keycap">← →</span> Frame by frame{" "}
            <button onClick={() => setModal("help")}>
              All shortcuts <ChevronRight size={11} />
            </button>
          </span>
        </footer>
      </section>
      <input
        hidden
        ref={fileInput}
        type="file"
        multiple
        accept="video/*,audio/*,image/*,.mkv,.mov,.avi,.mxf,.mts,.m2ts,.flac,.aiff,.wav,.webm"
        onChange={(e) => {
          uploadFiles([...e.target.files]);
          e.target.value = "";
        }}
      />
      <input
        hidden
        ref={projectInput}
        type="file"
        accept=".json"
        onChange={(e) => {
          readProject(e.target.files[0]);
          e.target.value = "";
        }}
      />
      {toast && (
        <div className="toast" role="status">
          <Check size={16} />
          {toast}
          <button onClick={() => setToast("")}>
            <X size={14} />
          </button>
        </div>
      )}
      {modal && (
        <div
          className="modal-backdrop"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget) setModal(null);
          }}
        >
          <div className="modal">
            <div className="modal-heading">
              <div className="modal-icon">
                {modal === "export" ? (
                  <Upload size={21} />
                ) : modal === "help" ? (
                  <HelpCircle size={21} />
                ) : modal === "range" ? (
                  <Scissors size={21} />
                ) : (
                  <Settings2 size={21} />
                )}
              </div>
              <IconButton
                icon={X}
                title="Close dialog"
                onClick={() => setModal(null)}
              />
            </div>
            {modal === "snapshots" ? (
              <>
                <span className="eyebrow">YOUR WORK, PRESERVED</span>
                <h2>Project snapshots</h2>
                <p className="modal-description">
                  A snapshot is saved before creating, opening, or loading
                  another project. Originals remain linked on disk.
                </p>
                <div className="snapshot-list">
                  {snapshots.length ? (
                    snapshots.map((s) => (
                      <button
                        key={s.id}
                        onClick={() =>
                          api("/snapshots/" + s.id)
                            .then(switchProject)
                            .catch((e) => notify(e.message))
                        }
                      >
                        <FileJson size={18} />
                        <span>
                          <strong>{s.name}</strong>
                          <small>
                            {s.clips} clips ·{" "}
                            {new Date(s.modified * 1000).toLocaleString()}
                          </small>
                        </span>
                        <RotateCcw size={15} />
                      </button>
                    ))
                  ) : (
                    <p className="hint">
                      No snapshots yet. Save one from the project menu.
                    </p>
                  )}
                </div>
              </>
            ) : modal === "export" ? (
              <>
                <span className="eyebrow">READY FOR LIFTOFF</span>
                <h2>Let your story take flight.</h2>
                <p className="modal-description">
                  Render from your original footage with FFmpeg. MP4 · H.264
                  video · AAC audio.
                </p>
                <div className="export-summary">
                  <Film size={20} />
                  <div>
                    <strong>{project.name}</strong>
                    <span>
                      {timecode(duration, project.fps)} · {project.fps} fps ·{" "}
                      {project.clips.length} clips
                    </span>
                  </div>
                </div>
                <label className="modal-label">
                  Resolution
                  <select
                    aria-label="Export resolution"
                    value={exportSize}
                    onChange={(e) => setExportSize(e.target.value)}
                  >
                    <option value="1920x1080">1080p · 1920 × 1080</option>
                    <option value="3840x2160">4K UHD · 3840 × 2160</option>
                    <option value="1280x720">720p · 1280 × 720</option>
                    <option value="1080x1920">Vertical · 1080 × 1920</option>
                    <option value="1080x1080">Square · 1080 × 1080</option>
                    <option value="640x360">360p · Fast test export</option>
                  </select>
                </label>
                {job && (
                  <div className={`export-progress ${job.status}`}>
                    <div>
                      <span>
                        {job.status === "done"
                          ? "Your video is ready"
                          : ["error", "cancelled", "interrupted"].includes(
                                job.status,
                              )
                            ? job.status === "error"
                              ? "Export failed"
                              : "Export stopped"
                            : job.status === "queued"
                              ? "Waiting to render…"
                              : "Rendering your story…"}
                      </span>
                      <strong>{Math.round(job.progress * 100)}%</strong>
                    </div>
                    <progress value={job.progress} max="1" />
                    {job.error && <p>{job.error}</p>}
                  </div>
                )}
                {job?.status === "done" ? (
                  <>
                    <a
                      className="primary-button full"
                      href={`/api/exports/${job.id}/download`}
                      download
                    >
                      <Download size={16} />
                      Download video
                    </a>
                    <button
                      className="text-button centered"
                      onClick={() => setJob(null)}
                    >
                      Render another version
                    </button>
                  </>
                ) : (
                  <button
                    className="primary-button full"
                    disabled={
                      !duration ||
                      (job && ["queued", "rendering"].includes(job.status))
                    }
                    onClick={startExport}
                  >
                    {job && ["queued", "rendering"].includes(job.status) ? (
                      <Loader2 className="spin" size={16} />
                    ) : (
                      <Upload size={16} />
                    )}
                    Export video
                  </button>
                )}
                {job && ["queued", "rendering"].includes(job.status) && (
                  <button
                    className="text-button centered"
                    onClick={() =>
                      api("/exports/" + job.id + "/cancel", {
                        method: "POST",
                      }).catch((e) => notify(e.message))
                    }
                  >
                    Cancel export
                  </button>
                )}
                <p className="export-note">
                  Keep the server running during export. Preview color,
                  typography, and motion may differ slightly from the final
                  render.
                </p>
              </>
            ) : modal === "settings" ? (
              <>
                <span className="eyebrow">YOUR CANVAS</span>
                <h2>Project settings</h2>
                <label className="modal-label">
                  Project name
                  <input
                    aria-label="Project name"
                    value={project.name}
                    maxLength={200}
                    onChange={(e) =>
                      change((p) => ({ ...p, name: e.target.value }))
                    }
                  />
                </label>
                <label className="modal-label">
                  Canvas
                  <select
                    value={`${project.width}x${project.height}`}
                    onChange={(e) => {
                      const [width, height] = e.target.value
                        .split("x")
                        .map(Number);
                      change((p) => ({ ...p, width, height }));
                    }}
                  >
                    <option value="1920x1080">YouTube · 16:9 · 1080p</option>
                    <option value="3840x2160">YouTube · 16:9 · 4K</option>
                    <option value="1080x1920">Shorts · 9:16</option>
                    <option value="1080x1080">Square · 1:1</option>
                  </select>
                </label>
                <label className="modal-label">
                  Frame rate
                  <select
                    value={project.fps}
                    onChange={(e) =>
                      change((p) => ({ ...p, fps: +e.target.value }))
                    }
                  >
                    {[24, 25, 30, 50, 60].map((n) => (
                      <option key={n} value={n}>
                        {n} fps
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  className="primary-button full"
                  onClick={() => {
                    setModal(null);
                    saveNow();
                  }}
                >
                  Done
                </button>
              </>
            ) : modal === "range" ? (
              <>
                <span className="eyebrow">FRAME-PRECISE EDITING</span>
                <h2>Remove a range</h2>
                <p className="modal-description">
                  Delete this time range across every track and close the gap.
                  Arrow keys step through individual frames. You can undo this
                  edit.
                </p>
                <label className="modal-label">
                  From (seconds)
                  <input
                    type="number"
                    aria-label="Range start"
                    min="0"
                    step={1 / project.fps}
                    value={range.start}
                    onChange={(e) =>
                      setRange((r) => ({
                        ...r,
                        start: Math.max(0, +e.target.value),
                      }))
                    }
                  />
                </label>
                <label className="modal-label">
                  To (seconds)
                  <input
                    type="number"
                    aria-label="Range end"
                    min="0"
                    step={1 / project.fps}
                    value={range.end}
                    onChange={(e) =>
                      setRange((r) => ({
                        ...r,
                        end: Math.max(0, +e.target.value),
                      }))
                    }
                  />
                </label>
                <button
                  className="primary-button full"
                  disabled={range.end <= range.start}
                  onClick={() => {
                    change((p) => removeRange(p, range.start, range.end));
                    setTime(range.start);
                    setModal(null);
                  }}
                >
                  Remove{" "}
                  {Math.max(
                    0,
                    Math.round((range.end - range.start) * project.fps),
                  )}{" "}
                  frames & close gap
                </button>
              </>
            ) : (
              <>
                <span className="eyebrow">A LITTLE LESS CLICKING</span>
                <h2>Find your flow.</h2>
                <div className="shortcut-list">
                  {[
                    ["Play / pause", "Space"],
                    ["Split selected clip", "S"],
                    ["Previous / next frame", "← / →"],
                    ["Jump ten frames", "Shift + ← / →"],
                    ["Delete selected clip", "Delete"],
                    ["Undo / redo", "⌘ Z / ⌘ Shift Z"],
                    ["Save project", "⌘ S"],
                  ].map(([a, b]) => (
                    <div key={a}>
                      <span>{a}</span>
                      <kbd>{b}</kbd>
                    </div>
                  ))}
                </div>
                <p className="hint">
                  Use Ctrl on Windows and Linux. Add media with the + button or
                  drag it onto a track. Drag clip edges to trim. Audio
                  waveforms, optical-flow slow motion, custom masks, and
                  collaboration are not included in this version.
                </p>
                <button
                  className="secondary-button full"
                  onClick={() => downloadJSON(project)}
                >
                  <FileJson size={15} />
                  Download project backup
                </button>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
function Slider({ label, value, min, max, step = 1, unit = "", onChange }) {
  return (
    <label className="slider-field">
      <span>
        {label}
        <strong>
          {Number(value).toFixed(step < 1 ? 1 : 0)}
          {unit}
        </strong>
      </span>
      <input
        aria-label={label}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        style={{ "--progress": `${((value - min) / (max - min)) * 100}%` }}
        onChange={(e) => onChange(+e.target.value)}
      />
    </label>
  );
}

createRoot(document.getElementById("root")).render(<App />);
