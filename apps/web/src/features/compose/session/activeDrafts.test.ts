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
