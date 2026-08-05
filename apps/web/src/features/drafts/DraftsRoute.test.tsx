/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { DraftsRoute } from "./DraftsRoute";

const api = vi.hoisted(() => ({
  deleteDraft: vi.fn<(draftId: string) => Promise<unknown>>(),
  fetchDrafts: vi.fn<() => Promise<unknown>>(),
}));

vi.mock("./api", () => ({ deleteDraft: api.deleteDraft, fetchDrafts: api.fetchDrafts }));
vi.mock("sonner", () => ({
  toast: { error: vi.fn<() => void>(), success: vi.fn<() => void>() },
}));
vi.mock("@tanstack/react-router", () => ({
  Link: ({
    children,
    to,
    params,
    ...props
  }: {
    children: ReactNode;
    to: string;
    params?: Record<string, string>;
  }) => (
    <a href={params?.draftId ? to.replace("$draftId", params.draftId) : to} {...props}>
      {children}
    </a>
  ),
}));

function renderWithClient(node: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{node}</QueryClientProvider>);
}

describe("DraftsRoute", () => {
  beforeEach(() => {
    api.deleteDraft.mockReset();
    api.deleteDraft.mockResolvedValue({ ok: true });
    api.fetchDrafts.mockReset();
  });

  test("lists mxr-local drafts and opens the stored draft composer", async () => {
    api.fetchDrafts.mockResolvedValue({
      drafts: [
        {
          id: "draft-1",
          account_id: "account-1",
          subject: "Quarterly plan",
          recipients: "Buwang <buwang@example.com>",
          updated_at: "2026-08-05T09:00:00Z",
          updated_at_label: "Today",
          updated_at_full: "5 Aug 2026, 10:00",
          updated_at_relative: "edited 2m ago",
          attachment_count: 1,
          content_kind: "markdown",
          inline_asset_count: 0,
        },
      ],
    });

    renderWithClient(<DraftsRoute />);

    const draft = await screen.findByRole("link", { name: /Quarterly plan/i });
    expect(draft).toHaveAttribute("href", "/compose/draft-1");
    expect(screen.getByText("Buwang <buwang@example.com>")).toBeVisible();
    expect(screen.getByText("edited 2m ago")).toBeVisible();
  });

  test("explains that the list reads mxr's local draft store", async () => {
    api.fetchDrafts.mockResolvedValue({ drafts: [] });

    renderWithClient(<DraftsRoute />);

    expect(await screen.findByText("No saved drafts")).toBeVisible();
    expect(screen.getByText(/persisted in mxr’s local store/)).toBeVisible();
  });

  test("confirms before permanently deleting a local draft", async () => {
    api.fetchDrafts.mockResolvedValue({
      drafts: [
        {
          id: "draft-1",
          account_id: "account-1",
          subject: "Old version",
          recipients: "buwang@example.com",
          updated_at: "2026-08-05T09:00:00Z",
          updated_at_label: "Today",
          updated_at_full: "5 Aug 2026, 10:00",
          updated_at_relative: "edited 2m ago",
          attachment_count: 0,
          content_kind: "markdown",
          inline_asset_count: 0,
        },
      ],
    });
    renderWithClient(<DraftsRoute />);

    fireEvent.click(await screen.findByRole("button", { name: "Delete draft Old version" }));
    expect(api.deleteDraft).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Delete draft" }));

    await waitFor(() => expect(api.deleteDraft.mock.calls[0]?.[0]).toBe("draft-1"));
  });
});
