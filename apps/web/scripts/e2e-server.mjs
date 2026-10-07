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
// Overridable so the suite can run next to a dev stack that already owns 5173.
const appPort = Number(process.env.MXR_E2E_APP_PORT ?? "5173");
const appUrl = `http://127.0.0.1:${appPort}`;
const token = process.env.MXR_E2E_BRIDGE_TOKEN ?? "mxr-e2e-token";
const once = process.argv.includes("--once");
// "preview" serves a production build (vite build, then vite preview, still
// proxying /api to the e2e bridge). The speed gate runs there: React's
// development build adds work that users never pay for.
const appMode = process.env.MXR_E2E_APP_MODE === "preview" ? "preview" : "dev";

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
  mkdirSync(join(runtimeDir, "spool"), { recursive: true });
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
email = "alex@demo.mxr.local"

[accounts.fake.sync]
type = "fake"

[accounts.fake.send]
type = "fake"
`,
  );
}

function startDaemon() {
  // CLI builds land in target-cli/ (.cargo/config.toml isolates them from
  // rust-analyzer's target/); CI sets MXR_E2E_BIN to the same path.
  const bin = resolve(process.env.MXR_E2E_BIN ?? join(repoRoot, "target-cli/debug/mxr"));
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
      // The account uses the demo's own address so sent mail syncs as
      // outbound (the desk's owed and waiting lanes need it). That makes it
      // the personal demo profile, which keeps 55% of this count: 120.
      MXR_FAKE_MESSAGE_COUNT: "218",
      // Specs drop messages and sync failures here (crates/provider-fake
      // spool.rs); e2e/helpers/spool.ts writes them.
      MXR_FAKE_SPOOL_DIR: join(runtimeDir, "spool"),
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

// The daemon's output is drained (a full pipe would stall it) and its tail
// kept, so an unexpected exit says why instead of just "signal=SIGABRT".
const daemonTail = [];
function keepTail(stream) {
  stream?.setEncoding("utf8");
  stream?.on("data", (chunk) => {
    for (const line of chunk.split("\n")) {
      if (!line) continue;
      daemonTail.push(line);
      if (daemonTail.length > 80) daemonTail.shift();
    }
  });
}

function watchDaemon(child) {
  keepTail(child.stdout);
  keepTail(child.stderr);
  child.on("exit", (code, signal) => {
    if (shuttingDown) return;
    if (daemonStoppedByControl) {
      daemonStoppedByControl = false;
      console.error(`[e2e-server] daemon stopped by control code=${code} signal=${signal}`);
      return;
    }
    console.error(`[e2e-server] daemon exited code=${code} signal=${signal}`);
    console.error("[e2e-server] last daemon output:\n" + daemonTail.join("\n"));
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
  // The daemon being stopped, not whichever one `daemon` names when the
  // timer fires: a restart replaces it within the grace period.
  const child = daemon;
  return new Promise((resolveStop) => {
    child.once("exit", () => resolveStop());
    child.kill("SIGTERM");
    setTimeout(() => {
      if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
    }, 2_000).unref();
  });
}

function respondJson(res, body, status = 200) {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}

function startVite() {
  const env = { ...process.env, MXR_BRIDGE_URL: bridgeUrl };
  const serve = ["--host", "127.0.0.1", "--port", String(appPort), "--strictPort"];
  if (appMode === "dev") {
    return spawn("npm", ["run", "dev", "--", ...serve], { cwd: appDir, env, stdio: "inherit" });
  }
  // vite preview's proxy defaults to the dev server's, so /api still
  // reaches this bridge.
  return spawn("sh", ["-c", `npx vite build && npx vite preview ${serve.join(" ")}`], {
    cwd: appDir,
    env,
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
      const ready = appMode === "dev" ? html.includes("/src/main.tsx") : html.includes("/assets/");
      if (!ready) throw new Error("vite app not ready");
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
