/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { useUiPrefs } from "@/state/uiPrefsStore";
import { useUndo } from "@/state/undoStore";
import type { ComposeSessionResponse, DraftSafetyReport } from "./api";
import { ComposeHost } from "./ComposeHost";
import { useComposeUi } from "./composeUiStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
  location: { pathname: "/m/inbox", search: {} },
}));

const api = vi.hoisted(() => ({
  checkComposeSafety:
    vi.fn<(draftPath: string, accountId: string) => Promise<{ report: DraftSafetyReport }>>(),
  createScheduledSend: vi.fn<(draftId: string, at: Date) => Promise<unknown>>(),
  discardComposeSession: vi.fn<(draftPath: string) => Promise<unknown>>(),
  fetchAccounts: vi.fn<() => Promise<unknown>>(),
  fetchContactsAutocomplete: vi.fn<(query: string) => Promise<unknown[]>>(),
  refreshComposeSession: vi.fn<(draftPath: string) => Promise<unknown>>(),
  restoreComposeSession: vi.fn<(draftId: string) => Promise<unknown>>(),
  saveComposeSession:
    vi.fn<(draftPath: string, accountId: string, draftId?: string) => Promise<unknown>>(),
  saveLocalDraft: vi.fn<(draft: unknown) => Promise<unknown>>(),
  sendComposeSession:
    vi.fn<(draftPath: string, accountId: string, token?: string) => Promise<unknown>>(),
  startComposeSession:
    vi.fn<(kind: string, messageId?: string) => Promise<ComposeSessionResponse>>(),
  suggestComposeCollaborators:
    vi.fn<(draftPath: string, accountId: string) => Promise<{ suggestions: unknown[] }>>(),
  updateComposeSession: vi.fn<(input: unknown) => Promise<ComposeSessionResponse>>(),
  uploadComposeAttachment: vi.fn<(input: unknown) => Promise<unknown>>(),
}));

const rawApi = vi.hoisted(() => ({
  fetch: vi.fn<(path: string, init?: unknown) => Promise<unknown>>(),
}));

const toasts = vi.hoisted(() => {
  const base = vi.fn<(message: string, options?: unknown) => string>(() => "toast-id");
  return Object.assign(base, {
    error: vi.fn<(message: string, options?: unknown) => void>(),
    success: vi.fn<(message: string, options?: unknown) => void>(),
    info: vi.fn<(message: string, options?: unknown) => void>(),
    warning: vi.fn<(message: string, options?: unknown) => void>(),
    dismiss: vi.fn<(id?: string) => void>(),
  });
});

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    useNavigate: () => router.navigate,
    useRouterState: ({
      select,
    }: {
      select: (state: { location: typeof router.location }) => unknown;
    }) => select({ location: router.location }),
  };
});

vi.mock("@/api/client", () => ({ apiFetch: rawApi.fetch }));
vi.mock("@/features/accounts/api", () => ({
  fetchAccountAddresses: () => Promise.resolve({ addresses: [] }),
}));
vi.mock("./api", () => api);
vi.mock("sonner", () => ({ toast: toasts }));

// The mocked editor exposes the editor's own send hotkey (CodeMirror's
// Mod-Enter), which calls onSend directly and bypasses the surface's
// `busy` gate. Only the send pipeline itself can stop a second send there.
function MockEditor({ value, onSend }: { value: string; onSend: () => void }) {
  return (
    <div>
      <textarea aria-label="Message body" value={value} readOnly />
      <button type="button" onClick={onSend}>
        Editor send hotkey
      </button>
    </div>
  );
}
vi.mock("./tiptap/TiptapComposeEditor", () => ({ TiptapComposeEditor: MockEditor }));
vi.mock("./codemirror/CodeMirrorComposeEditor", () => ({ CodeMirrorComposeEditor: MockEditor }));

const cleanReport: DraftSafetyReport = { allowed: true, verdict: "SAFE", issues: [] };

function session(overrides: Partial<ComposeSessionResponse["session"]> = {}) {
  return {
    session: {
      draftPath: "/tmp/mxr-compose.md",
      rawContent: "",
      frontmatter: {
        to: "alice@example.com",
        cc: "",
        bcc: "",
        subject: "Quarterly plan",
        from: "me@example.com",
        attach: [],
      },
      bodyMarkdown: "Numbers attached.",
      issues: [],
      accountId: "account-1",
      kind: "new",
      ...overrides,
    },
  } satisfies ComposeSessionResponse;
}

function renderHost(children: ReactNode = <ComposeHost />) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

function openNewMessage() {
  act(() => {
    useComposeUi
      .getState()
      .openCompose({ key: "compose:new:new", title: "New message", kind: "new" }, "overlay");
  });
}

beforeEach(() => {
  rawApi.fetch.mockResolvedValue({ snippets: [] });
  api.fetchContactsAutocomplete.mockResolvedValue([]);
  api.suggestComposeCollaborators.mockResolvedValue({ suggestions: [] });
  api.fetchAccounts.mockResolvedValue({
    accounts: [
      {
        account_id: "account-1",
        name: "Work",
        email: "me@example.com",
        provider_kind: "fake",
        enabled: true,
        is_default: true,
        capabilities: { supports_send: true, supports_local_drafts: true },
      },
    ],
  });
  api.startComposeSession.mockResolvedValue(session());
  api.checkComposeSafety.mockResolvedValue({ report: cleanReport });
  api.sendComposeSession.mockResolvedValue({ ok: true, draft_id: "draft-1" });
  useUiPrefs.setState({ undoSendSeconds: 5 });
});

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
  act(() => useComposeUi.setState({ intent: null, surface: "overlay" }));
  useUndo.setState({ pendingSendCancel: null });
});

describe("ComposeHost send safety", () => {
  test("pressing send again during the undo window never sends twice", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    renderHost();
    openNewMessage();

    const send = await screen.findByRole("button", { name: "Send⌘↵" });
    fireEvent.click(send);
    await waitFor(() => expect(toasts).toHaveBeenCalledWith("Sending in 5s", expect.anything()));

    // Every other way to send while the undo countdown runs.
    expect(send).toBeDisabled();
    fireEvent.keyDown(screen.getByLabelText("To"), { key: "Enter", metaKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Editor send hotkey" }));
    fireEvent.click(screen.getByRole("button", { name: "Editor send hotkey" }));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(6000);
    });

    await waitFor(() => expect(api.sendComposeSession).toHaveBeenCalledTimes(1));
    expect(api.checkComposeSafety).toHaveBeenCalledTimes(1);
    expect(toasts).toHaveBeenCalledTimes(1);
  });

  test("two presses in the same tick start one send", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    renderHost();
    openNewMessage();

    const editorSend = await screen.findByRole("button", { name: "Editor send hotkey" });
    act(() => {
      editorSend.click();
      editorSend.click();
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(6000);
    });
    await waitFor(() => expect(api.sendComposeSession).toHaveBeenCalledTimes(1));
  });

  test("undo cancels the send and lets the user send again", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    renderHost();
    openNewMessage();

    const send = await screen.findByRole("button", { name: "Send⌘↵" });
    fireEvent.click(send);
    await waitFor(() => expect(useUndo.getState().pendingSendCancel).not.toBeNull());

    act(() => useUndo.getState().pendingSendCancel?.());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(6000);
    });
    expect(api.sendComposeSession).not.toHaveBeenCalled();
    expect(toasts.info).toHaveBeenCalledWith("Send cancelled");

    await waitFor(() => expect(send).toBeEnabled());
    fireEvent.click(send);
    await waitFor(() => expect(useUndo.getState().pendingSendCancel).not.toBeNull());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(6000);
    });
    await waitFor(() => expect(api.sendComposeSession).toHaveBeenCalledTimes(1));
  });
});
