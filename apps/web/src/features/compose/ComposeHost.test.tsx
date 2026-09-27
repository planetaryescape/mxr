/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { useUiPrefs } from "@/state/uiPrefsStore";
import { useUndo } from "@/state/undoStore";
import type { ComposeSessionResponse, DraftSafetyReport } from "./api";
import { ComposeHost } from "./ComposeHost";
import { inviteReplyIntent, useComposeUi } from "./composeUiStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
  location: { pathname: "/m/inbox", search: {} },
}));

const api = vi.hoisted(() => ({
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
vi.mock("@/features/time/api", async (importOriginal) => {
  const { fakeResolvedTime } = await import("@/features/time/testing");
  return {
    ...(await importOriginal<typeof import("@/features/time/api")>()),
    resolveTime: (input: string) => Promise.resolve(fakeResolvedTime(input)),
  };
});
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

  test("Ctrl+O overrides a block and sends, like the TUI", async () => {
    api.checkComposeSafety.mockResolvedValue({ report: blockedReport });
    renderHost();
    openNewMessage();

    fireEvent.click(await screen.findByRole("button", { name: "Send⌘↵" }));
    fireEvent.keyDown(await screen.findByRole("alertdialog"), { key: "o", ctrlKey: true });

    await waitFor(() => expect(api.sendComposeSession).toHaveBeenCalledTimes(1));
    expect(api.sendComposeSession.mock.calls[0]?.[2]).toBe("tok-1");
  });

  test("adding a suggested Cc from the dialog re-checks the edited draft", async () => {
    api.suggestComposeCollaborators.mockResolvedValue({
      suggestions: [
        {
          email: "carol@example.com",
          display_name: "Carol",
          reason: "On the last 3 threads",
          confidence: "high",
        },
      ],
    });
    api.checkComposeSafety
      .mockResolvedValueOnce({ report: warnReport })
      .mockResolvedValueOnce({ report: cleanReport });
    renderHost();
    openNewMessage();

    // The header row offers the same suggestion once recipients settle.
    expect(
      await screen.findByRole("button", { name: "Add carol@example.com to Cc" }, { timeout: 3000 }),
    ).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "Send⌘↵" }));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Add carol@example.com to Cc" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Send" }));

    await waitFor(() => expect(api.sendComposeSession).toHaveBeenCalledTimes(1));
    expect(api.checkComposeSafety).toHaveBeenCalledTimes(2);
    const saved = api.updateComposeSession.mock.calls.at(-1)?.[0] as {
      frontmatter: { cc: string };
    };
    expect(saved.frontmatter.cc).toBe("carol@example.com");
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

    fireEvent.click(await screen.findByRole("button", { name: "Send later" }));
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

  test("a scheduled send can be cancelled from its confirmation toast", async () => {
    api.scheduleComposeSession.mockResolvedValue({
      ok: true,
      draft_id: "stored-1",
      send_at: new Date().toISOString(),
    });
    api.cancelScheduledSend.mockResolvedValue({ ok: true });
    renderHost();
    openNewMessage();

    fireEvent.click(await screen.findByRole("button", { name: "Send later" }));
    fireEvent.click(await screen.findByRole("button", { name: /In 2 hours.*\d\d:\d\d/ }));

    await waitFor(() =>
      expect(toasts.success).toHaveBeenCalledWith(
        expect.stringMatching(/^Send scheduled for \w+ \d+ \w+, \d\d:\d\d$/),
        expect.anything(),
      ),
    );
    // The bridge stores and schedules the compose file itself, so reply
    // headers and the From alias carry over.
    const request = api.scheduleComposeSession.mock.calls[0]?.[0];
    expect(request?.draftPath).toBeTruthy();
    expect(request?.sendAt).toBeInstanceOf(Date);
    const scheduledId = "stored-1";
    // The composer closes like a send.
    await waitFor(() => expect(screen.queryByLabelText("Subject")).not.toBeInTheDocument());

    const options = toasts.success.mock.calls.find(([title]) =>
      title.startsWith("Send scheduled for "),
    )?.[1] as {
      action: { label: string; onClick: () => void };
    };
    expect(options.action.label).toBe("Cancel");
    act(() => options.action.onClick());

    await waitFor(() => expect(api.cancelScheduledSend).toHaveBeenCalledWith(scheduledId));
    await waitFor(() =>
      expect(toasts.success).toHaveBeenCalledWith("Scheduled send cancelled", {
        description: "The message is kept in Drafts.",
      }),
    );
  });
});

describe("ComposeHost send and remind", () => {
  beforeEach(() => {
    useUiPrefs.setState({ undoSendSeconds: 0 });
    api.setAutoReminder.mockResolvedValue({ ok: true });
    api.cancelAutoReminder.mockResolvedValue({ ok: true });
  });

  async function chooseRemindIn(label: string) {
    const trigger = await screen.findByRole("button", { name: "More send options" });
    fireEvent.keyDown(trigger, { key: "Enter" });
    const submenu = await screen.findByRole("menuitem", {
      name: /Send and remind me if no reply in/,
    });
    fireEvent.keyDown(submenu, { key: "ArrowRight" });
    // Each preset shows the exact time it resolves to before it is chosen.
    const item = await screen.findByRole("menuitem", { name: new RegExp(`^${label}`) });
    await waitFor(() => expect(item).toHaveTextContent(/\d\d:\d\d$/));
    fireEvent.click(item);
  }

  test("sends, then sets a cancellable reminder for the sent message", async () => {
    api.sendComposeSession.mockResolvedValue({
      ok: true,
      draft_id: "draft-1",
      message_id: "msg-9",
    });
    renderHost();
    openNewMessage();

    const before = Date.now();
    await chooseRemindIn("3 days");

    await waitFor(() => expect(api.setAutoReminder).toHaveBeenCalledTimes(1));
    expect(api.sendComposeSession).toHaveBeenCalledTimes(1);
    const [sentId, remindAt] = api.setAutoReminder.mock.calls[0] ?? [];
    expect(sentId).toBe("msg-9");
    const days = ((remindAt as Date).getTime() - before) / 86_400_000;
    expect(days).toBeGreaterThan(2.99);
    expect(days).toBeLessThan(3.01);

    await waitFor(() =>
      expect(toasts.success).toHaveBeenCalledWith(
        expect.stringMatching(/^Reminder set for \w+ \d+ \w+, \d\d:\d\d$/),
        expect.anything(),
      ),
    );
    const options = toasts.success.mock.calls.find(([title]) =>
      title.startsWith("Reminder set for "),
    )?.[1] as {
      action: { label: string; onClick: () => void };
    };
    act(() => options.action.onClick());
    await waitFor(() => expect(api.cancelAutoReminder).toHaveBeenCalledWith("msg-9"));
  });

  test("says so when the bridge can't identify the sent message", async () => {
    api.sendComposeSession.mockResolvedValue({ ok: true, draft_id: "draft-1" });
    renderHost();
    openNewMessage();

    await chooseRemindIn("1 day");

    await waitFor(() =>
      expect(toasts.warning).toHaveBeenCalledWith("Sent, but no reminder was set", {
        description: "The mxr bridge did not return the sent message id.",
      }),
    );
    expect(api.sendComposeSession).toHaveBeenCalledTimes(1);
    expect(api.setAutoReminder).not.toHaveBeenCalled();
  });
});

describe("ComposeHost invite replies", () => {
  test("open a session carrying the invite action, and schedule it with the answer", async () => {
    api.scheduleComposeSession.mockResolvedValue({
      ok: true,
      draft_id: "stored-invite",
      send_at: new Date().toISOString(),
    });
    renderHost();
    act(() => {
      useComposeUi.getState().openCompose(inviteReplyIntent("m-3", "tentative"), "overlay");
    });

    expect((await screen.findAllByText("Tentative with comment"))[0]).toBeVisible();
    expect(api.startComposeSession).toHaveBeenCalledWith("invite_reply", "m-3", "tentative");

    // Scheduling goes through the compose session, which carries the RSVP,
    // so invite replies can be sent later like any other reply.
    fireEvent.click(await screen.findByRole("button", { name: "Send later" }));
    fireEvent.click(await screen.findByRole("button", { name: /In 2 hours.*\d\d:\d\d/ }));
    await waitFor(() => expect(api.scheduleComposeSession).toHaveBeenCalledTimes(1));
  });
});

describe("ComposeHost after a delayed send", () => {
  test("a send landing later does not close the composer the user moved on to", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    renderHost();
    openNewMessage();

    fireEvent.click(await screen.findByRole("button", { name: "Send⌘↵" }));
    await waitFor(() => expect(useUndo.getState().pendingSendCancel).not.toBeNull());
    fireEvent.click(screen.getByRole("button", { name: /close composer/i }));
    act(() => {
      useComposeUi
        .getState()
        .openCompose({ key: "compose:reply:m-2", title: "Reply", kind: "reply", messageId: "m-2" });
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(6000);
    });
    await waitFor(() => expect(api.sendComposeSession).toHaveBeenCalledTimes(1));
    expect(useComposeUi.getState().intent?.key).toBe("compose:reply:m-2");
  });

  test("a failed discard is reported and keeps the draft open", async () => {
    api.discardComposeSession.mockRejectedValue(new Error("permission denied"));
    renderHost();
    openNewMessage();

    const subject = await screen.findByLabelText("Subject");
    fireEvent.keyDown(subject, { key: "Backspace", metaKey: true });

    await waitFor(() =>
      expect(toasts.error).toHaveBeenCalledWith("Discard failed", {
        description: "permission denied",
      }),
    );
    expect(screen.getByLabelText("Subject")).toBeInTheDocument();
  });
});
