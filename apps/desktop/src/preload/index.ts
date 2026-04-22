import { contextBridge, ipcRenderer } from "electron";
import type { DesktopApi } from "../shared/types.js";

const api: DesktopApi = {
  getBridgeState: () => ipcRenderer.invoke("mxr:getBridgeState"),
  retryBridge: () => ipcRenderer.invoke("mxr:retryBridge"),
  useBundledMxr: () => ipcRenderer.invoke("mxr:useBundledMxr"),
  setExternalBinaryPath: (path) => ipcRenderer.invoke("mxr:setExternalBinaryPath", path),
  getDesktopSettings: () => ipcRenderer.invoke("mxr:getDesktopSettings"),
  updateDesktopSettings: (patch) => ipcRenderer.invoke("mxr:updateDesktopSettings", patch),
  pickAttachments: () => ipcRenderer.invoke("mxr:pickAttachments"),
  openDraftInEditor: (request) => ipcRenderer.invoke("mxr:openDraftInEditor", request),
  openBrowserDocument: (request) => ipcRenderer.invoke("mxr:openBrowserDocument", request),
  openExternalUrl: (url) => ipcRenderer.invoke("mxr:openExternalUrl", url),
  openLocalPath: (path) => ipcRenderer.invoke("mxr:openLocalPath", path),
  openConfigFile: () => ipcRenderer.invoke("mxr:openConfigFile"),
};

ipcRenderer.on("mxr:commandPaletteShortcut", () => {
  window.dispatchEvent(new CustomEvent("mxr:command-palette"));
});

contextBridge.exposeInMainWorld("mxrDesktop", api);
