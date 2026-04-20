import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BridgeManager } from "./bridge-manager.js";

vi.mock("electron", () => ({
  app: {
    getPath: () => join(tmpdir(), "mxr-desktop-tests"),
  },
}));

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

describe("BridgeManager", () => {
  let userDataDir: string;

  beforeEach(async () => {
    userDataDir = await mkdtemp(join(tmpdir(), "mxr-desktop-bridge-"));
  });

  afterEach(async () => {
    await rm(userDataDir, { force: true, recursive: true });
  });

  it("concurrent bridge transitions resolve to the newest requested state", async () => {
    const launchGate = deferred<{
      connection: { baseUrl: string; authToken: string };
      stop: () => Promise<void>;
    }>();
    const stopFirst = vi.fn(async () => {});
    const stopSecond = vi.fn(async () => {});
    const inspectBinary = vi.fn(async (binaryPath: string) => ({
      protocol_version: 1,
      daemon_version: binaryPath.includes("external") ? "2.0.0" : "1.0.0",
    }));
    const launchBridge = vi
      .fn()
      .mockImplementationOnce(async () => await launchGate.promise)
      .mockImplementationOnce(async (_binaryPath: string, authToken: string) => ({
        connection: { baseUrl: "http://bridge/external", authToken },
        stop: stopSecond,
      }));

    const manager = new BridgeManager({
      inspectBinary,
      launchBridge,
      verifyBridgeContract: vi.fn(async () => {}),
      createAuthToken: () => "token-1",
      getUserDataPath: () => userDataDir,
      getBundledBinaryPath: () => "/tmp/bundled-mxr",
      getEnvBinaryPath: () => null,
    });

    const initial = manager.getState();
    const retried = manager.retry();
    const external = manager.setExternalBinaryPath("/tmp/external-mxr");

    launchGate.resolve({
      connection: { baseUrl: "http://bridge/bundled", authToken: "token-1" },
      stop: stopFirst,
    });

    const [initialState, retriedState, externalState] = await Promise.all([
      initial,
      retried,
      external,
    ]);

    expect(initialState).toMatchObject({
      kind: "ready",
      binaryPath: "/tmp/external-mxr",
      usingBundled: false,
    });
    expect(retriedState).toEqual(externalState);
    expect(stopFirst).toHaveBeenCalledTimes(1);
    expect(launchBridge).toHaveBeenCalledTimes(2);
    expect(launchBridge.mock.calls.map((call) => call[0])).toEqual([
      "/tmp/bundled-mxr",
      "/tmp/external-mxr",
    ]);
  });

  it("does not let an older successful startup overwrite a newer mismatch", async () => {
    const launchGate = deferred<{
      connection: { baseUrl: string; authToken: string };
      stop: () => Promise<void>;
    }>();
    const stopFirst = vi.fn(async () => {});
    const inspectBinary = vi.fn(async (binaryPath: string) => {
      if (binaryPath.includes("legacy")) {
        return {
          protocol_version: 0,
          daemon_version: "0.3.0",
        };
      }
      return {
        protocol_version: 1,
        daemon_version: "1.0.0",
      };
    });
    const launchBridge = vi.fn().mockImplementationOnce(async () => await launchGate.promise);

    const manager = new BridgeManager({
      inspectBinary,
      launchBridge,
      verifyBridgeContract: vi.fn(async () => {}),
      createAuthToken: () => "token-2",
      getUserDataPath: () => userDataDir,
      getBundledBinaryPath: () => "/tmp/bundled-mxr",
      getEnvBinaryPath: () => null,
    });

    const initial = manager.getState();
    const legacy = manager.setExternalBinaryPath("/tmp/legacy-mxr");

    launchGate.resolve({
      connection: { baseUrl: "http://bridge/bundled", authToken: "token-2" },
      stop: stopFirst,
    });

    const [initialState, legacyState] = await Promise.all([initial, legacy]);

    expect(initialState).toMatchObject({
      kind: "mismatch",
      binaryPath: "/tmp/legacy-mxr",
      actualProtocol: 0,
      requiredProtocol: 1,
    });
    expect(legacyState).toEqual(initialState);
    expect(launchBridge).toHaveBeenCalledTimes(1);
    expect(stopFirst).toHaveBeenCalledTimes(1);
  });
});
