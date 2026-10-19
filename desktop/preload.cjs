const { contextBridge, ipcRenderer, webUtils } = require("electron");
contextBridge.exposeInMainWorld(
  "frameDesktop",
  Object.freeze({
    platform: process.platform,
    importMedia: () => ipcRenderer.invoke("frame:import"),
    importDroppedMedia: (files) => {
      const paths = Array.from(files, (file) =>
        webUtils.getPathForFile(file),
      ).filter(Boolean);
      return ipcRenderer.invoke("frame:link-dropped", paths);
    },
    relinkMedia: (id) => ipcRenderer.invoke("frame:relink", id),
    getVersion: () => ipcRenderer.invoke("frame:version"),
  }),
);
