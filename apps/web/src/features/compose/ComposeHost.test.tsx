import { observeWorkspace, resetWorkspaceOwnerForTest } from "@/lib/workspaceOwner";
/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { useUiPrefs } from "@/state/uiPrefsStore";
import { useConnectionStore } from "@/state/connectionStore";
import { useUndo } from "@/state/undoStore";
import type { ComposeSessionResponse, DraftSafetyReport } from "./api";
import { ComposeHost } from "./ComposeHost";
import { draftIntent, useComposeUi } from "./composeUiStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
  location: { pathname: "/m/inbox", search: {} },
}));

const api = vi.hoisted(() => ({
  rememberComposeBinding: vi.fn<typeof import("./api").rememberComposeBinding>(),
  setAutoReminder: vi.fn<(sentMessageId: string, remindAt: Date) => Promise<unknown>>(),
  cancelAutoReminder: vi.fn<(sentMessageId: string) => Promise<unknown>>(),
  cancelScheduledSend: vi.fn<(draftId: string) => Promise<unknown>>(),
  checkComposeSafety:
    vi.fn<(draftPath: string, accountId: string) => Promise<{ report: DraftSafetyReport }>>(),
  discardComposeSession: vi.fn<(draftPath: string) => Promise<unknown>>(),
  fetchAccounts: vi.fn<() => Promise<unknown>>(),
  fetchContactsAutocomplete: vi.fn<(query: string) => Promise<unknown[]>>(),
  refreshComposeSession: vi.fn<(draftPath: string) => Promise<unknown>>(),
  restoreComposeSession: vi.fn<(draftId: string) => Promise<unknown>>(),
  saveComposeSession:
    vi.fn<(draftPath: string, accountId: string, draftId?: string) => Promise<unknown>>(),
  scheduleComposeSession: vi.fn<
    (input: { draftPath: string; accountId: string; draftId?: string; sendAt: Date }) => Promise<{
      ok: boolean;
      draft_id: string;
      send_at: string;
    }>
  >(),
  sendComposeSession: vi.fn<typeof import("./api").sendComposeSession>(),
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
const accountAddresses = vi.hoisted(() => ({ addresses: [{ email: "" }] }));

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

vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch: rawApi.fetch,
}));
vi.mock("@/features/accounts/api", () => ({
  fetchAccountAddresses: () => Promise.resolve({ addresses: accountAddresses.addresses }),
}));
vi.mock("./api", () => api);
vi.mock("@/features/time/api", async (importOriginal) => {
  const { fakeResolvedTime } = await import("@/features/time/testing");
  return {
    ...(await importOriginal<typeof import("@/features/time/api")>()),
    resolveTime: (input: string) => Promise.resolve(fakeResolvedTime(input)),
  };
});
vi.mock("sonner", () => ({ toast: toasts }));

function MockEditor({
  value,
  onChange,
  onSend,
}: {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
}) {
  return (
    <div>
      <textarea
        aria-label="Message body"
        value={value}
        onChange={(event) => onChange(event.currentTarget.value)}
      />
      <button type="button" onClick={onSend}>
        Editor send hotkey
      </button>
    </div>
  );
}
vi.mock("./tiptap/TiptapComposeEditor", () => ({ TiptapComposeEditor: MockEditor }));
vi.mock("./codemirror/CodeMirrorComposeEditor", () => ({ CodeMirrorComposeEditor: MockEditor }));

function savedSubjects(): string[] {
  return api.updateComposeSession.mock.calls.map(
    ([input]) => (input as { frontmatter: { subject: string } }).frontmatter.subject,
  );
}

const cleanReport: DraftSafetyReport = { allowed: true, verdict: "SAFE", issues: [] };

function originalBinding() {
  return {
    target: window.location.origin,
    workspace: {
      workspace_id: "00000000-0000-7000-8000-000000000001",
      recovery_generation: "00000000-0000-7000-8000-000000000002",
    },
  };
}
function session(overrides: Partial<ComposeSessionResponse["session"]> = {}) {
  return {
    session: {
      binding: originalBinding(),
      draftPath: "/tmp/mxr-compose.md",
      draftId: "draft-1",
      revision: 1,
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
      previewHtml: overrides.previewHtml ?? "",
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

async function openSendLater() {
  fireEvent.keyDown(await screen.findByRole("button", { name: "More send options" }), {
    key: "Enter",
  });
  fireEvent.click(await screen.findByRole("menuitem", { name: /Send later/ }));
}

beforeEach(() => {
  accountAddresses.addresses = [];
  resetWorkspaceOwnerForTest();
  observeWorkspace(originalBinding());
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
  cleanup();
  act(() => useComposeUi.setState({ intent: null, surface: "overlay" }));
  act(() => useConnectionStore.getState().setState({ daemonDown: false }));
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
    await waitFor(() =>
      expect(toasts).toHaveBeenCalledWith(
        expect.anything(),
        expect.objectContaining({ duration: 5000 }),
      ),
    );

    expect(send).toBeDisabled();
    fireEvent.keyDown(screen.getByLabelText("To"), { key: "Enter", metaKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Editor send hotkey" }));
    fireEvent.click(screen.getByRole("button", { name: "Editor send hotkey" }));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(6000);
    });

    await waitFor(() => expect(api.sendComposeSession).toHaveBeenCalledTimes(1));
    expect(api.sendComposeSession).toHaveBeenCalledWith(
      "/tmp/mxr-compose.md",
      "account-1",
      undefined,
      1,
      "draft-1:1",
    );
    await waitFor(() => expect(toasts.success).toHaveBeenCalledWith("Message sent"));
    expect(toasts.error).not.toHaveBeenCalled();
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

  test("a send whose undo window closes while mxr is down is cancelled, not sent once it is back", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    renderHost();
    openNewMessage();

    fireEvent.click(await screen.findByRole("button", { name: "Send⌘↵" }));
    await waitFor(() => expect(useUndo.getState().pendingSendCancel).not.toBeNull());
    act(() => useConnectionStore.getState().setState({ daemonDown: true }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(6000);
    });
    expect(toasts.warning).toHaveBeenCalledWith(
      "Can't send while mxr is unavailable",
      expect.anything(),
    );

    act(() => useConnectionStore.getState().setState({ daemonDown: false }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(12_000);
    });
    expect(api.sendComposeSession).not.toHaveBeenCalled();
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
});

describe("ComposeHost send confirmation", () => {
  const warnReport: DraftSafetyReport = {
    allowed: true,
    verdict: "warn",
    issues: [
      { code: "missing_attachment", severity: "warning", message: "Mentions an attachment" },
    ],
  };
  const blockedReport: DraftSafetyReport = {
    allowed: false,
    verdict: "blocked",
    issues: [
      {
        code: "wrong_recipient",
        severity: "blocker",
        message: "alice@example.com has never received this thread",
        override_token: "tok-1",
      },
    ],
  };

  beforeEach(() => {
    useUiPrefs.setState({ undoSendSeconds: 0 });
    api.updateComposeSession.mockImplementation(async (input) => {
      const { frontmatter, body } = input as {
        frontmatter: ComposeSessionResponse["session"]["frontmatter"];
        body: string;
      };
      return session({ frontmatter, bodyMarkdown: body });
    });
  });

  test("states recipients, sender and verdict before a warned send", async () => {
    api.startComposeSession.mockResolvedValue(
      session({
        frontmatter: {
          to: "alice@example.com",
          cc: "bob@example.com",
          bcc: "",
          subject: "Quarterly plan",
          from: "me@example.com",
          attach: [],
        },
      }),
    );
    api.checkComposeSafety.mockResolvedValue({ report: warnReport });
    renderHost();
    openNewMessage();

    fireEvent.click(await screen.findByRole("button", { name: "Send⌘↵" }));
    const dialog = await screen.findByRole("alertdialog");

    expect(dialog).toHaveTextContent("Warning");
    expect(dialog).toHaveTextContent("From me@example.com");
    const recipients = screen.getByLabelText("Recipients");
    expect(recipients).toHaveTextContent("Toalice@example.com");
    expect(recipients).toHaveTextContent("Ccbob@example.com");
    expect(dialog).toHaveTextContent("Mentions an attachment");

    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(api.sendComposeSession).toHaveBeenCalledTimes(1));
    expect(api.sendComposeSession.mock.calls[0]?.[2]).toBeUndefined();
  });

  test("a blocked draft only sends after an explicit override", async () => {
    api.checkComposeSafety.mockResolvedValue({ report: blockedReport });
    renderHost();
    openNewMessage();

    fireEvent.click(await screen.findByRole("button", { name: "Send⌘↵" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("Blocked");

    const sendAnyway = screen.getByRole("button", { name: "Send anyway" });
    expect(sendAnyway).toBeDisabled();
    fireEvent.keyDown(dialog, { key: "Enter" });
    expect(api.sendComposeSession).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("checkbox", { name: /override the block/i }));
    fireEvent.click(sendAnyway);

    await waitFor(() => expect(api.sendComposeSession).toHaveBeenCalledTimes(1));
    expect(api.sendComposeSession.mock.calls[0]?.[2]).toBe("tok-1");
  });
});

describe("ComposeHost send later", () => {
  test("three rapid Enters schedule the message once", async () => {
    let finishSchedule: (() => void) | undefined;
    api.scheduleComposeSession.mockImplementation(
      () =>
        new Promise((resolve) => {
          finishSchedule = () =>
            resolve({ ok: true, draft_id: "stored-once", send_at: new Date().toISOString() });
        }),
    );
    renderHost();
    openNewMessage();

    await openSendLater();
    const field = await screen.findByLabelText("Or type a time");
    fireEvent.change(field, { target: { value: "in 2 hours" } });
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/in 2 hours$/));

    fireEvent.keyDown(field, { key: "Enter" });
    fireEvent.keyDown(field, { key: "Enter" });
    fireEvent.keyDown(field, { key: "Enter" });

    await waitFor(() => expect(api.scheduleComposeSession).toHaveBeenCalledTimes(1));
    await act(async () => finishSchedule?.());
    await waitFor(() =>
      expect(toasts.success).toHaveBeenCalledWith(
        expect.stringMatching(/^Send scheduled for /),
        expect.anything(),
      ),
    );
    expect(api.scheduleComposeSession).toHaveBeenCalledTimes(1);
  });
});

describe("saved draft discard preview", () => {
  test("a clean restored draft waits for confirmation, and Cancel keeps it", async () => {
    api.restoreComposeSession.mockResolvedValue(
      session({ draftId: "confirmed-draft", revision: 1 }),
    );
    renderHost();
    act(() => useComposeUi.getState().openCompose(draftIntent("confirmed-draft"), "overlay"));
    await screen.findByLabelText("Subject");
    fireEvent.keyDown(screen.getByRole("button", { name: "More compose actions" }), {
      key: "Backspace",
      metaKey: true,
    });
    expect(await screen.findByText(/saved draft and any linked provider copy/)).toBeVisible();
    expect(api.discardComposeSession).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(api.discardComposeSession).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Subject")).toBeVisible();
  });
  test("editor deletion chords never ask to delete a saved draft", async () => {
    api.restoreComposeSession.mockResolvedValue(
      session({ draftId: "editing-delete", revision: 1 }),
    );
    renderHost();
    act(() => useComposeUi.getState().openCompose(draftIntent("editing-delete"), "overlay"));
    const subject = await screen.findByLabelText("Subject");
    fireEvent.keyDown(subject, { key: "Backspace", metaKey: true });
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Message body" }), {
      key: "Backspace",
      ctrlKey: true,
    });
    expect(screen.queryByText("Discard draft?")).not.toBeInTheDocument();
    expect(api.discardComposeSession).not.toHaveBeenCalled();
  });
});
