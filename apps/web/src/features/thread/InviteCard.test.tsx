import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CalendarMetadataView } from "@/features/mailbox/types";

import { InviteCard } from "./InviteCard";

const apiFetchMock = vi.hoisted(() => vi.fn());

vi.mock("@/api/client", () => ({
  apiFetch: apiFetchMock,
}));

vi.mock("sonner", () => ({
  toast: Object.assign(vi.fn(), {
    success: vi.fn(),
    error: vi.fn(),
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

function baseCalendar(
  overrides: Partial<CalendarMetadataView> = {},
): CalendarMetadataView {
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
      invite_pending_accept: "Accepting invite — u to undo (1s)",
      invite_pending_tentative: "Tentative — u to undo (1s)",
      invite_pending_decline: "Declining — u to undo (1s)",
      invite_cancelled: "Cancelled — no reply sent",
    },
  });
  vi.useFakeTimers({ shouldAdvanceTime: true });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("InviteCard", () => {
  it("renders all three action buttons when the viewer has not responded", () => {
    render(
      wrap(
        <InviteCard
          messageId="m1"
          threadId="t1"
          metadata={baseCalendar()}
        />,
      ),
    );
    expect(screen.getByRole("region", { name: /calendar invite/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /accept/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /maybe/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /decline/i })).toBeInTheDocument();
  });

  it("collapses to the responded state when viewer_partstat is set", () => {
    render(
      wrap(
        <InviteCard
          messageId="m1"
          threadId="t1"
          metadata={baseCalendar({ viewer_partstat: "accepted" })}
        />,
      ),
    );
    expect(screen.getByText(/you accepted/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^accept$/i })).not.toBeInTheDocument();
  });

  it("shows the cancelled banner and hides actions on CANCEL method", () => {
    render(
      wrap(
        <InviteCard
          messageId="m1"
          threadId="t1"
          metadata={baseCalendar({ method: "CANCEL" })}
        />,
      ),
    );
    expect(screen.getByText(/event canceled by organizer/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^accept$/i })).not.toBeInTheDocument();
  });

  it("shows the updated banner when is_update is true", () => {
    render(
      wrap(
        <InviteCard
          messageId="m1"
          threadId="t1"
          metadata={baseCalendar({ is_update: true })}
        />,
      ),
    );
    expect(screen.getByText(/updated invite/i)).toBeInTheDocument();
  });

  it("clicking Accept does not fire the network call immediately (1s undo window)", () => {
    render(
      wrap(
        <InviteCard
          messageId="m1"
          threadId="t1"
          metadata={baseCalendar()}
        />,
      ),
    );
    apiFetchMock.mockClear();

    fireEvent.click(screen.getByRole("button", { name: /accept/i }));

    // The hold-and-send pattern guarantees no network call fires within the
    // 1s window. Don't advance timers — component unmount clears the timer.
    expect(
      apiFetchMock.mock.calls.filter(([url]) =>
        String(url).includes("/actions/invite/reply"),
      ),
    ).toHaveLength(0);
  });
});
