/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { useComposeUi } from "@/features/compose/composeUiStore";
import { DraftsRoute } from "./DraftsRoute";

const api = vi.hoisted(() => ({
  deleteDraft: vi.fn<typeof import("./api").deleteDraft>(),
  fetchDrafts: vi.fn<() => Promise<unknown>>(),
  fetchOrphanedDrafts: vi.fn<() => Promise<unknown[]>>(),
  resetOrphanedDraft: vi.fn<(draftId: string) => Promise<unknown>>(),
  sendStoredDraft: vi.fn<(draftId: string) => Promise<unknown>>(),
  fetchScheduledSends: vi.fn<(account: string | null) => Promise<unknown[]>>(),
  cancelScheduledSend: vi.fn<(draftId: string) => Promise<unknown>>(),
}));

vi.mock("./api", () => api);
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
    api.fetchOrphanedDrafts.mockReset();
    api.fetchOrphanedDrafts.mockResolvedValue([]);
    api.resetOrphanedDraft.mockReset();
    api.resetOrphanedDraft.mockResolvedValue({ kind: "Ack" });
    api.sendStoredDraft.mockReset();
    api.sendStoredDraft.mockResolvedValue({ kind: "SendReceipt" });
    api.fetchScheduledSends.mockReset();
    api.fetchScheduledSends.mockResolvedValue([]);
    api.cancelScheduledSend.mockReset();
    api.cancelScheduledSend.mockResolvedValue({ ok: true });
  });

  test("lists scheduled sends at the top and cancels one", async () => {
    api.fetchDrafts.mockResolvedValue({ drafts: [] });
    api.fetchScheduledSends.mockResolvedValue([
      {
        draft_id: "draft-9",
        account_id: "account-1",
        send_at: new Date(Date.now() + 3 * 3_600_000).toISOString(),
        subject: "Launch note",
        to: [{ name: "Ada", email: "ada@example.com" }],
        cc: [],
        bcc: [],
        last_attempt_at: null,
        last_attempt_outcome: "failed",
      },
    ]);
    renderWithClient(<DraftsRoute />);

    expect(await screen.findByText("Launch note")).toBeVisible();
    expect(screen.getByText(/in 3 hours/)).toBeVisible();
    expect(screen.getByText(/last attempt failed/i)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Cancel send" }));

    await waitFor(() => expect(api.cancelScheduledSend).toHaveBeenCalledWith("draft-9"));
  });

  test("hides the scheduled section when nothing is scheduled", async () => {
    api.fetchDrafts.mockResolvedValue({ drafts: [] });
    renderWithClient(<DraftsRoute />);

    expect(await screen.findByText("No saved drafts")).toBeVisible();
    expect(screen.queryByText("Scheduled")).not.toBeInTheDocument();
  });

  test("lists mxr-local drafts and opens the stored draft composer", async () => {
    api.fetchDrafts.mockResolvedValue({
      drafts: [
        {
          id: "draft-1",
          revision: 1,
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
    // Still a real link for new-tab use; a plain click opens the composer here.
    expect(draft).toHaveAttribute("href", "/compose/draft-1");
    expect(screen.getByText("Buwang <buwang@example.com>")).toBeVisible();
    expect(screen.getByText("edited 2m ago")).toBeVisible();

    fireEvent.click(draft);
    expect(useComposeUi.getState().intent).toMatchObject({ draftId: "draft-1" });
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
          revision: 1,
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

    await waitFor(() =>
      expect(api.deleteDraft.mock.calls[0]?.[0]).toMatchObject({ id: "draft-1", revision: 1 }),
    );
  });

  describe("drafts stuck mid-send", () => {
    const orphan = {
      id: "draft-7",
      account_id: "account-1",
      subject: "Contract signed",
      to: [{ name: "Lee", email: "lee@example.com" }],
      updated_at: new Date(Date.now() - 2 * 3_600_000).toISOString(),
    };

    beforeEach(() => {
      api.fetchDrafts.mockResolvedValue({ drafts: [] });
    });

    test("stay out of the way when there are none", async () => {
      renderWithClient(<DraftsRoute />);
      expect(await screen.findByText("No saved drafts")).toBeVisible();
      expect(screen.queryByText("Needs attention")).not.toBeInTheDocument();
    });

    test("can be reset back to an editable draft", async () => {
      api.fetchOrphanedDrafts.mockResolvedValue([orphan]);
      renderWithClient(<DraftsRoute />);

      expect(await screen.findByText("Needs attention")).toBeVisible();
      expect(screen.getByText(/Lee · stuck since/)).toBeVisible();
      fireEvent.click(screen.getByRole("button", { name: "Reset Contract signed" }));

      await waitFor(() => expect(api.resetOrphanedDraft).toHaveBeenCalledWith("draft-7"));
      expect(api.sendStoredDraft).not.toHaveBeenCalled();
    });

    test("send asks first, then resets and sends", async () => {
      api.fetchOrphanedDrafts.mockResolvedValue([orphan]);
      renderWithClient(<DraftsRoute />);

      fireEvent.click(await screen.findByRole("button", { name: "Send Contract signed" }));
      expect(await screen.findByText(/may already have been delivered/)).toBeVisible();
      expect(api.sendStoredDraft).not.toHaveBeenCalled();

      fireEvent.click(screen.getByRole("button", { name: "Send now" }));
      await waitFor(() => expect(api.sendStoredDraft).toHaveBeenCalledWith("draft-7"));
      expect(api.resetOrphanedDraft).toHaveBeenCalledWith("draft-7");
      expect(api.resetOrphanedDraft.mock.invocationCallOrder[0]).toBeLessThan(
        api.sendStoredDraft.mock.invocationCallOrder[0] ?? 0,
      );
    });
  });
});
