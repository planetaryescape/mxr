/* @vitest-environment node */
import { afterEach, expect, it } from "vitest";
import { createServer } from "node:net";
import { existsSync, mkdtempSync, rmSync, writeFileSync, readFileSync } from "node:fs";
import { spawn } from "node:child_process";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { OwnedProcesses, cleanupManifest } from "./e2e-owned-processes.mjs";

let owned;
let root;
afterEach(async () => {
  // Test-owned fallback only, so the red-phase orphan cannot leak a port.
  if (owned)
    for (const child of owned.state.children) {
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {}
    }
  if (root && existsSync(join(root, "owned.json"))) await cleanupManifest(join(root, "owned.json"));
  if (root) rmSync(root, { recursive: true, force: true });
  owned = undefined;
});

it("keeps GNU timeout failure 124 and cleans detached owned descendants before reusing their port", async () => {
  root = mkdtempSync(join(tmpdir(), "mxr-e2e-timeout-"));
  const manifest = join(root, "owned.json");
  const port = await freePort();
  const nested = join(root, "nested.mjs");
  writeFileSync(
    nested,
    `import {spawn} from 'node:child_process';
    spawn(process.execPath,['-e', ${JSON.stringify(`require('node:http').createServer((q,s)=>s.end('owned')).listen(${port},'127.0.0.1')`)}],{stdio:'ignore'});
    setInterval(()=>{},1000);`,
  );
  const launcher = join(root, "launcher.mjs");
  writeFileSync(
    launcher,
    `import {OwnedProcesses} from ${JSON.stringify(new URL("./e2e-owned-processes.mjs", import.meta.url).href)};
    const owned=new OwnedProcesses(${JSON.stringify(manifest)});
    owned.spawn('server',process.execPath,[${JSON.stringify(nested)}],{stdio:'ignore'});
    console.log('partial log retained');setInterval(()=>{},1000);`,
  );
  const timeout = ["/usr/bin/timeout", "/opt/homebrew/bin/gtimeout"].find(existsSync);
  if (!timeout) throw new Error("GNU timeout is required for the bounded lifecycle regression");
  const deadline = spawn(
    timeout,
    ["--signal=TERM", "--kill-after=1s", "2s", process.execPath, launcher],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  let log = "";
  deadline.stdout.on("data", (data) => {
    log += data;
  });
  const ended = new Promise((done) => deadline.once("close", (code) => done(code)));
  await expect.poll(() => listening(port), { timeout: 3000 }).toBe(true);
  expect(await ended).toBe(124);
  expect(log).toContain("partial log retained");
  // GNU timeout correctly cannot reach Playwright-style detached groups.
  expect(await listening(port)).toBe(true);
  await cleanupManifest(manifest);
  expect(await listening(port)).toBe(false);
  const rebound = createServer();
  await new Promise((done, fail) => {
    rebound.once("error", fail);
    rebound.listen(port, "127.0.0.1", done);
  });
  await new Promise((done) => rebound.close(done));
});

async function freePort() {
  const server = createServer();
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  const port = server.address().port;
  await new Promise((done) => server.close(done));
  return port;
}
async function listening(port) {
  return fetch(`http://127.0.0.1:${port}`).then(
    () => true,
    () => false,
  );
}

it("stops the owned nested listener, not just its parent, so the same port can be rebound", async () => {
  root = mkdtempSync(join(tmpdir(), "mxr-e2e-processes-"));
  const manifest = join(root, "owned.json");
  const port = await freePort();
  const nested = join(root, "nested.mjs");
  writeFileSync(
    nested,
    `import {spawn} from 'node:child_process';
    spawn(process.execPath,['-e', ${JSON.stringify(`require('node:http').createServer((q,s)=>s.end('owned')).listen(${port},'127.0.0.1')`)}],{stdio:'ignore'});
    setInterval(()=>{},1000);`,
  );
  owned = new OwnedProcesses(manifest);
  const parent = owned.spawn("listener", process.execPath, [nested], { stdio: "ignore" });
  await expect.poll(() => listening(port)).toBe(true);
  await owned.stop(parent);
  expect(await listening(port)).toBe(false);
  const rebound = createServer();
  await new Promise((done, fail) => {
    rebound.once("error", fail);
    rebound.listen(port, "127.0.0.1", done);
  });
  await new Promise((done) => rebound.close(done));
  expect(JSON.parse(readFileSync(manifest, "utf8")).children).toEqual([]);
});
