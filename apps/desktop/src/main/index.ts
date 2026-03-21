import { app, BrowserWindow, ipcMain } from "electron";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { BridgeManager } from "./bridge-manager.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const bridgeManager = new BridgeManager();

async function createWindow(): Promise<void> {
  const window = new BrowserWindow({
    width: 1560,
    height: 980,
    backgroundColor: "#f4efe2",
    webPreferences: {
      preload: join(__dirname, "../preload/index.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

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
