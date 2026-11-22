import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CalendarMetadataView } from "@/features/mailbox/types";

import { RSVP_HOLD_MS } from "@/features/invites/inviteResponse";

import { InviteCard } from "./InviteCard";

const apiFetchMock = vi.hoisted(() =>
  vi.fn<(path: string, init?: RequestInit) => Promise<unknown>>(),
);

vi.mock("@/api/client", () => ({
  apiFetch: apiFetchMock,
}));

vi.mock("sonner", () => ({
  toast: Object.assign(vi.fn<(message?: string) => void>(), {
    success: vi.fn<(message?: string) => void>(),
    error: vi.fn<(message?: string) => void>(),
    loading: vi.fn<(message?: string) => void>(),
  }),
}));

function wrap(node: React.ReactNode) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      mutations: { retry: false },
    },
  });
  return <QueryClientProvider client={client}>{node}</QueryClientProvider>;
}

function baseCalendar(overrides: Partial<CalendarMetadataView> = {}): CalendarMetadataView {
  return {
    method: "REQUEST",
    summary: "Demo Meeting",
    starts_at: "Wed 14:00",
    ends_at: "Wed 15:00",
    location: "Zoom",
    organizer: { email: "alice@example.com", name: "Alice" },
    attendees: [],
    rsvp_requested: true,
    warnings: [],
    ...overrides,
  };
}

beforeEach(() => {
  apiFetchMock.mockReset();
  apiFetchMock.mockResolvedValue({
    code: "en",
    invite: {
      card_title: "Calendar invite",
      chip_label_accept: "Accept",
      chip_label_tentative: "Maybe",
      chip_label_decline: "Decline",
      state_label_accepted: "✓ You accepted",
      state_label_tentative: "? You said maybe",
      state_label_declined: "✗ You declined",
      hint_change_response: "press ia/im/id to change",
      hint_comment: "Shift+iA/iM/iD to comment",
      banner_cancelled: "Event canceled by organizer",
      banner_publish: "Informational",
      banner_parse_warning: "Could not be parsed",
      banner_updated: "Updated invite",
      banner_counter: "Counter-proposal received",
    },
    status: {
      invite_pending_accept: "Accepting invite: u to undo (1s)",
      invite_pending_tentative: "Tentative — u to undo (1s)",
      invite_pending_decline: "Declining — u to undo (1s)",
      invite_cancelled: "Cancelled, no reply sent",
    },
  });
  vi.useFakeTimers({ shouldAdvanceTime: true });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("InviteCard", () => {
  it("clicking Accept holds the reply for the undo window, then sends it", async () => {
    render(wrap(<InviteCard messageId="m1" metadata={baseCalendar()} />));
    apiFetchMock.mockClear();
    const replies = () =>
      apiFetchMock.mock.calls.filter(([url]) => String(url).includes("/actions/invite/reply"));

    fireEvent.click(screen.getByRole("button", { name: /accept/i }));
    expect(replies()).toHaveLength(0);

    await vi.advanceTimersByTimeAsync(RSVP_HOLD_MS);
    expect(replies()).toHaveLength(1);
  });
});
