import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { playSound } from "@/features/sound/player";

import type { Now } from "./api";
import { NowRoute } from "./NowRoute";

const state = vi.hoisted(() => ({ now: undefined as unknown }));

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  useNowQuery: () => ({
    data: state.now,
    isLoading: false,
    isError: false,
    error: null,
    refetch: vi.fn<() => void>(),
  }),
}));
vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: unknown }) => children,
  useNavigate: () => vi.fn<() => void>(),
}));
vi.mock("@/features/hints/useHint", () => ({
  useHint: () => ({ hint: undefined, dismiss: () => undefined }),
  useActiveHintDismiss: () => undefined,
}));
vi.mock("@/hooks/useSearchParam", () => ({ useSearchParam: () => [null, vi.fn<() => void>()] }));
vi.mock("@/features/arrivals/ArrivalsLine", () => ({ ArrivalsSection: () => null }));
vi.mock("@/features/modes/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/modes/api")>()),
  useModeGuide: () => ({ data: undefined }),
}));
vi.mock("@/features/modes/membership", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/modes/membership")>()),
  useThreadModesMap: () => ({ data: undefined }),
}));
vi.mock("@/features/sound/player", () => ({ playSound: vi.fn<typeof playSound>(() => true) }));

const cleared = {
  generated_at: "2026-10-03T09:00:00Z",
  header: "",
  headline: "",
  people: { rows: [], total: 0, more_line: "" },
  due_soon: { todos: [], total: 0 },
  updates: null,
  reading: null,
  item_count: 0,
  first_run: { complete: true, scanned: 100 },
} as unknown as Now;

const withWork = {
  ...cleared,
  people: {
    rows: [
      {
        why: "your turn since Wednesday",
        row: {
          lane: "owed",
          account_id: "acct",
          thread_id: "t1",
          message_id: "m-t1",
          message_ids: ["m-t1"],
          counterparty_email: "maya@example.com",
          counterparty_name: "Maya",
          subject: "Launch checklist",
          reason: "asked you a question",
          since: "2026-10-03T06:00:00Z",
          age_seconds: 10_800,
          usual_samples: 0,
          overdue: false,
          unread: true,
        },
      },
    ],
    total: 1,
    more_line: "",
  },
  item_count: 1,
} as unknown as Now;

/** One visit: Now mounts with `now`, and its data is the fixture passed to `show`. */
function visit(now: Now) {
  state.now = now;
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const ui = () => (
    <QueryClientProvider client={client}>
      <NowRoute />
    </QueryClientProvider>
  );
  const view = render(ui());
  return {
    show: (next: Now) => {
      state.now = next;
      view.rerender(ui());
    },
    unmount: () => view.unmount(),
  };
}

const sounds = () => vi.mocked(playSound).mock.calls.filter(([e]) => e === "low_tide").length;

beforeEach(() => {
  vi.mocked(playSound).mockClear();
  // jsdom has no layout; the list scrolls the lit row into view.
  Element.prototype.scrollIntoView = vi.fn<() => void>();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Now's finishing scene", () => {
  test("plays once on the first finish of the day, not again on a later visit that day", () => {
    const first = visit(withWork);
    first.show(cleared);
    expect(sounds()).toBe(1);
    first.unmount();

    visit(withWork).unmount();
    const revisit = visit(cleared);
    expect(sounds()).toBe(1);
    expect(screen.getByTestId("page-empty")).toBeInTheDocument();
    revisit.unmount();
  });

  test("plays again on the next local day it is finished", () => {
    vi.useFakeTimers({ toFake: ["Date"], now: new Date(2026, 9, 20, 10) });
    visit(withWork).unmount();
    visit(cleared).unmount();
    expect(sounds()).toBe(1);

    vi.setSystemTime(new Date(2026, 9, 21, 10));
    visit(withWork).unmount();
    visit(cleared).unmount();
    expect(sounds()).toBe(2);
  });
});
