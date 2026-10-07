/* @vitest-environment jsdom */

import { act, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { Freshness } from "./copy";
import { FreshnessIndicator } from "./FreshnessIndicator";
import { TooltipProvider } from "@/components/ui/tooltip";

const query = vi.hoisted(() => ({ data: undefined as Freshness | undefined }));

vi.mock("./api", () => ({
  useFreshnessQuery: () => ({ data: query.data }),
}));

vi.mock("@tanstack/react-router", () => ({
  Link: ({
    to,
    params,
    children,
    ...rest
  }: {
    to: string;
    params?: Record<string, string>;
    search?: unknown;
    children: ReactNode;
  }) => {
    const href = Object.entries(params ?? {}).reduce(
      (path, [key, value]) => path.replace(`$${key}`, value),
      to,
    );
    const { search: _search, ...attrs } = rest as { search?: unknown };
    return (
      <a href={href} {...attrs}>
        {children}
      </a>
    );
  },
  useNavigate: () => vi.fn<() => Promise<void>>(),
}));

const start = new Date("2026-10-07T09:42:00");
const minutesAgo = (minutes: number) => new Date(start.getTime() - minutes * 60_000).toISOString();

function freshness(overrides: Partial<Freshness> = {}): Freshness {
  return {
    generated_at: start.toISOString(),
    newest_message_at: minutesAgo(5),
    stale_after_secs: 900,
    accounts: [
      {
        account_id: "acct-1",
        account_name: "work",
        label: "Gmail",
        health: "ok",
        last_sync_ok_at: minutesAgo(1),
        newest_message_at: minutesAgo(5),
        sync_in_progress: false,
      },
    ],
    arrivals: [
      {
        account_id: "acct-1",
        message_id: "m-1",
        thread_id: "t-1",
        from: { email: "notifications@github.com", name: "GitHub" },
        subject: "Build passed",
        received_at: minutesAgo(5),
        in_inbox: true,
        modes: [
          {
            mode: "updates",
            name: "Updates",
            key: "g u",
            reason: "Here because: automated sender (rule).",
            also_in: "",
            tag: "automated",
          },
        ],
      },
    ],
    ...overrides,
  };
}

function renderIndicator(compact = false) {
  return render(
    <TooltipProvider>
      <FreshnessIndicator compact={compact} />
    </TooltipProvider>,
  );
}

describe("FreshnessIndicator", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(start);
  });
  afterEach(() => {
    vi.useRealTimers();
    query.data = undefined;
  });

  test("the newest mail's age ticks forward without a refetch", () => {
    query.data = freshness();
    renderIndicator();
    expect(screen.getByText("Latest mail 5m ago")).toBeInTheDocument();
    expect(screen.getByText("synced 1m ago")).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(3 * 60_000);
    });
    expect(screen.getByText("Latest mail 8m ago")).toBeInTheDocument();
    expect(screen.getByText("synced 4m ago")).toBeInTheDocument();
  });

  test("a good sync that ages past the limit becomes a stale warning linking to details", () => {
    query.data = freshness();
    renderIndicator();
    expect(screen.queryByTestId("freshness-warning")).toBeNull();
    act(() => {
      vi.advanceTimersByTime(2 * 60 * 60_000);
    });
    const warning = screen.getByTestId("freshness-warning");
    expect(warning).toHaveTextContent("Last sync 2h ago");
    expect(warning).toHaveAttribute("href", "/diagnostics");
  });

  test("a rate limit shows the paused warning with its retry time", () => {
    query.data = freshness({
      accounts: [
        {
          account_id: "acct-1",
          account_name: "work",
          label: "Gmail",
          health: "paused",
          last_sync_ok_at: minutesAgo(3),
          sync_in_progress: false,
          last_sync_error: {
            kind: "rate_limited",
            message: "Rate limited",
            retry_at: new Date(start.getTime() + 8 * 60_000).toISOString(),
            consecutive_failures: 1,
          },
        },
      ],
    });
    renderIndicator();
    expect(screen.getByTestId("freshness-warning")).toHaveTextContent(
      /Gmail paused: rate limited, retrying 9:50/,
    );
    expect(screen.getByTestId("freshness-trigger").querySelector("[data-tone]")).toHaveAttribute(
      "data-tone",
      "warn",
    );
  });

  test("an auth failure turns the dot red on a phone, where only the age shows", () => {
    query.data = freshness({
      accounts: [
        {
          account_id: "acct-1",
          account_name: "work",
          label: "Gmail",
          health: "failing",
          sync_in_progress: false,
          last_sync_error: { kind: "auth", message: "oauth", consecutive_failures: 3 },
        },
      ],
    });
    renderIndicator(true);
    const trigger = screen.getByTestId("freshness-trigger");
    expect(trigger).toHaveTextContent(/^5m ago$/);
    expect(trigger.querySelector("[data-tone]")).toHaveAttribute("data-tone", "bad");
    expect(trigger).toHaveAccessibleName(/Gmail needs you to sign in again/);
    expect(screen.queryByTestId("freshness-warning")).toBeNull();
  });

  test("the popover lists each arrival with where it went and opens the email", () => {
    query.data = freshness();
    renderIndicator();
    fireEvent.click(screen.getByTestId("freshness-trigger"));
    const arrival = screen.getByTestId("freshness-arrival");
    expect(arrival).toHaveTextContent("GitHub");
    expect(arrival).toHaveTextContent("→ Updates · automated");
    expect(arrival).toHaveAttribute("href", "/m/inbox/t-1");
  });
});
