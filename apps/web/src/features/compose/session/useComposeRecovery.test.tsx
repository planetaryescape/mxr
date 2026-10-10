/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import { useRef, useState, type ReactNode } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { BridgeRequestError, DaemonUnavailableError } from "@/api/client";
import type { ComposeSessionResponse } from "../api";
import { draftFromSession, type ComposeDraftState } from "./composeDraft";
import { useComposeAutosave } from "./useComposeAutosave";

const availability = vi.hoisted(() => ({ down: false }));
vi.mock("@/lib/daemonAvailability", () => ({ useDaemonDown: () => availability.down }));
const update = vi.hoisted(() => vi.fn<typeof import("../api").updateComposeSession>());
vi.mock("../api", () => ({ updateComposeSession: update }));

function saved(body: string): ComposeSessionResponse {
  return {
    session: {
      draftPath: "/tmp/timer-recovery",
      draftId: "timer-draft",
      revision: 1,
      recoverySourcePath: "/tmp/timer-shared",
      accountId: "account",
      rawContent: body,
      frontmatter: {
        to: "alice@example.com",
        cc: "",
        bcc: "",
        subject: "Timer",
        from: "me@example.com",
        attach: [],
      },
      bodyMarkdown: body,
      previewHtml: "",
      issues: [],
    },
  };
}

function setup() {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return renderHook(
    () => {
      const [draft, setDraft] = useState<ComposeDraftState | null>(() => ({
        ...draftFromSession(saved("losing text").session),
        draftPath: "/tmp/timer-shared",
        recoverySourcePath: undefined,
      }));
      const draftRef = useRef(draft);
      draftRef.current = draft;
      const [dirty, setDirty] = useState(true);
      const autosave = useComposeAutosave({
        intentKey: "timer",
        queryClient: client,
        draft,
        draftRef,
        setDraft,
        dirty,
        setDirty,
      });
      if (!draft) throw new Error("The recovery test always has an editor draft");
      return { ...autosave, draft, dirty, setDraft, setDirty };
    },
    { wrapper },
  );
}

afterEach(() => {
  availability.down = false;
  vi.useRealTimers();
  update.mockReset();
  window.localStorage.clear();
});

for (const disconnected of [false, true]) {
  test(`unchanged confirmed recovery stops autosave across two more windows (connection lost: ${disconnected})`, async () => {
    vi.useFakeTimers();
    const conflict = new BridgeRequestError(
      409,
      "Draft changed; text preserved",
      "draft_revision_conflict",
      {
        details: { current_revision: 2 },
        recovery_session: {
          draft_path: "/tmp/timer-recovery",
          draft_id: "timer-draft",
          revision: 1,
        },
      },
    );
    // Every attempted canonical save would return a real-shaped recovery response.
    update.mockRejectedValue(
      disconnected ? new DaemonUnavailableError({ cause: conflict }) : conflict,
    );
    const hook = setup();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3100);
    });
    expect(update).toHaveBeenCalledTimes(1);
    expect(hook.result.current.draft.recoverySourcePath).toBe("/tmp/timer-shared");
    expect(hook.result.current.dirty).toBe(false);
    expect(hook.result.current.isCurrentDraftSaved(hook.result.current.draft)).toBe(false);
    expect(hook.result.current.saveError).toContain("preserved");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(7000);
    });
    expect(update).toHaveBeenCalledTimes(1);
    update.mockImplementation(async (request) => saved(request.body ?? ""));
    act(() => {
      hook.result.current.setDraft((draft) =>
        draft ? { ...draft, bodyMarkdown: "latest before close" } : draft,
      );
      hook.result.current.setDirty(true);
    });
    await act(async () => {
      await hook.result.current.saveCurrentDraft();
    });
    expect(update).toHaveBeenLastCalledWith(
      expect.objectContaining({
        draftPath: "/tmp/timer-recovery",
        expectedRevision: 1,
        body: "latest before close",
      }),
    );
    expect(hook.result.current.dirty).toBe(false);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(7000);
      await hook.result.current.saveCurrentDraft();
    });
    expect(update).toHaveBeenCalledTimes(2);
    expect(hook.result.current.draft.bodyMarkdown).toBe("latest before close");
    hook.unmount();
    expect(update).toHaveBeenCalledTimes(2);
  });
}

test("an unconfirmed lost response stays dirty without an automatic request loop", async () => {
  vi.useFakeTimers();
  update.mockRejectedValue(new DaemonUnavailableError());
  const hook = setup();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(11000);
  });
  expect(update).toHaveBeenCalledTimes(1);
  expect(hook.result.current.dirty).toBe(true);
  expect(hook.result.current.draft.revision).toBe(1);
  hook.result.current.markSessionFinished();
  hook.unmount();
});

test("an observed daemon recovery retries the pending text once", async () => {
  vi.useFakeTimers();
  update.mockRejectedValueOnce(new DaemonUnavailableError());
  const hook = setup();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(3100);
  });
  expect(update).toHaveBeenCalledTimes(1);
  availability.down = true;
  hook.rerender();
  update.mockResolvedValue({
    session: {
      ...saved("losing text").session,
      draftPath: "/tmp/timer-shared",
      recoverySourcePath: null,
      revision: 2,
    },
  });
  availability.down = false;
  hook.rerender();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(11000);
  });
  expect(update).toHaveBeenCalledTimes(2);
  expect(hook.result.current.dirty).toBe(false);
  expect(hook.result.current.draft.revision).toBe(2);
  hook.unmount();
});
