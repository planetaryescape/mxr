import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, test, vi, beforeEach } from "vitest";
import { useState, useRef, type ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { RawFetchOpts } from "@/api/client";
import {
  assertWorkspaceBinding,
  observeWorkspace,
  resetWorkspaceOwnerForTest,
  WorkspaceConflictError,
} from "@/lib/workspaceOwner";

import type { ComposeDraftState, ComposeIntent } from "./composeDraft";
import { useDraftAssist } from "./useDraftAssist";

const apiFetch = vi.hoisted(() =>
  vi.fn<(path: string, init?: RawFetchOpts) => Promise<ReturnType<typeof proposal>>>(),
);
vi.mock("@/api/client", () => ({ apiFetch }));
vi.mock("sonner", () => ({
  toast: { error: vi.fn<() => void>(), success: vi.fn<() => void>(), info: vi.fn<() => void>() },
}));

const REPLY: ComposeIntent = { key: "k", title: "Reply", kind: "reply", messageId: "m1" };

function draftWith(body: string, extra: Partial<ComposeDraftState> = {}): ComposeDraftState {
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
    binding: ownerA,
    ...extra,
  };
}

const ownerA = {
  target: window.location.origin,
  workspace: { workspace_id: "owner-a", recovery_generation: "generation-a" },
};

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

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: Number.POSITIVE_INFINITY }, mutations: { retry: false } },
});

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

beforeEach(() => {
  apiFetch.mockReset();
  resetWorkspaceOwnerForTest();
  queryClient.clear();
  queryClient.setQueryData(["compose-models"], {
    kind: "AssistantModels",
    models: { current: "gemma4:latest", choices: [] },
  });
  queryClient.setQueryData(["llm-config"], {
    config: { compose_model: null, model: "gemma4:latest" },
  });
});

describe("useDraftAssist", () => {
  test("a new draft starts from the configured default after a per-draft choice", async () => {
    const { result, rerender } = renderHook(
      ({ intent }: { intent: ComposeIntent }) => useHarness(draftWith("Draft body."), intent),
      { initialProps: { intent: REPLY }, wrapper },
    );
    act(() => result.current.assist.setModel("gpt-6-luna"));
    await waitFor(() => expect(result.current.assist.model).toBe("gpt-6-luna"));

    rerender({ intent: { key: "new-draft", title: "New", kind: "new" } });
    await waitFor(() => expect(result.current.assist.model).toBeUndefined());
    expect(result.current.assist.defaultModel).toBe("gemma4:latest");
  });

  test("a model retry becomes the draft choice for later revisions", async () => {
    apiFetch.mockResolvedValue(
      proposal("Thursday works.", [{ kind: "kept", text: "Thursday works." }]),
    );
    const { result } = renderHook(() => useHarness(draftWith("Thursday works.")), { wrapper });
    act(() => result.current.assist.setSelection({ from: 0, to: 8 }));
    act(() => result.current.assist.setModel("gpt-6-luna"));
    await waitFor(() => expect(result.current.assist.model).toBe("gpt-6-luna"));
    await waitFor(() => expect(result.current.assist.selection).toEqual({ from: 0, to: 8 }));
    act(() => result.current.assist.submit());
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));
    expect(apiFetch.mock.calls[0]?.[1]?.body).toMatchObject({
      model: "gpt-6-luna",
      text: "Thursday works.",
      action: "edit_selection",
    });

    act(() => result.current.assist.againWithModel("claude-haiku-5-5"));
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(2));
    const retryBody = apiFetch.mock.calls[1]?.[1]?.body;
    expect(retryBody).toMatchObject({
      model: "claude-haiku-5-5",
      text: "Thursday works.",
      action: "edit_selection",
      selection: { start: 0, end: 8 },
    });
    expect(retryBody).not.toHaveProperty("revise");
    expect(result.current.assist.model).toBe("claude-haiku-5-5");

    act(() => result.current.assist.again("Keep the same meaning."));
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(3));
    expect(apiFetch.mock.calls[2]?.[1]?.body).toMatchObject({
      model: "claude-haiku-5-5",
      revise: "p1",
      text: "Thursday works.",
      selection: { start: 0, end: 8 },
    });
  });

  test("retrying with another model after editing uses the live text", async () => {
    apiFetch.mockResolvedValue(
      proposal("Thursday works.", [{ kind: "kept", text: "Thursday works." }]),
    );
    const { result } = renderHook(() => useHarness(draftWith("Thursday works.")), { wrapper });
    act(() => result.current.assist.submit());
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));
    act(() => result.current.setDraft(draftWith("Friday works.\n\n-- \nBK")));
    act(() => result.current.assist.againWithModel("gpt-6-luna"));
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(2));
    expect(apiFetch.mock.calls[1]?.[1]?.body).toMatchObject({
      model: "gpt-6-luna",
      text: "Friday works.",
      action: "finish",
    });
  });

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

  test("a resumed canonical reply keeps its stored mode for assist", async () => {
    apiFetch.mockResolvedValue(proposal("Hello", [{ kind: "added", text: "Hello" }]));
    const { result } = renderHook(
      () =>
        useHarness(draftWith("Hello", { draftIntent: "reply" }), { ...REPLY, kind: "reply_all" }),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(1));
    expect(apiFetch.mock.calls[0]?.[1]?.body).toMatchObject({
      draft: { kind: "reply", message_id: "m1", all: false },
    });
  });

  test("an assist stays with its original workspace and fails closed after ownership changes", async () => {
    observeWorkspace(ownerA);
    let networkRequests = 0;
    apiFetch.mockImplementation((path, options) => {
      const binding = options?.binding;
      if (binding) assertWorkspaceBinding(binding);
      networkRequests += 1;
      return Promise.resolve(proposal("Hi", [{ kind: "added", text: "Hi" }]));
    });
    const { result } = renderHook(
      () => useHarness(draftWith("private draft", { binding: ownerA })),
      { wrapper },
    );
    observeWorkspace({
      target: ownerA.target,
      workspace: { workspace_id: "owner-b", recovery_generation: "generation-b" },
    });

    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledOnce());
    await waitFor(() => expect(apiFetch.mock.results[0]?.type).toBe("throw"));
    expect(networkRequests).toBe(0);
    expect(apiFetch.mock.results[0]?.value).toBeInstanceOf(WorkspaceConflictError);
    expect(result.current.assist.proofOpen).toBe(false);
    expect(result.current.draft?.bodyMarkdown).toBe("private draft");
  });

  test("an assist request carries its original binding while that owner is current", async () => {
    observeWorkspace(ownerA);
    apiFetch.mockResolvedValue(proposal("Hi", [{ kind: "added", text: "Hi" }]));
    const { result } = renderHook(
      () => useHarness(draftWith("private draft", { binding: ownerA })),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledOnce());
    expect(apiFetch.mock.calls[0]?.[1]).toMatchObject({ binding: ownerA });
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));
  });

  test("a response that arrives after an owner transition cannot become the shown proof", async () => {
    observeWorkspace(ownerA);
    let resolve!: (value: ReturnType<typeof proposal>) => void;
    const pending = new Promise<ReturnType<typeof proposal>>((accept) => {
      resolve = accept;
    });
    apiFetch.mockImplementation(async (_path, options) => {
      const response = await pending;
      const binding = options?.binding;
      if (binding) assertWorkspaceBinding(binding);
      return response;
    });
    const { result } = renderHook(
      () => useHarness(draftWith("private draft", { binding: ownerA })),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledOnce());
    observeWorkspace({
      target: ownerA.target,
      workspace: { workspace_id: "owner-b", recovery_generation: "generation-b" },
    });
    await act(async () => {
      resolve(proposal("Changed private text", [{ kind: "added", text: "Changed private text" }]));
      await pending;
    });

    expect(result.current.assist.proofOpen).toBe(false);
    expect(result.current.assist.version).toBeNull();
    expect(result.current.draft?.bodyMarkdown).toBe("private draft");
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

  test("two presses before the first answer send one request", async () => {
    apiFetch.mockReturnValue(new Promise(() => undefined));
    const { result } = renderHook(() => useHarness(draftWith("Thursday works.")), { wrapper });
    act(() => {
      result.current.assist.submit();
      result.current.assist.submit();
    });
    await waitFor(() => expect(apiFetch).toHaveBeenCalled());
    expect(apiFetch).toHaveBeenCalledTimes(1);
  });

  test("a new email with a metadata id but no daemon save is sent as new, not stored", async () => {
    apiFetch.mockResolvedValue(proposal("Hi", [{ kind: "added", text: "Hi" }]));
    const newEmail: ComposeIntent = { key: "n", title: "New", kind: "new" };
    const { result } = renderHook(
      () => useHarness(draftWith("Hi", { draftId: "meta-1", revision: null }), newEmail),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(1));
    const [, init] = apiFetch.mock.calls[0] as [string, { body: Record<string, unknown> }];
    expect(init.body.draft).toMatchObject({ kind: "new", account_id: "acc" });
  });

  test("a new email the daemon has saved is sent as new, with its live fields", async () => {
    apiFetch.mockResolvedValue(proposal("Hi", [{ kind: "added", text: "Hi" }]));
    const newEmail: ComposeIntent = { key: "n", title: "New", kind: "new" };
    const { result } = renderHook(
      () =>
        useHarness(
          draftWith("Hi", {
            draftId: "saved-1",
            revision: 2,
            kind: "new",
            frontmatter: {
              to: "pat@example.com",
              cc: "",
              bcc: "",
              subject: "Thursday",
              from: "",
              attach: [],
            },
          }),
          newEmail,
        ),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(1));
    const [, init] = apiFetch.mock.calls[0] as [string, { body: Record<string, unknown> }];
    expect(init.body.draft).toEqual({
      kind: "new",
      account_id: "acc",
      to: [{ name: null, email: "pat@example.com" }],
      subject: "Thursday",
    });
  });

  test("an autosaved new email that gained a recipient is sent with the live recipient", async () => {
    apiFetch.mockResolvedValue(proposal("Hi", [{ kind: "added", text: "Hi" }]));
    const newEmail: ComposeIntent = { key: "n", title: "New", kind: "new" };
    // The save held no recipient; the user has since added one.
    const { result } = renderHook(
      () =>
        useHarness(
          draftWith("Hi", {
            draftId: "saved-3",
            revision: 1,
            kind: "new",
            frontmatter: {
              to: "live@example.com",
              cc: "",
              bcc: "",
              subject: "",
              from: "",
              attach: [],
            },
          }),
          newEmail,
        ),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(1));
    const [, init] = apiFetch.mock.calls[0] as [string, { body: Record<string, unknown> }];
    expect(init.body.draft).toMatchObject({
      kind: "new",
      to: [{ name: null, email: "live@example.com" }],
    });
  });

  test("an autosaved new email whose subject changed is sent with the live subject", async () => {
    apiFetch.mockResolvedValue(proposal("Hi", [{ kind: "added", text: "Hi" }]));
    const newEmail: ComposeIntent = { key: "n", title: "New", kind: "new" };
    const { result } = renderHook(
      () =>
        useHarness(
          draftWith("Hi", {
            draftId: "saved-4",
            revision: 1,
            kind: "new",
            frontmatter: {
              to: "pat@example.com",
              cc: "",
              bcc: "",
              subject: "Decline Thursday's invitation",
              from: "",
              attach: [],
            },
          }),
          newEmail,
        ),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(1));
    const [, init] = apiFetch.mock.calls[0] as [string, { body: Record<string, unknown> }];
    expect(init.body.draft).toMatchObject({
      kind: "new",
      subject: "Decline Thursday's invitation",
    });
  });

  test("a saved reply reopened from Drafts, with no message in the session, is sent as stored", async () => {
    apiFetch.mockResolvedValue(proposal("Hi", [{ kind: "added", text: "Hi" }]));
    const reopened: ComposeIntent = {
      key: "d",
      title: "Saved draft",
      kind: "new",
      draftId: "reply-1",
    };
    const { result } = renderHook(
      () =>
        useHarness(
          draftWith("Hi", {
            draftId: "reply-1",
            revision: 3,
            kind: "new",
            draftIntent: "reply",
          }),
          reopened,
        ),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(1));
    const [, init] = apiFetch.mock.calls[0] as [string, { body: Record<string, unknown> }];
    expect(init.body.draft).toEqual({ kind: "stored", draft_id: "reply-1" });
  });

  test("a saved forward reopened from Drafts, with no message in the session, is sent as stored", async () => {
    apiFetch.mockResolvedValue(proposal("FYI", [{ kind: "added", text: "FYI" }]));
    const reopened: ComposeIntent = {
      key: "d",
      title: "Saved draft",
      kind: "new",
      draftId: "fwd-1",
    };
    const { result } = renderHook(
      () =>
        useHarness(
          draftWith("FYI", {
            draftId: "fwd-1",
            revision: 5,
            kind: "new",
            draftIntent: "forward",
          }),
          reopened,
        ),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(1));
    const [, init] = apiFetch.mock.calls[0] as [string, { body: Record<string, unknown> }];
    expect(init.body.draft).toEqual({ kind: "stored", draft_id: "fwd-1" });
  });

  test("a saved forward still names its message, not the stored draft", async () => {
    apiFetch.mockResolvedValue(proposal("FYI", [{ kind: "added", text: "FYI" }]));
    const forward: ComposeIntent = { key: "f", title: "Forward", kind: "forward", messageId: "m9" };
    const { result } = renderHook(
      () =>
        useHarness(
          draftWith("", {
            draftId: "saved-2",
            revision: 4,
            frontmatter: { ...draftWith("").frontmatter, to: "pat@example.com" },
          }),
          forward,
        ),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(1));
    const [, init] = apiFetch.mock.calls[0] as [string, { body: Record<string, unknown> }];
    expect(init.body.draft).toMatchObject({ kind: "forward", message_id: "m9" });
  });

  test("a stale proof is asked again with the action the live text calls for", async () => {
    apiFetch.mockResolvedValue(proposal("Hi", [{ kind: "added", text: "Hi" }]));
    const { result } = renderHook(() => useHarness(draftWith("")), { wrapper });
    act(() => result.current.assist.submit());
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));
    act(() =>
      result.current.setDraft((draft) =>
        draft ? { ...draft, bodyMarkdown: "I cannot make Thursday" } : draft,
      ),
    );
    expect(result.current.assist.stale).toBe(true);
    act(() => result.current.assist.again());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(2));
    const [, init] = apiFetch.mock.calls[1] as [string, { body: Record<string, unknown> }];
    expect(init.body).toMatchObject({ action: "finish", text: "I cannot make Thursday" });
  });

  test("an unchanged proof repeats its own action", async () => {
    apiFetch.mockResolvedValue(proposal("Hi", [{ kind: "added", text: "Hi" }]));
    const { result } = renderHook(() => useHarness(draftWith("")), { wrapper });
    act(() => result.current.assist.submit());
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));
    act(() => result.current.assist.again());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(2));
    const [, init] = apiFetch.mock.calls[1] as [string, { body: Record<string, unknown> }];
    expect(init.body).toMatchObject({ action: "write" });
  });

  test("revising the displayed version sends its id and original request text and action", async () => {
    apiFetch.mockResolvedValueOnce(
      proposal("Decline Thursday and suggest Friday.", [
        { kind: "added", text: "Decline Thursday and suggest Friday." },
      ]),
    );
    apiFetch.mockResolvedValueOnce(
      proposal("Friday works instead.", [{ kind: "added", text: "Friday works instead." }]),
    );
    const { result } = renderHook(
      () =>
        useHarness(
          draftWith("", {
            frontmatter: { ...draftWith("").frontmatter, to: "pat@example.com" },
          }),
          { key: "new", title: "New", kind: "new" },
        ),
      { wrapper },
    );
    act(() => {
      result.current.assist.setInstruction("Decline Thursday and suggest Friday");
      result.current.assist.submit();
    });
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));
    act(() => result.current.assist.setInstruction("Remove the last sentence of this proof"));
    act(() => result.current.assist.again("Remove the last sentence of this proof"));
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(2));
    expect(apiFetch.mock.calls[1]?.[1]).toMatchObject({
      body: {
        draft: {
          kind: "new",
          account_id: "acc",
          to: [{ name: null, email: "pat@example.com" }],
        },
        text: "",
        action: "write",
        instruction: "Remove the last sentence of this proof",
        revise: "p1",
      },
    });
  });

  test("a revision keeps the original UTF-8 selection even if the editor selection moves", async () => {
    apiFetch.mockResolvedValueOnce({
      proposal: {
        ...proposal("café Friday", [{ kind: "kept", text: "café Friday" }]).proposal,
        proposal_id: "selected-proof",
      },
    });
    apiFetch.mockResolvedValueOnce(proposal("café works", [{ kind: "kept", text: "café works" }]));
    const body = "café maybe Friday";
    const { result } = renderHook(() => useHarness(draftWith(body)), { wrapper });
    act(() => result.current.assist.setSelection({ from: 5, to: 10 }));
    act(() => result.current.assist.submit());
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));
    act(() => result.current.assist.setSelection({ from: 11, to: body.length }));
    act(() => result.current.assist.again("Replace the selected phrase"));
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(2));
    expect(apiFetch.mock.calls[1]?.[1]).toMatchObject({
      body: {
        text: body,
        action: "edit_selection",
        selection: { start: 6, end: 11 },
        revise: "selected-proof",
        instruction: "Replace the selected phrase",
      },
    });
  });

  test("version navigation makes the displayed proposal the revision target", async () => {
    apiFetch.mockResolvedValueOnce({
      proposal: {
        ...proposal("First answer", [{ kind: "added", text: "First answer" }]).proposal,
        proposal_id: "p-first",
      },
    });
    apiFetch.mockResolvedValueOnce({
      proposal: {
        ...proposal("Second answer", [{ kind: "added", text: "Second answer" }]).proposal,
        proposal_id: "p-second",
      },
    });
    apiFetch.mockResolvedValueOnce(
      proposal("Revised first answer", [{ kind: "added", text: "Revised first answer" }]),
    );
    const { result } = renderHook(() => useHarness(draftWith("")), { wrapper });
    act(() => result.current.assist.submit());
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));
    act(() => result.current.assist.again());
    await waitFor(() => expect(result.current.assist.versionCount).toBe(2));
    act(() => result.current.assist.stepVersion(-1));
    expect(result.current.assist.version?.proposal.proposal_id).toBe("p-first");
    act(() => result.current.assist.again("Keep the first answer concise"));
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(3));
    expect(apiFetch.mock.calls[2]?.[1]).toMatchObject({
      body: { revise: "p-first", text: "", action: "write" },
    });
  });

  test.each(["reply", "reply_all", "forward"] as const)(
    "a reopened %s draft keeps its stored intent",
    async (draftIntent) => {
      apiFetch.mockResolvedValue(proposal("Hi", [{ kind: "added", text: "Hi" }]));
      const restored = draftWith("existing text", {
        draftId: "saved-1",
        revision: 4,
        draftIntent,
        kind: "new",
        frontmatter: { ...draftWith("").frontmatter, subject: "Ignore this incoming subject" },
      });
      const { result } = renderHook(
        () => useHarness(restored, { key: "saved", title: "Saved draft", kind: "new" }),
        { wrapper },
      );
      act(() => result.current.assist.submit());
      await waitFor(() => expect(apiFetch).toHaveBeenCalledOnce());
      expect(apiFetch.mock.calls[0]?.[1]).toMatchObject({
        body: { draft: { kind: "stored", draft_id: "saved-1" } },
      });
      expect(apiFetch.mock.calls[0]?.[1]?.body).not.toHaveProperty("draft.subject");
    },
  );

  test("a genuinely new draft still uses its live recipient and subject", async () => {
    apiFetch.mockResolvedValue(proposal("Hi", [{ kind: "added", text: "Hi" }]));
    const newEmail: ComposeIntent = { key: "n", title: "New", kind: "new" };
    const { result } = renderHook(
      () =>
        useHarness(
          draftWith("", {
            frontmatter: {
              ...draftWith("").frontmatter,
              to: "new@example.com",
              subject: "Live subject",
            },
          }),
          newEmail,
        ),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledOnce());
    expect(apiFetch.mock.calls[0]?.[1]).toMatchObject({
      body: {
        draft: {
          kind: "new",
          to: [{ name: null, email: "new@example.com" }],
          subject: "Live subject",
        },
      },
    });
  });

  test("accepting a greeting and sign-off keeps the mark on an unedited model sentence", async () => {
    const marked = "Friday works.";
    apiFetch.mockResolvedValue({
      proposal: {
        proposal_id: "p-g",
        action: "finish",
        base: "",
        text: "Hi Pat,\n\nFriday works.\n\nThanks, B",
        changes: [
          { kind: "added", text: "Hi Pat,\n\n" },
          { kind: "kept", text: marked },
          { kind: "added", text: "\n\nThanks, B" },
        ],
        gaps: [],
        provenance: null,
        yours_changed: [],
        composer_version: 1,
      },
    });
    const { result } = renderHook(
      () => useHarness(draftWith(marked, { modelSpans: [{ start: 0, end: marked.length }] })),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));
    act(() => result.current.assist.accept());

    const body = result.current.draft?.bodyMarkdown ?? "";
    expect(body).toBe("Hi Pat,\n\nFriday works.\n\nThanks, B");
    const marks = (result.current.draft?.modelSpans ?? []).map((span) =>
      body.slice(span.start, span.end),
    );
    expect(marks.toSorted()).toEqual(["\n\nThanks, B", "Friday works.", "Hi Pat,\n\n"].toSorted());
  });

  test("an edit inside a marked sentence clears only the edited words", async () => {
    const marked = "Friday works.";
    apiFetch.mockResolvedValue({
      proposal: {
        proposal_id: "p-e",
        action: "finish",
        base: "",
        text: "Friday is set.",
        changes: [
          { kind: "kept", text: "Friday" },
          { kind: "removed", text: " works." },
          { kind: "added", text: " is set." },
        ],
        gaps: [],
        provenance: null,
        yours_changed: [],
        composer_version: 1,
      },
    });
    const { result } = renderHook(
      () => useHarness(draftWith(marked, { modelSpans: [{ start: 0, end: marked.length }] })),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(result.current.assist.proofOpen).toBe(true));
    act(() => result.current.assist.accept());

    const body = result.current.draft?.bodyMarkdown ?? "";
    expect(body).toBe("Friday is set.");
    const marks = (result.current.draft?.modelSpans ?? []).map((span) =>
      body.slice(span.start, span.end),
    );
    expect(marks.toSorted()).toEqual(["Friday", " is set."].toSorted());
  });

  test("a forward sends its recipients as address objects", async () => {
    apiFetch.mockResolvedValue(proposal("FYI", [{ kind: "added", text: "FYI" }]));
    const forward: ComposeIntent = { key: "f", title: "Forward", kind: "forward", messageId: "m2" };
    const draft = draftWith("");
    const { result } = renderHook(
      () =>
        useHarness(
          {
            ...draft,
            frontmatter: { ...draft.frontmatter, to: "Pat Lee <pat@example.com>, sam@example.com" },
          },
          forward,
        ),
      { wrapper },
    );
    act(() => result.current.assist.submit());
    await waitFor(() => expect(apiFetch).toHaveBeenCalledTimes(1));
    const [, init] = apiFetch.mock.calls[0] as [string, { body: Record<string, unknown> }];
    expect(init.body.draft).toEqual({
      kind: "forward",
      message_id: "m2",
      to: [
        { name: "Pat Lee", email: "pat@example.com" },
        { name: null, email: "sam@example.com" },
      ],
    });
  });

  test("placeholder and the next action follow the selection", () => {
    const body = "Thursday works.";
    const { result } = renderHook(() => useHarness(draftWith(body)), { wrapper });
    expect(result.current.assist.placeholder).toBe("Finish my draft, or say what to change");
    act(() => result.current.assist.setSelection({ from: 0, to: 8 }));
    expect(result.current.assist.placeholder).toBe("Edit the selection: say how");
  });
});
