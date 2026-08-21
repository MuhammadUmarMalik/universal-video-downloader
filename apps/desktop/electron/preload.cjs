const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("electronAPI", {
  invoke(command, args) {
    return ipcRenderer.invoke("desktop-rpc", command, args || {});
  },
  onDownloadProgress(callback) {
    const listener = (_event, payload) => callback(payload);
    ipcRenderer.on("download-progress", listener);
    return () => ipcRenderer.removeListener("download-progress", listener);
  },
  onBridgeError(callback) {
    const listener = (_event, payload) => callback(payload);
    ipcRenderer.on("desktop-bridge-error", listener);
    return () => ipcRenderer.removeListener("desktop-bridge-error", listener);
  },
});
