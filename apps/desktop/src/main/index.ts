import { app, BrowserWindow, dialog, ipcMain, shell, type IpcMainInvokeEvent } from "electron";
import { mkdtemp, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { updateElectronApp, UpdateSourceType } from "update-electron-app";
import type { OpenBrowserDocumentRequest } from "../shared/types.js";
import { BridgeManager } from "./bridge-manager.js";
import { LinuxUpdateManager } from "./linux-updater.js";
import { openDraftInEditor } from "./open-editor.js";
import { runBinary } from "./run-binary.js";
import { DesktopSettingsStore } from "./settings-store.js";
import { configureMainTelemetry } from "./telemetry.js";
import {
  assertTrustedSender,
  validateDesktopSettingsPatch,
  validateExternalBinaryPath,
  validateExternalUrl,
  validateKnownLocalPath,
  validateOpenBrowserDocumentRequest,
  validateOpenDraftInEditorRequest,
} from "./ipc-validation.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const RENDERER_ENTRY = join(__dirname, "../../dist/renderer/index.html");
const bridgeManager = new BridgeManager();
const settingsStore = new DesktopSettingsStore();
const linuxUpdateManager = new LinuxUpdateManager({
  currentVersion: app.getVersion(),
  packaged: app.isPackaged,
});
const CONFIG_PATH_TIMEOUT_MS = 8_000;
let bridgeStopInProgress = false;
let bridgeStoppedForQuit = false;

async function createWindow(): Promise<void> {
  const window = new BrowserWindow({
    width: 1560,
    height: 980,
    backgroundColor: "#090b12",
    webPreferences: {
      preload: join(__dirname, "../preload/index.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  window.webContents.on("will-navigate", (event, url) => {
    if (stripUrlNoise(url) !== stripUrlNoise(window.webContents.getURL())) {
      event.preventDefault();
    }
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
    if (process.env.MXR_DESKTOP_OPEN_DEVTOOLS === "1") {
      window.webContents.openDevTools({ mode: "detach" });
    }
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

  await window.loadFile(RENDERER_ENTRY);
  await maybeRunPackagedSmoke(window);
}

app.whenReady().then(async () => {
  configureMainTelemetry(settingsStore.get(), mainTelemetryOptions());
  configureMacAutoUpdate();
  secureHandle("mxr:getBridgeState", () => bridgeManager.getState());
  secureHandle("mxr:retryBridge", () => bridgeManager.retry());
  secureHandle("mxr:useBundledMxr", () => bridgeManager.useBundledBinary());
  secureHandle("mxr:setExternalBinaryPath", (_event, path: unknown) =>
    bridgeManager.setExternalBinaryPath(validateExternalBinaryPath(path)),
  );
  secureHandle("mxr:getDesktopSettings", () => settingsStore.get());
  secureHandle("mxr:updateDesktopSettings", (_event, patch: unknown) => {
    const settings = settingsStore.set(validateDesktopSettingsPatch(patch));
    configureMainTelemetry(settings, mainTelemetryOptions());
    return settings;
  });
  secureHandle("mxr:pickAttachments", async () => {
    const result = await dialog.showOpenDialog({
      title: "Attach files",
      properties: ["openFile", "multiSelections"],
    });
    return { paths: result.canceled ? [] : result.filePaths };
  });
  secureHandle("mxr:openDraftInEditor", (_event, request: unknown) =>
    openDraftInEditor(validateOpenDraftInEditorRequest(request)),
  );
  secureHandle("mxr:openBrowserDocument", async (_event, request: unknown) => {
    const path = await writeBrowserDocument(validateOpenBrowserDocumentRequest(request));
    const errorMessage = await shell.openPath(path);
    if (errorMessage) {
      throw new Error(errorMessage);
    }
    return { ok: true };
  });
  secureHandle("mxr:openExternalUrl", async (_event, url: unknown) => {
    await shell.openExternal(validateExternalUrl(url));
    return { ok: true };
  });
  secureHandle("mxr:openLocalPath", async (_event, path: unknown) => {
    const errorMessage = await shell.openPath(validateKnownLocalPath(path));
    if (errorMessage) {
      throw new Error(errorMessage);
    }
    return { ok: true };
  });
  secureHandle("mxr:openConfigFile", async () => {
    const binaryPath = await bridgeManager.resolveBinaryPath();
    const configPath = await readMxrConfigPath(binaryPath);
    const errorMessage = await shell.openPath(configPath);
    if (errorMessage) {
      throw new Error(errorMessage);
    }
    return { ok: true };
  });
  secureHandle("mxr:checkForDesktopUpdate", () => linuxUpdateManager.check());
  secureHandle("mxr:downloadDesktopUpdate", () => linuxUpdateManager.download());
  secureHandle("mxr:openDownloadedUpdate", async () => {
    await linuxUpdateManager.openDownloaded();
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

app.on("before-quit", (event) => {
  if (bridgeStoppedForQuit) {
    return;
  }
  event.preventDefault();
  if (bridgeStopInProgress) {
    return;
  }
  bridgeStopInProgress = true;
  void bridgeManager.stop().finally(() => {
    bridgeStoppedForQuit = true;
    app.quit();
  });
});

function secureHandle<TArgs extends unknown[], TResult>(
  channel: string,
  handler: (event: IpcMainInvokeEvent, ...args: TArgs) => TResult | Promise<TResult>,
): void {
  ipcMain.handle(channel, async (event, ...args: TArgs) => {
    assertTrustedSender(event, RENDERER_ENTRY);
    return await handler(event, ...args);
  });
}

function stripUrlNoise(value: string): string {
  const url = new URL(value);
  url.hash = "";
  url.search = "";
  return url.toString();
}

function configureMacAutoUpdate(): void {
  if (!shouldEnableMacAutoUpdate()) {
    return;
  }

  updateElectronApp({
    updateSource: {
      type: UpdateSourceType.ElectronPublicUpdateService,
      repo: "planetaryescape/mxr",
    },
    updateInterval: "1 hour",
  });
}

function shouldEnableMacAutoUpdate(): boolean {
  if (process.platform !== "darwin" || !app.isPackaged) {
    return false;
  }

  try {
    execFileSync("codesign", ["--verify", "--deep", "--strict", app.getPath("exe")], {
      stdio: "ignore",
    });
    return true;
  } catch {
    return false;
  }
}

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

async function writeBrowserDocument(request: OpenBrowserDocumentRequest): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "mxr-desktop-browser-"));
  const filename = sanitizeBrowserFilename(request.suggestedFilename ?? request.title);
  const path = join(dir, filename.endsWith(".html") ? filename : `${filename}.html`);
  await writeFile(path, request.html, "utf8");
  return path;
}

function sanitizeBrowserFilename(value: string): string {
  const trimmed = value.trim().toLowerCase();
  const normalized = trimmed.replace(/[^a-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "");
  return normalized || "message";
}

async function maybeRunPackagedSmoke(window: BrowserWindow): Promise<void> {
  const resultPath = process.env.MXR_DESKTOP_SMOKE_RESULT;
  if (!resultPath) {
    return;
  }

  try {
    const result = (await window.webContents.executeJavaScript(
      `(() => {
        return Promise.resolve()
          .then(async () => {
            const api = window.mxrDesktop;
            const state = await api.getBridgeState();
            return {
              rendererLoaded: Boolean(document.getElementById("root")),
              preloadApi: Boolean(api && api.getBridgeState),
              bridgeKind: state.kind,
              mailboxHydrated: state.kind === "ready" && Boolean(state.initialMailbox?.mailbox),
            };
          });
      })()`,
      true,
    )) as unknown;
    await writeFile(resultPath, JSON.stringify(result, null, 2), "utf8");
  } catch (error) {
    await writeFile(
      resultPath,
      JSON.stringify(
        {
          error: error instanceof Error ? error.message : String(error),
        },
        null,
        2,
      ),
      "utf8",
    );
  } finally {
    app.quit();
  }
}

function mainTelemetryOptions(): {
  dsn?: string;
  version: string;
  environment?: string;
} {
  return {
    dsn: process.env.MXR_DESKTOP_SENTRY_DSN,
    version: app.getVersion(),
    environment: process.env.MXR_DESKTOP_SENTRY_ENVIRONMENT,
  };
}
