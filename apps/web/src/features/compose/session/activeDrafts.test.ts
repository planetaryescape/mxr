import { requestCoordinator } from "@/lib/requestCoordinator";
import { observeWorkspace, resetWorkspaceOwnerForTest } from "@/lib/workspaceOwner";
import { beforeEach, expect, test, vi } from "vitest";
import { privateStorage } from "@/test/privateStorage";
import type { ComposeDraftState } from "./composeDraft";
const api = vi.hoisted(() => ({
  rememberComposeBinding: vi.fn<typeof import("../api").rememberComposeBinding>(),
  refreshComposeSession: vi.fn<typeof import("../api").refreshComposeSession>(),
  restoreComposeSession: vi.fn<typeof import("../api").restoreComposeSession>(),
  startComposeSession: vi.fn<typeof import("../api").startComposeSession>(),
}));
vi.mock("../api", () => api);
const { loadInitialComposeSession, rememberActiveDraft, forgetActiveDraft } =
  await import("./activeDrafts");
const intent = { key: "private-a", title: "Private", kind: "new" as const, draftId: "draft-a" };
function draft(): ComposeDraftState {
  return {
    draftId: "draft-a",
    revision: 2,
    draftPath: "/private/a.md",
    accountId: "account-a",
    bodyMarkdown: "private unsent edits\n\n",
    rawContent: "",
    issues: [],
    kind: "new",
    draftIntent: "forward",
    binding: {
      target: "http://127.0.0.1:7777",
      workspace: { workspace_id: "owner-a", recovery_generation: "generation-a" },
    },
    frontmatter: {
      to: "to@example.com",
      cc: "",
      bcc: "",
      subject: "Private",
      from: "from@example.com",
      attach: ["mxr-resource:private/report.txt"],
    },
  };
}
beforeEach(() => {
  resetWorkspaceOwnerForTest();
  vi.stubGlobal("localStorage", privateStorage());
  vi.clearAllMocks();
});
test("reload retains actual private text and original owner without contacting another bridge", async () => {
  const original = draft();
  expect(rememberActiveDraft(intent.key, original)).toBe(true);
  const loaded = await loadInitialComposeSession(intent);
  expect(loaded.session.bodyMarkdown).toBe(original.bodyMarkdown);
  expect(loaded.session.draftIntent).toBe("forward");
  expect(loaded.session.binding).toEqual(original.binding);
  expect(loaded.session.frontmatter.attach).toEqual(original.frontmatter.attach);
  expect(loaded.retainedSnapshot).toBe(true);
  expect(api.startComposeSession).not.toHaveBeenCalled();
  expect(api.restoreComposeSession).not.toHaveBeenCalled();
  expect(api.refreshComposeSession).not.toHaveBeenCalled();
});
test("legacy unowned recovery failure preserves its journal and does not start a new session", async () => {
  const saved = JSON.stringify({ [intent.key]: { draftPath: "/private/legacy.md", updatedAt: 1 } });
  localStorage.setItem("mxr.compose.activeDrafts", saved);
  api.restoreComposeSession.mockResolvedValueOnce({
    session: { ...draft(), draftPath: "/private/b.md", draftId: intent.draftId, previewHtml: "" },
  });
  await expect(loadInitialComposeSession(intent)).rejects.toThrow("no workspace owner");
  expect(api.restoreComposeSession).not.toHaveBeenCalled();
  expect(localStorage.getItem("mxr.compose.activeDrafts")).toBe(saved);
  expect(api.startComposeSession).not.toHaveBeenCalled();
});
test("malformed private journal is never overwritten or deleted for another intent", async () => {
  localStorage.setItem("mxr.compose.activeDrafts", "unreadable private journal");
  expect(rememberActiveDraft("another", draft())).toBe(false);
  expect(forgetActiveDraft(intent.key)).toBe(false);
  await expect(loadInitialComposeSession(intent)).rejects.toThrow("journal was preserved");
  expect(localStorage.getItem("mxr.compose.activeDrafts")).toBe("unreadable private journal");
});
test("storage failure reports unsuccessful retention without erasing editor text", () => {
  const original = draft();
  vi.spyOn(localStorage, "setItem").mockImplementation(() => {
    throw new Error("quota");
  });
  expect(rememberActiveDraft(intent.key, original)).toBe(false);
  expect(original.bodyMarkdown).toBe("private unsent edits\n\n");
});

test("reply entry from another message resumes the matching canonical draft and its unsaved text", async () => {
  const local = { ...draft(), draftIntent: "reply_all" as const };
  observeWorkspace(local.binding!);
  rememberActiveDraft("compose:reply:message-a", local);
  api.startComposeSession.mockResolvedValueOnce({
    session: {
      ...local,
      draftPath: "/private/new-bridge-session.md",
      bodyMarkdown: "older canonical body",
      previewHtml: "",
      binding: local.binding,
      kind: "reply",
    },
  });

  const loaded = await loadInitialComposeSession({
    key: "compose:reply:message-b",
    title: "Reply",
    kind: "reply",
    messageId: "message-b",
  });

  expect(api.startComposeSession).toHaveBeenCalledOnce();
  expect(loaded.session.draftPath).toBe(local.draftPath);
  expect(loaded.session.draftId).toBe(local.draftId);
  expect(loaded.session.draftIntent).toBe("reply_all");
  expect(loaded.session.bodyMarkdown).toBe(local.bodyMarkdown);
  expect(loaded.retainedSnapshot).toBe(true);
});

test("canonical reply journal lookup does not cross workspace owners", async () => {
  const local = draft();
  observeWorkspace(local.binding!);
  rememberActiveDraft("compose:reply:message-a", local);
  observeWorkspace({
    target: local.binding!.target,
    workspace: { workspace_id: "owner-b", recovery_generation: "generation-b" },
  });
  api.startComposeSession.mockResolvedValueOnce({
    session: {
      ...local,
      draftPath: "/private/fresh.md",
      bodyMarkdown: "canonical",
      previewHtml: "",
    },
  });

  const loaded = await loadInitialComposeSession({
    key: "compose:reply:message-b",
    title: "Reply",
    kind: "reply",
    messageId: "message-b",
  });

  expect(loaded.session.draftPath).toBe("/private/fresh.md");
  expect(loaded.session.bodyMarkdown).toBe("canonical");
  expect(loaded.retainedSnapshot).not.toBe(true);
});

test("another owner's explicit reopen retains A and selects its private snapshot on return", async () => {
  const a = draft();
  expect(rememberActiveDraft(intent.key, a)).toBe(true);
  const b = {
    ...draft(),
    draftPath: "/private/b.md",
    bodyMarkdown: "B private text",
    binding: {
      target: a.binding!.target,
      workspace: { workspace_id: "owner-b", recovery_generation: "generation-b" },
    },
  };
  expect(rememberActiveDraft(intent.key, b)).toBe(true);
  observeWorkspace(a.binding!);
  expect((await loadInitialComposeSession(intent)).session.bodyMarkdown).toBe(a.bodyMarkdown);
  observeWorkspace(b.binding);
  expect((await loadInitialComposeSession(intent)).session.bodyMarkdown).toBe(b.bodyMarkdown);
  expect(api.restoreComposeSession).not.toHaveBeenCalled();
});
test("a malformed entry prevents replacing other private journal records", () => {
  const raw = JSON.stringify({
    damaged: { draftPath: "/private/damaged", snapshot: { bodyMarkdown: "keep me" } },
  });
  localStorage.setItem("mxr.compose.activeDrafts", raw);
  expect(rememberActiveDraft(intent.key, draft())).toBe(false);
  expect(localStorage.getItem("mxr.compose.activeDrafts")).toBe(raw);
});

test("finishing retained A removes A only, leaving B private work recoverable", async () => {
  const a = draft();
  const b = {
    ...draft(),
    bodyMarkdown: "B private text",
    binding: {
      target: a.binding!.target,
      workspace: { workspace_id: "owner-b", recovery_generation: "generation-b" },
    },
  };
  rememberActiveDraft(intent.key, a);
  rememberActiveDraft(intent.key, b);
  expect(forgetActiveDraft(intent.key, a.binding)).toBe(true);
  observeWorkspace(b.binding);
  expect((await loadInitialComposeSession(intent)).session.bodyMarkdown).toBe(b.bodyMarkdown);
});

test("deliberate canonical reopen permits review while keeping the unowned private reference", async () => {
  const saved = JSON.stringify({ [intent.key]: { draftPath: "/private/legacy.md", updatedAt: 1 } });
  localStorage.setItem("mxr.compose.activeDrafts", saved);
  api.restoreComposeSession.mockResolvedValueOnce({ session: { ...draft(), previewHtml: "" } });
  await loadInitialComposeSession(intent, true);
  expect(api.restoreComposeSession).toHaveBeenCalledExactlyOnceWith(intent.draftId);
  expect(localStorage.getItem("mxr.compose.activeDrafts")).toBe(saved);
});

test("owner transition while an original save settles keeps private text without refreshing", async () => {
  const a = {
    ...draft(),
    recoverySourcePath: "/private/source.md",
    binding: { ...draft().binding!, target: window.location.origin },
  };
  observeWorkspace(a.binding);
  rememberActiveDraft(intent.key, a);
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const saving = requestCoordinator.queueComposeLatest("/private/source.md", () => held);
  const loading = loadInitialComposeSession(intent);
  observeWorkspace({
    target: window.location.origin,
    workspace: { workspace_id: "owner-b", recovery_generation: "generation-b" },
  });
  release();
  await saving;
  expect((await loading).session.bodyMarkdown).toBe(a.bodyMarkdown);
  expect(api.refreshComposeSession).not.toHaveBeenCalled();
});

test("same-count different server attachment selection cannot replace retained private references", async () => {
  const current = {
    ...draft(),
    recoverySourcePath: "/private/source.md",
    binding: { ...draft().binding!, target: window.location.origin },
    frontmatter: { ...draft().frontmatter, attach: ["mxr-resource:new-resource/new.txt"] },
  };
  observeWorkspace(current.binding);
  rememberActiveDraft(intent.key, current);
  api.refreshComposeSession.mockImplementationOnce(async (_path, expectedAttachments) => {
    expect(expectedAttachments).toEqual(current.frontmatter.attach);
    throw new Error(
      "compose_attachment_conflict: persisted recovery still references old-resource/old.txt",
    );
  });
  const reopened = await loadInitialComposeSession(intent);
  expect(reopened.session.frontmatter.attach).toEqual(current.frontmatter.attach);
  expect(reopened.session.bodyMarkdown).toBe(current.bodyMarkdown);
  expect(reopened.session.revision).toBe(current.revision);
  expect(reopened.retainedSnapshot).toBe(true);
});

test("returning to an earlier message uses the latest canonical journal text", async () => {
  const first = { ...draft(), draftIntent: "reply" as const };
  const second = { ...first, revision: 3, bodyMarkdown: "latest text from message B" };
  const a = { key: "compose:reply:A", title: "Reply", kind: "reply" as const, messageId: "A" };
  rememberActiveDraft(a.key, first);
  rememberActiveDraft("compose:reply:B", first);
  rememberActiveDraft("compose:reply:B", second);
  const reopened = await loadInitialComposeSession(a);
  expect(reopened.session.bodyMarkdown).toBe(second.bodyMarkdown);
  expect(reopened.session.revision).toBe(3);
});
