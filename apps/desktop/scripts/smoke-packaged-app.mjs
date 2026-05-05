import { mkdtemp, readFile, rm } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn } from "node:child_process";

const root = resolve(import.meta.dirname, "..");
const platform = process.argv.includes("--platform")
  ? process.argv[process.argv.indexOf("--platform") + 1]
  : process.platform;
const arch = process.argv.includes("--arch")
  ? process.argv[process.argv.indexOf("--arch") + 1]
  : process.arch;

const executable = findPackagedExecutable(platform, arch);
const tempDir = await mkdtemp(join(tmpdir(), "mxr-desktop-smoke-"));
const resultPath = join(tempDir, "result.json");
const child = spawn(executable, [`--user-data-dir=${join(tempDir, "profile")}`], {
  env: {
    ...process.env,
    MXR_DESKTOP_SMOKE_RESULT: resultPath,
    MXR_DESKTOP_DEBUG_RENDERER: "1",
  },
  stdio: ["ignore", "pipe", "pipe"],
});

let output = "";
child.stdout.on("data", (chunk) => {
  output += chunk.toString();
});
child.stderr.on("data", (chunk) => {
  output += chunk.toString();
});

try {
  const result = await waitForResult(resultPath, 60_000);
  if (
    !result.rendererLoaded ||
    !result.preloadApi ||
    result.bridgeKind !== "ready" ||
    !result.mailboxHydrated
  ) {
    throw new Error(`packaged smoke failed: ${JSON.stringify(result)}`);
  }
  console.log(`Packaged app smoke passed (${platform}/${arch})`);
} finally {
  await terminateChild();
  await rm(tempDir, { force: true, maxRetries: 5, recursive: true, retryDelay: 100 });
}

function findPackagedExecutable(targetPlatform, targetArch) {
  const releaseArch = targetArch === "x64" ? "x64" : targetArch;
  const candidates =
    targetPlatform === "darwin"
      ? [
          join(
            root,
            "out",
            `mxr-darwin-${releaseArch}`,
            "mxr.app",
            "Contents",
            "MacOS",
            "mxr-desktop",
          ),
          join(root, "out", `mxr-darwin-${releaseArch}`, "mxr.app", "Contents", "MacOS", "mxr"),
        ]
      : [
          join(root, "out", `mxr-linux-${releaseArch}`, "mxr-desktop"),
          join(root, "out", `mxr-linux-${releaseArch}`, "mxr"),
        ];

  const found = candidates.find((candidate) => existsSync(candidate));
  if (!found) {
    throw new Error(`packaged executable not found; checked ${candidates.join(", ")}`);
  }
  return found;
}

async function waitForResult(path, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (existsSync(path)) {
      return JSON.parse(await readFile(path, "utf8"));
    }
    if (child.exitCode != null) {
      throw new Error(`packaged app exited early with ${child.exitCode}\n${output}`);
    }
    await new Promise((resolveTimeout) => setTimeout(resolveTimeout, 250));
  }
  throw new Error(`timed out waiting for packaged app smoke result\n${output}`);
}

async function terminateChild() {
  if (child.exitCode != null) {
    return;
  }

  child.kill("SIGTERM");
  await new Promise((resolveExit) => {
    const killTimer = setTimeout(() => {
      if (child.exitCode == null) {
        child.kill("SIGKILL");
      }
    }, 2_000);
    child.once("exit", () => {
      clearTimeout(killTimer);
      resolveExit();
    });
  });
}
