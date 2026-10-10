/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { useRef, useState, type ReactNode } from "react";
import { beforeEach, expect, test, vi } from "vitest";
import type { ComposeSessionResponse } from "../api";
import { draftFromSession, type ComposeDraftState } from "./composeDraft";
import { useComposeAutosave } from "./useComposeAutosave";

const update = vi.hoisted(() => vi.fn<typeof import("../api").updateComposeSession>());
vi.mock("../api", () => ({ updateComposeSession: update }));

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
      frontmatter: {
        to: "alice@example.com",
        cc: "",
        bcc: "",
        subject: "test",
        from: "me@example.com",
        attach: [],
      },
      issues: [],
    },
  };
}
function deferred() {
  let resolve!: (value: ComposeSessionResponse) => void;
  const promise = new Promise<ComposeSessionResponse>((accept) => {
    resolve = accept;
  });
  return { promise, resolve };
}
function setup(path: string) {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return renderHook(
    () => {
      const [draft, setDraft] = useState<ComposeDraftState | null>(
        draftFromSession(session(path, 1, "first")),
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
      return { ...autosave, draft, draftRef, setDraft };
    },
    { wrapper },
  );
}

beforeEach(() => {
  update.mockReset();
});

test("queued own saves advance the token even when the first UI result is stale", async () => {
  const first = deferred();
  const second = deferred();
  update.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
  const hook = setup("/tmp/overlapping-draft.md");
  let savingFirst!: ReturnType<typeof hook.result.current.saveCurrentDraft>;
  act(() => {
    savingFirst = hook.result.current.saveCurrentDraft();
  });
  await waitFor(() => expect(update).toHaveBeenCalledTimes(1));
  act(() =>
    hook.result.current.setDraft((current) =>
      current ? { ...current, bodyMarkdown: "latest text" } : current,
    ),
  );
  let savingSecond!: ReturnType<typeof hook.result.current.saveCurrentDraft>;
  act(() => {
    savingSecond = hook.result.current.saveCurrentDraft();
  });
  await act(async () => first.resolve(session("/tmp/overlapping-draft.md", 2, "first")));
  await waitFor(() => expect(update).toHaveBeenCalledTimes(2));
  expect(update.mock.calls[1][0].expectedRevision).toBe(2);
  expect(update.mock.calls[1][0].body).toBe("latest text");
  await act(async () => {
    second.resolve(session("/tmp/overlapping-draft.md", 3, "latest text"));
    await Promise.all([savingFirst, savingSecond]);
  });
  expect(hook.result.current.draftRef.current?.revision).toBe(3);
  expect(hook.result.current.draft?.bodyMarkdown).toBe("latest text");
  hook.result.current.markSessionFinished();
  hook.unmount();
});

test("a save finishing after a session switch cannot copy identity or revision to the new draft", async () => {
  const first = deferred();
  update.mockReturnValueOnce(first.promise);
  const hook = setup("/tmp/old-session.md");
  let saving!: ReturnType<typeof hook.result.current.saveCurrentDraft>;
  act(() => {
    saving = hook.result.current.saveCurrentDraft();
  });
  await waitFor(() => expect(update).toHaveBeenCalledTimes(1));
  act(() =>
    hook.result.current.setDraft(
      draftFromSession(session("/tmp/new-session.md", 7, "new session text")),
    ),
  );
  await act(async () => {
    first.resolve(session("/tmp/old-session.md", 2, "first"));
    await saving;
  });
  expect(hook.result.current.draftRef.current?.draftId).toBe("/tmp/new-session.md");
  expect(hook.result.current.draftRef.current?.revision).toBe(7);
  expect(hook.result.current.draft?.bodyMarkdown).toBe("new session text");
  hook.result.current.markSessionFinished();
  hook.unmount();
});
