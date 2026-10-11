/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { useRef, useState, type ReactNode } from "react";
import { afterEach, expect, test, vi } from "vitest";
import type { ComposeSessionResponse } from "../api";
import { draftFromSession, type ComposeDraftState } from "./composeDraft";
import { useDraftSaveActions } from "./useDraftSaveActions";

const api = vi.hoisted(() => ({
  refreshComposeSession: vi.fn<typeof import("../api").refreshComposeSession>(),
  restoreComposeSession: vi.fn<typeof import("../api").restoreComposeSession>(),
  saveComposeSession: vi.fn<typeof import("../api").saveComposeSession>(),
}));
vi.mock("../api", () => api);

function session(recovery: boolean): ComposeSessionResponse {
  return {
    session: {
      draftPath: recovery ? "/tmp/action-recovery" : "/tmp/action-canonical",
      draftId: "action-draft",
      revision: recovery ? 1 : 2,
      recoverySourcePath: recovery ? "/tmp/action-source" : null,
      accountId: "account",
      rawContent: "",
      bodyMarkdown: recovery ? "rejected edits" : "winner text",
      previewHtml: "",
      issues: [],
      frontmatter: {
        to: "alice@example.com",
        cc: "",
        bcc: "",
        subject: "Action",
        from: "me@example.com",
        attach: [],
      },
    },
  };
}

function setup(recovery: boolean) {
  const client = new QueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return renderHook(
    () => {
      const [draft, setDraft] = useState<ComposeDraftState | null>(() => ({
        ...draftFromSession(session(recovery).session),
        preservedRecoveryPath: recovery ? undefined : "/tmp/action-recovery",
      }));
      const draftRef = useRef(draft);
      draftRef.current = draft;
      const actions = useDraftSaveActions({
        intent: { key: "actions", kind: "new", title: "Actions" },
        draftRef,
        setDraft,
        setDirty: () => {},
        saveCurrentDraft: async () => {},
        isCurrentDraftSaved: () => true,
        setSaveError: () => {},
        setLastSavedAt: () => {},
        lastSavedFingerprintRef: useRef<string | null>(null),
      });
      return { ...actions, draft, setDraft };
    },
    { wrapper },
  );
}

afterEach(() => {
  Object.values(api).forEach((mock) => mock.mockReset());
  window.localStorage.clear();
});

for (const canonical of [false, true]) {
  for (const switchIdentity of [false, true]) {
    test(`${canonical ? "canonical" : "preserved"} reopen cannot replace ${switchIdentity ? "another draft identity" : "typing during the response"}`, async () => {
      let resolve!: (value: ComposeSessionResponse) => void;
      const pending = new Promise<ComposeSessionResponse>((accept) => {
        resolve = accept;
      });
      const request = canonical ? api.restoreComposeSession : api.refreshComposeSession;
      request.mockReturnValue(pending);
      const hook = setup(canonical);
      let opening!: Promise<void>;
      act(() => {
        opening = canonical
          ? hook.result.current.handleRefreshClick()
          : hook.result.current.handleRecoveryClick();
      });
      await waitFor(() => expect(request).toHaveBeenCalledTimes(1));
      act(() =>
        hook.result.current.setDraft((draft) =>
          draft
            ? {
                ...draft,
                ...(switchIdentity
                  ? { draftId: "another-draft" }
                  : { bodyMarkdown: "newer typed text" }),
              }
            : draft,
        ),
      );
      await act(async () => {
        resolve(session(!canonical));
        await opening;
      });
      expect(hook.result.current.draft).toMatchObject(
        switchIdentity ? { draftId: "another-draft" } : { bodyMarkdown: "newer typed text" },
      );
      expect(hook.result.current.draft?.draftPath).toBe(
        canonical ? "/tmp/action-recovery" : "/tmp/action-canonical",
      );
      hook.unmount();
    });
  }
}
