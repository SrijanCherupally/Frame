const { app, BrowserWindow, dialog, ipcMain, Menu } = require("electron");
const { spawn } = require("node:child_process");
const { randomBytes } = require("node:crypto");
const net = require("node:net");
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");

let backend, window, baseURL, backendLog;
const ownsLock = app.requestSingleInstanceLock();
if (!ownsLock) app.quit();
app.on("second-instance", () => {
  if (window) {
    if (window.isMinimized()) window.restore();
    window.show();
    window.focus();
  }
});
const root = path.join(__dirname, "..");
const token = randomBytes(32).toString("hex");
const python =
  process.env.FRAME_PYTHON ||
  (process.platform === "win32" ? "python" : "python3");
const chooseOptions = {
  properties: ["openFile", "multiSelections"],
  filters: [
    {
      name: "Video, audio & images",
      extensions: [
        "mp4",
        "mov",
        "mkv",
        "webm",
        "avi",
        "mxf",
        "mts",
        "m2ts",
        "m4v",
        "mpg",
        "mpeg",
        "mp3",
        "wav",
        "aac",
        "m4a",
        "flac",
        "aiff",
        "ogg",
        "opus",
        "png",
        "jpg",
        "jpeg",
        "webp",
        "bmp",
        "tif",
        "tiff",
      ],
    },
    { name: "All files", extensions: ["*"] },
  ],
};
function freePort() {
  return new Promise((resolve, reject) => {
    const s = net.createServer();
    s.on("error", reject);
    s.listen(0, "127.0.0.1", () => {
      const port = s.address().port;
      s.close(() => resolve(port));
    });
  });
}
async function api(route, body) {
  const response = await fetch(baseURL + "/api" + route, {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-Frame-Token": token },
    body: JSON.stringify(body),
  });
  const result = await response.json();
  if (!response.ok)
    throw new Error(
      typeof result.detail === "string"
        ? result.detail
        : "Could not link this file",
    );
  return result;
}
function requireOwnWindow(event) {
  if (!event.senderFrame || !event.senderFrame.url.startsWith(baseURL + "/"))
    throw new Error("Untrusted application window");
}
ipcMain.handle("frame:link-dropped", async (event, paths) => {
  requireOwnWindow(event);
  if (
    !Array.isArray(paths) ||
    paths.length > 1000 ||
    paths.some((p) => typeof p !== "string" || !path.isAbsolute(p))
  )
    throw new Error("Drop local media files from your file manager");
  const assets = [];
  for (const file of paths)
    assets.push(await api("/media/link", { path: file }));
  return assets;
});
ipcMain.handle("frame:import", async (event) => {
  requireOwnWindow(event);
  const result = await dialog.showOpenDialog(window, chooseOptions);
  if (result.canceled) return [];
  const assets = [];
  for (const file of result.filePaths)
    assets.push(await api("/media/link", { path: file }));
  return assets;
});
ipcMain.handle("frame:relink", async (event, id) => {
  requireOwnWindow(event);
  if (typeof id !== "string" || !/^[a-zA-Z0-9-]+$/.test(id))
    throw new Error("Invalid media ID");
  const result = await dialog.showOpenDialog(window, {
    ...chooseOptions,
    properties: ["openFile"],
  });
  if (result.canceled) return null;
  return api(`/media/${id}/relink`, { path: result.filePaths[0] });
});
ipcMain.handle("frame:version", () => app.getVersion());
async function boot() {
  if (!fs.existsSync(path.join(root, "dist", "index.html")))
    throw new Error("Build Frame first with npm run build.");
  const port = await freePort();
  baseURL = `http://127.0.0.1:${port}`;
  const data =
    process.env.FRAME_DATA_DIR ||
    path.join(app.getPath("userData"), "media-library");
  fs.mkdirSync(data, { recursive: true });
  backendLog = fs.createWriteStream(path.join(data, "server.log"), {
    flags: "a",
  });
  backend = spawn(
    python,
    [
      "-m",
      "uvicorn",
      "backend.app:app",
      "--host",
      "127.0.0.1",
      "--port",
      String(port),
    ],
    {
      cwd: root,
      env: {
        ...process.env,
        FRAME_DATA_DIR: data,
        FRAME_DESKTOP_TOKEN: token,
        PYTHONUNBUFFERED: "1",
      },
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    },
  );
  let failure;
  backend.on("error", (e) => {
    failure = e;
  });
  backend.on("exit", (code) => {
    if (code !== 0)
      failure = new Error(
        `Media engine exited (${code}). See ${path.join(data, "server.log")}`,
      );
  });
  backend.stdout.pipe(backendLog);
  backend.stderr.pipe(backendLog);
  let healthy = false;
  for (let i = 0; i < 150; i++) {
    if (failure) throw failure;
    try {
      const r = await fetch(baseURL + "/api/health");
      if (r.ok) {
        const h = await r.json();
        if (!h.ffmpeg)
          throw new Error(
            "Install FFmpeg and add it to PATH before starting Frame.",
          );
        healthy = true;
        break;
      }
    } catch (e) {
      if (e.message.includes("Install FFmpeg")) throw e;
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  if (!healthy)
    throw new Error(
      "The media engine did not start. Check Python dependencies and the server log.",
    );
  window = new BrowserWindow({
    width: 1440,
    height: 960,
    minWidth: 980,
    minHeight: 720,
    title: "Frame Studio",
    backgroundColor: "#101113",
    show: false,
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      spellcheck: false,
    },
  });
  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  window.webContents.on("will-navigate", (e, url) => {
    if (!url.startsWith(baseURL + "/")) e.preventDefault();
  });
  window.webContents.session.setPermissionRequestHandler(
    (_wc, _permission, callback) => callback(false),
  );
  window.once("ready-to-show", () => window.show());
  window.on("close", (event) => {
    if (!quitting) {
      event.preventDefault();
      app.quit();
    }
  });
  await window.loadURL(baseURL);
  Menu.setApplicationMenu(
    Menu.buildFromTemplate([
      ...(process.platform === "darwin" ? [{ role: "appMenu" }] : []),
      {
        label: "File",
        submenu: [
          {
            label: "Import media…",
            accelerator: "CmdOrCtrl+I",
            click: () =>
              window.webContents.executeJavaScript(
                'document.querySelector(".import-button")?.click()',
              ),
          },
          { role: "quit" },
        ],
      },
      { role: "editMenu" },
      { role: "viewMenu" },
    ]),
  );
}
app
  .whenReady()
  .then(() => {
    if (ownsLock) return boot();
  })
  .catch((e) => {
    dialog.showErrorBox(
      "Frame could not start",
      e.message +
        "\n\nRun python -m pip install -r backend/requirements.txt and ensure FFmpeg is installed.",
    );
    app.quit();
  });
app.on("window-all-closed", () => app.quit());
let quitting = false;
let shuttingDown = false;
app.on("before-quit", (event) => {
  if (quitting) return;
  event.preventDefault();
  if (shuttingDown) return;
  shuttingDown = true;
  (async () => {
    if (window && !window.isDestroyed()) {
      let saved = false;
      try {
        saved = await Promise.race([
          window.webContents.executeJavaScript(
            "window.frameFlushProject ? window.frameFlushProject() : true",
          ),
          new Promise((resolve) => setTimeout(() => resolve(false), 4000)),
        ]);
      } catch {}
      if (!saved) {
        const answer = await dialog.showMessageBox(window, {
          type: "warning",
          title: "Your latest edits could not be saved",
          message:
            "The local engine could not save the project. Stay in Frame to download a project backup, or quit without the latest edits.",
          buttons: ["Stay in Frame", "Quit without saving"],
          defaultId: 0,
          cancelId: 0,
        });
        if (answer.response === 0) {
          shuttingDown = false;
          return;
        }
      }
    }
    quitting = true;
    if (backend && backend.exitCode === null) {
      const child = backend;
      backend = undefined;
      await new Promise((resolve) => {
        const force = setTimeout(() => {
          child.kill("SIGKILL");
          resolve();
        }, 7000);
        child.once("exit", () => {
          clearTimeout(force);
          resolve();
        });
        child.kill();
      });
    }
    if (backendLog) backendLog.end();
    app.quit();
  })().catch((error) => {
    shuttingDown = false;
    dialog.showErrorBox("Frame could not close safely", error.message);
  });
});
