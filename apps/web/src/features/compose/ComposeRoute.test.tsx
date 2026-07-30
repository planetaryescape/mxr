/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { ComposeRoute } from "./ComposeRoute";
import type { ComposeSessionResponse } from "./api";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
  location: { pathname: "/compose/new", search: {} },
}));

const api = vi.hoisted(() => ({
  discardComposeSession: vi.fn<(draftPath: string) => Promise<unknown>>(),
  fetchAccounts: vi.fn<() => Promise<unknown>>(),
  fetchContactsAutocomplete: vi.fn<(query: string) => Promise<unknown[]>>(),
  refreshComposeSession: vi.fn<(draftPath: string) => Promise<unknown>>(),
  restoreComposeSession: vi.fn<(draftId: string) => Promise<unknown>>(),
  saveComposeSession:
    vi.fn<(draftPath: string, accountId: string, draftId?: string) => Promise<unknown>>(),
  sendComposeSession: vi.fn<(draftPath: string, accountId: string) => Promise<unknown>>(),
  startComposeSession:
    vi.fn<(kind: string, messageId?: string) => Promise<ComposeSessionResponse>>(),
  suggestComposeCollaborators:
    vi.fn<(draftPath: string, accountId: string) => Promise<{ suggestions: unknown[] }>>(),
  updateComposeSession: vi.fn<(input: unknown) => Promise<ComposeSessionResponse>>(),
  uploadComposeAttachment: vi.fn<(input: unknown) => Promise<unknown>>(),
}));

const rawApi = vi.hoisted(() => ({
  fetch: vi.fn<(path: string) => Promise<unknown>>(),
}));

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

vi.mock("@/api/client", () => ({
  apiFetch: rawApi.fetch,
}));

vi.mock("./api", () => ({
  discardComposeSession: api.discardComposeSession,
  fetchAccounts: api.fetchAccounts,
  fetchContactsAutocomplete: api.fetchContactsAutocomplete,
  refreshComposeSession: api.refreshComposeSession,
  restoreComposeSession: api.restoreComposeSession,
  saveComposeSession: api.saveComposeSession,
  sendComposeSession: api.sendComposeSession,
  startComposeSession: api.startComposeSession,
  suggestComposeCollaborators: api.suggestComposeCollaborators,
  updateComposeSession: api.updateComposeSession,
  uploadComposeAttachment: api.uploadComposeAttachment,
}));

vi.mock("./tiptap/TiptapComposeEditor", () => ({
  TiptapComposeEditor: ({ autoFocus }: { autoFocus?: boolean }) => (
    <textarea aria-label="Message body" autoFocus={autoFocus} />
  ),
}));

vi.mock("./codemirror/CodeMirrorComposeEditor", () => ({
  CodeMirrorComposeEditor: ({ autoFocus }: { autoFocus?: boolean }) => (
    <textarea aria-label="Message body" autoFocus={autoFocus} />
  ),
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn<(message: string, options?: unknown) => void>(),
    success: vi.fn<(message: string, options?: unknown) => void>(),
  },
}));

function renderWithQueryClient(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

const composeSession: ComposeSessionResponse = {
  session: {
    draftPath: "/tmp/mxr-compose.md",
    rawContent: "",
    frontmatter: { to: "", cc: "", bcc: "", subject: "", from: "me@example.com", attach: [] },
    bodyMarkdown: "",
    issues: [],
    accountId: "account-1",
    kind: "new",
  },
};

describe("ComposeRoute keyboard flow", () => {
  beforeEach(() => {
    try {
      window.localStorage?.clear();
    } catch {
      // jsdom may disable localStorage for opaque test origins.
    }
    rawApi.fetch.mockResolvedValue({ snippets: [] });
    api.fetchContactsAutocomplete.mockResolvedValue([]);
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
    api.startComposeSession.mockResolvedValue(composeSession);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  test("starts keyboard flow in To and reveals Cc with ctrl-shift-c", async () => {
    renderWithQueryClient(<ComposeRoute />);

    const to = await screen.findByLabelText("To");

    await waitFor(() => expect(to).toHaveFocus());

    fireEvent.keyDown(to, { key: "C", ctrlKey: true, shiftKey: true });

    const cc = await screen.findByLabelText("Cc");
    await waitFor(() => expect(cc).toHaveFocus());
  });

  test("removes address chips with backspace", async () => {
    api.startComposeSession.mockResolvedValue({
      session: {
        ...composeSession.session,
        frontmatter: {
          ...composeSession.session.frontmatter,
          to: "alpha@example.com, beta@example.com",
        },
      },
    });
    renderWithQueryClient(<ComposeRoute />);

    const removeBeta = await screen.findByRole("button", { name: "Remove beta@example.com" });
    removeBeta.focus();

    fireEvent.keyDown(removeBeta, { key: "Backspace" });

    expect(screen.queryByText("beta@example.com")).not.toBeInTheDocument();
    expect(screen.getByText("alpha@example.com")).toBeVisible();
  });

  test("does not mark an unchanged loaded draft dirty", async () => {
    api.startComposeSession.mockResolvedValue({
      session: {
        ...composeSession.session,
        frontmatter: {
          ...composeSession.session.frontmatter,
          to: "alpha@example.com",
          cc: "gamma@example.com",
        },
      },
    });
    renderWithQueryClient(<ComposeRoute />);

    // Rendering recipient chips for a loaded draft must not flip the autosave
    // fingerprint: the status stays "Saved", not "Unsaved changes".
    await screen.findByLabelText("To");
    expect(await screen.findByText(/saved/i)).toBeVisible();
    expect(screen.queryByText("Unsaved changes")).not.toBeInTheDocument();
    expect(api.updateComposeSession).not.toHaveBeenCalled();
  });

  test("surfaces the send shortcut and writing controls", async () => {
    renderWithQueryClient(<ComposeRoute />);

    const send = await screen.findByRole("button", { name: "Send⌘↵" });
    expect(send).toBeVisible();
    expect(screen.getByText("⌘↵")).toBeVisible();
    expect(screen.getByRole("button", { name: "Send later" })).toBeVisible();
    expect(screen.getByRole("button", { name: /attach/i })).toBeVisible();
    expect(screen.getByRole("button", { name: /more compose actions/i })).toBeVisible();
  });
});

describe("ComposeRoute saving to a stored draft", () => {
  const savedDraftSession: ComposeSessionResponse = {
    session: {
      ...composeSession.session,
      frontmatter: {
        ...composeSession.session.frontmatter,
        to: "alice@example.com",
        subject: "Quarterly plan",
      },
      bodyMarkdown: "First pass.",
    },
  };

  beforeEach(() => {
    rawApi.fetch.mockResolvedValue({ snippets: [] });
    api.fetchContactsAutocomplete.mockResolvedValue([]);
    api.fetchAccounts.mockResolvedValue({
      accounts: [
        {
          account_id: "account-1",
          name: "Work",
          email: "me@example.com",
          provider_kind: "fake",
          enabled: true,
          is_default: true,
          capabilities: {
            supports_send: true,
            supports_local_drafts: true,
            supports_server_drafts: true,
          },
        },
      ],
    });
    api.restoreComposeSession.mockResolvedValue(savedDraftSession);
    api.startComposeSession.mockResolvedValue(composeSession);
    api.saveComposeSession.mockResolvedValue({ ok: true });
    api.suggestComposeCollaborators.mockResolvedValue({ suggestions: [] });
  });

  afterEach(() => {
    vi.clearAllMocks();
    router.location = { pathname: "/compose/new", search: {} };
  });

  // Without the id the daemon has nothing to update and stores a second copy,
  // so every save of an opened draft leaves another indistinguishable row.
  test("saves back into the draft it opened instead of storing another copy", async () => {
    router.location = { pathname: "/compose/draft-42", search: {} };
    renderWithQueryClient(<ComposeRoute />);

    const to = await screen.findByLabelText("To");
    fireEvent.keyDown(to, { key: "S", ctrlKey: true, shiftKey: true });

    await waitFor(() =>
      expect(api.saveComposeSession).toHaveBeenCalledWith(
        "/tmp/mxr-compose.md",
        "account-1",
        "draft-42",
      ),
    );
  });

  test("saves a session that was never a stored draft without an id, so one is created", async () => {
    renderWithQueryClient(<ComposeRoute />);

    const to = await screen.findByLabelText("To");
    fireEvent.keyDown(to, { key: "S", ctrlKey: true, shiftKey: true });

    await waitFor(() =>
      expect(api.saveComposeSession).toHaveBeenCalledWith(
        "/tmp/mxr-compose.md",
        "account-1",
        undefined,
      ),
    );
  });
});

describe("ComposeRoute opening a saved draft that has an HTML body", () => {
  beforeEach(() => {
    router.location = { pathname: "/compose/draft-42", search: {} };
    rawApi.fetch.mockResolvedValue({ snippets: [] });
    api.fetchContactsAutocomplete.mockResolvedValue([]);
    api.fetchAccounts.mockResolvedValue({ accounts: [] });
  });

  afterEach(() => {
    vi.clearAllMocks();
    router.location = { pathname: "/compose/new", search: {} };
  });

  test("previews the document read-only instead of offering a hopeless retry", async () => {
    api.restoreComposeSession.mockRejectedValue(
      Object.assign(
        new Error("this draft has an HTML body, which the compose editor cannot edit"),
        {
          status: 409,
          code: "html_draft_not_editable",
          details: {
            previewHtml: '<p>Ship on Friday.</p><img src="https://tracker.example.com/p.png">',
          },
        },
      ),
    );

    renderWithQueryClient(<ComposeRoute />);

    expect(await screen.findByText(/This draft has an HTML body/i)).toBeVisible();
    // Retrying a permanent refusal can only fail again.
    expect(screen.queryByRole("button", { name: /retry/i })).not.toBeInTheDocument();

    const frame = screen.getByTitle("HTML message body") as HTMLIFrameElement;
    const srcDoc = frame.getAttribute("srcdoc") ?? "";
    expect(srcDoc).toContain("Ship on Friday.");
    const preview = new DOMParser().parseFromString(srcDoc, "text/html");
    expect(preview.querySelector('img[src^="http"]')).toBeNull();
  });

  test("still offers a retry for a failure that might be transient", async () => {
    api.restoreComposeSession.mockRejectedValue(
      new Error("failed to connect to mxr daemon at /tmp/mxr.sock"),
    );

    renderWithQueryClient(<ComposeRoute />);

    expect(await screen.findByText(/failed to connect to mxr daemon/i)).toBeVisible();
    expect(screen.getByRole("button", { name: /retry/i })).toBeVisible();
    expect(screen.queryByText(/This draft has an HTML body/i)).not.toBeInTheDocument();
  });
});
