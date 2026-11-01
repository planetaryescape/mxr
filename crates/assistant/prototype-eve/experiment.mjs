// THROWAWAY executable experiment, not part of mxr's test suite.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createServer } from "node:http";
import { randomUUID } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync, existsSync, readdirSync, statSync, symlinkSync, copyFileSync } from "node:fs";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { Client } from "eve/client";

const localModel = process.argv.includes("--local");
const smokeOnly = process.argv.includes("--smoke-only");
const sendCrash = process.argv.includes("--send-crash");
const immediateCrash = process.argv.includes("--immediate-crash");
mkdirSync("evidence", { recursive: true });
const evidence = mkdtempSync(resolve("evidence", localModel ? "local-" : "scripted-"));
const stateRoot = resolve(evidence, "fixture");
const runtimeRoot = resolve(evidence, "runtime");
mkdirSync(runtimeRoot);
for (const name of ["node_modules", ".output", "agent"]) symlinkSync(resolve(name), resolve(runtimeRoot, name), "dir");
copyFileSync("package.json", resolve(runtimeRoot, "package.json"));
process.env.SPIKE_STATE = stateRoot;
const fixture = await import("./fixture.ts");
fixture.save(fixture.initial());
const rows = [];
const sessions = [];
const password = randomUUID();
let child;
let modelServer;
let host;
let serverPort;
let starts = 0;
const record = (name, data) => {
  const row = { name, ...data };
  rows.push(row);
  writeFileSync(resolve(evidence, "results.json"), JSON.stringify(rows, null, 2));
  console.log(JSON.stringify(row));
};
const deadline = setTimeout(() => {
  if (child?.pid) process.kill(-child.pid, "SIGKILL");
  console.error(`Experiment timed out; evidence: ${evidence}`);
  process.exit(1);
}, localModel ? 300_000 : 180_000);
for (const signal of ["SIGINT", "SIGTERM"]) process.once(signal, () => {
  if (child?.pid && child.exitCode === null) process.kill(-child.pid, "SIGKILL");
  process.exit(130);
});

async function waitUntil(check, label, ms = 20_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (await check()) return;
    await delay(100);
  }
  throw new Error(`Timed out: ${label}`);
}

async function scriptedProvider() {
  let sequence = 0;
  const server = createServer(async (req, res) => {
    try {
      let body = "";
      for await (const part of req) body += part;
      const request = JSON.parse(body);
      writeFileSync(resolve(evidence, `model-${++sequence}.json`), JSON.stringify(request, null, 2));
      const calls = request.messages.flatMap((m) => m.tool_calls ?? []);
      const toolResults = request.messages.filter((m) => m.role === "tool");
      const user = request.messages.filter((m) => m.role === "user").map((m) => JSON.stringify(m.content)).join(" ");
      let name, args;
      let content = "Completed synthetic mailbox operation.";
      if (user.includes("[scenario:send]")) {
        const succeeded = toolResults.some((m) => m.content?.includes('"sent":true'));
        if (!succeeded) {
          name = "mail_send";
          args = { operation_id: "send-once", to: "recipient@example.invalid", body: "Synthetic hello" };
        }
      } else if (!calls.some((c) => c.function.name === "mail_search")) {
        name = "mail_search";
        args = { query: "shipping" };
      } else if (!user.includes("[scenario:read]")) {
        const archiveCalls = calls.filter((c) => c.function.name === "mail_archive");
        if (archiveCalls.length === 0) {
          name = "mail_archive";
          args = { ids: ["shipping-1", "shipping-2"], dry_run: true };
        } else if (archiveCalls.length === 1) {
          const preview = toolResults.map((m) => {
            try { return JSON.parse(m.content); } catch { return null; }
          }).find((r) => r?.preview_token);
          assert(preview, "The real tool must provide the preview token");
          name = "mail_archive";
          args = { ids: preview.ids, dry_run: false, preview_token: preview.preview_token };
        }
      }
      const id = `chatcmpl-${sequence}`;
      const tool_calls = name ? [{ index: 0, id: `call-${sequence}`, type: "function", function: { name, arguments: JSON.stringify(args) } }] : undefined;
      res.writeHead(200, { "content-type": "text/event-stream" });
      const chunk = (delta, finish_reason = null) => res.write(`data: ${JSON.stringify({ id, object: "chat.completion.chunk", created: Math.floor(Date.now() / 1000), model: "scripted-fixture", choices: [{ index: 0, delta, finish_reason }] })}\n\n`);
      chunk(name ? { role: "assistant", tool_calls } : { role: "assistant", content });
      chunk({}, name ? "tool_calls" : "stop");
      res.end("data: [DONE]\n\n");
    } catch (error) {
      console.error(error);
      res.writeHead(500).end(String(error));
    }
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  return server;
}

async function start() {
  // Launch only our own built process; no shared mxr or Ollama process is stopped.
  if (!serverPort) {
    const portServer = createServer();
    await new Promise((r) => portServer.listen(0, "127.0.0.1", r));
    serverPort = portServer.address().port;
    await new Promise((r) => portServer.close(r));
  }
  const port = serverPort;
  host = `http://127.0.0.1:${port}`;
  const env = {
    ...process.env, HOST: "127.0.0.1", PORT: String(port),
    SPIKE_PASSWORD: password, SPIKE_STATE: stateRoot,
    WORKFLOW_LOCAL_DATA_DIR: resolve(evidence, "workflow"),
    WORKFLOW_LOCAL_BASE_URL: host,
    EVE_TELEMETRY_DISABLED: "1", EVE_TRACES: "off", EVE_TRACES_CONTENT: "off", EVE_SCHEDULES_DISABLED: "1",
  };
  if (!localModel) {
    env.SPIKE_MODEL_URL = `http://127.0.0.1:${modelServer.address().port}/v1`;
    env.SPIKE_MODEL = "scripted-fixture";
  }
  child = spawn(process.execPath, [resolve("node_modules/eve/bin/eve.js"), "start", "--host", "127.0.0.1", "--port", String(port), "--no-schedules"], { cwd: runtimeRoot, env, detached: true, stdio: ["ignore", "pipe", "pipe"] });
  const logPath = resolve(evidence, `server-${++starts}.log`);
  const logs = [];
  for (const stream of [child.stdout, child.stderr]) stream.on("data", (b) => {
    logs.push(b.toString());
    writeFileSync(logPath, logs.join(""));
  });
  const began = performance.now();
  await waitUntil(async () => {
    if (child.exitCode !== null) throw new Error(`Server exited: ${logs.join("")}`);
    try { return (await fetch(`${host}/eve/v1/health`)).ok; } catch { return false; }
  }, "server healthy");
  record("startup", { milliseconds: Math.round(performance.now() - began), pid: child.pid });
}
async function stop() {
  if (!child || child.exitCode !== null) return;
  const exited = once(child, "exit");
  process.kill(-child.pid, "SIGKILL");
  await exited;
}
function client() {
  return new Client({ host, auth: { basic: { username: "spike", password } } });
}
async function turn(label, message) {
  const began = performance.now();
  const { session, response } = await client().sessions.create({ message });
  sessions.push(session.state.sessionId);
  const result = await response.result();
  writeFileSync(resolve(evidence, `${label}.json`), JSON.stringify(result, null, 2));
  record(label, { status: result.status, turnCompleted: result.events.some((e) => e.type === "turn.completed"), pendingInputs: result.inputRequests.length, milliseconds: Math.round(performance.now() - began), sessionId: response.sessionId });
  assert.notEqual(result.status, "failed", JSON.stringify(result));
  return { session, result };
}
function approval(result) {
  const request = result.events.filter((e) => e.type === "input.requested").flatMap((e) => e.data.requests).find((r) => r.kind === "tool-approval");
  assert(request, "Must wait on a real runtime approval");
  return request;
}
function completed(result) {
  // 0.83.2 reports status=waiting even after turn.completed, because the session
  // remains open for another user message. The public event is authoritative.
  assert(result.events.some((e) => e.type === "turn.completed"), JSON.stringify(result));
  assert.equal(result.inputRequests.length, 0);
}
async function decide(session, request, optionId) {
  const response = await session.respond([{ requestId: request.requestId, optionId }]);
  let timer;
  const result = await Promise.race([
    response.result(),
    new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`Approval ${optionId} did not settle in 30s`)), 30_000); }),
  ]).finally(() => clearTimeout(timer));
  writeFileSync(resolve(evidence, `${optionId}-${request.requestId}.json`), JSON.stringify(result, null, 2));
  return result;
}

try {
  if (!localModel) modelServer = await scriptedProvider();
  await start();
  const unauthorized = await fetch(`${host}/eve/v1/session`, { method: "POST" });
  assert.equal(unauthorized.status, 401);
  record("unauthenticated-request", { status: unauthorized.status });

  const read = await turn("read", 'Search the exact subject substring "shipping" and summarize the results. Do not change anything. [scenario:read]');
  completed(read.result);
  assert.equal(fixture.load().archiveExecutions.length, 0);
  assert(read.result.events.some((e) => JSON.stringify(e).includes("mail_search")));
  assert(read.result.events.some((e) => e.type === "action.result" && e.data.result.toolName === "mail_search" && e.data.result.output.messages?.length === 2), "Search must return both fixture notices");

  const denied = await turn("denial-pending", 'Search the exact subject substring "shipping", then preview and archive both results. [scenario:archive]');
  assert.equal(denied.result.status, "waiting");
  assert.equal(fixture.load().archiveExecutions.length, 0);
  const denial = await decide(denied.session, approval(denied.result), "cancel");
  completed(denial);
  assert.equal(fixture.load().archiveExecutions.length, 0);
  record("denial", { passed: true, mutations: 0 });

  const ordinary = await turn("ordinary-approval-pending", 'Search the exact subject substring "shipping", then preview and archive both results. [scenario:archive]');
  completed(await decide(ordinary.session, approval(ordinary.result), "approve"));
  assert.deepEqual(fixture.load().archiveExecutions, [["shipping-1", "shipping-2"]]);
  record("ordinary-approval", { passed: true, exactSelection: fixture.load().archiveExecutions });
  fixture.save(fixture.initial());

  if (!smokeOnly) {
  const pending = await turn("archive-pending", 'Search the exact subject substring "shipping", then preview and archive both results. [scenario:archive]');
  const request = approval(pending.result);
  const state = pending.session.state;
  assert.equal(fixture.load().archiveExecutions.length, 0);
  // The input event can reach the client just before the producing step commits.
  // First test the ordinary parked case, not that separate checkpoint race.
  if (!immediateCrash) await delay(1_000);
  const stepDir = resolve(runtimeRoot, ".eve", ".workflow-data", "steps", state.sessionId);
  record("pre-crash-checkpoints", { steps: readdirSync(stepDir).map((f) => {
    const s = JSON.parse(readFileSync(resolve(stepDir, f)));
    return { name: s.stepName, status: s.status };
  }) });
  await stop();
  await start();
  const attached = client().sessions.attach(state.sessionId, { streamIndex: state.streamIndex });
  const afterRestart = await decide(attached, request, "approve");
  completed(afterRestart);
  assert.deepEqual(fixture.load().archiveExecutions, [["shipping-1", "shipping-2"]]);
  assert.equal(fixture.load().messages.find((m) => m.id === "personal-1").archived, false);
  record("approval-after-process-crash", { passed: true, exactSelection: fixture.load().archiveExecutions });

  // Repeat the same approval; it must not run the mutation twice.
  const duplicate = await decide(attached, request, "approve");
  assert.equal(fixture.load().archiveExecutions.length, 1);
  record("duplicate-approval", { status: duplicate.status, mutations: 1 });

  if (!localModel && sendCrash) {
    try {
    const send = await turn("send-pending", "Send one synthetic email. [scenario:send]");
    await send.session.respond([{ requestId: approval(send.result).requestId, optionId: "approve" }]);
    await waitUntil(() => existsSync(resolve(stateRoot, "send-committed")), "synthetic send effect");
    assert.equal(Object.keys(fixture.load().sends).length, 1);
    await stop();
    await start();
    const retry = client().sessions.attach(send.session.state.sessionId);
    // A fresh user turn asks the provider to retry the uncertain operation.
    const response = await retry.send("Check and retry the uncertain send with the SAME operation id. [scenario:send]");
    let timer;
    const result = await Promise.race([
      response.result(),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("Uncertain send did not recover within 30s")), 30_000); }),
    ]).finally(() => clearTimeout(timer));
    writeFileSync(resolve(evidence, "send-recovered.json"), JSON.stringify(result, null, 2));
    if (result.inputRequests.length) await decide(retry, approval(result), "approve");
    assert.equal(Object.keys(fixture.load().sends).length, 1);
    record("send-crash", { passed: true, effects: Object.keys(fixture.load().sends).length, attempts: fixture.load().sendAttempts, protection: "fixture operation ledger, NOT Eve exactly-once" });
    } catch (error) {
      record("send-crash", { passed: false, error: String(error), effects: Object.keys(fixture.load().sends).length, attempts: fixture.load().sendAttempts });
      process.exitCode = 1;
    }
  }
  }

  for (const id of sessions) {
    try { await client().sessions.attach(id).reset({ reason: "Synthetic experiment complete" }); } catch (e) { record("retirement-error", { sessionId: id, error: String(e) }); }
  }
  await delay(750);
  for (const id of sessions) {
    const response = await fetch(`${host}/eve/v1/session/${id}/stream?follow=false`, { headers: { authorization: `Basic ${Buffer.from(`spike:${password}`).toString("base64")}` } });
    const text = await response.text();
    record("retired-session-read", { sessionId: id, httpStatus: response.status, bytes: Buffer.byteLength(text), containsSyntheticBody: text.includes("Synthetic hello"), containsShippingSubject: text.includes("Shipping notice") });
    const run = JSON.parse(readFileSync(resolve(runtimeRoot, ".eve", ".workflow-data", "runs", `${id}.json`)));
    record("retired-run-metadata", { sessionId: id, status: run.status, retainsRunInput: run.input !== undefined, retainsUserPromptTitle: Boolean(run.attributes?.["$eve.title"]) });
  }
  const retained = [];
  function inspectDirectory(directory) {
    if (!existsSync(directory)) return;
    for (const entry of readdirSync(directory)) {
      const path = resolve(directory, entry);
      if (statSync(path).isDirectory()) { inspectDirectory(path); continue; }
      const text = readFileSync(path, "utf8");
      // Workflow JSON also wraps serialized byte payloads in base64.
      const decoded = [text, ...[...text.matchAll(/"data"\s*:\s*"([A-Za-z0-9+/=]+)"/g)].map((m) => Buffer.from(m[1], "base64").toString("utf8"))].join("\n");
      if (decoded.includes("Synthetic hello") || decoded.includes("Shipping notice: parcel")) retained.push(path);
    }
  }
  inspectDirectory(resolve(runtimeRoot, ".eve", ".workflow-data"));
  record("retired-workflow-content-scan", { filesContainingFixtureContent: retained });
  record("complete", { mode: localModel ? "real-local-model" : "scripted-provider-real-eve", evidence });
} catch (error) {
  record("failure", { error: error.stack ?? String(error), evidence });
  process.exitCode = 1;
} finally {
  await stop();
  if (modelServer) {
    modelServer.closeAllConnections();
    await new Promise((r) => modelServer.close(r));
  }
  clearTimeout(deadline);
}
