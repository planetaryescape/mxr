/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { RightRail } from "./RightRail";
import { useModals } from "@/state/modalStore";

vi.mock("@/features/thread/AttachmentActions", () => ({
  AttachmentActions: () => null,
}));

const api = vi.hoisted(() => ({
  resolveCommitment: vi.fn<(commitmentId: string) => Promise<unknown>>(),
}));

vi.mock("@/features/mailbox/api", () => ({
  resolveCommitment: api.resolveCommitment,
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

describe("RightRail", () => {
  afterEach(() => {
    useModals.setState({ rightRail: null });
    vi.clearAllMocks();
  });

  test("renders sender profiles as relationship stats instead of raw JSON", () => {
    useModals.setState({
      rightRail: {
        kind: "sender-profile",
        payload: {
          kind: "SenderProfile",
          profile: {
            account_id: "account-1",
            email: "sender@example.com",
            display_name: "Sender Example",
            first_seen_at: "2026-05-01T10:00:00Z",
            last_seen_at: "2026-05-11T10:00:00Z",
            last_inbound_at: "2026-05-11T10:00:00Z",
            last_outbound_at: "2026-05-10T10:00:00Z",
            total_inbound: 5,
            total_outbound: 2,
            replied_count: 2,
            cadence_days_p50: 1.5,
            is_list_sender: false,
            list_id: null,
            open_thread_count: 3,
            inbound_storage_bytes: 4096,
            outbound_storage_bytes: 1024,
            attachment_count: 2,
            attachment_bytes: 2048,
            recent_messages: [
              {
                message_id: "message-1",
                thread_id: "thread-1",
                subject: "Earlier question",
                snippet: "Can you send the documents?",
                from_name: "Sender Example",
                from_email: "sender@example.com",
                date: "2026-05-10T10:00:00Z",
                direction: "inbound",
                has_attachments: true,
              },
            ],
          },
        },
      },
    });

    renderWithQueryClient(<RightRail />);

    expect(screen.getByRole("heading", { name: "Sender Example" })).toBeVisible();
    expect(screen.getByText("sender@example.com")).toBeVisible();
    expect(screen.getByText("Mostly inbound")).toBeVisible();
    expect(screen.getByText("5 in / 2 out")).toBeVisible();
    expect(screen.getByText(/They send \+3 KB/)).toBeVisible();
    expect(screen.getByText("Other emails from sender")).toBeVisible();
    expect(screen.getByRole("link", { name: /Earlier question/ })).toHaveAttribute(
      "href",
      "/m/inbox/thread-1",
    );
    expect(screen.queryByText(/"kind"/)).not.toBeInTheDocument();
  });

  test("resolves sender profile commitments from the relationship panel", async () => {
    api.resolveCommitment.mockResolvedValue({ ok: true });
    useModals.setState({
      rightRail: {
        kind: "sender-profile",
        payload: {
          kind: "SenderProfile",
          profile: {
            account_id: "account-1",
            email: "sender@example.com",
            first_seen_at: "2026-05-01T10:00:00Z",
            last_seen_at: "2026-05-11T10:00:00Z",
            total_inbound: 5,
            total_outbound: 2,
            replied_count: 2,
            is_list_sender: false,
            open_thread_count: 3,
            relationship: {
              open_commitments: [
                {
                  id: "commitment-1",
                  direction: "yours",
                  who_owes: "You",
                  what: "Send the revised deck",
                  by_when: "2026-05-13T10:00:00Z",
                },
              ],
            },
          },
        },
      },
    });

    renderWithQueryClient(<RightRail />);
    fireEvent.click(screen.getByRole("button", { name: /resolve/i }));

    await waitFor(() => expect(api.resolveCommitment.mock.calls[0]?.[0]).toBe("commitment-1"));
    await waitFor(() =>
      expect(screen.queryByText("Send the revised deck")).not.toBeInTheDocument(),
    );
  });

  test("renders commitment lists from command palette payloads", () => {
    useModals.setState({
      rightRail: {
        kind: "commitments",
        payload: {
          commitments: [
            {
              id: "commitment-2",
              email: "sender@example.com",
              direction: "theirs",
              who_owes: "Sender",
              what: "Reply with pricing",
            },
          ],
        },
      },
    });

    renderWithQueryClient(<RightRail />);

    expect(screen.getByRole("heading", { name: "Open commitments" })).toBeVisible();
    expect(screen.getByText("Reply with pricing")).toBeVisible();
    expect(screen.getByText(/sender@example.com/)).toBeVisible();
  });
});
