import { beforeEach, expect, test, vi } from "vitest";
import { privateStorage } from "@/test/privateStorage";
import type { ComposeDraftState } from "./composeDraft";
const api = vi.hoisted(() => ({ rememberComposeBinding: vi.fn(), refreshComposeSession: vi.fn(), restoreComposeSession: vi.fn(), startComposeSession: vi.fn() }));
vi.mock("../api", () => api);
const { loadInitialComposeSession, rememberActiveDraft, forgetActiveDraft } = await import("./activeDrafts");
const intent = { key: "private-a", title: "Private", kind: "new" as const, draftId: "draft-a" };
function draft(): ComposeDraftState {
  return { draftId: "draft-a", revision: 2, draftPath: "/private/a.md", accountId: "account-a", bodyMarkdown: "private unsent edits\n\n", rawContent: "", issues: [], kind: "new",
    binding: { target: "http://127.0.0.1:7777", workspace: { workspace_id: "owner-a", recovery_generation: "generation-a" } },
    frontmatter: { to: "to@example.com", cc: "", bcc: "", subject: "Private", from: "from@example.com", attach: ["mxr-resource:private/report.txt"] } };
}
beforeEach(() => { vi.stubGlobal("localStorage", privateStorage()); vi.clearAllMocks(); });
test("reload retains actual private text and original owner without contacting another bridge", async () => {
  const original = draft();
  expect(rememberActiveDraft(intent.key, original)).toBe(true);
  const loaded = await loadInitialComposeSession(intent);
  expect(loaded.session.bodyMarkdown).toBe(original.bodyMarkdown);
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
  api.restoreComposeSession.mockRejectedValueOnce(new Error("Legacy owner missing"));
  await expect(loadInitialComposeSession(intent)).rejects.toThrow("Legacy owner missing");
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
  vi.spyOn(localStorage, "setItem").mockImplementation(() => { throw new Error("quota"); });
  expect(rememberActiveDraft(intent.key, original)).toBe(false);
  expect(original.bodyMarkdown).toBe("private unsent edits\n\n");
});
