import type { ChildProcessByStdio } from "node:child_process";
import { spawn } from "node:child_process";
import type { Readable } from "node:stream";

type SpawnedChild = ChildProcessByStdio<null, Readable, Readable>;

export interface RunBinaryResult {
  stdout: string;
  stderr: string;
}

export interface RunBinaryOptions {
  timeoutMs?: number;
  stopTimeoutMs?: number;
}

const DEFAULT_STOP_TIMEOUT_MS = 1_000;

export async function runBinary(
  binaryPath: string,
  args: string[],
  options: RunBinaryOptions = {},
): Promise<RunBinaryResult> {
  return await new Promise<RunBinaryResult>((resolve, reject) => {
    const child = spawn(binaryPath, args, { stdio: ["ignore", "pipe", "pipe"] });
    const stdout: string[] = [];
    const stderr: string[] = [];
    let settled = false;
    let timeoutId: ReturnType<typeof setTimeout> | null = null;

    const clear = () => {
      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }
      child.removeListener("error", onError);
      child.removeListener("exit", onExit);
    };

    const settleResolve = (value: RunBinaryResult) => {
      if (settled) {
        return;
      }
      settled = true;
      clear();
      resolve(value);
    };

    const settleReject = (error: Error) => {
      if (settled) {
        return;
      }
      settled = true;
      clear();
      reject(error);
    };

    const onError = (error: Error) => settleReject(error);
    const onExit = (code: number | null) => {
      if (code === 0) {
        settleResolve({ stdout: stdout.join(""), stderr: stderr.join("") });
        return;
      }
      settleReject(
        new Error(
          `mxr command failed with code ${code ?? "unknown"}: ${stderr.join("") || stdout.join("")}`,
        ),
      );
    };

    child.stdout.on("data", (chunk) => stdout.push(chunk.toString()));
    child.stderr.on("data", (chunk) => stderr.push(chunk.toString()));
    child.once("error", onError);
    child.once("exit", onExit);

    if (options.timeoutMs) {
      timeoutId = setTimeout(() => {
        void stopChildProcess(child, options.stopTimeoutMs ?? DEFAULT_STOP_TIMEOUT_MS);
        settleReject(new Error(`Timed out running ${binaryPath} ${args.join(" ")}`));
      }, options.timeoutMs);
    }
  });
}

async function stopChildProcess(child: SpawnedChild, timeoutMs: number): Promise<void> {
  if (child.exitCode !== null || child.killed) {
    return;
  }

  const exited = waitForExit(child);
  child.kill("SIGTERM");
  const terminated = await raceWithTimeout(
    exited.then(() => true),
    timeoutMs,
    false,
  );
  if (terminated) {
    return;
  }

  child.kill("SIGKILL");
  await raceWithTimeout(exited, timeoutMs, undefined);
}

async function waitForExit(child: SpawnedChild): Promise<void> {
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
        timeoutId = setTimeout(() => resolve(fallback), timeoutMs);
      }),
    ]);
  } finally {
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
  }
}
