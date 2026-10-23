const { mkdtempSync, readFileSync, rmSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join } = require("node:path");
const { after, test } = require("node:test");
const assert = require("node:assert/strict");
const Module = require("node:module");
const ts = require("typescript");
const dataDirectory = mkdtempSync(join(tmpdir(), "mxr-raycast-tests-"));
after(() => rmSync(dataDirectory, { recursive: true, force: true }));

require.extensions[".ts"] = (loaded, filename) => {
  const source = readFileSync(filename, "utf8");
  const { outputText } = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
      esModuleInterop: true,
    },
    fileName: filename,
  });
  loaded._compile(outputText, filename);
};

const loadModule = Module._load;
Module._load = function (request, parent, isMain) {
  if (request === "@raycast/api") return { getPreferenceValues: () => ({ dataDirectory }) };
  return loadModule.call(this, request, parent, isMain);
};

const { assistReply } = require("../src/bridge/compose.ts");
const { proposalMarkdown } = require("../src/components/proofMarkdown.ts");
const {
  joinTextRegion,
  splitTextRegion,
} = require("../../web/src/features/compose/assist/textRegion.ts");
const {
  hasUnfilledGap,
  proposalStillOwnsReply,
  requestIsCurrent,
  shouldDiscardReply,
} = require("../src/components/proposalLifecycle.ts");

const workspace = { workspace_id: "ws-1", recovery_generation: "gen-1" };
const replyDraft = {
  workspace,
  session: {
    draftPath: "/tmp/reply.md",
    accountId: "account-1",
    frontmatter: {
      to: "pat@example.test",
      cc: "",
      bcc: "",
      subject: "Re: Hi",
      from: "me@example.test",
      attach: [],
    },
    revision: 2,
    bodyMarkdown: "",
  },
};

function fakeBridgeFetch(capture) {
  return async (input, init = {}) => {
    const url = new URL(input);
    if (url.pathname === "/api/v1/auth/local-token") {
      return Response.json({ token: "test-token" });
    }
    if (url.pathname === "/api/v1/mail/compose/assist") {
      capture({
        url: url.pathname,
        headers: new Headers(init.headers),
        body: JSON.parse(init.body),
      });
      return Response.json({ proposal: { proposal_id: "p-1" } });
    }
    throw new Error(`Unexpected fake bridge request: ${url.pathname}`);
  };
}

test("assistReply sends the exact reply target, text, and workspace owner", async () => {
  const originalFetch = global.fetch;
  let request;
  global.fetch = fakeBridgeFetch((value) => (request = value));
  try {
    await assistReply(replyDraft, "message-1", "reply", "Thursday works.");
    assert.equal(request.url, "/api/v1/mail/compose/assist");
    assert.deepEqual(request.body, {
      draft: { kind: "reply", message_id: "message-1", all: false },
      text: "Thursday works.",
    });
    assert.equal(request.headers.get("x-mxr-workspace"), JSON.stringify(workspace));
    assert.equal(request.headers.get("authorization"), "Bearer test-token");
  } finally {
    global.fetch = originalFetch;
  }
});

test("assistReply preserves reply-all, original text, action, displayed proposal and note", async () => {
  const originalFetch = global.fetch;
  let request;
  global.fetch = fakeBridgeFetch((value) => (request = value));
  try {
    await assistReply(replyDraft, "message-2", "reply_all", "My original words", {
      action: "polish",
      revise: "shown-proposal",
      instruction: "Keep the first sentence.",
    });
    assert.deepEqual(request.body, {
      draft: { kind: "reply", message_id: "message-2", all: true },
      text: "My original words",
      action: "polish",
      revise: "shown-proposal",
      instruction: "Keep the first sentence.",
    });
    assert.equal(request.headers.get("x-mxr-workspace"), JSON.stringify(workspace));
  } finally {
    global.fetch = originalFetch;
  }
});

test("proposal proof marks edits and gap, escaping proposal markdown", () => {
  const proposal = {
    text: "Meet [[?: `when`]] tomorrow **please**",
    changes: [
      { kind: "kept", text: "Meet " },
      { kind: "removed", text: "soon" },
      { kind: "added", text: "[[?: `when`]]" },
      { kind: "kept", text: " tomorrow " },
      { kind: "added", text: "**please**" },
    ],
    gaps: [{ label: "when", start: 5, end: 18 }],
  };
  assert.equal(
    proposalMarkdown(proposal),
    "Meet ~~soon~~``[[?: `when`]]`` tomorrow **\\*\\*please\\*\\***",
  );
});

test("proposal markdown escapes list and link syntax on every line", () => {
  assert.equal(
    proposalMarkdown({
      text: "safe\n- [open](https://mail.example)",
      changes: [{ kind: "added", text: "safe\n- [open](https://mail.example)" }],
      gaps: [],
    }),
    "**safe\n\\- \\[open\\](https://mail.example)**",
  );
});

test("accept replaces only the user's text and keeps the signature region", () => {
  const body = "Thursday works.\n\n-- \nBK";
  const accepted = joinTextRegion(splitTextRegion(body), "Thursday works by [[?: when]]");
  assert.equal(accepted.body, "Thursday works by [[?: when]]\n\n-- \nBK");
});

test("proposal text and workspace must still belong to the same open reply", () => {
  const owner = replyDraft;
  const valid = {
    closed: false,
    currentDraft: owner,
    owner,
    currentWorkspace: JSON.stringify(workspace),
    workspace: JSON.stringify(workspace),
    currentBody: "mine",
    baseline: "mine",
  };
  assert.equal(proposalStillOwnsReply(valid), true);
  assert.equal(proposalStillOwnsReply({ ...valid, currentBody: "edited while waiting" }), false);
  assert.equal(proposalStillOwnsReply({ ...valid, currentWorkspace: "other" }), false);
  assert.equal(proposalStillOwnsReply({ ...valid, currentDraft: { ...owner } }), false);
  assert.equal(proposalStillOwnsReply({ ...valid, closed: true }), false);
});

test("closing a proof keeps its reply session and ignores a late request result", async () => {
  assert.equal(shouldDiscardReply(false, true), false);
  assert.equal(shouldDiscardReply(false, false), true);
  assert.equal(shouldDiscardReply(true, false), false);
  assert.equal(requestIsCurrent(3, 3), true);
  assert.equal(requestIsCurrent(3, 4), false);

  let resolve;
  const pending = new Promise((done) => (resolve = done));
  let generation = 3;
  const result = pending.then((proposal) =>
    requestIsCurrent(3, generation) ? proposal : undefined,
  );
  generation += 1;
  resolve("stale proposal");
  assert.equal(await result, undefined);
});

test("a gap from the daemon safety check cannot be overridden in the reply form", () => {
  assert.equal(
    hasUnfilledGap({ issues: [{ code: "unfilled_gap", override_token: "not-usable" }] }),
    true,
  );
  assert.equal(hasUnfilledGap({ issues: [{ code: "wrong_recipient" }] }), false);
});
