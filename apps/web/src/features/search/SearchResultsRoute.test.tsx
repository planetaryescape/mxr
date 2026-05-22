/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { SearchResultsRoute } from "./SearchResultsRoute";
import type { MessageGroupView, MessageRowView } from "@/features/mailbox/types";
import { useMailboxPane } from "@/state/mailboxPaneStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
  search: { q: "invoice", mode: "lexical", sort: "relevance" } as {
    q?: string;
    mode?: "lexical" | "semantic" | "hybrid";
    sort?: "relevance" | "newest" | "oldest";
    account?: string;
  },
}));

const searchApi = vi.hoisted(() => ({
  createSavedSearch: vi.fn<(input: unknown) => Promise<unknown>>(),
  fetchSavedSearches: vi.fn<() => Promise<unknown>>(),
  fetchSearch: vi.fn<(params: unknown, opts?: unknown) => Promise<unknown>>(),
}));

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => router.navigate,
  useSearch: () => router.search,
}));

vi.mock("./api", () => ({
  createSavedSearch: searchApi.createSavedSearch,
  fetchSavedSearches: searchApi.fetchSavedSearches,
  fetchSearch: searchApi.fetchSearch,
  searchKey: (params: unknown) => ["search", params],
}));

// MailboxList is the canonical list and has its own tests; here we only
// verify the search route hands it the result groups and renders the
// search chrome. Stub it to surface the rows it received.
vi.mock("@/features/mailbox/MailboxList", () => ({
  MailboxList: ({ groups }: { groups: MessageGroupView[] }) => (
    <div data-testid="mailbox-list">
      {groups
        .flatMap((group) => group.rows)
        .map((row) => (
          <div key={row.id}>{row.subject}</div>
        ))}
    </div>
  ),
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn<(message: string, options?: unknown) => void>(),
    success: vi.fn<(message: string) => void>(),
  },
}));

const rows: MessageRowView[] = ["msg-1", "msg-2"].map((id, index) => ({
  id,
  kind: "thread",
  thread_id: `thread-${index + 1}`,
  provider_id: `provider-${index + 1}`,
  sender: `Sender ${index + 1}`,
  subject: `Subject ${index + 1}`,
  snippet: "Snippet",
  date: "2026-05-11T10:00:00Z",
  date_label: "May 11",
  date_full: "May 11, 2026, 10:00 AM",
  date_relative: "now",
  unread: false,
  starred: false,
  has_attachments: false,
}));

function renderWithQueryClient(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

describe("SearchResultsRoute", () => {
  beforeEach(() => {
    router.search = { q: "invoice", mode: "lexical", sort: "relevance" };
    useMailboxPane.setState({
      activePane: "sidebar",
      sidebarIndex: 0,
      suppressNextReaderFocus: false,
    });
    searchApi.fetchSavedSearches.mockResolvedValue({ searches: [] });
    searchApi.fetchSearch.mockResolvedValue({
      scope: "threads",
      sort: "relevance",
      mode: "lexical",
      total: rows.length,
      has_more: false,
      groups: [{ id: "today", label: "Today", rows }],
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  test("renders results through the shared mailbox list", async () => {
    renderWithQueryClient(<SearchResultsRoute />);

    expect(await screen.findByTestId("mailbox-list")).toBeVisible();
    expect(screen.getByText("Subject 1")).toBeVisible();
    expect(screen.getByText("Subject 2")).toBeVisible();
    expect(screen.getByText(/2 results/i)).toBeVisible();
  });

  test("submitting the query blurs the input and navigates", async () => {
    renderWithQueryClient(<SearchResultsRoute />);

    await screen.findByTestId("mailbox-list");
    const input = screen.getByLabelText("Search query") as HTMLInputElement;
    input.focus();
    fireEvent.change(input, { target: { value: "alice" } });
    const form = input.closest("form");
    if (!form) throw new Error("missing search form");
    fireEvent.submit(form);

    expect(router.navigate).toHaveBeenCalledWith({
      to: "/search",
      search: {
        q: "alice",
        mode: "lexical",
        sort: "relevance",
        scope: "threads",
        account: undefined,
      },
    });
    await waitFor(() => expect(document.activeElement).not.toBe(input));
    // Control moves to the results list so j/k/o work without a click.
    expect(useMailboxPane.getState().activePane).toBe("mailbox");
  });

  test("pressing / refocuses the query input", async () => {
    renderWithQueryClient(<SearchResultsRoute />);

    const input = (await screen.findByLabelText("Search query")) as HTMLInputElement;
    input.blur();
    expect(document.activeElement).not.toBe(input);

    fireEvent.keyDown(document.body, { key: "/" });

    expect(document.activeElement).toBe(input);
  });
});
