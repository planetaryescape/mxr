import { spawn, execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, renameSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";

function identity(pid) {
  try {
    if (process.platform === "linux") {
      const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
      return stat.slice(stat.lastIndexOf(")") + 2).split(" ")[19];
    }
    return (
      execFileSync("/bin/ps", ["-p", String(pid), "-o", "lstart="], {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "ignore"],
      }).trim() || null
    );
  } catch {
    return null;
  }
}

function groupRunning(pid) {
  try {
    const rows = execFileSync("ps", ["-axo", "pgid=,stat="], { encoding: "utf8" });
    return rows.split("\n").some((line) => {
      const [group, state] = line.trim().split(/\s+/);
      return Number(group) === pid && state && !state.startsWith("Z");
    });
  } catch {
    return false;
  }
}

async function waitStopped(pid, timeout) {
  const until = Date.now() + timeout;
  while (groupRunning(pid)) {
    if (Date.now() >= until) return false;
    await new Promise((done) => setTimeout(done, 25));
  }
  return true;
}

async function stopRecord(record) {
  if (!Number.isSafeInteger(record.pid) || record.pid < 2 || record.pid === process.pid)
    throw new Error("Invalid owned process identity");
  const current = identity(record.pid);
  if (current !== null && current !== record.identity)
    throw new Error("Owned PID was reused; refusing to signal it");
  // Each child was spawned as its own group leader; include npm/sh workers.
  try {
    process.kill(-record.pid, "SIGTERM");
  } catch (error) {
    if (error.code !== "ESRCH") throw error;
  }
  if (await waitStopped(record.pid, 1_000)) return;
  try {
    process.kill(-record.pid, "SIGKILL");
  } catch (error) {
    if (error.code !== "ESRCH") throw error;
  }
  if (!(await waitStopped(record.pid, 500))) throw new Error("Owned process group stayed alive");
}

export class OwnedProcesses {
  constructor(manifest) {
    this.manifest = resolve(manifest);
    this.state = {
      owner: { pid: process.pid, identity: identity(process.pid) },
      run: randomUUID(),
      children: [],
    };
    if (existsSync(this.manifest)) {
      const prior = JSON.parse(readFileSync(this.manifest, "utf8"));
      if (prior.owner.identity !== null && identity(prior.owner.pid) === prior.owner.identity)
        throw new Error("An owned e2e server is already alive");
    }
    this.persist();
  }
  persist() {
    const temp = this.manifest + ".tmp";
    writeFileSync(temp, JSON.stringify(this.state), { mode: 0o600 });
    renameSync(temp, this.manifest);
  }
  spawn(name, command, args, options) {
    const child = spawn(command, args, { ...options, detached: true });
    if (!child.pid) return child;
    this.state.children.push({ name, pid: child.pid, identity: identity(child.pid) });
    this.persist();
    return child;
  }
  async stop(child) {
    const record = this.state.children.find((entry) => entry.pid === child?.pid);
    if (!record) return;
    await stopRecord(record);
    this.state.children = this.state.children.filter((entry) => entry !== record);
    this.persist();
  }
  async stopAll() {
    const failures = [];
    for (const record of [...this.state.children].reverse()) {
      try {
        await stopRecord(record);
        this.state.children = this.state.children.filter((entry) => entry !== record);
        this.persist();
      } catch (error) {
        failures.push(error);
      }
    }
    if (failures.length) throw new AggregateError(failures, "Owned process cleanup failed");
  }
}

export async function cleanupManifest(manifest) {
  if (!existsSync(manifest)) return;
  const state = JSON.parse(readFileSync(manifest, "utf8"));
  if (typeof state.run !== "string" || !Array.isArray(state.children))
    throw new Error("Invalid owned manifest");
  if (
    state.owner.pid !== process.pid &&
    state.owner.identity !== null &&
    identity(state.owner.pid) === state.owner.identity
  ) {
    try {
      process.kill(state.owner.pid, "SIGTERM");
    } catch (error) {
      if (error.code !== "ESRCH") throw error;
    }
    const until = Date.now() + 2_000;
    while (identity(state.owner.pid) === state.owner.identity && Date.now() < until) {
      await new Promise((done) => setTimeout(done, 25));
    }
    if (identity(state.owner.pid) === state.owner.identity)
      process.kill(state.owner.pid, "SIGKILL");
  }
  for (const record of [...state.children].reverse()) await stopRecord(record);
  state.children = [];
  writeFileSync(manifest, JSON.stringify(state), { mode: 0o600 });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const manifest = resolve(process.argv[2] ?? ".playwright/e2e-processes.json");
  cleanupManifest(manifest).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
