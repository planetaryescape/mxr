/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { SearchResultsRoute } from "./SearchResultsRoute";
import type { MessageRowView, ThreadResponse } from "@/features/mailbox/types";

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

const mailboxApi = vi.hoisted(() => ({
  fetchThread: vi.fn<(threadId: string) => Promise<ThreadResponse>>(),
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

vi.mock("@/features/mailbox/api", () => ({
  fetchThread: mailboxApi.fetchThread,
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

function threadResponse(threadId: string): ThreadResponse {
  return {
    thread: {
      account_id: "account-1",
      id: threadId,
      latest_date: "2026-05-11T10:00:00Z",
      message_count: 1,
      participants: [],
      snippet: "Snippet",
      subject: `Preview ${threadId}`,
      unread_count: 0,
    },
    messages: [],
    bodies: [],
  };
}

describe("SearchResultsRoute keyboard flow", () => {
  beforeEach(() => {
    router.search = { q: "invoice", mode: "lexical", sort: "relevance" };
    searchApi.fetchSavedSearches.mockResolvedValue({ searches: [] });
    searchApi.fetchSearch.mockResolvedValue({
      scope: "threads",
      sort: "relevance",
      mode: "lexical",
      total: rows.length,
      has_more: false,
      groups: [{ id: "today", label: "Today", rows }],
    });
    mailboxApi.fetchThread.mockImplementation((threadId) =>
      Promise.resolve(threadResponse(threadId)),
    );
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  test("blurs submitted input and routes j/k to result selection", async () => {
    renderWithQueryClient(<SearchResultsRoute />);

    expect(await screen.findByText("Subject 1")).toBeVisible();
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
    expect(document.activeElement).not.toBe(input);

    fireEvent.keyDown(window, { key: "j" });
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /Subject 2/ })).toHaveAttribute(
        "aria-current",
        "true",
      );
    });
    expect(input).toHaveValue("alice");

    fireEvent.keyDown(window, { key: "k" });
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /Subject 1/ })).toHaveAttribute(
        "aria-current",
        "true",
      );
    });
  });
});
