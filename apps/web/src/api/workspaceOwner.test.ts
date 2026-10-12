import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { api, apiFetch } from "./client";
import { observeWorkspace, resetWorkspaceOwnerForTest, selectedWorkspace, WorkspaceConflictError } from "@/lib/workspaceOwner";
import { WebRequestCoordinator } from "@/lib/requestCoordinator";
import { tryLocalHandshake } from "@/lib/localHandshake";
const fixture = vi.hoisted(() => ({ target: "http://127.0.0.1:7777" }));
vi.mock("@/lib/tokenStorage", () => ({ getBridgeBaseUrl: () => fixture.target, getToken: () => "fixture-token", clearToken: vi.fn() }));
vi.mock("@/lib/localHandshake", () => ({ tryLocalHandshake: vi.fn() }));
const a = { workspace_id: "workspace-a", recovery_generation: "generation-a" };
const b = { workspace_id: "workspace-b", recovery_generation: "generation-b" };
function deferred<T>() {
  let resolve: (value: T) => void = () => { throw new Error("not initialized"); };
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}
function response(owner = a) {
  return new Response("{}", { headers: { "content-type": "application/json", "x-mxr-workspace": JSON.stringify(owner) } });
}
beforeEach(() => { fixture.target = "http://127.0.0.1:7777"; resetWorkspaceOwnerForTest(); vi.clearAllMocks(); });
afterEach(() => { vi.unstubAllGlobals(); });

test("late same-target A read cannot replace selected B", async () => {
  observeWorkspace({ target: fixture.target, workspace: a });
  const late = deferred<Response>();
  vi.stubGlobal("fetch", vi.fn(() => late.promise));
  const pending = apiFetch("/api/v1/mail/accounts").catch((error: unknown) => error);
  await Promise.resolve();
  observeWorkspace({ target: fixture.target, workspace: b });
  late.resolve(response(a));
  expect(await pending).toBeInstanceOf(WorkspaceConflictError);
  expect(selectedWorkspace()?.workspace).toEqual(b);
});

test.each(["raw", "typed"])("%s auth retry sends no stale body after target transition", async (kind) => {
  observeWorkspace({ target: fixture.target, workspace: a });
  const handshake = deferred<string>();
  vi.mocked(tryLocalHandshake).mockReturnValueOnce(handshake.promise);
  const fetch = vi.fn(async () => new Response("{}", { status: 401 }));
  vi.stubGlobal("fetch", fetch);
  const pending = (kind === "raw" ? apiFetch("/api/v1/mail/compose/session/update", { method: "POST", body: { body: "private A text" }, binding: { target: fixture.target, workspace: a } }) : api.GET("/api/v1/platform/accounts", { fetch })).catch((error: unknown) => error);
  await vi.waitFor(() => expect(tryLocalHandshake).toHaveBeenCalledOnce());
  fixture.target = "http://127.0.0.1:8888";
  observeWorkspace({ target: fixture.target, workspace: b });
  handshake.resolve("replacement-token");
  expect(await pending).toBeInstanceOf(WorkspaceConflictError);
  expect(fetch).toHaveBeenCalledOnce();
});

test("queued ordinary mutation does not invoke its callback after owner transition", async () => {
  observeWorkspace({ target: fixture.target, workspace: a });
  const coordinator = new WebRequestCoordinator();
  const held = deferred<void>();
  const started = vi.fn(() => held.promise);
  const first = coordinator.enqueueMutation(started);
  await vi.waitFor(() => expect(started).toHaveBeenCalledOnce());
  const mutate = vi.fn(async () => "applied");
  const pending = coordinator.enqueueMutation(mutate).catch((error: unknown) => error);
  observeWorkspace({ target: fixture.target, workspace: b });
  held.resolve();
  await first;
  expect(await pending).toBeInstanceOf(WorkspaceConflictError);
  expect(mutate).not.toHaveBeenCalled();
});

test("compose execution guard runs before private callback", async () => {
  const coordinator = new WebRequestCoordinator();
  const write = vi.fn(async () => "saved");
  await expect(coordinator.queueComposeLatest("private", write, () => { throw new WorkspaceConflictError(); })).rejects.toBeInstanceOf(WorkspaceConflictError);
  expect(write).not.toHaveBeenCalled();
});
