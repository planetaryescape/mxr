import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, test, vi, beforeEach } from "vitest";
import { useState, useRef, type ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import type { ComposeDraftState, ComposeIntent } from "./composeDraft";
import { useDraftAssist } from "./useDraftAssist";

const apiFetch = vi.hoisted(() => vi.fn<(path: string, init?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", () => ({ apiFetch }));
vi.mock("sonner", () => ({
  toast: { error: vi.fn<() => void>(), success: vi.fn<() => void>(), info: vi.fn<() => void>() },
}));

const REPLY: ComposeIntent = { key: "k", title: "Reply", kind: "reply", messageId: "m1" };

function draftWith(body: string): ComposeDraftState {
  return {
    draftPath: "",
    rawContent: "",
    frontmatter: {
      to: "pat@example.com",
      cc: "",
      bcc: "",
      subject: "Thursday",
      from: "",
      attach: [],
    },
    bodyMarkdown: body,
    issues: [],
    accountId: "acc",
    kind: "reply",
  };
}

function useHarness(initial: ComposeDraftState, intent: ComposeIntent = REPLY) {
  const [draft, setDraft] = useState<ComposeDraftState | null>(initial);
  const [, setDirty] = useState(false);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const assist = useDraftAssist({ intent, draftRef, setDraft, setDirty });
  return { draft, setDraft, assist };
}

function proposal(text: string, changes: { kind: "kept" | "added" | "removed"; text: string }[]) {
  return {
    proposal: {
      proposal_id: "p1",
      action: "finish",
      base: "",
      text,
      changes,
      gaps: [],
      provenance: null,
      yours_changed: [],
      composer_version: 1,
    },
  };
}

const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } });

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

beforeEach(() => {
  apiFetch.mockReset();
  queryClient.clear();
});

describe("useDraftAssist", () => {
  test("the request carries the text without the signature and UTF-8 selection offsets", async () => {
    apiFetch.mockResolvedValue(proposal("café au lait", [{ kind: "kept", text: "café au lait" }]));
    const { result } = renderHook(() => useHarness(draftWith("café au lait\n\n-- \nBK")), {
      wrapper,
    });
    const body = "café au lait\n\n-- \nBK";
    act(() =>
      result.current.assist.setSelection({ from: body.indexOf("au"), to: body.indexOf("au") + 2 }),
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(1));
    const [path, init] = apiFetch.mock.calls[0] as [string, { body: Record<string, unknown> }];
    expect(path).toBe("/api/v1/mail/compose/assist");
    expect(init.body).toMatchObject({
      draft: { kind: "reply", message_id: "m1", all: false },
      text: "café au lait",
      selection: { start: 6, end: 8 },
      action: "edit_selection",
      adjust: { shorter: false, warmer: false, more_formal: false, proofread: false },
    });
    expect(init.body).not.toHaveProperty("instruction");
  });

  test("accepting writes the proof text back and keeps the signature; ⌘Z restores the text in one step", async () => {
    const before = "Thursday works.\n\n-- \nBK";
    apiFetch.mockResolvedValue(
      proposal("Thursday works. Any time after 2.", [
        { kind: "kept", text: "Thursday works." },
        { kind: "added", text: " Any time after 2." },
      ]),
    );
    const { result } = renderHook(() => useHarness(draftWith(before)), { wrapper });
    act(() => result.current.assist.submit());
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));

    act(() => result.current.assist.accept());
    const accepted = "Thursday works. Any time after 2.\n\n-- \nBK";
    expect(result.current.draft?.bodyMarkdown).toBe(accepted);
    expect(result.current.assist.proofOpen).toBe(false);
    expect(result.current.assist.canUndoAccept).toBe(true);

    // Only the model's added words are marked.
    const [span] = result.current.draft?.modelSpans ?? [];
    expect(span && accepted.slice(span.start, span.end)).toBe(" Any time after 2.");

    act(() => {
      expect(result.current.assist.undoAccept()).toBe(true);
    });
    expect(result.current.draft?.bodyMarkdown).toBe(before);
    expect(result.current.draft?.modelSpans ?? []).toEqual([]);
    expect(result.current.assist.canUndoAccept).toBe(false);
  });

  test("Esc keeps the user's text and never changes the body", async () => {
    const before = "Thursday works.";
    apiFetch.mockResolvedValue(
      proposal("Thursday works. Friday too.", [
        { kind: "kept", text: "Thursday works." },
        { kind: "added", text: " Friday too." },
      ]),
    );
    const { result } = renderHook(() => useHarness(draftWith(before)), { wrapper });
    act(() => result.current.assist.submit());
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));
    act(() => result.current.assist.keepMine());
    expect(result.current.assist.proofOpen).toBe(false);
    expect(result.current.draft?.bodyMarkdown).toBe(before);
    expect(result.current.assist.canUndoAccept).toBe(false);
  });

  test("a proof made from text that has since changed cannot be applied", async () => {
    apiFetch.mockResolvedValue(
      proposal("Thursday works. Friday.", [
        { kind: "kept", text: "Thursday works." },
        { kind: "added", text: " Friday." },
      ]),
    );
    const { result } = renderHook(() => useHarness(draftWith("Thursday works.")), { wrapper });
    act(() => result.current.assist.submit());
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));

    act(() =>
      result.current.setDraft((draft) =>
        draft ? { ...draft, bodyMarkdown: "Thursday works, mostly." } : draft,
      ),
    );
    expect(result.current.assist.stale).toBe(true);
    act(() => result.current.assist.accept());
    expect(result.current.draft?.bodyMarkdown).toBe("Thursday works, mostly.");
    expect(result.current.assist.proofOpen).toBe(true);
  });

  test("a new email with nothing to say is refused before any request", async () => {
    const newEmail: ComposeIntent = { key: "n", title: "New", kind: "new" };
    const { result } = renderHook(
      () =>
        useHarness(
          { ...draftWith(""), frontmatter: { ...draftWith("").frontmatter, subject: "" } },
          newEmail,
        ),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(apiFetch).not.toHaveBeenCalled();
  });

  test("placeholder and the next action follow the selection", () => {
    const body = "Thursday works.";
    const { result } = renderHook(() => useHarness(draftWith(body)), { wrapper });
    expect(result.current.assist.placeholder).toBe("Finish my draft, or say what to change");
    act(() => result.current.assist.setSelection({ from: 0, to: 8 }));
    expect(result.current.assist.placeholder).toBe("Edit the selection: say how");
  });
});
