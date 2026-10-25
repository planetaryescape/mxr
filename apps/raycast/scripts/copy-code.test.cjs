const { mkdtempSync, readFileSync, rmSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join } = require("node:path");
const { after, beforeEach, test } = require("node:test");
const assert = require("node:assert/strict");
const Module = require("node:module");
const ts = require("typescript");
const dataDirectory = mkdtempSync(join(tmpdir(), "mxr-copy-code-tests-"));
const originalFetch = global.fetch;
const copies = [];
const hud = [];
after(() => {
  global.fetch = originalFetch;
  rmSync(dataDirectory, { recursive: true, force: true });
});
beforeEach(() => {
  copies.length = 0;
  hud.length = 0;
});
require.extensions[".ts"] = (loaded, filename) => {
  const { outputText } = ts.transpileModule(readFileSync(filename, "utf8"), {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
      esModuleInterop: true,
    },
    fileName: filename,
  });
  loaded._compile(outputText, filename);
};
const originalLoad = Module._load;
Module._load = function (request, parent, isMain) {
  if (request === "@raycast/api")
    return {
      getPreferenceValues: () => ({ dataDirectory }),
      Clipboard: { copy: async (...args) => copies.push(args) },
      showHUD: async (text) => hud.push(text),
    };
  return originalLoad.call(this, request, parent, isMain);
};
const copyLatestCode = require("../src/copy-code.ts").default;
const account = { account_id: "work", account_name: "Work", result: "started" };
function bridge(handler) {
  global.fetch = async (input, init = {}) => {
    const url = new URL(input);
    if (url.pathname === "/api/v1/auth/local-token") return Response.json({ token: "test-token" });
    return handler(url, init);
  };
}
test("copy waits for started and joined syncs before querying and copying a newly arrived code", async () => {
  const calls = [];
  let passes = 0;
  bridge((url, init) => {
    calls.push(url.pathname);
    if (url.pathname === "/api/v1/mail/sync/all") {
      assert.equal(init.method, "POST");
      return Response.json({
        results: [account, { ...account, account_id: "personal", result: "joined" }],
      });
    }
    if (url.pathname === "/api/v1/mail/sync/status") {
      if (url.searchParams.get("account_id") === "work") passes++;
      return Response.json({ sync: { sync_in_progress: passes < 2, last_error: null } });
    }
    assert.equal(url.pathname, "/api/v1/mail/codes");
    assert.equal(passes, 2, "code lookup must wait for sync completion");
    return Response.json({
      codes: [
        { current: true, code: "482913", sender_name: "Example", sender_host: "example.test" },
      ],
    });
  });
  await copyLatestCode();
  assert.equal(calls[0], "/api/v1/mail/sync/all");
  assert.equal(calls.at(-1), "/api/v1/mail/codes");
  assert.deepEqual(copies, [["482913", { concealed: true }]]);
  assert.match(hud.at(-1), /Copied 482913/);
});
test("a sync startup failure leaves the clipboard unchanged and never searches cached codes", async () => {
  bridge((url) => {
    assert.equal(url.pathname, "/api/v1/mail/sync/all");
    return Response.json({ results: [{ ...account, result: "failed", error: "Sign in again" }] });
  });
  await copyLatestCode();
  assert.deepEqual(copies, []);
  assert.match(hud.at(-1), /Could not refresh mail: Work: Sign in again/);
});
test("a failed completed sync does not fall back to cached codes", async () => {
  bridge((url) => {
    if (url.pathname === "/api/v1/mail/sync/all") return Response.json({ results: [account] });
    assert.equal(url.pathname, "/api/v1/mail/sync/status");
    return Response.json({ sync: { sync_in_progress: false, last_error: "Connection failed" } });
  });
  await copyLatestCode();
  assert.deepEqual(copies, []);
  assert.match(hud.at(-1), /Connection failed/);
});
test("a refresh that stays busy times out without copying an old code", async (t) => {
  let clock = 0;
  t.mock.method(Date, "now", () => (clock++ === 0 ? 0 : 30_001));
  bridge((url) => {
    if (url.pathname === "/api/v1/mail/sync/all") return Response.json({ results: [account] });
    assert.equal(url.pathname, "/api/v1/mail/sync/status");
    return Response.json({ sync: { sync_in_progress: true, last_error: null } });
  });
  await copyLatestCode();
  assert.deepEqual(copies, []);
  assert.match(hud.at(-1), /still syncing/);
});

test("no syncable accounts does not search historical codes", async () => {
  bridge((url) => {
    assert.equal(url.pathname, "/api/v1/mail/sync/all");
    return Response.json({ results: [] });
  });
  await copyLatestCode();
  assert.deepEqual(copies, []);
  assert.match(hud.at(-1), /No enabled mail accounts/);
});
