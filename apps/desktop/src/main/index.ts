import { app, BrowserWindow, ipcMain, shell } from "electron";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { BridgeManager } from "./bridge-manager.js";
import { openDraftInEditor } from "./open-editor.js";
import { runBinary } from "./run-binary.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const bridgeManager = new BridgeManager();
const CONFIG_PATH_TIMEOUT_MS = 8_000;

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

  window.webContents.on("before-input-event", (event, input) => {
    if (
      input.type === "keyDown" &&
      input.meta &&
      !input.control &&
      input.key.toLowerCase() === "p"
    ) {
      event.preventDefault();
      window.webContents.send("mxr:commandPaletteShortcut");
    }
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
  ipcMain.handle("mxr:openDraftInEditor", (_event, request) => openDraftInEditor(request));
  ipcMain.handle("mxr:openExternalUrl", async (_event, url: string) => {
    await shell.openExternal(url);
    return { ok: true };
  });
  ipcMain.handle("mxr:openLocalPath", async (_event, path: string) => {
    const errorMessage = await shell.openPath(path);
    if (errorMessage) {
      throw new Error(errorMessage);
    }
    return { ok: true };
  });
  ipcMain.handle("mxr:openConfigFile", async () => {
    const binaryPath = await bridgeManager.resolveBinaryPath();
    const configPath = await readMxrConfigPath(binaryPath);
    const errorMessage = await shell.openPath(configPath);
    if (errorMessage) {
      throw new Error(errorMessage);
    }
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

async function readMxrConfigPath(binaryPath: string): Promise<string> {
  const { stdout, stderr } = await runBinary(binaryPath, ["config", "path"], {
    timeoutMs: CONFIG_PATH_TIMEOUT_MS,
  });
  const configPath = stdout.trim();
  if (!configPath) {
    throw new Error(stderr || "mxr config path returned no output");
  }
  return configPath;
}
