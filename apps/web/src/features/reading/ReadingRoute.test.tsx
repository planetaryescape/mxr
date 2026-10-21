import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { playSound } from "@/features/sound/player";

import type { ReadingEdition } from "./api";
import { EDITION_KEY, ReadingRoute } from "./ReadingRoute";

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  fetchEdition: async () => null,
  useAccountScope: () => null,
  useMarginaliaStatus: () => ({ data: undefined }),
}));
vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: unknown }) => children,
  Outlet: () => null,
  useNavigate: () => vi.fn<() => void>(),
  useParams: () => ({}),
}));
vi.mock("@/features/places/PlaceLayout", () => ({
  PlaceLayout: ({ children }: { children: unknown }) => children,
}));
vi.mock("@/features/modes/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/modes/api")>()),
  useModeGuide: () => ({ data: undefined }),
}));
vi.mock("@/features/hints/useHint", () => ({
  useHint: () => ({ hint: undefined, dismiss: () => undefined }),
  useActiveHintDismiss: () => undefined,
}));
vi.mock("@/hooks/useSearchParam", () => ({
  useSearchParam: () => [null, vi.fn<() => void>()],
  pickSearch: () => ({}),
}));
vi.mock("@/features/sound/player", () => ({ playSound: vi.fn<typeof playSound>(() => true) }));

const issue = {
  item_key: "issue-1",
  thread_id: "thread-1",
  account_id: "acct",
  message_id: "msg-1",
  sender_email: "digest@longreads.example",
  title: "The quiet death of the three-pane layout",
  domain: "longreads.example",
  url: null,
  tracked: false,
  article_cached: false,
  links: [],
  finished: false,
} as unknown as ReadingEdition["bands"][number]["items"][number];

function edition(items: unknown[]): ReadingEdition {
  return {
    generated_at: "2026-10-20T09:00:00Z",
    header: "",
    bands: items.length
      ? [{ band: "since_last_visit", label: "Since you were last here", items }]
      : [],
    later: [],
    later_count: 0,
    left_off_here: false,
    pace_measured: false,
    pace_wpm: 230,
    sources: [],
    expired_now: 0,
    last_visit_at: null,
    empty: items.length ? null : { line: "Nothing new since Tuesday.", never_had_any: false },
  } as unknown as ReadingEdition;
}

const withWork = edition([issue]);
const cleared = edition([]);

/** One visit to Reading: it mounts with `data`, and `show` moves it to the next edition. */
function visit(data: ReadingEdition) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(EDITION_KEY("all"), data);
  const view = render(
    <QueryClientProvider client={client}>
      <ReadingRoute />
    </QueryClientProvider>,
  );
  return {
    show: (next: ReadingEdition) => client.setQueryData(EDITION_KEY("all"), next),
    unmount: () => view.unmount(),
  };
}

/** Lets the edition's query update and the scene's effects run before the test reads the sound. */
const settle = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));

const sounds = () => vi.mocked(playSound).mock.calls.filter(([e]) => e === "low_tide").length;

beforeEach(() => {
  vi.mocked(playSound).mockClear();
  Element.prototype.scrollIntoView = vi.fn<() => void>();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Reading's finishing scene", () => {
  test("plays once on the first finish of the day, not again on a later visit that day", async () => {
    const first = visit(withWork);
    await screen.findByTestId("reading-edition");
    first.show(cleared);
    await screen.findByTestId("page-empty");
    await settle();
    expect(sounds()).toBe(1);
    first.unmount();

    visit(withWork).unmount();
    const revisit = visit(cleared);
    await screen.findByTestId("page-empty");
    await settle();
    expect(sounds()).toBe(1);
    revisit.unmount();
  });

  test("plays again on the next local day it is finished", async () => {
    vi.useFakeTimers({ toFake: ["Date"], now: new Date(2026, 9, 20, 10) });
    const day1 = visit(withWork);
    await screen.findByTestId("reading-edition");
    day1.show(cleared);
    await screen.findByTestId("page-empty");
    await settle();
    day1.unmount();
    expect(sounds()).toBe(1);

    vi.setSystemTime(new Date(2026, 9, 21, 10));
    const day2 = visit(withWork);
    await screen.findByTestId("reading-edition");
    day2.show(cleared);
    await screen.findByTestId("page-empty");
    await settle();
    day2.unmount();
    expect(sounds()).toBe(2);
  });
});
