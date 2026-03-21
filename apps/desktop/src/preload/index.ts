import { contextBridge, ipcRenderer } from "electron";
import type { DesktopApi } from "../shared/types.js";

const api: DesktopApi = {
  getBridgeState: () => ipcRenderer.invoke("mxr:getBridgeState"),
  retryBridge: () => ipcRenderer.invoke("mxr:retryBridge"),
  useBundledMxr: () => ipcRenderer.invoke("mxr:useBundledMxr"),
  setExternalBinaryPath: (path) => ipcRenderer.invoke("mxr:setExternalBinaryPath", path),
};

contextBridge.exposeInMainWorld("mxrDesktop", api);
