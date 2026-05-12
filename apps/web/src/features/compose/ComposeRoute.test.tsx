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
  refreshComposeSession: vi.fn<(draftPath: string) => Promise<unknown>>(),
  restoreComposeSession: vi.fn<(draftId: string) => Promise<unknown>>(),
  saveComposeSession: vi.fn<(draftPath: string, accountId: string) => Promise<unknown>>(),
  sendComposeSession: vi.fn<(draftPath: string, accountId: string) => Promise<unknown>>(),
  startComposeSession:
    vi.fn<(kind: string, messageId?: string) => Promise<ComposeSessionResponse>>(),
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
  refreshComposeSession: api.refreshComposeSession,
  restoreComposeSession: api.restoreComposeSession,
  saveComposeSession: api.saveComposeSession,
  sendComposeSession: api.sendComposeSession,
  startComposeSession: api.startComposeSession,
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
    rawApi.fetch.mockResolvedValue({ snippets: [] });
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

  test("does not re-save an unchanged draft", async () => {
    renderWithQueryClient(<ComposeRoute />);

    const save = await screen.findByRole("button", { name: /save/i });
    fireEvent.click(save);

    expect(api.updateComposeSession).not.toHaveBeenCalled();
  });

  test("shows keyboard shortcuts for compose actions", async () => {
    renderWithQueryClient(<ComposeRoute />);

    expect(await screen.findByRole("button", { name: /refresh/i })).toBeVisible();
    expect(screen.getByText("⇧⌘R")).toBeVisible();
    expect(screen.getByText("⌘S")).toBeVisible();
    expect(screen.getByText("⌘⌫")).toBeVisible();
    expect(screen.getByText("⌘Enter")).toBeVisible();
    expect(screen.getByText("⇧⌘A")).toBeVisible();
  });
});
