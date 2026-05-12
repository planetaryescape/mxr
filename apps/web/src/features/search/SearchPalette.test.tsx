/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { SearchPalette } from "./SearchPalette";
import type { MessageRowView } from "@/features/mailbox/types";
import { useModals } from "@/state/modalStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
}));

const api = vi.hoisted(() => ({
  fetchSearch: vi.fn<(params: unknown, opts?: unknown) => Promise<unknown>>(),
}));

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => router.navigate,
}));

vi.mock("@/features/search/api", () => ({
  fetchSearch: api.fetchSearch,
}));

const rows: MessageRowView[] = ["msg-1", "msg-2"].map((id, index) => ({
  id,
  kind: "message",
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

describe("SearchPalette", () => {
  beforeEach(() => {
    useModals.setState({ searchPaletteOpen: true });
    api.fetchSearch.mockResolvedValue({
      scope: "messages",
      sort: "relevance",
      mode: "lexical",
      total: rows.length,
      has_more: false,
      groups: [{ id: "today", label: "Today", rows }],
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
    useModals.setState({ searchPaletteOpen: false });
  });

  test("queries the daemon mailbox index for modal suggestions", async () => {
    renderWithQueryClient(<SearchPalette />);

    fireEvent.change(await screen.findByRole("textbox", { name: "Search mail" }), {
      target: { value: "invoice" },
    });

    await waitFor(() => expect(api.fetchSearch).toHaveBeenCalled());
    expect(api.fetchSearch.mock.calls.at(-1)?.[0]).toMatchObject({
      q: "invoice",
      mode: "lexical",
      sort: "relevance",
      scope: "messages",
      limit: 8,
    });
  });

  test("opens the selected modal result with arrow keys", async () => {
    renderWithQueryClient(<SearchPalette />);

    const input = await screen.findByRole("textbox", { name: "Search mail" });
    fireEvent.change(input, { target: { value: "invoice" } });

    expect(await screen.findByText("Subject 1")).toBeVisible();
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(router.navigate).toHaveBeenCalledWith({
      to: "/m/$mailbox/$threadId",
      params: { mailbox: "inbox", threadId: "thread-1" },
    });
  });

  test("moves modal selection through suggestions with up and down keys", async () => {
    renderWithQueryClient(<SearchPalette />);

    const input = await screen.findByRole("textbox", { name: "Search mail" });
    fireEvent.change(input, { target: { value: "invoice" } });

    expect(await screen.findByText("Subject 1")).toBeVisible();
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "ArrowDown" });

    expect(screen.getByRole("option", { name: /open subject 2/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    fireEvent.keyDown(input, { key: "Enter" });

    expect(router.navigate).toHaveBeenCalledWith({
      to: "/m/$mailbox/$threadId",
      params: { mailbox: "inbox", threadId: "thread-2" },
    });
  });
});
