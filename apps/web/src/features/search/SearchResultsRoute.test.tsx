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
    sort?: "relevance" | "newest" | "oldest" | "verdict";
    scope?: "threads" | "messages" | "attachments" | "triage";
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
const accountApi = vi.hoisted(() => ({ accounts: [] as Array<Record<string, unknown>> }));
const recoveryState = vi.hoisted(() => ({ restored: false }));

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
  fetchAccounts: () => Promise.resolve({ accounts: accountApi.accounts }),
}));

vi.mock("@/features/recovery/RecoveryNotice", () => ({
  isRestoredProfile: (
    recovery: { restore_pending: boolean; source_owner_stopped: boolean } | null,
  ) => Boolean(recovery && (recovery.restore_pending || recovery.source_owner_stopped)),
  useRecoveryStatus: () => ({
    data: recoveryState.restored ? { restore_pending: false, source_owner_stopped: true } : null,
    isSuccess: true,
    isError: false,
  }),
}));

// The list-and-reader layout has its own tests (MailboxList); here we only
// check what the search route hands it: projected rows, meta, toolbar,
// paging and the empty state.
vi.mock("@/features/mailbox/ListWithReader", () => ({
  ListWithReader: (props: {
    title: string;
    meta?: ReactNode;
    toolbar?: ReactNode;
    groups: MessageGroupView[];
    empty: ReactNode;
    hasMore?: boolean;
    onLoadMore?: () => void;
    onFilterKey?: () => void;
  }) => (
    <div>
      <h1>{props.title}</h1>
      <p>{props.meta}</p>
      {props.toolbar}
      <button onClick={props.onFilterKey}>Run filter shortcut</button>
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

function lastSearchParams(): { sort?: string; limit?: number } {
  return (searchApi.fetchSearch.mock.lastCall?.[0] ?? {}) as { sort?: string; limit?: number };
}

function navigatedSearch(): { sort?: string } {
  const call = router.navigate.mock.lastCall?.[0] as { search: { sort?: string } };
  return call.search;
}

const facet = (label: string) => ({
  key: `${label}@example.com`,
  label,
  count: 2,
  unread: 1,
  oldest: 1_779_000_000,
  newest: 1_779_086_400,
});

describe("SearchResultsRoute", () => {
  beforeEach(() => {
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      value: vi.fn<() => void>(),
    });
    accountApi.accounts = [];
    recoveryState.restored = false;
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
    expect(screen.getByText("2 conversations")).toBeVisible();
  });

  test("says so above the results when the daemon answered from the limited search", async () => {
    const notice =
      "Search is limited while the index is unavailable: subject, sender and preview only. Restart the daemon or run `mxr doctor --reindex`.";
    searchApi.fetchSearch.mockResolvedValue({
      scope: "threads",
      sort: "relevance",
      mode: "lexical",
      total: rows.length,
      has_more: false,
      groups: [{ id: "today", label: "Today", rows }],
      degraded: {
        reason: "index_stopped",
        searched_fields: ["subject", "sender", "preview"],
        notice,
      },
    });

    renderWithQueryClient(<SearchResultsRoute />);

    expect(await screen.findByRole("status")).toHaveTextContent(notice);
    expect(screen.getByText("Subject 1")).toBeVisible();
  });

  test("shows no limited-search line when the index answered", async () => {
    renderWithQueryClient(<SearchResultsRoute />);

    expect(await screen.findByTestId("mailbox-list")).toBeVisible();
    expect(screen.queryByRole("status")).toBeNull();
  });

  test("the filter shortcut focuses a control, and Escape returns to the filter button", async () => {
    renderWithQueryClient(<SearchResultsRoute />);
    fireEvent.click(screen.getByRole("button", { name: "Run filter shortcut" }));
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Show" })).toHaveFocus());
    fireEvent.keyDown(screen.getByRole("combobox", { name: "Show" }), { key: "Escape" });
    expect(screen.queryByRole("combobox", { name: "Show" })).toBeNull();
    expect(screen.getByRole("button", { name: /Filters/ })).toHaveFocus();
  });

  test("the query is the page's title, and the filters wait behind one button", async () => {
    searchApi.fetchSearchGroups.mockResolvedValue({
      query: "invoice",
      group_by: "from",
      total: rows.length,
      groups: [facet("Ada"), facet("Sam")],
    });
    renderWithQueryClient(<SearchResultsRoute />);

    expect(await screen.findByRole("heading", { name: "invoice", level: 1 })).toBeVisible();
    expect(screen.getByRole("radiogroup", { name: "Search mode" })).toBeVisible();
    expect(screen.queryByTestId("search-filters")).toBeNull();
    expect(screen.queryByRole("combobox", { name: "Narrow by" })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /Filters/ }));
    expect(screen.getByTestId("search-filters")).toBeVisible();
    expect(await screen.findByRole("combobox", { name: "Narrow by" })).toBeVisible();
    // Facet counts are messages, so beside conversation rows they are left off.
    expect(screen.getByRole("button", { name: /Ada/ }).textContent).not.toMatch(/\d/);
  });

  test("search account options include cached accounts from restored profiles", async () => {
    recoveryState.restored = true;
    accountApi.accounts = [
      { account_id: "active", name: "Active", email: "active@example.com", enabled: true },
      { account_id: "personal", name: "Personal", email: "personal@example.com", enabled: true },
      { account_id: "cached", name: "Cached", email: "cached@example.com", enabled: false },
    ];
    renderWithQueryClient(<SearchResultsRoute />);

    fireEvent.click(await screen.findByRole("button", { name: /Filters/ }));
    fireEvent.click(await screen.findByRole("combobox", { name: "Account" }));
    expect(await screen.findByRole("option", { name: "cached@example.com" })).toBeVisible();
  });

  test("search account options keep ordinary disabled accounts hidden", async () => {
    accountApi.accounts = [
      { account_id: "active", name: "Active", email: "active@example.com", enabled: true },
      { account_id: "personal", name: "Personal", email: "personal@example.com", enabled: true },
      { account_id: "cached", name: "Cached", email: "cached@example.com", enabled: false },
    ];
    renderWithQueryClient(<SearchResultsRoute />);

    fireEvent.click(await screen.findByRole("button", { name: /Filters/ }));
    fireEvent.click(await screen.findByRole("combobox", { name: "Account" }));
    expect(await screen.findByRole("option", { name: "active@example.com" })).toBeVisible();
    expect(screen.getByRole("option", { name: "personal@example.com" })).toBeVisible();
    expect(screen.queryByRole("option", { name: "cached@example.com" })).not.toBeInTheDocument();
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
      expect.objectContaining({ offset: 100 }),
      expect.anything(),
    );
    // Page size is the daemon's [search] max_results, so the page sends none.
    expect(lastSearchParams().limit).toBeUndefined();
  });

  test("sends no sort or page size until the user picks a sort, so the daemon applies its [search] defaults", async () => {
    router.search = { q: "invoice", mode: "lexical" };
    renderWithQueryClient(<SearchResultsRoute />);

    await screen.findByTestId("mailbox-list");
    expect(lastSearchParams().sort).toBeUndefined();
    expect(lastSearchParams().limit).toBeUndefined();
  });

  test("the sort control reads Default while the sort is left to the daemon", async () => {
    router.search = { q: "invoice", mode: "lexical" };
    renderWithQueryClient(<SearchResultsRoute />);

    fireEvent.click(await screen.findByRole("button", { name: /Filters/ }));
    expect(await screen.findByRole("combobox", { name: "Sort" })).toHaveTextContent("Default");
  });

  test("an explicit sort is sent as chosen", async () => {
    router.search = { q: "invoice", mode: "lexical", sort: "newest" };
    renderWithQueryClient(<SearchResultsRoute />);

    await screen.findByTestId("mailbox-list");
    expect(lastSearchParams().sort).toBe("newest");
  });

  test("choosing Default in the sort control clears the sort from the URL", async () => {
    router.search = { q: "invoice", mode: "lexical", sort: "oldest" };
    renderWithQueryClient(<SearchResultsRoute />);

    fireEvent.click(await screen.findByRole("button", { name: /Filters/ }));
    fireEvent.click(await screen.findByRole("combobox", { name: "Sort" }));
    fireEvent.click(await screen.findByRole("option", { name: "Default" }));

    expect(navigatedSearch().sort).toBeUndefined();
  });

  test("triage keeps its own page of 100", async () => {
    router.search = { q: "invoice", mode: "lexical", scope: "triage", sort: "verdict" };
    renderWithQueryClient(<SearchResultsRoute />);

    await screen.findByTestId("mailbox-list");
    expect(lastSearchParams().limit).toBe(100);
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
