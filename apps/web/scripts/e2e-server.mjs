import { spawn } from "node:child_process";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(appDir, "../..");
const runtimeDir = join(appDir, ".playwright", "mxr-e2e");
const statePath = join(appDir, ".playwright", "state.json");
const bridgePort = Number(process.env.MXR_E2E_BRIDGE_PORT ?? "17777");
const controlPort = Number(process.env.MXR_E2E_CONTROL_PORT ?? String(bridgePort + 1));
const bridgeUrl = `http://127.0.0.1:${bridgePort}`;
const controlUrl = `http://127.0.0.1:${controlPort}`;
const appUrl = "http://127.0.0.1:5173";
const token = process.env.MXR_E2E_BRIDGE_TOKEN ?? "mxr-e2e-token";
const once = process.argv.includes("--once");

let daemon;
let vite;
let controlServer;
let shuttingDown = false;
let daemonStoppedByControl = false;

main().catch((error) => {
  console.error(`[e2e-server] ${error.stack ?? error.message}`);
  shutdown(1);
});

async function main() {
  prepareRuntime();
  daemon = startDaemon();
  watchDaemon(daemon);
  await waitForHealth();
  await seedMailbox();
  writeState();
  controlServer = startControlServer();
  vite = startVite();

  vite.on("exit", (code, signal) => {
    if (!shuttingDown) {
      console.error(`[e2e-server] vite exited code=${code} signal=${signal}`);
      shutdown(code ?? 1);
    }
  });

  if (once) {
    await waitForApp();
    shutdown(0);
  }
}

function prepareRuntime() {
  rmSync(runtimeDir, { recursive: true, force: true });
  mkdirSync(join(runtimeDir, "config"), { recursive: true });
  mkdirSync(join(runtimeDir, "data"), { recursive: true });
  mkdirSync(join(runtimeDir, "run"), { recursive: true });
  mkdirSync(dirname(statePath), { recursive: true });

  const tokenPath = join(runtimeDir, "config", "bridge-token");
  writeFileSync(tokenPath, `${token}\n`, { mode: 0o600 });
  writeFileSync(
    join(runtimeDir, "config", "config.toml"),
    `[general]
default_account = "fake"

[bridge]
enabled = true
bind = "127.0.0.1"
port = ${bridgePort}
token_path = ${JSON.stringify(tokenPath)}

[accounts.fake]
name = "Fake Account"
email = "fake@example.com"

[accounts.fake.sync]
type = "fake"

[accounts.fake.send]
type = "fake"
`,
  );
}

function startDaemon() {
  const bin = resolve(process.env.MXR_E2E_BIN ?? join(repoRoot, "target/debug/mxr"));
  if (!existsSync(bin)) {
    throw new Error(`missing mxr binary at ${bin}; run cargo build -p mxr or set MXR_E2E_BIN`);
  }
  return spawn(bin, ["daemon", "--foreground", "--bridge-port", String(bridgePort)], {
    cwd: repoRoot,
    env: {
      ...process.env,
      MXR_INSTANCE: `mxr-e2e-${process.pid}`,
      MXR_CONFIG_DIR: join(runtimeDir, "config"),
      MXR_DATA_DIR: join(runtimeDir, "data"),
      MXR_SOCKET_PATH: join(runtimeDir, "run", "mxr.sock"),
      MXR_FAKE_DATASET: "demo",
      MXR_FAKE_MESSAGE_COUNT: "120",
    },
    stdio: ["ignore", "pipe", "pipe"],
  })
    .on("error", (error) => {
      throw error;
    })
    .on("spawn", () => {
      console.error(`[e2e-server] daemon listening target ${bridgeUrl}`);
    })
    .on("close", () => {});
}

function watchDaemon(child) {
  child.on("exit", (code, signal) => {
    if (shuttingDown) return;
    if (daemonStoppedByControl) {
      daemonStoppedByControl = false;
      console.error(`[e2e-server] daemon stopped by control code=${code} signal=${signal}`);
      return;
    }
    console.error(`[e2e-server] daemon exited code=${code} signal=${signal}`);
    shutdown(1);
  });
}

function startControlServer() {
  const server = createServer(async (req, res) => {
    try {
      if (req.method === "POST" && req.url === "/daemon/stop") {
        await stopDaemon();
        respondJson(res, { ok: true });
        return;
      }
      if (req.method === "POST" && req.url === "/daemon/restart") {
        await stopDaemon();
        daemon = startDaemon();
        watchDaemon(daemon);
        await waitForHealth();
        writeState();
        respondJson(res, { ok: true });
        return;
      }
      respondJson(res, { error: "not found" }, 404);
    } catch (error) {
      respondJson(res, { error: error.message }, 500);
    }
  });
  server.listen(controlPort, "127.0.0.1", () => {
    console.error(`[e2e-server] control listening ${controlUrl}`);
  });
  return server;
}

function stopDaemon() {
  if (!daemon || daemon.exitCode !== null || daemon.killed) return Promise.resolve();
  daemonStoppedByControl = true;
  return new Promise((resolveStop) => {
    daemon.once("exit", () => resolveStop());
    daemon.kill("SIGTERM");
    setTimeout(() => daemon?.kill("SIGKILL"), 2_000).unref();
  });
}

function respondJson(res, body, status = 200) {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}

function startVite() {
  return spawn("npm", ["run", "dev", "--", "--host", "127.0.0.1"], {
    cwd: appDir,
    env: { ...process.env, MXR_BRIDGE_URL: bridgeUrl },
    stdio: "inherit",
  });
}

async function waitForHealth() {
  await eventually(
    async () => {
      const response = await fetch(`${bridgeUrl}/api/v1/health`);
      if (!response.ok) throw new Error(`health ${response.status}`);
    },
    45_000,
    "bridge health",
  );
}

async function waitForApp() {
  await eventually(
    async () => {
      const response = await fetch(appUrl);
      if (!response.ok) throw new Error(`app ${response.status}`);
      const html = await response.text();
      if (!html.includes("/src/main.tsx")) throw new Error("vite app not ready");
    },
    45_000,
    "vite app",
  );
}

async function seedMailbox() {
  await fetchJson("/api/v1/mail/sync", { method: "POST" });
  await eventually(
    async () => {
      const mailbox = await fetchJson("/api/v1/mail/mailbox?lens_kind=inbox&limit=5");
      const groups = mailbox?.mailbox?.groups ?? [];
      const rows = groups.flatMap((group) => group.rows ?? []);
      if (rows.length === 0) throw new Error("mailbox empty");
    },
    60_000,
    "fake mailbox seed",
  );
}

async function fetchJson(path, init = {}) {
  const response = await fetch(`${bridgeUrl}${path}`, {
    ...init,
    headers: {
      authorization: `Bearer ${token}`,
      "content-type": "application/json",
      ...init.headers,
    },
  });
  if (!response.ok) {
    const body = await response.text().catch(() => "");
    throw new Error(`${path} ${response.status} ${body}`);
  }
  return response.json();
}

async function eventually(fn, timeoutMs, label) {
  const started = Date.now();
  let lastError;
  while (Date.now() - started < timeoutMs) {
    try {
      await fn();
      return;
    } catch (error) {
      lastError = error;
      await new Promise((resolveDelay) => setTimeout(resolveDelay, 250));
    }
  }
  throw new Error(`timed out waiting for ${label}: ${lastError?.message ?? "unknown"}`);
}

function writeState() {
  writeFileSync(
    statePath,
    JSON.stringify(
      { bridgeUrl, controlUrl, token, runtimeDir, writtenAt: new Date().toISOString() },
      null,
      2,
    ),
  );
}

function shutdown(code = 0) {
  if (shuttingDown) return;
  shuttingDown = true;
  controlServer?.close();
  vite?.kill("SIGTERM");
  daemon?.kill("SIGTERM");
  setTimeout(() => {
    vite?.kill("SIGKILL");
    daemon?.kill("SIGKILL");
    process.exit(code);
  }, 1_000).unref();
}

process.on("SIGTERM", () => shutdown(0));
process.on("SIGINT", () => shutdown(0));
