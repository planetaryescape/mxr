import { app } from "electron";
import type { ChildProcessByStdio } from "node:child_process";
import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import type { Readable } from "node:stream";
import type { BridgeState, MailboxResponse, MxrStatusSnapshot } from "../shared/types.js";
import { runBinary } from "./run-binary.js";
import {
  assertBridgeMailboxContract,
  evaluateCompatibility,
  parseStatusOutput,
} from "./compatibility.js";

const STATUS_TIMEOUT_MS = 8_000;
const BRIDGE_START_TIMEOUT_MS = 15_000;
const BRIDGE_VERIFY_TIMEOUT_MS = 8_000;
const BRIDGE_STOP_TIMEOUT_MS = 5_000;

type SpawnedBridgeProcess = ChildProcessByStdio<null, Readable, Readable>;

interface DesktopSettings {
  externalBinaryPath?: string;
}

interface BridgeConnection {
  baseUrl: string;
  authToken: string;
}

interface ManagedBridgeProcess {
  connection: BridgeConnection;
  stop(): Promise<void>;
}

interface PendingWaiter {
  generation: number;
  resolve: (state: BridgeState) => void;
}

type BridgeIntent =
  | { kind: "ensure" }
  | { kind: "retry" }
  | { kind: "useBundled" }
  | { kind: "setExternal"; path: string }
  | { kind: "stop" };

interface BridgeManagerOptions {
  inspectBinary?: (binaryPath: string) => Promise<MxrStatusSnapshot>;
  launchBridge?: (binaryPath: string, authToken: string) => Promise<ManagedBridgeProcess>;
  verifyBridgeContract?: (connection: BridgeConnection) => Promise<MailboxResponse>;
  createAuthToken?: () => string;
  getUserDataPath?: () => string;
  getBundledBinaryPath?: () => string | null;
  getEnvBinaryPath?: () => string | null;
}

export class BridgeManager {
  private readonly inspectBinary: (binaryPath: string) => Promise<MxrStatusSnapshot>;
  private readonly launchBridge: (
    binaryPath: string,
    authToken: string,
  ) => Promise<ManagedBridgeProcess>;
  private readonly verifyBridgeContractImpl: (
    connection: BridgeConnection,
  ) => Promise<MailboxResponse>;
  private readonly createAuthToken: () => string;
  private readonly getUserDataPath: () => string;
  private readonly getBundledBinaryPath: () => string | null;
  private readonly getEnvBinaryPath: () => string | null;

  private bridgeProcess: ManagedBridgeProcess | null = null;
  private state: BridgeState = { kind: "idle" };
  private pendingIntent: BridgeIntent | null = null;
  private draining: Promise<void> | null = null;
  private requestedGeneration = 0;
  private settledGeneration = 0;
  private waiters: PendingWaiter[] = [];

  constructor(options: BridgeManagerOptions = {}) {
    this.inspectBinary = options.inspectBinary ?? ((binaryPath) => this.readStatus(binaryPath));
    this.launchBridge =
      options.launchBridge ??
      ((binaryPath, authToken) => launchBridgeProcess(binaryPath, authToken));
    this.verifyBridgeContractImpl =
      options.verifyBridgeContract ?? ((connection) => verifyBridgeContract(connection));
    this.createAuthToken = options.createAuthToken ?? (() => randomBytes(24).toString("hex"));
    this.getUserDataPath = options.getUserDataPath ?? (() => app.getPath("userData"));
    this.getBundledBinaryPath = options.getBundledBinaryPath ?? defaultBundledBinaryPath;
    this.getEnvBinaryPath =
      options.getEnvBinaryPath ?? (() => process.env.MXR_BINARY?.trim() ?? null);
  }

  async resolveBinaryPath(): Promise<string> {
    const defaultBinaryPath = this.resolveDefaultBinaryPath();
    const settings = await this.readSettings();
    return settings.externalBinaryPath?.trim() || defaultBinaryPath;
  }

  async getState(): Promise<BridgeState> {
    if (this.state.kind !== "idle" && !this.draining && !this.pendingIntent) {
      return this.state;
    }
    return await this.requestTransition({ kind: "ensure" });
  }

  async retry(): Promise<BridgeState> {
    return await this.requestTransition({ kind: "retry" });
  }

  async useBundledBinary(): Promise<BridgeState> {
    return await this.requestTransition({ kind: "useBundled" });
  }

  async setExternalBinaryPath(path: string): Promise<BridgeState> {
    return await this.requestTransition({ kind: "setExternal", path });
  }

  stop = async (): Promise<void> => {
    await this.requestTransition({ kind: "stop" });
  };

  private async requestTransition(intent: BridgeIntent): Promise<BridgeState> {
    if (
      intent.kind === "ensure" &&
      this.state.kind !== "idle" &&
      !this.draining &&
      !this.pendingIntent
    ) {
      return this.state;
    }

    const generation = ++this.requestedGeneration;
    this.pendingIntent = mergeIntent(this.pendingIntent, intent);

    const result = new Promise<BridgeState>((resolve) => {
      this.waiters.push({ generation, resolve });
    });

    if (!this.draining) {
      this.draining = this.drainTransitions();
    }

    return await result;
  }

  private async drainTransitions(): Promise<void> {
    try {
      while (this.pendingIntent) {
        const intent = this.pendingIntent;
        const generation = this.requestedGeneration;
        this.pendingIntent = null;
        this.state = await this.applyIntent(intent);
        if (!this.pendingIntent && generation >= this.requestedGeneration) {
          this.settledGeneration = generation;
          this.resolveWaiters();
        }
      }

      if (this.settledGeneration < this.requestedGeneration) {
        this.settledGeneration = this.requestedGeneration;
        this.resolveWaiters();
      }
    } finally {
      this.draining = null;
      if (this.pendingIntent && !this.draining) {
        this.draining = this.drainTransitions();
      }
    }
  }

  private resolveWaiters() {
    const pending: PendingWaiter[] = [];
    for (const waiter of this.waiters) {
      if (waiter.generation <= this.settledGeneration) {
        waiter.resolve(this.state);
        continue;
      }
      pending.push(waiter);
    }
    this.waiters = pending;
  }

  private async applyIntent(intent: BridgeIntent): Promise<BridgeState> {
    try {
      switch (intent.kind) {
        case "ensure":
          if (this.state.kind === "ready" && this.bridgeProcess) {
            return this.state;
          }
          return await this.connect();
        case "retry":
          await this.stopBridge();
          return await this.connect();
        case "useBundled": {
          const settings = await this.readSettings();
          delete settings.externalBinaryPath;
          await this.writeSettings(settings);
          await this.stopBridge();
          return await this.connect();
        }
        case "setExternal": {
          const nextPath = intent.path.trim();
          if (!nextPath) {
            const settings = await this.readSettings();
            delete settings.externalBinaryPath;
            await this.writeSettings(settings);
          } else {
            const settings = await this.readSettings();
            settings.externalBinaryPath = nextPath;
            await this.writeSettings(settings);
          }
          await this.stopBridge();
          return await this.connect();
        }
        case "stop":
          await this.stopBridge();
          return { kind: "idle" };
      }
    } catch (error) {
      const binaryPath = await this.safeResolveBinaryPath();
      const defaultBinaryPath = this.resolveDefaultBinaryPath();
      return {
        kind: "error",
        binaryPath,
        usingBundled: binaryPath === defaultBinaryPath,
        title: "Desktop bridge transition failed",
        detail: formatError(error),
      };
    }
  }

  private resolveDefaultBinaryPath(): string {
    return this.getBundledBinaryPath() ?? this.getEnvBinaryPath() ?? "mxr";
  }

  private async connect(): Promise<BridgeState> {
    const defaultBinaryPath = this.resolveDefaultBinaryPath();
    const binaryPath = await this.resolveBinaryPath();
    const usingBundled = binaryPath === defaultBinaryPath;

    let expectedStatus: MxrStatusSnapshot;
    try {
      expectedStatus = await this.inspectBinary(defaultBinaryPath);
    } catch (error) {
      return {
        kind: "error",
        binaryPath: defaultBinaryPath,
        usingBundled: true,
        title: "Could not inspect the bundled mxr binary",
        detail: formatError(error),
      };
    }

    let candidateStatus: MxrStatusSnapshot | null = null;
    try {
      candidateStatus = await this.inspectBinary(binaryPath);
    } catch (error) {
      const mismatch = evaluateCompatibility({
        expectedProtocol: expectedStatus.protocol_version,
        actual: null,
        binaryPath,
        usingBundled,
        detail: `Failed to inspect mxr at ${binaryPath}: ${formatError(error)}`,
      });
      if (mismatch) {
        return mismatch;
      }
      return {
        kind: "error",
        binaryPath,
        usingBundled,
        title: "Could not inspect the selected mxr binary",
        detail: formatError(error),
      };
    }

    const mismatch = evaluateCompatibility({
      expectedProtocol: expectedStatus.protocol_version,
      actual: candidateStatus,
      binaryPath,
      usingBundled,
    });
    if (mismatch) {
      return mismatch;
    }

    try {
      const authToken = this.createAuthToken();
      const bridge = await this.launchBridge(binaryPath, authToken);
      this.bridgeProcess = bridge;
      const initialMailbox = await this.verifyBridgeContractImpl(bridge.connection);
      return {
        kind: "ready",
        baseUrl: bridge.connection.baseUrl,
        authToken: bridge.connection.authToken,
        binaryPath,
        usingBundled,
        daemonVersion: candidateStatus.daemon_version ?? null,
        protocolVersion: candidateStatus.protocol_version,
        initialMailbox,
      };
    } catch (error) {
      await this.stopBridge();
      return {
        kind: "error",
        binaryPath,
        usingBundled,
        title: "Could not start mxr web bridge",
        detail: formatError(error),
      };
    }
  }

  private async stopBridge(): Promise<void> {
    const bridge = this.bridgeProcess;
    this.bridgeProcess = null;
    await bridge?.stop();
  }

  private async readStatus(binaryPath: string): Promise<MxrStatusSnapshot> {
    const { stdout, stderr } = await runBinary(binaryPath, ["status", "--format", "json"], {
      timeoutMs: STATUS_TIMEOUT_MS,
    });
    if (!stdout.trim()) {
      throw new Error(stderr || "mxr status returned no output");
    }
    return parseStatusOutput(stdout);
  }

  private settingsPath(): string {
    return join(this.getUserDataPath(), "desktop-settings.json");
  }

  private async readSettings(): Promise<DesktopSettings> {
    try {
      const contents = await readFile(this.settingsPath(), "utf8");
      return JSON.parse(contents) as DesktopSettings;
    } catch {
      return {};
    }
  }

  private async writeSettings(settings: DesktopSettings): Promise<void> {
    const filePath = this.settingsPath();
    await mkdir(dirname(filePath), { recursive: true });
    await writeFile(filePath, JSON.stringify(settings, null, 2));
  }

  private async safeResolveBinaryPath(): Promise<string> {
    try {
      return await this.resolveBinaryPath();
    } catch {
      return this.resolveDefaultBinaryPath();
    }
  }
}

function mergeIntent(current: BridgeIntent | null, next: BridgeIntent): BridgeIntent {
  if (!current) {
    return next;
  }
  if (next.kind === "ensure") {
    return current;
  }
  return next;
}

function defaultBundledBinaryPath() {
  const bundled = join(process.resourcesPath, "bin", "mxr");
  return existsSync(bundled) ? bundled : null;
}

async function launchBridgeProcess(
  binaryPath: string,
  authToken: string,
): Promise<ManagedBridgeProcess> {
  const child = spawn(binaryPath, ["web", "--host", "127.0.0.1", "--port", "0", "--print-url"], {
    env: { ...process.env, MXR_WEB_BRIDGE_TOKEN: authToken },
    stdio: ["ignore", "pipe", "pipe"],
  });

  const stderr: string[] = [];
  child.stderr.on("data", (chunk) => {
    stderr.push(chunk.toString());
  });

  try {
    const line = await waitForBridgeStartupLine(child, stderr);
    return {
      connection: parseBridgeConnection(line.trim(), authToken),
      stop: async () => {
        await stopChildProcess(child);
      },
    };
  } catch (error) {
    await stopChildProcess(child);
    throw error;
  }
}

async function waitForBridgeStartupLine(
  child: SpawnedBridgeProcess,
  stderr: string[],
): Promise<string> {
  const startupPromise = new Promise<string>((resolve, reject) => {
    const rl = createInterface({ input: child.stdout });

    const cleanup = () => {
      rl.close();
      child.removeListener("error", onError);
      child.removeListener("exit", onExit);
    };

    const onError = (error: Error) => {
      cleanup();
      reject(error);
    };

    const onExit = (code: number | null) => {
      cleanup();
      reject(
        new Error(`mxr web exited before startup (code ${code ?? "unknown"}): ${stderr.join("")}`),
      );
    };

    rl.once("line", (line) => {
      cleanup();
      resolve(line);
    });
    child.once("error", onError);
    child.once("exit", onExit);
  });

  return await withTimeout(
    startupPromise,
    BRIDGE_START_TIMEOUT_MS,
    () => `Timed out waiting for mxr web startup from ${child.spawnfile}`,
  );
}

async function stopChildProcess(child: SpawnedBridgeProcess): Promise<void> {
  if (child.exitCode !== null || child.killed) {
    return;
  }

  const exit = childExit(child);
  child.kill("SIGTERM");
  const terminated = await raceWithTimeout(
    exit.then(() => true),
    BRIDGE_STOP_TIMEOUT_MS,
    false,
  );
  if (terminated) {
    return;
  }

  child.kill("SIGKILL");
  await raceWithTimeout(exit, BRIDGE_STOP_TIMEOUT_MS, undefined);
}

async function childExit(child: SpawnedBridgeProcess): Promise<void> {
  if (child.exitCode !== null) {
    return;
  }
  await new Promise<void>((resolve) => {
    const finish = () => {
      child.removeListener("exit", onExit);
      child.removeListener("error", onError);
      resolve();
    };
    const onExit = () => finish();
    const onError = () => finish();
    child.once("exit", onExit);
    child.once("error", onError);
  });
}

async function verifyBridgeContract(
  connection: BridgeConnection,
): Promise<MailboxResponse> {
  const controller = new AbortController();
  const timeout = globalThis.setTimeout(() => controller.abort(), BRIDGE_VERIFY_TIMEOUT_MS);
  try {
    const response = await fetch(new URL("/mailbox", `${connection.baseUrl}/`), {
      headers: {
        "x-mxr-bridge-token": connection.authToken,
      },
      signal: controller.signal,
    });

    if (!response.ok) {
      throw new Error(`bridge validation failed with ${response.status}`);
    }

    try {
      const payload = await response.json();
      assertBridgeMailboxContract(payload);
      return payload as MailboxResponse;
    } catch (error) {
      throw new Error(
        `This mxr binary exposes a legacy desktop web bridge. Rebuild mxr and try again. (${formatError(error)})`,
      );
    }
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      throw new Error("Timed out validating the mxr desktop bridge");
    }
    throw error;
  } finally {
    globalThis.clearTimeout(timeout);
  }
}

function parseBridgeConnection(line: string, authToken: string): BridgeConnection {
  const url = new URL(line);
  url.search = "";
  return {
    baseUrl: url.toString().replace(/\/$/, ""),
    authToken,
  };
}

async function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
  message: () => string,
): Promise<T> {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  try {
    return await new Promise<T>((resolve, reject) => {
      timeoutId = globalThis.setTimeout(() => reject(new Error(message())), timeoutMs);
      promise.then(resolve, reject);
    });
  } finally {
    if (timeoutId) {
      globalThis.clearTimeout(timeoutId);
    }
  }
}

async function raceWithTimeout<T, F>(
  promise: Promise<T>,
  timeoutMs: number,
  fallback: F,
): Promise<T | F> {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  try {
    return await Promise.race([
      promise,
      new Promise<T | F>((resolve) => {
        timeoutId = globalThis.setTimeout(() => {
          resolve(fallback);
        }, timeoutMs);
      }),
    ]);
  } finally {
    if (timeoutId) {
      globalThis.clearTimeout(timeoutId);
    }
  }
}

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
