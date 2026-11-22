import { observeWorkspace, resetWorkspaceOwnerForTest } from "@/lib/workspaceOwner";
/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { beforeEach, expect, test, vi } from "vitest";
import { BridgeRequestError } from "@/api/client";
import type { ComposeSessionResponse } from "../api";
import { loadInitialComposeSession, rememberActiveDraft } from "./activeDrafts";
import { draftFingerprint, draftFromSession, type ComposeDraftState } from "./composeDraft";
import { useComposeAutosave } from "./useComposeAutosave";
import { useDraftSaveActions } from "./useDraftSaveActions";
import { useComposeSend } from "./useComposeSend";

const api = vi.hoisted(() => ({
  rememberComposeBinding: vi.fn<typeof import("../api").rememberComposeBinding>(),
  updateComposeSession: vi.fn<typeof import("../api").updateComposeSession>(),
  saveComposeSession: vi.fn<typeof import("../api").saveComposeSession>(),
  refreshComposeSession: vi.fn<typeof import("../api").refreshComposeSession>(),
  restoreComposeSession: vi.fn<typeof import("../api").restoreComposeSession>(),
  scheduleComposeSession: vi.fn<typeof import("../api").scheduleComposeSession>(),
}));
vi.mock("../api", () => api);
function originalBinding() {
  return {
    target: window.location.origin,
    workspace: {
      workspace_id: "00000000-0000-7000-8000-000000000001",
      recovery_generation: "00000000-0000-7000-8000-000000000002",
    },
  };
}
function session(path: string, revision: number, body: string): ComposeSessionResponse {
  return {
    session: {
      binding: originalBinding(),
      draftPath: path,
      draftId: path,
      revision,
      accountId: "account",
      rawContent: body,
      bodyMarkdown: body,
      previewHtml: "",
      issues: [],
      frontmatter: {
        to: "alice@example.com",
        cc: "",
        bcc: "",
        subject: "Restored",
        from: "alias@example.com",
        attach: [],
      },
    },
  };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((accept, refuse) => {
    resolve = accept;
    reject = refuse;
  });
  return { promise, resolve, reject };
}
function setup(path: string) {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return renderHook(
    () => {
      const [draft, setDraft] = useState<ComposeDraftState | null>(
        draftFromSession(session(path, 1, " original whitespace \n").session),
      );
      const draftRef = useRef(draft);
      draftRef.current = draft;
      const [dirty, setDirty] = useState(false);
      const autosave = useComposeAutosave({
        intentKey: path,
        queryClient: client,
        draft,
        draftRef,
        setDraft,
        dirty,
        setDirty,
      });
      useEffect(() => {
        if (draftRef.current)
          autosave.lastSavedFingerprintRef.current = draftFingerprint(draftRef.current);
      }, [autosave.lastSavedFingerprintRef]);
      const intent = { key: path, kind: "new" as const, title: "Restored" };
      const saves = useDraftSaveActions({ intent, draftRef, setDraft, setDirty, ...autosave });
      const send = useComposeSend({
        intent,
        options: {},
        navigate: async () => {},
        queryClient: client,
        draftRef,
        setDraft,
        ...autosave,
        markSendAttempted: () => {},
        onValidationBlocked: () => {},
      });
      return {
        ...autosave,
        ...saves,
        scheduleSend: send.scheduleSend,
        draft,
        draftRef,
        setDraft,
        dirty,
        setDirty,
      };
    },
    { wrapper },
  );
}
beforeEach(() => {
  resetWorkspaceOwnerForTest();
  observeWorkspace(originalBinding());
  window.localStorage.clear();
  for (const mock of Object.values(api)) mock.mockReset();
});

test("an explicit provider save adopts its canonical token before a queued autosave", async () => {
  const saved = deferred<Awaited<ReturnType<typeof import("../api").saveComposeSession>>>();
  api.saveComposeSession.mockReturnValueOnce(saved.promise);
  api.updateComposeSession.mockResolvedValue(session("/tmp/provider-save", 3, "latest text"));
  const hook = setup("/tmp/provider-save");
  let server!: Promise<void>;
  act(() => {
    server = hook.result.current.handleServerSaveClick();
  });
  await waitFor(() => expect(api.saveComposeSession).toHaveBeenCalledTimes(1));
  expect(api.updateComposeSession).not.toHaveBeenCalled();
  act(() =>
    hook.result.current.setDraft((draft) =>
      draft ? { ...draft, bodyMarkdown: "latest text" } : draft,
    ),
  );
  let autosaving!: ReturnType<typeof hook.result.current.saveCurrentDraft>;
  act(() => {
    autosaving = hook.result.current.saveCurrentDraft();
  });
  await act(async () => {
    saved.resolve({ ok: true, draft_id: "/tmp/provider-save", revision: 2 });
    await server;
    await autosaving;
  });
  expect(api.updateComposeSession.mock.calls[0]?.[0]).toMatchObject({
    expectedRevision: 2,
    body: "latest text",
  });
  expect(hook.result.current.draft?.revision).toBe(3);
  expect(hook.result.current.draft?.bodyMarkdown).toBe("latest text");
  hook.result.current.markSessionFinished();
  hook.unmount();
});

test("an explicit provider save finishing after a switch cannot change the new session", async () => {
  const saved = deferred<Awaited<ReturnType<typeof import("../api").saveComposeSession>>>();
  api.saveComposeSession.mockReturnValueOnce(saved.promise);
  const hook = setup("/tmp/provider-old");
  let saving!: Promise<void>;
  act(() => {
    saving = hook.result.current.handleServerSaveClick();
  });
  await waitFor(() => expect(api.saveComposeSession).toHaveBeenCalledTimes(1));
  act(() =>
    hook.result.current.setDraft(
      draftFromSession(session("/tmp/provider-new", 7, "new text").session),
    ),
  );
  await act(async () => {
    saved.resolve({ ok: true, draft_id: "/tmp/provider-old", revision: 2 });
    await saving;
  });
  expect(hook.result.current.draft).toMatchObject({
    draftId: "/tmp/provider-new",
    revision: 7,
    bodyMarkdown: "new text",
  });
  hook.result.current.markSessionFinished();
  hook.unmount();
});

test("unmount flushes text typed after an in-flight conflict into the reopening recovery session", async () => {
  const path = "/tmp/shared-unmount";
  const first = deferred<ComposeSessionResponse>();
  api.updateComposeSession.mockReturnValueOnce(first.promise);
  api.updateComposeSession.mockImplementationOnce(async (request) => {
    expect(request).toMatchObject({
      draftPath: "/tmp/unmount-first-recovery",
      expectedRevision: 1,
      body: "last unmounted text",
    });
    return {
      session: {
        binding: originalBinding(),
        ...session(request.draftPath, 1, request.body ?? "").session,
        draftId: path,
        recoverySourcePath: path,
      },
    };
  });
  api.refreshComposeSession.mockImplementation(async (target) => {
    expect(target).toBe("/tmp/unmount-first-recovery");
    return { session: { ...session(target, 1, "last unmounted text").session, draftId: path } };
  });
  const hook = setup(path);
  const openDraft = hook.result.current.draft;
  if (openDraft) rememberActiveDraft(path, openDraft);
  act(() =>
    hook.result.current.setDraft((draft) =>
      draft ? { ...draft, bodyMarkdown: "first unmounted edit" } : draft,
    ),
  );
  let saving!: Promise<unknown>;
  act(() => {
    saving = hook.result.current.saveCurrentDraft().catch(() => {});
  });
  await waitFor(() => expect(api.updateComposeSession).toHaveBeenCalledTimes(1));
  act(() =>
    hook.result.current.setDraft((draft) =>
      draft ? { ...draft, bodyMarkdown: "last unmounted text" } : draft,
    ),
  );
  hook.unmount();
  const reopened = loadInitialComposeSession({ key: path, title: "After close", kind: "new" });
  first.reject(
    new BridgeRequestError(409, "first conflict", "draft_revision_conflict", {
      recovery_session: { draft_path: "/tmp/unmount-first-recovery", draft_id: path, revision: 1 },
    }),
  );
  await saving;
  expect((await reopened).session).toMatchObject({
    draftId: path,
    revision: 1,
    bodyMarkdown: "last unmounted text",
  });
  expect(api.updateComposeSession).toHaveBeenCalledTimes(2);
});
