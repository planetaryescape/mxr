/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { useRef, useState, type ReactNode } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { BridgeRequestError } from "@/api/client";
import type { ComposeSessionResponse } from "../api";
import { loadInitialComposeSession, rememberActiveDraft } from "./activeDrafts";
import { draftFromSession, type ComposeDraftState } from "./composeDraft";
import { useDraftSaveActions } from "./useDraftSaveActions";
import { useComposeAutosave } from "./useComposeAutosave";

const api = vi.hoisted(() => ({
  updateComposeSession: vi.fn<typeof import("../api").updateComposeSession>(),
  refreshComposeSession: vi.fn<typeof import("../api").refreshComposeSession>(),
  restoreComposeSession: vi.fn<typeof import("../api").restoreComposeSession>(),
}));
vi.mock("../api", () => api);
const toasts = vi.hoisted(() => ({
  error: vi.fn<(message: string, options?: unknown) => void>(),
  success: vi.fn<(message: string) => void>(),
}));
vi.mock("sonner", () => ({ toast: toasts }));

function session(path: string, body: string, attach: string[]): ComposeSessionResponse {
  return {
    session: {
      draftPath: path,
      draftId: "attachment-draft",
      revision: 1,
      recoverySourcePath:
        path === "/tmp/attachments-shared" ? undefined : "/tmp/attachments-shared",
      accountId: "account",
      bodyMarkdown: body,
      rawContent: body,
      previewHtml: "",
      issues: [],
      frontmatter: {
        to: "alice@example.com",
        cc: "",
        bcc: "",
        subject: "Attachments",
        from: "me@example.com",
        attach,
      },
    },
  };
}

function setup(unsaved = false) {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return renderHook(
    () => {
      const [draft, setDraft] = useState<ComposeDraftState | null>(() => ({
        ...draftFromSession(
          session("/tmp/attachments-shared", "first text", ["/uploads/source-a"]).session,
        ),
        revision: unsaved ? null : 1,
      }));
      const draftRef = useRef(draft);
      draftRef.current = draft;
      const [dirty, setDirty] = useState(true);
      const autosave = useComposeAutosave({
        intentKey: "attachments",
        queryClient: client,
        draft,
        draftRef,
        setDraft,
        dirty,
        setDirty,
      });
      const saves = useDraftSaveActions({
        intent: { key: "attachments", kind: "new", title: "Attachments" },
        draftRef,
        setDraft,
        setDirty,
        ...autosave,
      });
      return { ...autosave, ...saves, draft, dirty, setDraft };
    },
    { wrapper },
  );
}

afterEach(() => {
  Object.values(api).forEach((mock) => mock.mockReset());
  Object.values(toasts).forEach((mock) => mock.mockReset());
  window.localStorage.clear();
});

for (const unmount of [false, true]) {
  test(`a delayed conflict maps attachment receipts before the latest ${unmount ? "unmount" : "Close"} snapshot saves`, async () => {
    let rejectFirst!: (error: Error) => void;
    api.updateComposeSession.mockReturnValueOnce(
      new Promise((_, reject) => {
        rejectFirst = reject;
      }),
    );
    api.updateComposeSession.mockImplementationOnce(async (input) => {
      expect(input).toMatchObject({
        draftPath: "/tmp/attachments-recovery",
        expectedRevision: 1,
        body: "last typed text",
        frontmatter: { attach: ["/uploads/private-a", "/uploads/source-b"] },
      });
      return session(input.draftPath, input.body, ["/uploads/private-a", "/uploads/private-b"]);
    });
    const hook = setup();
    if (!hook.result.current.draft) throw new Error("Expected the initial editor");
    rememberActiveDraft("attachments", hook.result.current.draft);
    let first!: ReturnType<typeof hook.result.current.saveCurrentDraft>;
    act(() => {
      first = hook.result.current.saveCurrentDraft();
    });
    await waitFor(() => expect(api.updateComposeSession).toHaveBeenCalledTimes(1));
    act(() =>
      hook.result.current.setDraft((draft) =>
        draft
          ? {
              ...draft,
              bodyMarkdown: "last typed text",
              frontmatter: {
                ...draft.frontmatter,
                attach: [...draft.frontmatter.attach, "/uploads/source-b"],
              },
            }
          : draft,
      ),
    );
    let closing: ReturnType<typeof hook.result.current.saveCurrentDraft> | undefined;
    if (unmount) hook.unmount();
    else
      act(() => {
        closing = hook.result.current.saveCurrentDraft();
      });
    await act(async () => {
      rejectFirst(
        new BridgeRequestError(409, "Draft revision conflict", "draft_conflict", {
          recovery_session: {
            draft_path: "/tmp/attachments-recovery",
            draft_id: "attachment-draft",
            revision: 1,
            attachment_paths: ["/uploads/private-a"],
          },
        }),
      );
      await first;
      await closing;
    });
    await waitFor(() => expect(api.updateComposeSession).toHaveBeenCalledTimes(2));
    if (!unmount) {
      hook.result.current.markSessionFinished();
      hook.unmount();
    }
    api.refreshComposeSession.mockImplementation(async (path) => {
      expect(path).toBe("/tmp/attachments-recovery");
      return session("/tmp/attachments-reopened-copy", "last typed text", [
        "/uploads/reopened-a",
        "/uploads/reopened-b",
      ]);
    });
    const reopened = await loadInitialComposeSession({
      key: "attachments",
      kind: "new",
      draftId: "attachment-draft",
      title: "Attachments",
    });
    expect(reopened.session).toMatchObject({
      revision: 1,
      bodyMarkdown: "last typed text",
      frontmatter: { attach: ["/uploads/reopened-a", "/uploads/reopened-b"] },
    });
    expect(api.restoreComposeSession).not.toHaveBeenCalled();
  });
}

for (const conflict of [false, true]) {
  test(`explicit Save retries a saved recovery at its observed revision (intervening writer: ${conflict})`, async () => {
    api.updateComposeSession.mockRejectedValueOnce(
      new BridgeRequestError(503, "daemon unavailable", "internal", {
        recovery_session: {
          draft_path: "/tmp/saved-recovery",
          draft_id: "attachment-draft",
          revision: 1,
          attachment_paths: ["/uploads/preserved-a"],
        },
      }),
    );
    const hook = setup();
    await act(async () => {
      await hook.result.current.saveCurrentDraft();
    });
    act(() =>
      hook.result.current.setDraft((draft) =>
        draft ? { ...draft, bodyMarkdown: "latest private text" } : draft,
      ),
    );
    api.updateComposeSession.mockImplementationOnce(async (input) => {
      expect(input).toMatchObject({
        draftPath: "/tmp/saved-recovery",
        expectedRevision: 1,
        body: "latest private text",
        frontmatter: { attach: ["/uploads/preserved-a"] },
      });
      expect(input.retryRecovery).toBeUndefined();
      return session(input.draftPath, input.body, input.frontmatter.attach);
    });
    api.updateComposeSession.mockImplementationOnce(async (input) => {
      expect(input).toMatchObject({
        retryRecovery: true,
        draftPath: "/tmp/saved-recovery",
        expectedRevision: 1,
        body: "latest private text",
        frontmatter: { attach: ["/uploads/preserved-a"] },
      });
      if (conflict)
        throw new BridgeRequestError(409, "Draft revision conflict", "draft_conflict", {
          current_revision: 2,
        });
      return {
        session: {
          ...session("/tmp/accepted-retry", input.body, ["/uploads/canonical-a"]).session,
          revision: 2,
          recoverySourcePath: undefined,
        },
      };
    });
    await act(async () => {
      await hook.result.current.handleSaveClick();
    });
    expect(api.updateComposeSession).toHaveBeenCalledTimes(3);
    expect(hook.result.current.draft).toMatchObject(
      conflict
        ? {
            draftPath: "/tmp/saved-recovery",
            revision: 1,
            recoverySourcePath: "/tmp/attachments-shared",
            bodyMarkdown: "latest private text",
            frontmatter: { attach: ["/uploads/preserved-a"] },
          }
        : {
            draftPath: "/tmp/accepted-retry",
            revision: 2,
            preservedRecoveryPath: "/tmp/saved-recovery",
            bodyMarkdown: "latest private text",
            frontmatter: { attach: ["/uploads/canonical-a"] },
          },
    );
    expect(toasts.success.mock.calls).toEqual(conflict ? [] : [["Draft saved locally"]]);
    expect(toasts.error.mock.calls).toEqual(
      conflict ? [["Save failed", { description: "Draft revision conflict" }]] : [],
    );
    expect(api.restoreComposeSession).not.toHaveBeenCalled();
    hook.result.current.markSessionFinished();
    hook.unmount();
  });
}

test("a conflict without this snapshot's attachment evidence cannot adopt its recovery path", async () => {
  const failure = new BridgeRequestError(409, "Draft revision conflict", "draft_conflict", {
    recovery_session: {
      draft_path: "/tmp/unconfirmed-recovery",
      draft_id: "attachment-draft",
      revision: 1,
    },
  });
  api.updateComposeSession.mockRejectedValue(failure);
  const hook = setup();
  await act(async () => {
    await expect(hook.result.current.saveCurrentDraft()).rejects.toBe(failure);
  });
  expect(hook.result.current.draft).toMatchObject({
    draftPath: "/tmp/attachments-shared",
    revision: 1,
    bodyMarkdown: "first text",
    frontmatter: { attach: ["/uploads/source-a"] },
  });
  expect(hook.result.current.dirty).toBe(true);
  hook.result.current.markSessionFinished();
  hook.unmount();
});

for (const conflict of [false, true]) {
  test(`only explicit Save retries an unsaved recovery (canonical create conflicted: ${conflict})`, async () => {
    const failedCreate = new BridgeRequestError(502, "connection lost", "internal", {
      recovery_session: {
        draft_path: "/tmp/unsaved-recovery",
        draft_id: "attachment-draft",
        revision: null,
        attachment_paths: ["/uploads/preserved-a"],
      },
    });
    api.updateComposeSession.mockRejectedValueOnce(failedCreate);
    const hook = setup(true);
    await act(async () => {
      await hook.result.current.saveCurrentDraft();
    });
    expect(hook.result.current.draft).toMatchObject({
      draftPath: "/tmp/unsaved-recovery",
      revision: null,
    });
    expect(api.updateComposeSession.mock.calls[0]?.[0].retryRecovery).toBeUndefined();
    api.updateComposeSession.mockImplementationOnce(async (input) => {
      expect(input).toMatchObject({
        retryRecovery: true,
        draftPath: "/tmp/unsaved-recovery",
        expectedRevision: null,
        body: "first text",
        frontmatter: { attach: ["/uploads/preserved-a"] },
      });
      if (conflict)
        throw new BridgeRequestError(409, "Draft revision conflict", "draft_conflict", {
          current_revision: 1,
        });
      const canonical = session("/tmp/retry-canonical", input.body, ["/uploads/canonical-a"]);
      return { session: { ...canonical.session, revision: 1, recoverySourcePath: undefined } };
    });
    await act(async () => {
      await hook.result.current.handleSaveClick();
    });
    expect(api.updateComposeSession).toHaveBeenCalledTimes(2);
    const expected = conflict
      ? {
          draftPath: "/tmp/unsaved-recovery",
          revision: null,
          bodyMarkdown: "first text",
          frontmatter: { attach: ["/uploads/preserved-a"] },
        }
      : {
          draftPath: "/tmp/retry-canonical",
          revision: 1,
          preservedRecoveryPath: "/tmp/unsaved-recovery",
          bodyMarkdown: "first text",
          frontmatter: { attach: ["/uploads/canonical-a"] },
        };
    expect(hook.result.current.draft).toMatchObject(expected);
    expect(hook.result.current.draft?.recoverySourcePath).toBe(
      conflict ? "/tmp/attachments-shared" : undefined,
    );
    expect(hook.result.current.dirty).toBe(false);
    expect(api.restoreComposeSession).not.toHaveBeenCalled();
    hook.result.current.markSessionFinished();
    hook.unmount();
  });
}

test("typing during an explicit recovery retry keeps the latest private text and original token", async () => {
  api.updateComposeSession.mockRejectedValueOnce(
    new BridgeRequestError(503, "daemon unavailable", "internal", {
      recovery_session: {
        draft_path: "/tmp/typing-recovery",
        draft_id: "attachment-draft",
        revision: 1,
        attachment_paths: ["/uploads/preserved-a"],
      },
    }),
  );
  const hook = setup();
  await act(async () => {
    await hook.result.current.saveCurrentDraft();
  });
  let accept!: (response: ComposeSessionResponse) => void;
  api.updateComposeSession.mockReturnValueOnce(
    new Promise((resolve) => {
      accept = resolve;
    }),
  );
  let saving!: Promise<void>;
  act(() => {
    saving = hook.result.current.handleSaveClick();
  });
  await waitFor(() => expect(api.updateComposeSession).toHaveBeenCalledTimes(2));
  act(() =>
    hook.result.current.setDraft((draft) =>
      draft ? { ...draft, bodyMarkdown: "typed during retry" } : draft,
    ),
  );
  await act(async () => {
    accept({
      session: {
        ...session("/tmp/accepted-before-typing", "first text", ["/uploads/canonical-a"]).session,
        revision: 2,
        recoverySourcePath: undefined,
      },
    });
    await saving;
  });
  expect(hook.result.current.draft).toMatchObject({
    draftPath: "/tmp/typing-recovery",
    revision: 1,
    bodyMarkdown: "typed during retry",
    frontmatter: { attach: ["/uploads/preserved-a"] },
  });
  expect(toasts.success).not.toHaveBeenCalled();
  api.updateComposeSession.mockImplementationOnce(async (input) =>
    session(input.draftPath, input.body, input.frontmatter.attach),
  );
  await act(async () => {
    await hook.result.current.saveCurrentDraft();
  });
  expect(api.updateComposeSession.mock.calls[2]?.[0]).toMatchObject({
    draftPath: "/tmp/typing-recovery",
    expectedRevision: 1,
    body: "typed during retry",
  });
  expect(api.updateComposeSession.mock.calls[2]?.[0].retryRecovery).toBeUndefined();
  hook.result.current.markSessionFinished();
  hook.unmount();
});

test("a normal Save does not call newer in-flight typing saved or preserved", async () => {
  let accept!: (response: ComposeSessionResponse) => void;
  api.updateComposeSession.mockReturnValueOnce(
    new Promise((resolve) => {
      accept = resolve;
    }),
  );
  const hook = setup();
  let saving!: Promise<void>;
  act(() => {
    saving = hook.result.current.handleSaveClick();
  });
  await waitFor(() => expect(api.updateComposeSession).toHaveBeenCalledTimes(1));
  act(() =>
    hook.result.current.setDraft((draft) =>
      draft ? { ...draft, bodyMarkdown: "newer unsaved typing" } : draft,
    ),
  );
  await act(async () => {
    accept({
      session: {
        ...session("/tmp/attachments-shared", "first text", ["/uploads/source-a"]).session,
        revision: 2,
      },
    });
    await saving;
  });
  expect(hook.result.current.draft?.bodyMarkdown).toBe("newer unsaved typing");
  expect(toasts.success).not.toHaveBeenCalled();
  expect(toasts.error).toHaveBeenCalledWith("Latest edits are not saved", {
    description: "Save again to save the current edits.",
  });
  hook.result.current.markSessionFinished();
  hook.unmount();
});
