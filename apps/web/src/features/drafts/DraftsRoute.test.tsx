import { observeWorkspace, resetWorkspaceOwnerForTest } from "@/lib/workspaceOwner";
/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { useComposeUi } from "@/features/compose/composeUiStore";
import { fetchSendOutcome } from "./sendOutcomes";
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
vi.mock("./sendOutcomes", () => ({
  fetchSendOutcome: vi.fn<typeof fetchSendOutcome>(),
  readSendReferences: () => [],
}));
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
    resetWorkspaceOwnerForTest();
    observeWorkspace({
      target: window.location.origin,
      workspace: {
        workspace_id: "00000000-0000-7000-8000-000000000001",
        recovery_generation: "00000000-0000-7000-8000-000000000002",
      },
    });
    vi.mocked(fetchSendOutcome).mockResolvedValue({ outcome: null });
    api.deleteDraft.mockReset();
    vi.mocked(fetchSendOutcome).mockReset();
    vi.mocked(fetchSendOutcome).mockResolvedValue({ outcome: null });
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

    test("no claim is distinct from uncertain acceptance", async () => {
      api.fetchOrphanedDrafts.mockResolvedValue([orphan]);
      renderWithClient(<DraftsRoute />);
      expect(
        await screen.findByText(
          "No submission was claimed for this operation. Review the saved draft before sending.",
        ),
      ).toBeVisible();
      expect(screen.queryByText("Acceptance is uncertain. Do not resend.")).not.toBeInTheDocument();
    });

    test("a pending lookup makes no acceptance claim", async () => {
      vi.mocked(fetchSendOutcome).mockReturnValue(new Promise(() => {}));
      api.fetchOrphanedDrafts.mockResolvedValue([orphan]);
      renderWithClient(<DraftsRoute />);
      expect(await screen.findByText("Checking send outcome…")).toBeVisible();
      expect(screen.queryByText("Acceptance is uncertain. Do not resend.")).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Check outcome" })).toBeDisabled();
    });

    test("a failed lookup offers checking without a retry", async () => {
      vi.mocked(fetchSendOutcome).mockRejectedValue(new Error("Connection lost"));
      api.fetchOrphanedDrafts.mockResolvedValue([orphan]);
      renderWithClient(<DraftsRoute />);
      expect(
        await screen.findByText(
          "Could not check the send outcome. Check again before deciding whether to send.",
        ),
      ).toBeVisible();
      expect(screen.queryByText("Acceptance is uncertain. Do not resend.")).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /Retry|Send now/ })).not.toBeInTheDocument();
    });

    test("uncertain sends only expose an outcome query", async () => {
      vi.mocked(fetchSendOutcome).mockResolvedValue({
        outcome: {
          identity: { operation_id: "draft:draft-7", expected_revision: 1 },
          state: "unknown",
          local_message_id: null,
        },
      });
      api.fetchOrphanedDrafts.mockResolvedValue([orphan]);
      renderWithClient(<DraftsRoute />);
      expect(await screen.findByText("Send outcomes")).toBeVisible();
      expect(await screen.findByText("Acceptance is uncertain. Do not resend.")).toBeVisible();
      await waitFor(() =>
        expect(fetchSendOutcome).toHaveBeenCalledWith("account-1", "draft:draft-7", {
          target: window.location.origin,
          workspace: {
            workspace_id: "00000000-0000-7000-8000-000000000001",
            recovery_generation: "00000000-0000-7000-8000-000000000002",
          },
        }),
      );
      expect(screen.queryByRole("button", { name: /Reset|Send now/ })).not.toBeInTheDocument();
      expect(api.resetOrphanedDraft).not.toHaveBeenCalled();
      expect(api.sendStoredDraft).not.toHaveBeenCalled();
      fireEvent.click(screen.getByRole("button", { name: "Check outcome" }));
      await waitFor(() => expect(fetchSendOutcome).toHaveBeenCalledTimes(2));
    });
  });
});
