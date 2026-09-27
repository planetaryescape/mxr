/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { SearchResultsRoute } from "./SearchResultsRoute";
import { usePendingMailOps } from "@/features/mail-actions/pendingMailOps";
import type { MessageGroupView, MessageRowView } from "@/features/mailbox/types";
import { useMailboxPane } from "@/state/mailboxPaneStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
  search: { q: "invoice", mode: "lexical", sort: "relevance" } as {
    q?: string;
    mode?: "lexical" | "semantic" | "hybrid";
    sort?: "relevance" | "newest" | "oldest";
    groupBy?: "from" | "list" | "category";
    account?: string;
  },
}));

const searchApi = vi.hoisted(() => ({
  createSavedSearch: vi.fn<(input: unknown) => Promise<unknown>>(),
  fetchSavedSearches: vi.fn<() => Promise<unknown>>(),
  fetchSearch: vi.fn<(params: unknown, opts?: unknown) => Promise<unknown>>(),
  fetchSearchGroups: vi.fn<(params: unknown, opts?: unknown) => Promise<unknown>>(),
}));

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => router.navigate,
  useSearch: () => router.search,
}));

vi.mock("./api", () => ({
  createSavedSearch: searchApi.createSavedSearch,
  fetchSavedSearches: searchApi.fetchSavedSearches,
  fetchSearch: searchApi.fetchSearch,
  fetchSearchGroups: searchApi.fetchSearchGroups,
  searchKey: (params: unknown) => ["search", params],
  searchGroupsKey: (params: unknown) => ["search-groups", params],
}));

vi.mock("@/features/accounts/api", () => ({
  fetchAccounts: () => Promise.resolve({ accounts: [] }),
}));

// The list-and-reader layout has its own tests (MailboxList); here we only
// check what the search route hands it: projected rows, meta, toolbar,
// paging and the empty state.
vi.mock("@/features/mailbox/ListWithReader", () => ({
  ListWithReader: (props: {
    meta?: ReactNode;
    toolbar?: ReactNode;
    groups: MessageGroupView[];
    empty: ReactNode;
    hasMore?: boolean;
    onLoadMore?: () => void;
  }) => (
    <div>
      <p>{props.meta}</p>
      {props.toolbar}
      {props.groups.length === 0 ? (
        props.empty
      ) : (
        <ul data-testid="mailbox-list">
          {props.groups
            .flatMap((group) => group.rows)
            .map((row) => (
              <li key={row.id}>{row.subject}</li>
            ))}
        </ul>
      )}
      {props.hasMore ? (
        <button type="button" onClick={props.onLoadMore}>
          Load more
        </button>
      ) : null}
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
    searchApi.fetchSearchGroups.mockResolvedValue({
      query: "invoice",
      group_by: "from",
      total: rows.length,
      groups: [
        {
          key: "sender@example.com",
          label: "Sender <sender@example.com>",
          count: 2,
          unread: 1,
          oldest: 1_779_000_000,
          newest: 1_779_086_400,
        },
      ],
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
    usePendingMailOps.setState({ ops: [] });
  });

  test("renders results through the shared mailbox list", async () => {
    renderWithQueryClient(<SearchResultsRoute />);

    expect(await screen.findByTestId("mailbox-list")).toBeVisible();
    expect(screen.getByText("Subject 1")).toBeVisible();
    expect(screen.getByText("Subject 2")).toBeVisible();
    expect(screen.getByText("2 results")).toBeVisible();
  });

  test("loads additional pages through offset pagination", async () => {
    const baseRow = rows[0] as MessageRowView;
    const nextRows: MessageRowView[] = [
      {
        ...baseRow,
        id: "msg-3",
        thread_id: "thread-3",
        provider_id: "provider-3",
        subject: "Subject 3",
      },
    ];
    searchApi.fetchSearch
      .mockResolvedValueOnce({
        scope: "threads",
        sort: "relevance",
        mode: "lexical",
        total: 3,
        has_more: true,
        next_offset: 100,
        groups: [{ id: "today", label: "Today", rows }],
      })
      .mockResolvedValueOnce({
        scope: "threads",
        sort: "relevance",
        mode: "lexical",
        total: 3,
        has_more: false,
        next_offset: null,
        groups: [{ id: "today", label: "Today", rows: nextRows }],
      });

    renderWithQueryClient(<SearchResultsRoute />);

    expect(await screen.findByText("Subject 1")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));

    expect(await screen.findByText("Subject 3")).toBeVisible();
    expect(searchApi.fetchSearch).toHaveBeenLastCalledWith(
      expect.objectContaining({ offset: 100, limit: 100 }),
      expect.anything(),
    );
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
        verdict: undefined,
        groupBy: "from",
        account: undefined,
      },
    });
    await waitFor(() => expect(document.activeElement).not.toBe(input));
    // Control moves to the results list so j/k/o work without a click.
    expect(useMailboxPane.getState().activePane).toBe("mailbox");
  });

  test("a pending trash hides the row; a pending archive doesn't", async () => {
    renderWithQueryClient(<SearchResultsRoute />);
    await screen.findByText("Subject 1");

    act(() => {
      usePendingMailOps.getState().add({
        id: "op-archive",
        action: "archive",
        messageIds: new Set(["msg-1"]),
      });
      usePendingMailOps.getState().add({
        id: "op-trash",
        action: "trash",
        messageIds: new Set(["msg-2"]),
      });
    });

    expect(screen.getByText("Subject 1")).toBeVisible();
    expect(screen.queryByText("Subject 2")).not.toBeInTheDocument();
  });

  test("no exact matches offers hybrid search", async () => {
    searchApi.fetchSearch.mockResolvedValue({
      scope: "threads",
      sort: "relevance",
      mode: "lexical",
      total: 0,
      has_more: false,
      groups: [],
    });
    renderWithQueryClient(<SearchResultsRoute />);

    fireEvent.click(await screen.findByRole("button", { name: "Try hybrid search" }));

    expect(router.navigate).toHaveBeenCalledWith({
      to: "/search",
      search: expect.objectContaining({ q: "invoice", mode: "hybrid" }),
    });
  });
});
