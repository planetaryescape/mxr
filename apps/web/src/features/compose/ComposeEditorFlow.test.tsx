/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { useUiPrefs } from "@/state/uiPrefsStore";
import { ComposeHost } from "./ComposeHost";
import { draftIntent, newMessageIntent, useComposeUi } from "./composeUiStore";
import type { ComposeIntent } from "./useComposeSession";
import type { ComposeSessionResponse } from "./api";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
  location: { pathname: "/m/inbox", search: {} },
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

vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
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

vi.mock("./tiptap/TiptapComposeEditor", async () => ({
  TiptapComposeEditor: (await import("./testing/EditorStub")).EditorStub,
}));

vi.mock("./codemirror/CodeMirrorComposeEditor", async () => ({
  CodeMirrorComposeEditor: (await import("./testing/EditorStub")).EditorStub,
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

vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
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

function renderCompose(intent: ComposeIntent = newMessageIntent()) {
  const view = renderWithQueryClient(<ComposeHost />);
  act(() => useComposeUi.getState().openCompose(intent, "overlay"));
  return view;
}

const composeSession: ComposeSessionResponse = {
  session: {
    binding: {
      target: window.location.origin,
      workspace: {
        workspace_id: "compose-flow-owner",
        recovery_generation: "compose-flow-generation",
      },
    },
    draftPath: "/tmp/mxr-compose.md",
    rawContent: "",
    frontmatter: { to: "", cc: "", bcc: "", subject: "", from: "me@example.com", attach: [] },
    bodyMarkdown: "",
    previewHtml: "",
    issues: [],
    accountId: "account-1",
    kind: "new",
  },
};

describe("Compose saving to a stored draft", () => {
  const savedDraftSession: ComposeSessionResponse = {
    session: {
      ...composeSession.session,
      draftId: "draft-42",
      revision: 1,
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
    act(() => useComposeUi.setState({ intent: null }));
  });

  test("saves back into the draft it opened instead of storing another copy", async () => {
    renderCompose(draftIntent("draft-42"));

    const to = await screen.findByLabelText("To");
    fireEvent.keyDown(to, { key: "S", ctrlKey: true, shiftKey: true });

    await waitFor(() =>
      expect(api.saveComposeSession).toHaveBeenCalledWith(
        "/tmp/mxr-compose.md",
        "account-1",
        "draft-42",
        1,
      ),
    );
  });

  test("saves a session that was never a stored draft without an id, so one is created", async () => {
    renderCompose();

    const to = await screen.findByLabelText("To");
    fireEvent.keyDown(to, { key: "S", ctrlKey: true, shiftKey: true });

    await waitFor(() =>
      expect(api.saveComposeSession).toHaveBeenCalledWith(
        "/tmp/mxr-compose.md",
        "account-1",
        undefined,
        undefined,
      ),
    );
  });
});

describe("Compose opening a saved draft that has an HTML body", () => {
  beforeEach(() => {
    rawApi.fetch.mockResolvedValue({ snippets: [] });
    api.fetchContactsAutocomplete.mockResolvedValue([]);
    api.fetchAccounts.mockResolvedValue({ accounts: [] });
  });

  afterEach(() => {
    vi.clearAllMocks();
    act(() => useComposeUi.setState({ intent: null }));
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

    renderCompose(draftIntent("draft-42"));

    expect(await screen.findByText(/This draft has an HTML body/i)).toBeVisible();
    expect(screen.queryByRole("button", { name: /retry/i })).not.toBeInTheDocument();

    const frame = screen.getByTitle("HTML message body") as HTMLIFrameElement;
    const srcDoc = frame.getAttribute("srcdoc") ?? "";
    expect(srcDoc).toContain("Ship on Friday.");
    const preview = new DOMParser().parseFromString(srcDoc, "text/html");
    expect(preview.querySelector('img[src^="http"]')).toBeNull();
  });
});

const hiThereProposal = {
  proposal: {
    proposal_id: "p-1",
    action: "finish",
    base: "hash",
    text: "Hi there",
    changes: [
      { kind: "kept", text: "Hi" },
      { kind: "added", text: " there" },
    ],
    gaps: [],
    provenance: null,
    yours_changed: [],
    composer_version: "1",
  },
};

async function askForHelp() {
  const editor = await screen.findByRole("textbox", { name: "Message body" });
  fireEvent.keyDown(editor, { key: "j", metaKey: true });
  const input = await screen.findByRole("textbox", { name: "Draft instruction" });
  fireEvent.keyDown(input, { key: "Enter" });
  return screen.findByRole("region", { name: "Draft proof" });
}

describe("Draft for me in the sheet", () => {
  beforeEach(() => {
    useUiPrefs.setState({ composeEditor: "codemirror-vim", composeEditorByDraft: {} });
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
    api.startComposeSession.mockResolvedValue({
      session: {
        ...composeSession.session,
        frontmatter: { ...composeSession.session.frontmatter, to: "pat@example.com" },
      },
    });
    rawApi.fetch.mockImplementation(async (path: string) =>
      path.includes("/mail/compose/assist") ? hiThereProposal : { snippets: [] },
    );
  });

  test("Esc on a proof keeps the user's text and leaves the sheet open", async () => {
    api.startComposeSession.mockResolvedValue({
      session: {
        ...composeSession.session,
        frontmatter: { ...composeSession.session.frontmatter, to: "pat@example.com" },
        bodyMarkdown: "Hi",
      },
    });
    renderCompose(newMessageIntent());
    const proof = await askForHelp();

    fireEvent.keyDown(proof, { key: "Escape" });

    expect(screen.queryByRole("region", { name: "Draft proof" })).not.toBeInTheDocument();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(useComposeUi.getState().intent).not.toBeNull();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Message body" })).toHaveValue("Hi");
  });
});
