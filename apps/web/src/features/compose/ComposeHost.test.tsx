/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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

/** Subjects the daemon was asked to save, in order. */
function savedSubjects(): string[] {
  return api.updateComposeSession.mock.calls.map(
    ([input]) => (input as { frontmatter: { subject: string } }).frontmatter.subject,
  );
}

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

afterEach(async () => {
  // Unmount first: tearing a session down legitimately flushes its draft
  // (asynchronously, through the save queue), and that call must not leak
  // into the next test's mock history.
  cleanup();
  act(() => useComposeUi.setState({ intent: null, surface: "overlay" }));
  useUndo.setState({ pendingSendCancel: null });
  vi.useRealTimers();
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
  vi.clearAllMocks();
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

describe("ComposeHost never loses typing on the way out", () => {
  beforeEach(() => {
    api.updateComposeSession.mockImplementation(async (input) => {
      const { frontmatter, body } = input as {
        frontmatter: ComposeSessionResponse["session"]["frontmatter"];
        body: string;
      };
      return session({ frontmatter, bodyMarkdown: body });
    });
  });

  test("closing inside the autosave window saves the last edit first", async () => {
    renderHost();
    openNewMessage();

    const subject = await screen.findByLabelText("Subject");
    fireEvent.change(subject, { target: { value: "Quarterly plan v2" } });
    fireEvent.click(screen.getByRole("button", { name: /close composer/i }));

    await waitFor(() => expect(screen.queryByLabelText("Subject")).not.toBeInTheDocument());
    expect(savedSubjects()).toEqual(["Quarterly plan v2"]);
  });

  test("a failed save on close keeps the composer open with the text", async () => {
    api.updateComposeSession.mockRejectedValue(new Error("daemon unavailable"));
    renderHost();
    openNewMessage();

    const subject = await screen.findByLabelText("Subject");
    fireEvent.change(subject, { target: { value: "Keep me" } });
    fireEvent.click(screen.getByRole("button", { name: /close composer/i }));

    await waitFor(() =>
      expect(toasts.error).toHaveBeenCalledWith("Draft not saved, composer kept open", {
        description: "daemon unavailable",
      }),
    );
    expect(screen.getByLabelText("Subject")).toHaveValue("Keep me");
  });

  test("switching to another reply target saves the draft being left", async () => {
    renderHost();
    openNewMessage();

    const subject = await screen.findByLabelText("Subject");
    fireEvent.change(subject, { target: { value: "Half-written" } });
    act(() => {
      useComposeUi
        .getState()
        .openCompose({ key: "compose:reply:m-2", title: "Reply", kind: "reply", messageId: "m-2" });
    });

    await waitFor(() => expect(savedSubjects()).toEqual(["Half-written"]));
  });
});

describe("ComposeHost validation", () => {
  const blank = () =>
    session({
      frontmatter: { to: "", cc: "", bcc: "", subject: "", from: "me@example.com", attach: [] },
      bodyMarkdown: "",
      issues: [
        { severity: "error", message: "No recipients (to: field is empty)" },
        { severity: "warning", message: "Subject is empty" },
      ],
    });

  test("a fresh message shows no validation before the user tries to send", async () => {
    api.startComposeSession.mockResolvedValue(blank());
    renderHost();
    openNewMessage();

    await screen.findByLabelText("Subject");
    expect(screen.queryByRole("status", { name: "Compose issues" })).not.toBeInTheDocument();
    expect(screen.queryByText(/No recipients/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Subject is empty/)).not.toBeInTheDocument();
  });

  test("a blocked send shows one line that separates blockers from warnings", async () => {
    api.startComposeSession.mockResolvedValue(blank());
    renderHost();
    openNewMessage();

    fireEvent.click(await screen.findByRole("button", { name: "Send⌘↵" }));

    const summary = await screen.findByRole("status", { name: "Compose issues" });
    expect(summary).toHaveTextContent(
      "Can't send yet:No recipients (to: field is empty)Check:Subject is empty",
    );
    expect(screen.getAllByRole("status", { name: "Compose issues" })).toHaveLength(1);
    expect(api.checkComposeSafety).not.toHaveBeenCalled();
    expect(api.sendComposeSession).not.toHaveBeenCalled();
    expect(toasts.error).not.toHaveBeenCalled();
  });

  test("leaving a recipient field flags a malformed address, and nothing else", async () => {
    api.startComposeSession.mockResolvedValue(blank());
    renderHost();
    openNewMessage();

    const to = await screen.findByLabelText("To");
    fireEvent.change(to, { target: { value: "bob" } });
    fireEvent.blur(to);

    const summary = await screen.findByRole("status", { name: "Compose issues" });
    expect(summary).toHaveTextContent("Can't send yet:Invalid email address: bob");
    expect(summary).not.toHaveTextContent("Subject is empty");
  });
});

describe("ComposeHost shortcut failures", () => {
  test("Cmd+S reports a failed save instead of throwing", async () => {
    api.updateComposeSession.mockRejectedValue(new Error("disk full"));
    renderHost();
    openNewMessage();

    const subject = await screen.findByLabelText("Subject");
    fireEvent.change(subject, { target: { value: "Edited" } });
    fireEvent.keyDown(subject, { key: "s", metaKey: true });

    await waitFor(() =>
      expect(toasts.error).toHaveBeenCalledWith("Save failed", { description: "disk full" }),
    );
    expect(toasts.success).not.toHaveBeenCalledWith("Draft saved locally");
  });

  test("Cmd+Shift+R reports a failed refresh instead of throwing", async () => {
    api.refreshComposeSession.mockRejectedValue(new Error("compose file missing"));
    renderHost();
    openNewMessage();

    const subject = await screen.findByLabelText("Subject");
    fireEvent.keyDown(subject, { key: "R", metaKey: true, shiftKey: true });

    await waitFor(() =>
      expect(toasts.error).toHaveBeenCalledWith("Refresh failed", {
        description: "compose file missing",
      }),
    );
    expect(subject).toHaveValue("Quarterly plan");
  });
});
