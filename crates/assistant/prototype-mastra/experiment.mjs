// THROWAWAY executable experiment, not part of mxr's shipped test suite.
import assert from "node:assert/strict";
import { fork, execFileSync } from "node:child_process";
import { once } from "node:events";
import { randomUUID } from "node:crypto";
import { appendFileSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { scriptedProvider } from "./provider.mjs";

const local = process.argv.includes("--local");
const smokeOnly = process.argv.includes("--smoke-only");
mkdirSync("evidence", { recursive: true });
const evidence = mkdtempSync(resolve("evidence", local ? "local-" : "scripted-"));
process.env.SPIKE_STATE = resolve(evidence, "fixture");
const fixture = await import("./fixture.ts");
fixture.save(fixture.initial());
const results = [];
const requests = new Map();
const runIds = [];
let child, provider, ready, exit, crashApprovalId, crashOnEffect;
let effectEvents = [];
let startupCount = 0;
function record(name, data) {
  const row = { name, ...data };
  results.push(row);
  writeFileSync(resolve(evidence, "results.json"), JSON.stringify(results, null, 2));
  console.log(JSON.stringify(row));
}
async function waitUntil(check, label, ms = 30_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (check()) return;
    await delay(20);
  }
  throw new Error(`Timed out after ${ms}ms: ${label}`);
}
async function start() {
  ready = undefined;
  exit = undefined;
  const started = performance.now();
  const env = {
    ...process.env,
    MASTRA_TELEMETRY_DISABLED: "1", DO_NOT_TRACK: "1",
    SPIKE_MODEL_URL: process.env.SPIKE_MODEL_URL ?? "http://127.0.0.1:11434/v1",
    SPIKE_MODEL: process.env.SPIKE_MODEL ?? "gemma4:latest",
  };
  if (!local) {
    env.SPIKE_MODEL_URL = `http://127.0.0.1:${provider.address().port}/v1`;
    env.SPIKE_MODEL = "scripted-fixture";
  }
  child = fork(resolve("worker.mjs"), [], { env, detached: true, stdio: ["ignore", "pipe", "pipe", "ipc"] });
  const log = resolve(evidence, `worker-${++startupCount}.log`);
  for (const output of [child.stdout, child.stderr]) output.on("data", (b) => appendFileSync(log, b));
  child.on("exit", (code, signal) => { exit = { code, signal }; });
  child.on("message", (event) => {
    appendFileSync(resolve(evidence, "events.jsonl"), JSON.stringify(event) + "\n");
    if (event.type === "ready") ready = event;
    const request = requests.get(event.id);
    if (request) {
      request.events.push(event);
      if (["settled", "answer", "error"].includes(event.type)) request.done = event;
    }
    if (event.type === "effect-committed") {
      effectEvents.push(event);
      if (crashOnEffect) { crashOnEffect = false; process.kill(-child.pid, "SIGKILL"); }
    }
    if (event.id === crashApprovalId && event.chunk?.type === "tool-call-approval") {
      crashApprovalId = undefined;
      process.kill(-child.pid, "SIGKILL");
    }
  });
  await waitUntil(() => {
    if (exit) throw new Error(`Worker exited during startup: ${JSON.stringify(exit)}; ${log}`);
    return ready;
  }, "SDK worker ready", 20_000);
  record("startup", { milliseconds: Math.round(performance.now() - started), rssBytes: ready.memory.rss, tools: ready.tools });
}
async function stop() {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  const done = once(child, "exit");
  process.kill(-child.pid, "SIGKILL");
  await done;
}
function command(action, args = {}, crashAtApproval = false) {
  const request = { id: randomUUID(), action, events: [], began: performance.now() };
  requests.set(request.id, request);
  if (crashAtApproval) crashApprovalId = request.id;
  child.send({ id: request.id, action, ...args });
  return request;
}
async function settle(request, ms = local ? 90_000 : 30_000) {
  await waitUntil(() => request.done, `${request.action} settled`, ms);
  writeFileSync(resolve(evidence, `${request.id}.json`), JSON.stringify(request.events, null, 2));
  if (request.done.type === "error") {
    const error = new Error(request.done.error.message);
    error.details = request.done.error;
    throw error;
  }
  return request;
}
function approvals(request) {
  return request.events.filter((e) => e.chunk?.type === "tool-call-approval").map((e) => e.chunk.payload);
}
function completed(request) {
  assert.equal(approvals(request).length, 0, "Turn must not still require approval");
  assert(request.events.some((e) => e.chunk?.type === "finish"), "Must have a terminal model result");
  assert(!request.events.some((e) => e.chunk?.type === "error"), "No runtime error chunks");
}
async function turn(name, message, crashAtApproval = false) {
  const runId = randomUUID();
  runIds.push(runId);
  const request = command("stream", { runId, message }, crashAtApproval);
  request.runId = runId;
  if (crashAtApproval) await waitUntil(() => exit, "crash at first approval notification");
  else await settle(request);
  record(name, { runId, milliseconds: Math.round(performance.now() - request.began), approvals: approvals(request).length, crashed: crashAtApproval });
  return request;
}
async function resume(runId, approved, ms = local ? 90_000 : 30_000) {
  // Use default run leases. Retry only the documented transient ownership conflict.
  const until = Date.now() + ms;
  for (;;) {
    try { return await settle(command("resume", { runId, approved }), Math.max(1, until - Date.now())); }
    catch (error) {
      if (Date.now() >= until || !/EXECUTION_CONFLICT|RUN_ACTIVE|claim|currently executing|already.*running/i.test(JSON.stringify(error.details ?? error.message))) throw error;
      await delay(250);
    }
  }
}
async function inspect() { return (await settle(command("inspect"))).done.runs; }
function databaseScan(marker) {
  const script = `import sqlite3,json,sys
c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True)
hits={}
for (name,) in c.execute("select name from sqlite_master where type='table'"):
 q='"'+name.replace('"','""')+'"'
 rows=c.execute('select * from '+q).fetchall()
 n=sum(any(sys.argv[2] in str(v) for v in row) for row in rows)
 if n: hits[name]=n
print(json.dumps(hits))`;
  return JSON.parse(execFileSync("python3", ["-c", script, resolve(fixture.root, "mastra.db"), marker], { encoding: "utf8" }));
}
const deadline = setTimeout(() => {
  if (child?.pid && child.exitCode === null && child.signalCode === null) process.kill(-child.pid, "SIGKILL");
  console.error(`Experiment timed out; evidence: ${evidence}`);
  process.exit(1);
}, local ? 300_000 : 240_000);
for (const signal of ["SIGINT", "SIGTERM"]) process.once(signal, () => {
  if (child?.pid && child.exitCode === null && child.signalCode === null) process.kill(-child.pid, "SIGKILL");
  process.exit(130);
});

try {
  if (!local) provider = await scriptedProvider(evidence);
  await start();
  const read = await turn("read", 'Search the exact subject substring "shipping" and summarize the results. Do not change anything. [scenario:read]');
  completed(read);
  assert(read.events.some((e) => e.chunk?.type === "tool-result" && e.chunk.payload?.result?.messages?.length === 2), "Search returns both notices");
  assert.equal(fixture.load().archiveExecutions.length, 0);
  const denied = await turn("denial-pending", 'Search the exact subject substring "shipping", then preview and archive both results. [scenario:archive]');
  assert.equal(approvals(denied).length, 1);
  assert.equal(fixture.load().archiveExecutions.length, 0);
  completed(await resume(denied.runId, false));
  assert.equal(fixture.load().archiveExecutions.length, 0);
  record("denial", { passed: true, mutations: 0 });
  const ordinary = await turn("ordinary-approval-pending", 'Search the exact subject substring "shipping", then preview and archive both results. [scenario:archive]');
  completed(await resume(ordinary.runId, true));
  assert.deepEqual(fixture.load().archiveExecutions, [["shipping-1", "shipping-2"]]);
  assert.equal(fixture.load().messages.find((m) => m.id === "personal-1").archived, false);
  record("ordinary-approval", { passed: true, exactSelection: fixture.load().archiveExecutions });
  let duplicate;
  try { duplicate = (await resume(ordinary.runId, true)).done; } catch (e) { duplicate = e.message; }
  assert.equal(fixture.load().archiveExecutions.length, 1);
  record("duplicate-approval", { passed: true, result: duplicate });
  fixture.save(fixture.initial());

  if (!smokeOnly) {
    for (const immediate of [false, true]) {
      const name = immediate ? "immediate-approval-crash" : "checkpointed-approval-crash";
      try {
        const pending = await turn(`${name}-pending`, 'Search the exact subject substring "shipping", then preview and archive both results. [scenario:archive]', immediate);
        if (!immediate) await stop();
        const began = performance.now();
        await start();
        const snapshots = await inspect();
        writeFileSync(resolve(evidence, `${name}-snapshots.json`), JSON.stringify(snapshots, null, 2));
        completed(await resume(pending.runId, true));
        assert.deepEqual(fixture.load().archiveExecutions, [["shipping-1", "shipping-2"]]);
        assert.equal(fixture.load().messages.find((m) => m.id === "personal-1").archived, false);
        record(name, { passed: true, recoveryMilliseconds: Math.round(performance.now() - began), exactSelection: fixture.load().archiveExecutions });
      } catch (error) { record(name, { passed: false, error: error.message, details: error.details }); process.exitCode = 1; }
      fixture.save(fixture.initial());
    }
    try {
      const send = await turn("send-pending", "Send one synthetic email. [scenario:send]");
      crashOnEffect = true;
      effectEvents = [];
      command("resume", { runId: send.runId, approved: true });
      await waitUntil(() => exit && effectEvents.length, "kill after synthetic send commits");
      assert.equal(Object.keys(fixture.load().sends).length, 1);
      const began = performance.now();
      await start();
      const recovery = await settle(command("recover-all"), 30_000);
      writeFileSync(resolve(evidence, "send-recovery.json"), JSON.stringify(recovery.events, null, 2));
      // A skipped live lease is not success. The boot helper schedules a retry
      // after the default 15-second claim expires; observe the persisted outcome.
      let statuses;
      const until = Date.now() + 30_000;
      for (;;) {
        statuses = (await settle(command("status", { runId: send.runId }))).done.runs;
        const loop = statuses.find((r) => r.workflowName === "durable-agentic-loop");
        if (!loop || !["running", "pending"].includes(loop.status)) break;
        if (Date.now() >= until) throw new Error(`Uncertain send did not recover within 30s: ${JSON.stringify(statuses)}`);
        await delay(250);
      }
      const reapprovalRequired = statuses.some((r) => r.status === "suspended");
      if (reapprovalRequired) completed(await resume(send.runId, true));
      statuses = (await settle(command("status", { runId: send.runId }))).done.runs;
      assert(!statuses.some((r) => ["running", "pending", "suspended", "failed"].includes(r.status)), "Send must actually settle after recovery");
      const state = fixture.load();
      assert.equal(Object.keys(state.sends).length, 1);
      assert(state.sendAttempts >= 2, "Recovery must exercise the uncertain tool again, not just report a skipped claim");
      record("send-crash", { passed: true, recoveryMilliseconds: Math.round(performance.now() - began), effects: Object.keys(state.sends).length, attempts: state.sendAttempts, reapprovalRequired, statuses, recovery: recovery.done.recovery, protection: "fixture operation ledger, NOT Mastra exactly-once" });
    } catch (error) { record("send-crash", { passed: false, error: error.message, details: error.details, effects: Object.keys(fixture.load().sends).length, attempts: fixture.load().sendAttempts }); process.exitCode = 1; }
    // Isolate the deletion probe from any failed recovery's background retries.
    await stop();
    await start();
  }

  fixture.save(fixture.initial());
  const marker = `mxr-erasure-${randomUUID()}`;
  const erase = await turn("erasure-pending", `Search the exact subject substring "shipping", then preview and archive both results. [scenario:archive] ${marker}`);
  record("before-deletion", { markerTables: databaseScan(marker) });
  await settle(command("cleanup", { runId: erase.runId }));
  record("cleanup-only", { markerTables: databaseScan(marker) });
  const deleted = await settle(command("delete", { runId: erase.runId }));
  record("snapshot-deletion", { deleted: deleted.done.deleted, markerTables: databaseScan(marker) });
  await stop();
  await start();
  let postDelete;
  try { postDelete = (await resume(erase.runId, true)).events; }
  catch (error) { postDelete = error.message; }
  assert.equal(fixture.load().archiveExecutions.length, 0);
  record("post-deletion-resume", { mutationCount: 0, result: postDelete, markerTables: databaseScan(marker) });
  record("complete", { passed: process.exitCode !== 1, mode: local ? "real-local-model" : "scripted-provider-real-mastra", evidence });
} catch (error) {
  record("failure", { error: error.stack ?? error.message, details: error.details, evidence });
  process.exitCode = 1;
} finally {
  await stop();
  if (provider) { provider.closeAllConnections(); await new Promise((r) => provider.close(r)); }
  clearTimeout(deadline);
}
