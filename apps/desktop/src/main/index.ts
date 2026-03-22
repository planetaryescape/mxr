import { app, BrowserWindow, ipcMain, shell } from "electron";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { BridgeManager } from "./bridge-manager.js";
import { openDraftInEditor } from "./open-editor.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const bridgeManager = new BridgeManager();

async function createWindow(): Promise<void> {
  const window = new BrowserWindow({
    width: 1560,
    height: 980,
    backgroundColor: "#090b12",
    webPreferences: {
      preload: join(__dirname, "../preload/index.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

  if (process.env.MXR_DESKTOP_DEBUG_RENDERER === "1") {
    window.webContents.on("console-message", (_event, level, message, line, sourceId) => {
      console.log(`[renderer:${level}] ${sourceId}:${line} ${message}`);
    });
    window.webContents.on("did-fail-load", (_event, code, description, url, isMainFrame) => {
      console.error(
        `[renderer:did-fail-load] code=${code} main=${isMainFrame} url=${url} ${description}`,
      );
    });
    window.webContents.on("render-process-gone", (_event, details) => {
      console.error(`[renderer:gone] ${details.reason} exitCode=${details.exitCode}`);
    });
    window.webContents.openDevTools({ mode: "detach" });
  }

  const rendererEntry = join(__dirname, "../../dist/renderer/index.html");
  await window.loadFile(rendererEntry);
}

app.whenReady().then(async () => {
  ipcMain.handle("mxr:getBridgeState", () => bridgeManager.getState());
  ipcMain.handle("mxr:retryBridge", () => bridgeManager.retry());
  ipcMain.handle("mxr:useBundledMxr", () => bridgeManager.useBundledBinary());
  ipcMain.handle("mxr:setExternalBinaryPath", (_event, path: string) =>
    bridgeManager.setExternalBinaryPath(path),
  );
  ipcMain.handle("mxr:openDraftInEditor", (_event, request) => openDraftInEditor(request));
  ipcMain.handle("mxr:openExternalUrl", async (_event, url: string) => {
    await shell.openExternal(url);
    return { ok: true };
  });

  await createWindow();

  app.on("activate", async () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      await createWindow();
    }
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});

app.on("before-quit", async () => {
  await bridgeManager.stop();
});
