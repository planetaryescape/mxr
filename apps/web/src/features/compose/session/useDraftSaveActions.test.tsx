/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { beforeEach, expect, test, vi } from "vitest";
import { BridgeRequestError, DaemonUnavailableError } from "@/api/client";
import type { ComposeSessionResponse } from "../api";
import { loadInitialComposeSession, rememberActiveDraft } from "./activeDrafts";
import { draftFingerprint, draftFromSession, type ComposeDraftState } from "./composeDraft";
import { useComposeAutosave } from "./useComposeAutosave";
import { useDraftSaveActions } from "./useDraftSaveActions";
import { useComposeSend } from "./useComposeSend";

const api = vi.hoisted(() => ({
  updateComposeSession: vi.fn<typeof import("../api").updateComposeSession>(),
  saveComposeSession: vi.fn<typeof import("../api").saveComposeSession>(),
  refreshComposeSession: vi.fn<typeof import("../api").refreshComposeSession>(),
  restoreComposeSession: vi.fn<typeof import("../api").restoreComposeSession>(),
  scheduleComposeSession: vi.fn<typeof import("../api").scheduleComposeSession>(),
}));
vi.mock("../api", () => api);
function session(path: string, revision: number, body: string): ComposeSessionResponse {
  return {
    session: {
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
  expect(api.updateComposeSession).not.toHaveBeenCalled(); // Restored fingerprint makes the initial flush a no-op.
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

test("failed scheduling adopts the accepted save token before an edit retries", async () => {
  const scheduled = deferred<Awaited<ReturnType<typeof import("../api").scheduleComposeSession>>>();
  api.scheduleComposeSession.mockReturnValueOnce(scheduled.promise);
  api.refreshComposeSession.mockResolvedValue(
    session("/tmp/schedule-failed", 2, "original whitespace"),
  );
  api.updateComposeSession.mockResolvedValue(session("/tmp/schedule-failed", 3, "retry text"));
  const hook = setup("/tmp/schedule-failed");
  let scheduling!: Promise<void>;
  act(() => {
    scheduling = hook.result.current.scheduleSend(new Date("2027-01-01T09:00:00Z"));
  });
  await waitFor(() => expect(api.scheduleComposeSession).toHaveBeenCalledTimes(1));
  act(() =>
    hook.result.current.setDraft((draft) =>
      draft ? { ...draft, bodyMarkdown: "retry text" } : draft,
    ),
  );
  let saving!: ReturnType<typeof hook.result.current.saveCurrentDraft>;
  act(() => {
    saving = hook.result.current.saveCurrentDraft();
  });
  await act(async () => {
    scheduled.reject(
      new DaemonUnavailableError({
        cause: new BridgeRequestError(502, "schedule unavailable", "connect", {
          accepted_draft: { draft_id: "/tmp/schedule-failed", revision: 2 },
        }),
      }),
    );
    await scheduling;
    await saving;
  });
  expect(api.updateComposeSession.mock.calls[0]?.[0]).toMatchObject({
    expectedRevision: 2,
    body: "retry text",
  });
  expect(hook.result.current.draft).toMatchObject({ revision: 3, bodyMarkdown: "retry text" });
  expect(api.scheduleComposeSession).toHaveBeenCalledTimes(1);
  hook.result.current.markSessionFinished();
  hook.unmount();
});

for (const operation of ["provider", "schedule"] as const) {
  for (const failure of ["conflict", "network", "wrong identity"] as const) {
    test(`${operation} ${failure} keeps the observed token despite a newer shared sidecar`, async () => {
      const path = `/tmp/${operation}-${failure}`;
      const error =
        failure === "network"
          ? new DaemonUnavailableError({ cause: new TypeError("network failed") })
          : new BridgeRequestError(409, "another editor won", "draft_revision_conflict", {
              details: { current_revision: 8 },
              ...(failure === "wrong identity"
                ? { accepted_draft: { draft_id: "/tmp/other", revision: 8 } }
                : {}),
            });
      api.refreshComposeSession.mockResolvedValue(session(path, 8, "another tab's text"));
      if (operation === "provider") api.saveComposeSession.mockRejectedValue(error);
      else api.scheduleComposeSession.mockRejectedValue(error);
      const hook = setup(path);
      await act(async () => {
        if (operation === "provider") await hook.result.current.handleServerSaveClick();
        else await hook.result.current.scheduleSend(new Date("2027-01-01T09:00:00Z"));
      });
      expect(api.refreshComposeSession).not.toHaveBeenCalled();
      expect(hook.result.current.draft).toMatchObject({
        revision: 1,
        bodyMarkdown: " original whitespace \n",
      });
      api.updateComposeSession.mockRejectedValue(error);
      act(() =>
        hook.result.current.setDraft((draft) =>
          draft ? { ...draft, bodyMarkdown: "retained retry" } : draft,
        ),
      );
      await act(async () => {
        await hook.result.current.saveCurrentDraft().catch(() => {});
      });
      expect(api.updateComposeSession.mock.calls[0]?.[0]).toMatchObject({
        expectedRevision: 1,
        body: "retained retry",
      });
      expect(hook.result.current.draft).toMatchObject({
        revision: 1,
        bodyMarkdown: "retained retry",
      });
      hook.result.current.markSessionFinished();
      hook.unmount();
    });
  }
}

test("a provider failure adopts its own accepted revision when another tab advances further", async () => {
  const path = "/tmp/provider-partial";
  api.saveComposeSession.mockRejectedValue(
    new BridgeRequestError(409, "provider conflict", "draft_revision_conflict", {
      accepted_draft: { draft_id: path, revision: 2 },
      details: { current_revision: 8 },
    }),
  );
  api.refreshComposeSession.mockResolvedValue(session(path, 8, "other tab"));
  const hook = setup(path);
  await act(async () => {
    await hook.result.current.handleServerSaveClick();
  });
  expect(hook.result.current.draft?.revision).toBe(2);
  expect(api.refreshComposeSession).not.toHaveBeenCalled();
  hook.result.current.markSessionFinished();
  hook.unmount();
});

for (const stored of [false, true]) {
  for (const latestText of ["last typed before close", " original whitespace \n"]) {
    test(`${stored ? "stored" : "new"} queued close reopens recovery text ${JSON.stringify(latestText)} at the observed token`, async () => {
      const path = "/tmp/shared-close";
      const firstRecovery = "/tmp/shared-close-recovery-1";
      const lastRecovery = firstRecovery;
      const first = deferred<ComposeSessionResponse>();
      const persisted = new Map([[path, session(path, 2, "winning tab text")]]);
      api.updateComposeSession.mockReturnValueOnce(first.promise);
      api.updateComposeSession.mockImplementationOnce(async (request) => {
        expect(request).toMatchObject({
          draftPath: firstRecovery,
          expectedRevision: 1,
          body: latestText,
        });
        const recovered = session(lastRecovery, 1, request.body ?? "");
        recovered.session.draftId = path;
        persisted.set(lastRecovery, recovered);
        recovered.session.recoverySourcePath = path;
        return recovered;
      });
      api.refreshComposeSession.mockImplementation(async (target) => {
        const restored = persisted.get(target);
        if (!restored) throw new Error("missing recovery file");
        return restored;
      });
      const hook = setup(path);
      const openDraft = hook.result.current.draft;
      expect(openDraft).not.toBeNull();
      if (openDraft) rememberActiveDraft(path, openDraft);
      act(() => {
        hook.result.current.setDraft((draft) =>
          draft ? { ...draft, bodyMarkdown: "first losing edit" } : draft,
        );
        hook.result.current.setDirty(true);
      });
      let saving!: Promise<unknown>;
      act(() => {
        saving = hook.result.current.saveCurrentDraft().catch(() => {});
      });
      await waitFor(() => expect(api.updateComposeSession).toHaveBeenCalledTimes(1));
      act(() =>
        hook.result.current.setDraft((draft) =>
          draft ? { ...draft, bodyMarkdown: latestText } : draft,
        ),
      );
      let closing!: Promise<unknown>;
      act(() => {
        closing = hook.result.current.saveCurrentDraft().catch(() => {});
      });
      const reopening = loadInitialComposeSession({
        key: path,
        title: "Reopened",
        kind: "new",
        ...(stored ? { draftId: path } : {}),
      });
      await act(async () => {
        const recovered = session(firstRecovery, 1, "first losing edit");
        recovered.session.draftId = path;
        persisted.set(firstRecovery, recovered);
        first.reject(
          new BridgeRequestError(409, "first stale edit", "draft_revision_conflict", {
            details: { current_revision: 2 },
            recovery_session: { draft_path: firstRecovery, draft_id: path, revision: 1 },
          }),
        );
        await saving;
        await closing;
      });
      const resumed = await reopening;
      expect(api.restoreComposeSession).not.toHaveBeenCalled();
      expect(resumed.session).toMatchObject({
        draftPath: lastRecovery,
        draftId: path,
        revision: 1,
        bodyMarkdown: latestText,
      });
      expect(hook.result.current.draft).toMatchObject({
        draftPath: lastRecovery,
        revision: 1,
        bodyMarkdown: latestText,
      });
      expect(hook.result.current.dirty).toBe(false);
      expect(hook.result.current.saveError).toBe("first stale edit");
      expect(persisted.get(path)?.session).toMatchObject({
        revision: 2,
        bodyMarkdown: "winning tab text",
      });
      await act(async () => {
        await hook.result.current.saveCurrentDraft();
      });
      expect(api.updateComposeSession).toHaveBeenCalledTimes(2);
      const local = hook.result.current.draft;
      if (!local) throw new Error("Expected preserved editor draft");
      expect(hook.result.current.isCurrentDraftSaved(local)).toBe(false);
      expect(persisted.get(path)?.session).toMatchObject({
        revision: 2,
        bodyMarkdown: "winning tab text",
      });
      hook.result.current.markSessionFinished();
      hook.unmount();
    });
  }
}

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

for (const invalid of [
  { draft_id: "/tmp/someone-else", revision: 1 },
  { draft_id: "/tmp/invalid-recovery", revision: 8 },
]) {
  test(`cannot adopt another operation's recovery ${JSON.stringify(invalid)}`, async () => {
    const path = "/tmp/invalid-recovery";
    api.updateComposeSession.mockRejectedValue(
      new BridgeRequestError(409, "conflict", "draft_revision_conflict", {
        recovery_session: { draft_path: "/tmp/unowned-recovery", ...invalid },
      }),
    );
    const hook = setup(path);
    act(() =>
      hook.result.current.setDraft((draft) =>
        draft ? { ...draft, bodyMarkdown: "retained" } : draft,
      ),
    );
    await act(async () => {
      await hook.result.current.saveCurrentDraft().catch(() => {});
    });
    expect(hook.result.current.draft).toMatchObject({
      draftPath: path,
      revision: 1,
      bodyMarkdown: "retained",
    });
    hook.result.current.markSessionFinished();
    hook.unmount();
  });
}
