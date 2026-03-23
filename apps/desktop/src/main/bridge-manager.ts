import { app } from "electron";
import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { spawn } from "node:child_process";
import type { BridgeState, MxrStatusSnapshot } from "../shared/types.js";
import {
  assertBridgeMailboxContract,
  evaluateCompatibility,
  parseStatusOutput,
} from "./compatibility.js";

interface DesktopSettings {
  externalBinaryPath?: string;
}

interface BridgeConnection {
  baseUrl: string;
  authToken: string;
}

export class BridgeManager {
  private bridgeProcess: ReturnType<typeof spawn> | null = null;
  private state: BridgeState = { kind: "idle" };

  async resolveBinaryPath(): Promise<string> {
    const defaultBinaryPath = this.resolveDefaultBinaryPath();
    const settings = await this.readSettings();
    return settings.externalBinaryPath?.trim() || defaultBinaryPath;
  }

  async getState(): Promise<BridgeState> {
    return this.state.kind === "idle" ? this.connect() : this.state;
  }

  async retry(): Promise<BridgeState> {
    await this.stopBridge();
    return this.connect();
  }

  async useBundledBinary(): Promise<BridgeState> {
    const settings = await this.readSettings();
    delete settings.externalBinaryPath;
    await this.writeSettings(settings);
    await this.stopBridge();
    return this.connect();
  }

  async setExternalBinaryPath(path: string): Promise<BridgeState> {
    const settings = await this.readSettings();
    settings.externalBinaryPath = path.trim();
    await this.writeSettings(settings);
    await this.stopBridge();
    return this.connect();
  }

  async connect(): Promise<BridgeState> {
    const defaultBinaryPath = this.resolveDefaultBinaryPath();
    const binaryPath = await this.resolveBinaryPath();
    const usingBundled = binaryPath === defaultBinaryPath;

    let expectedStatus: MxrStatusSnapshot;
    try {
      expectedStatus = await this.readStatus(defaultBinaryPath);
    } catch (error) {
      this.state = {
        kind: "error",
        binaryPath: defaultBinaryPath,
        usingBundled: true,
        title: "Could not inspect the bundled mxr binary",
        detail: formatError(error),
      };
      return this.state;
    }

    let candidateStatus: MxrStatusSnapshot | null = null;
    try {
      candidateStatus = await this.readStatus(binaryPath);
    } catch (error) {
      const mismatch = evaluateCompatibility({
        expectedProtocol: expectedStatus.protocol_version,
        actual: null,
        binaryPath,
        usingBundled,
        detail: `Failed to inspect mxr at ${binaryPath}: ${formatError(error)}`,
      });
      if (!mismatch) {
        throw error;
      }
      this.state = mismatch;
      return mismatch;
    }

    const mismatch = evaluateCompatibility({
      expectedProtocol: expectedStatus.protocol_version,
      actual: candidateStatus,
      binaryPath,
      usingBundled,
    });
    if (mismatch) {
      this.state = mismatch;
      return mismatch;
    }

    try {
      const connection = await this.startBridge(binaryPath);
      await this.verifyBridgeContract(connection);
      this.state = {
        kind: "ready",
        baseUrl: connection.baseUrl,
        authToken: connection.authToken,
        binaryPath,
        usingBundled,
        daemonVersion: candidateStatus.daemon_version ?? null,
        protocolVersion: candidateStatus.protocol_version,
      };
      return this.state;
    } catch (error) {
      await this.stopBridge();
      this.state = {
        kind: "error",
        binaryPath,
        usingBundled,
        title: "Could not start mxr web bridge",
        detail: formatError(error),
      };
      return this.state;
    }
  }

  stop = async (): Promise<void> => {
    await this.stopBridge();
  };

  private resolveDefaultBinaryPath(): string {
    const bundled = join(process.resourcesPath, "bin", "mxr");
    if (existsSync(bundled)) {
      return bundled;
    }
    if (process.env.MXR_BINARY?.trim()) {
      return process.env.MXR_BINARY.trim();
    }
    return "mxr";
  }

  private async startBridge(binaryPath: string): Promise<BridgeConnection> {
    const authToken = randomBytes(24).toString("hex");
    const child = spawn(binaryPath, ["web", "--host", "127.0.0.1", "--port", "0", "--print-url"], {
      env: { ...process.env, MXR_WEB_BRIDGE_TOKEN: authToken },
      stdio: ["ignore", "pipe", "pipe"],
    });
    this.bridgeProcess = child;

    const stderr: string[] = [];
    child.stderr.on("data", (chunk) => {
      stderr.push(chunk.toString());
    });

    return await new Promise<BridgeConnection>((resolve, reject) => {
      const rl = createInterface({ input: child.stdout });

      const cleanup = () => {
        rl.removeAllListeners();
        child.removeAllListeners();
      };

      rl.once("line", (line) => {
        cleanup();
        resolve(parseBridgeConnection(line.trim(), authToken));
      });

      child.once("error", (error) => {
        cleanup();
        reject(error);
      });

      child.once("exit", (code) => {
        cleanup();
        reject(
          new Error(
            `mxr web exited before startup (code ${code ?? "unknown"}): ${stderr.join("")}`,
          ),
        );
      });
    });
  }

  private async stopBridge(): Promise<void> {
    if (!this.bridgeProcess) {
      return;
    }
    const child = this.bridgeProcess;
    this.bridgeProcess = null;
    child.kill();
  }

  private async readStatus(binaryPath: string): Promise<MxrStatusSnapshot> {
    const { stdout, stderr } = await runBinary(binaryPath, ["status", "--format", "json"]);
    if (!stdout.trim()) {
      throw new Error(stderr || "mxr status returned no output");
    }
    return parseStatusOutput(stdout);
  }

  private async verifyBridgeContract(connection: BridgeConnection): Promise<void> {
    const response = await fetch(new URL("/mailbox", `${connection.baseUrl}/`), {
      headers: {
        "x-mxr-bridge-token": connection.authToken,
      },
    });

    if (!response.ok) {
      throw new Error(`bridge validation failed with ${response.status}`);
    }

    try {
      assertBridgeMailboxContract(await response.json());
    } catch (error) {
      throw new Error(
        `This mxr binary exposes a legacy desktop web bridge. Rebuild mxr and try again. (${formatError(error)})`,
      );
    }
  }

  private settingsPath(): string {
    return join(app.getPath("userData"), "desktop-settings.json");
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
}

function parseBridgeConnection(line: string, authToken: string): BridgeConnection {
  const url = new URL(line);
  url.search = "";
  return {
    baseUrl: url.toString().replace(/\/$/, ""),
    authToken,
  };
}

async function runBinary(
  binaryPath: string,
  args: string[],
): Promise<{ stdout: string; stderr: string }> {
  return await new Promise((resolve, reject) => {
    const child = spawn(binaryPath, args, { stdio: ["ignore", "pipe", "pipe"] });
    const stdout: string[] = [];
    const stderr: string[] = [];

    child.stdout.on("data", (chunk) => stdout.push(chunk.toString()));
    child.stderr.on("data", (chunk) => stderr.push(chunk.toString()));
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) {
        resolve({ stdout: stdout.join(""), stderr: stderr.join("") });
      } else {
        reject(
          new Error(
            `mxr command failed with code ${code ?? "unknown"}: ${stderr.join("") || stdout.join("")}`,
          ),
        );
      }
    });
  });
}

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
