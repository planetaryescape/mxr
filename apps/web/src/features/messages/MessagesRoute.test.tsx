/* @vitest-environment jsdom */
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { notifyManager, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router";
import type { ReactNode } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { MessagesRoute, type MessagesSearch } from "./MessagesRoute";
import type { Conversation, MessagesData, MessagesRow, MessagesTopic, PersonPage } from "./api";
import { setActiveQueryClient } from "@/lib/queryClient";
import { useModeDone, type ModeDoneOutcome } from "@/features/modes/modeDone";
import { useUndo } from "@/state/undoStore";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { runCommand } from "@/lib/keys/controllers";
import { optionalEnum, optionalFlag, optionalString } from "@/lib/searchParams";
import { focusActivePane } from "@/lib/keys/focusPane";

type FixtureReply =
  | { page: PersonPage }
  | { messages: MessagesData }
  | { items: ModeDoneOutcome[]; mutation_id?: string }
  | { result: { succeeded: number } };

const http = vi.hoisted(() => ({
  send: vi.fn<
    (path: string, options?: { body?: { thread_ids?: string[] } }) => Promise<FixtureReply>
  >(),
}));
vi.mock("@/api/client", async (original) => ({
  ...(await original<typeof import("@/api/client")>()),
  apiFetch: (path: string, options?: { body?: { thread_ids?: string[] } }) =>
    http.send(path, options),
}));
vi.mock("@/components/ui/resizable", () => ({
  ResizablePanelGroup: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ResizablePanel: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ResizableHandle: () => null,
}));
vi.mock("@/components/PageFrame", async () => ({
  FramePanesContext: (await import("react")).createContext({}),
  PageFrame: () => null,
}));
vi.mock("@/features/hints/useHint", () => ({
  useHint: () => ({ dismiss: () => {}, hint: null }),
  useActiveHintDismiss: () => null,
}));
vi.mock("@/features/modes/api", async (original) => ({
  ...(await original<typeof import("@/features/modes/api")>()),
  useModeGuide: () => ({}),
}));
vi.mock("@/features/modes/membership", () => ({ useThreadModesMap: () => ({}) }));
const lowTide = vi.hoisted(() => vi.fn<(...args: unknown[]) => boolean>(() => false));
const layout = vi.hoisted(() => ({ wide: true }));
vi.mock("@/features/low-tide/lowTideMemory", () => ({ useLowTide: lowTide }));
vi.mock("@/hooks/useMediaQuery", () => ({ useMediaQuery: () => layout.wide }));
vi.mock("@/hooks/useSplitPane", () => ({ useSplitPane: () => ({}) }));
vi.mock("@/features/mail-actions/verbFeedback", () => ({
  soundFor: () => null,
  toneFor: () => "success",
}));
vi.mock("./MessagesList", () => ({
  MessagesList: ({ data }: { data: MessagesData }) => (
    <ul>
      {data.your_turn.map((listed) => (
        <li key={listed.id} data-testid="listed-row">
          {listed.id}
        </li>
      ))}
    </ul>
  ),
}));
vi.mock("./PersonPane", () => ({
  PersonPane: ({ page, onDone }: { page: PersonPage; onDone: () => void }) => (
    <div>
      <span data-testid="shown-topic">{page.conversation?.thread_id}</span>
      <button onClick={onDone}>Done</button>
    </div>
  ),
}));

function topic(id: string, state: MessagesTopic["state"] = "your_turn"): MessagesTopic {
  return {
    account_id: "acc",
    thread_id: id,
    subject: id,
    shape: "one_to_one",
    state,
    last_at: "2026-10-19T00:00:00Z",
    message_count: 1,
    message_ids: [],
    reply_to_message_id: `m-${id}`,
  };
}
function row(id: string, topics: MessagesTopic[]): MessagesRow {
  return {
    id,
    kind: "person",
    account_id: "acc",
    band: "your_turn",
    title: id,
    closeness: "regular",
    your_turn: true,
    last_at: "2026-10-19T00:00:00Z",
    topics,
    overdue: false,
    pinned: false,
    unread: false,
    why: "",
  };
}
function personPage(person: MessagesRow, thread: string): PersonPage {
  const conversation: Conversation = {
    account_id: "acc",
    thread_id: thread,
    subject: thread,
    shape: "one_to_one",
    state: "your_turn",
    earlier_count: 0,
    messages: [],
    composer: { label: "Reply", reply_to_message_id: `m-${thread}`, reply_all: false },
  };
  return {
    row: person,
    topics: person.topics,
    conversation,
    header_line: person.title,
    relationship_line: "Known correspondent",
  };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
let client: QueryClient;
const listKey = ["messages-mode", "list", "all", "mine"];
function listOf(...people: MessagesRow[]): MessagesData {
  return {
    generated_at: "2026-10-19T00:00:00Z",
    header: "Messages",
    your_turn: people,
    pinned: [],
    recent: [],
    quiet: [],
    recent_total: 0,
    quiet_total: 0,
    row_count: people.length,
    thread_count: people.flatMap((person) => person.topics).length,
    merge_suggestion_count: 0,
  };
}
function cachePage(person: MessagesRow, requested: string | null, shown: string) {
  client.setQueryData(
    ["messages-mode", "person", "all", person.id, requested],
    personPage(person, shown),
  );
}
async function mountMessages(initial: string) {
  const root = createRootRoute();
  const route = createRoute({
    getParentRoute: () => root,
    path: "/messages",
    validateSearch: (search: Record<string, unknown>): MessagesSearch => ({
      person: optionalString(search.person),
      topic: optionalString(search.topic),
      turn: optionalEnum(search.turn, ["mine", "theirs"] as const),
      unread: optionalFlag(search.unread),
      pace: optionalFlag(search.pace),
    }),
    component: () => <MessagesRoute search={route.useSearch()} />,
  });
  const router = createRouter({
    routeTree: root.addChildren([route]),
    history: createMemoryHistory({ initialEntries: [initial] }),
  });
  await act(async () => {
    await router.load();
  });
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return router;
}
beforeEach(() => {
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  setActiveQueryClient(client);
});
afterEach(() => {
  layout.wide = true;
  lowTide.mockClear();
  cleanup();
  client?.clear();
  http.send.mockReset();
  useModeDone.getState().show("messages", [...useModeDone.getState().hidden.messages]);
  useUndo.setState({ lastUndo: null, lastMutationId: null });
  vi.restoreAllMocks();
});

test("Done advances to the next person's current open topic, not their cached closed default, while its fetch is delayed", async () => {
  const a = row("person:a", [topic("a-live")]);
  const b = row("person:b", [topic("b-old", "done"), topic("b-live")]);
  const list = listOf(a, b);
  client.setQueryData(listKey, list);
  cachePage(a, "a-live", "a-live");
  cachePage(b, null, "b-old");
  const delayed = deferred<FixtureReply>();
  const done = deferred<FixtureReply>();
  const posts: string[][] = [];
  const fetches: string[] = [];
  http.send.mockImplementation(async (path, options) => {
    const url = new URL(path, "http://synthetic.local");
    if (url.pathname.endsWith("/people/page")) {
      fetches.push(url.search);
      return delayed.promise;
    }
    if (url.pathname.endsWith("/people")) return { messages: list };
    if (path.endsWith("/messages/done")) {
      posts.push(options?.body?.thread_ids ?? []);
      return done.promise;
    }
    throw new Error(`Unexpected synthetic request: ${path}`);
  });
  const router = await mountMessages("/messages?turn=mine&person=person%3Aa&topic=a-live");
  await screen.findByText("a-live");
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  await waitFor(() => expect(posts).toEqual([["a-live"]]));
  await waitFor(() => expect(router.state.location.search).toMatchObject({ person: b.id }));
  const staleButton = screen.queryByRole("button", { name: "Done" });
  if (staleButton) fireEvent.click(staleButton);
  expect(posts).toEqual([["a-live"]]);
  await waitFor(() =>
    expect(router.state.location.search).toMatchObject({ person: b.id, topic: "b-live" }),
  );
  expect(fetches.some((search) => new URLSearchParams(search).get("topic") === "b-live")).toBe(
    true,
  );
  const pendingButton = screen.queryByRole("button", { name: "Done" });
  if (pendingButton) fireEvent.click(pendingButton);
  expect(posts).toEqual([["a-live"]]);
  await act(async () => {
    delayed.resolve({ page: personPage(b, "b-live") });
  });
  await screen.findByText("b-live");
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  await waitFor(() => expect(posts).toEqual([["a-live"], ["b-live"]]));
  // Complete both real action requests without launching post-test refreshes.
  await act(async () => {
    done.resolve({ items: [] });
  });
});

test("a sync closing the shown topic selects the same person's remaining topic without mutating cached history", async () => {
  const a = row("person:a", [topic("a-old"), topic("a-live")]);
  client.setQueryData(listKey, listOf(a));
  cachePage(a, "a-old", "a-old");
  const delayed = deferred<FixtureReply>();
  http.send.mockImplementation(() => delayed.promise);
  const router = await mountMessages("/messages?turn=mine&person=person%3Aa&topic=a-old");
  await screen.findByText("a-old");
  await act(async () => {
    client.setQueryData(listKey, listOf(row(a.id, [topic("a-old", "done"), topic("a-live")])));
  });
  await waitFor(() =>
    expect(router.state.location.search).toMatchObject({ person: a.id, topic: "a-live" }),
  );
  const stale = screen.queryByRole("button", { name: "Done" });
  if (stale) fireEvent.click(stale);
  expect(http.send.mock.calls.every(([path]) => !path.endsWith("/messages/done"))).toBe(true);
  await act(async () => {
    delayed.resolve({ page: personPage(a, "a-live") });
  });
  await screen.findByText("a-live");
});

test("opening a person with a closed cached default heals the URL to their current topic", async () => {
  const a = row("person:a", [topic("a-old", "done"), topic("a-live")]);
  client.setQueryData(listKey, listOf(a));
  cachePage(a, null, "a-old");
  const delayed = deferred<FixtureReply>();
  http.send.mockImplementation(() => delayed.promise);
  const router = await mountMessages("/messages?turn=mine&person=person%3Aa");
  await waitFor(() =>
    expect(router.state.location.search).toMatchObject({ person: a.id, topic: "a-live" }),
  );
  const pending = screen.queryByRole("button", { name: "Done" });
  if (pending) fireEvent.click(pending);
  expect(http.send.mock.calls.every(([path]) => !path.endsWith("/messages/done"))).toBe(true);
  await act(async () => {
    delayed.resolve({ page: personPage(a, "a-live") });
  });
  await screen.findByText("a-live");
});

test("focus returns into Messages when an overlay closes with nothing to return to", async () => {
  const a = row("person:a", [topic("a-live")]);
  client.setQueryData(listKey, listOf(a));
  cachePage(a, "a-live", "a-live");
  await mountMessages("/messages?turn=mine&person=person%3Aa&topic=a-live");
  await screen.findByText("a-live");

  focusActivePane();

  expect(screen.getByRole("region", { name: "Messages" })).toHaveFocus();
});

test("deliberately opening a historical topic does not redirect or mark it done again", async () => {
  const a = row("person:a", [topic("a-old", "done"), topic("a-live")]);
  client.setQueryData(listKey, listOf(a));
  cachePage(a, "a-old", "a-old");
  const router = await mountMessages("/messages?turn=mine&person=person%3Aa&topic=a-old");
  await screen.findByText("a-old");
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  expect(http.send).not.toHaveBeenCalled();
  expect(router.state.location.search).toMatchObject({ person: a.id, topic: "a-old" });
});

test.each(["person", "topic"])(
  "Done refuses a cached %s identity mismatch during refetch and works when the matching page arrives",
  async (mismatch) => {
    const a = row("person:a", [topic("a-live"), topic("a-other")]);
    client.setQueryData(listKey, listOf(a));
    client.setQueryData(
      ["messages-mode", "person", "all", a.id, "a-live"],
      personPage(mismatch === "person" ? row("person:other", a.topics) : a, "a-other"),
    );
    await client.invalidateQueries({ queryKey: ["messages-mode", "person"], refetchType: "none" });
    const delayed = deferred<FixtureReply>();
    const done = deferred<FixtureReply>();
    const posts: string[][] = [];
    http.send.mockImplementation(async (path, options) => {
      if (path.endsWith("/messages/done")) {
        posts.push(options?.body?.thread_ids ?? []);
        return done.promise;
      }
      if (new URL(path, "http://synthetic.local").pathname.endsWith("/people"))
        return { messages: listOf(a) };
      return delayed.promise;
    });
    await mountMessages("/messages?turn=mine&person=person%3Aa&topic=a-live");
    await screen.findByText("a-other");
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(posts).toEqual([]);
    await act(async () => {
      delayed.resolve({ page: personPage(a, "a-live") });
    });
    await screen.findByText("a-live");
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(posts).toEqual([["a-live"]]));
    await act(async () => {
      done.resolve({ items: [] });
    });
  },
);

test("Undo from the explicit landing restores the original topic after a cached-default transition", async () => {
  const a = row("person:a", [topic("a-live")]);
  const b = row("person:b", [topic("b-old", "done"), topic("b-live")]);
  let list = listOf(a, b);
  client.setQueryData(listKey, list);
  cachePage(a, "a-live", "a-live");
  cachePage(b, null, "b-old");
  cachePage(b, "b-live", "b-live");
  const posts: string[][] = [];
  http.send.mockImplementation(async (path, options) => {
    const url = new URL(path, "http://synthetic.local");
    if (path.endsWith("/messages/done")) {
      posts.push(options?.body?.thread_ids ?? []);
      list = listOf(b);
      return {
        mutation_id: "synthetic-done",
        items: [{ thread_id: "a-live", mode: "messages", provider: "fake", copy: "Done." }],
      };
    }
    if (path.includes("/undo")) {
      list = listOf(a, b);
      return { result: { succeeded: 1 } };
    }
    if (url.pathname.endsWith("/people")) return { messages: list };
    if (url.pathname.endsWith("/people/page")) {
      const person = url.searchParams.get("person") === a.id ? a : b;
      return { page: personPage(person, url.searchParams.get("topic") ?? "b-old") };
    }
    throw new Error(`Unexpected synthetic request: ${path}`);
  });
  const router = await mountMessages("/messages?turn=mine&person=person%3Aa&topic=a-live");
  await screen.findByText("a-live");
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  await waitFor(() =>
    expect(router.state.location.search).toMatchObject({ person: b.id, topic: "b-live" }),
  );
  await waitFor(() => expect(useUndo.getState().lastMutationId).toBe("synthetic-done"));
  await act(async () => {
    expect(await useUndo.getState().lastUndo?.()).toBe(true);
  });
  await waitFor(() =>
    expect(router.state.location.search).toMatchObject({ person: a.id, topic: "a-live" }),
  );
  expect(posts).toEqual([["a-live"]]);
});

test("rapid Done does not revisit settled topics while query-notification delivery is deferred", async () => {
  const people = [
    row("person:a", [topic("a-live")]),
    row("person:b", [topic("b-live")]),
    row("person:c", [topic("c-live")]),
  ];
  let remaining = [...people];
  client.setQueryData(listKey, listOf(...remaining));
  for (const person of people)
    cachePage(person, person.topics[0]?.thread_id ?? "", person.topics[0]?.thread_id ?? "");
  const posts: string[] = [];
  http.send.mockImplementation(async (path, options) => {
    const url = new URL(path, "http://synthetic.local");
    if (path.endsWith("/messages/done")) {
      const thread = options?.body?.thread_ids?.[0] ?? "";
      posts.push(thread);
      remaining = remaining.filter(
        (person) => !person.topics.some((item) => item.thread_id === thread),
      );
      return {
        mutation_id: `done-${thread}`,
        items: [{ thread_id: thread, mode: "messages", provider: "fake", copy: "Done." }],
      };
    }
    if (url.pathname.endsWith("/people"))
      return {
        messages: { ...listOf(...remaining), empty_state: remaining.length ? null : "All done." },
      };
    if (url.pathname.endsWith("/people/page")) {
      const person = people.find((entry) => entry.id === url.searchParams.get("person"));
      if (!person) throw new Error("Unknown synthetic person");
      return { page: personPage(person, url.searchParams.get("topic") ?? "") };
    }
    throw new Error("Unexpected synthetic endpoint");
  });
  await mountMessages("/messages?turn=mine&person=person%3Aa&topic=a-live");
  await screen.findByText("a-live");
  const pending: (() => void)[] = [];
  notifyManager.setScheduler((callback) => pending.push(callback));
  try {
    for (const thread of ["a-live", "b-live", "c-live"]) {
      // oxlint-disable-next-line no-await-in-loop
      await screen.findByText(thread);
      fireEvent.click(screen.getByRole("button", { name: "Done" }));
      // oxlint-disable-next-line no-await-in-loop
      await waitFor(() => expect(useModeDone.getState().hidden.messages.has(thread)).toBe(false));
    }
    const stale = screen.queryByRole("button", { name: "Done" });
    if (stale) fireEvent.click(stale);
    expect(posts).toEqual(["a-live", "b-live", "c-live"]);
  } finally {
    notifyManager.setScheduler((callback) => setTimeout(callback, 0));
    await act(async () => {
      for (const callback of pending) callback();
    });
  }
  expect(await screen.findByText("All done.")).toBeInTheDocument();
});

function listWith(overrides: Partial<MessagesData>): MessagesData {
  return { ...listOf(), ...overrides };
}

test("the filter toggles write the URL, ask the bridge with them, and Clear filters resets", async () => {
  const a = row("person:a", [topic("a-live")]);
  client.setQueryData(listKey, listOf(a));
  http.send.mockImplementation(async (path) => {
    const url = new URL(path, "http://synthetic.local");
    if (url.pathname.endsWith("/people")) return { messages: listOf(a) };
    throw new Error(`Unexpected synthetic request: ${path}`);
  });
  const router = await mountMessages("/messages?turn=mine");
  fireEvent.click(await screen.findByRole("button", { name: "Unread" }));
  await waitFor(() =>
    expect(router.state.location.search).toEqual(
      expect.objectContaining({ turn: "mine", unread: true }),
    ),
  );
  await waitFor(() =>
    expect(http.send.mock.calls.map(([path]) => path)).toContainEqual(
      expect.stringMatching(/unread=true/),
    ),
  );
  fireEvent.click(screen.getByRole("button", { name: "Past your pace" }));
  await waitFor(() =>
    expect(router.state.location.search).toEqual(
      expect.objectContaining({ turn: "mine", unread: true, pace: true }),
    ),
  );
  expect(screen.getByRole("button", { name: "Unread" }).getAttribute("aria-pressed")).toBe("true");
  expect(await screen.findByText("Showing 1 of 1")).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
  await waitFor(() => expect(router.state.location.search).not.toHaveProperty("unread"));
  expect(router.state.location.search).not.toHaveProperty("pace");
  expect(router.state.location.search).not.toHaveProperty("turn");
  expect(screen.queryByRole("button", { name: "Clear filters" })).toBeNull();
});

test("an empty filtered view shows the daemon's sentence and Clear filters, not Your turn's clear line", async () => {
  const empty = listWith({
    your_turn: [],
    row_count: 2,
    matched: 0,
    your_turn_split: { write_with: 2, others: 0 },
    empty_state: "No unread messages from people.\nYour turn still has 2 people.",
    filter: { turn: null, unread: true, past_pace: false },
  });
  http.send.mockImplementation(async (path) => {
    const url = new URL(path, "http://synthetic.local");
    if (url.pathname.endsWith("/people")) return { messages: empty };
    throw new Error(`Unexpected synthetic request: ${path}`);
  });
  await mountMessages("/messages?unread=true");

  expect(await screen.findByText(/No unread messages from people\./)).toBeTruthy();
  expect(screen.getByText(/Your turn still has 2 people\./)).toBeTruthy();
  expect(screen.queryByText("Nobody is waiting on you.")).toBeNull();
  expect(screen.getByRole("button", { name: "Clear filters" })).toBeTruthy();
});

test("a filter change keeps the list but hides the count and empty line until its answer lands", async () => {
  const a = row("person:a", [topic("a-live")]);
  client.setQueryData(listKey, listOf(a));
  const pending = deferred<FixtureReply>();
  http.send.mockImplementation(async (path) => {
    const url = new URL(path, "http://synthetic.local");
    if (url.pathname.endsWith("/people") && url.searchParams.get("unread") === "true") {
      return pending.promise;
    }
    if (url.pathname.endsWith("/people")) return { messages: listOf(a) };
    throw new Error(`Unexpected synthetic request: ${path}`);
  });
  await mountMessages("/messages?turn=mine");
  expect(await screen.findByText("Showing 1 of 1")).toBeTruthy();

  fireEvent.click(await screen.findByRole("button", { name: "Unread" }));
  await waitFor(() =>
    expect(http.send.mock.calls.map(([path]) => path)).toContainEqual(
      expect.stringMatching(/unread=true/),
    ),
  );
  // The previous filter's rows stay, but no count or empty line describes them.
  expect(screen.getByTestId("listed-row").textContent).toBe("person:a");
  expect(screen.queryByText(/Showing/)).toBeNull();

  await act(async () => {
    pending.resolve({
      messages: listWith({
        your_turn: [],
        row_count: 1,
        matched: 0,
        your_turn_split: { write_with: 1, others: 0 },
        empty_state: "No unread messages from people.\nYour turn still has 1 person.",
        filter: { turn: "mine", unread: true, past_pace: false },
      }),
    });
  });
  expect(await screen.findByText(/No unread messages from people\./)).toBeTruthy();
  expect(screen.getByText("Showing 0 of 1")).toBeTruthy();
});

test("switching account never shows the previous account's rows", async () => {
  const previousScope = useUiPrefs.getState().accountScope;
  const a = row("person:a", [topic("a-live")]);
  const b = row("person:b", [topic("b-live")]);
  client.setQueryData(listKey, listOf(a));
  const otherAccount = deferred<FixtureReply>();
  http.send.mockImplementation(async (path) => {
    const url = new URL(path, "http://synthetic.local");
    if (url.pathname.endsWith("/people") && url.searchParams.get("account") === "acc-2") {
      return otherAccount.promise;
    }
    if (url.pathname.endsWith("/people")) return { messages: listOf(a) };
    throw new Error(`Unexpected synthetic request: ${path}`);
  });
  try {
    await mountMessages("/messages?turn=mine");
    expect(await screen.findByText("person:a")).toBeTruthy();

    act(() => useUiPrefs.setState({ accountScope: "acc-2" }));
    await waitFor(() =>
      expect(http.send.mock.calls.map(([path]) => path)).toContainEqual(
        expect.stringMatching(/account=acc-2/),
      ),
    );
    expect(screen.queryByText("person:a")).toBeNull();
    expect(screen.queryByTestId("listed-row")).toBeNull();

    await act(async () => {
      otherAccount.resolve({ messages: listOf(b) });
    });
    expect(await screen.findByText("person:b")).toBeTruthy();
  } finally {
    act(() => useUiPrefs.setState({ accountScope: previousScope }));
  }
});

test("g f below the split reveals the people list before focusing the filter line", async () => {
  layout.wide = false;
  const a = row("person:a", [topic("a-live")]);
  client.setQueryData(listKey, listOf(a));
  cachePage(a, "a-live", "a-live");
  http.send.mockImplementation(async (path) => {
    const url = new URL(path, "http://synthetic.local");
    if (url.pathname.endsWith("/people")) return { messages: listOf(a) };
    throw new Error(`Unexpected synthetic request: ${path}`);
  });
  await mountMessages("/messages?turn=mine&person=person%3Aa&topic=a-live");
  // The person's page covers the list on a phone.
  expect(await screen.findByTestId("shown-topic")).toBeTruthy();

  act(() => {
    runCommand("messages", "filter");
  });
  await waitFor(() => expect(screen.queryByTestId("shown-topic")).toBeNull());
  await waitFor(() =>
    expect(document.activeElement).toBe(
      screen.getByRole("radio", { name: "Your turn", hidden: false }),
    ),
  );
});

test("an empty filtered view does not count as finishing Messages while Your turn is open", async () => {
  // Four people still owe a reply; none is unread, so the filtered view is empty.
  const empty = listWith({
    your_turn: [],
    row_count: 4,
    matched: 0,
    your_turn_split: { write_with: 4, others: 0 },
    empty_state: "No unread messages from people.\nYour turn still has 4 people.",
    filter: { turn: null, unread: true, past_pace: false },
  });
  http.send.mockImplementation(async (path) => {
    const url = new URL(path, "http://synthetic.local");
    if (url.pathname.endsWith("/people")) return { messages: empty };
    throw new Error(`Unexpected synthetic request: ${path}`);
  });
  await mountMessages("/messages?unread=true");
  expect(await screen.findByText(/No unread messages from people\./)).toBeTruthy();
  await waitFor(() => expect(lowTide.mock.calls.at(-1)?.[2]).toBe(true));
});

test("an empty filtered view finishes Messages when the unfiltered Your turn is clear", async () => {
  const clear = listWith({
    your_turn: [],
    row_count: 1,
    matched: 0,
    your_turn_split: { write_with: 0, others: 0 },
    empty_state: "No unread messages from people.",
    filter: { turn: null, unread: true, past_pace: false },
  });
  http.send.mockImplementation(async (path) => {
    const url = new URL(path, "http://synthetic.local");
    if (url.pathname.endsWith("/people")) return { messages: clear };
    throw new Error(`Unexpected synthetic request: ${path}`);
  });
  await mountMessages("/messages?unread=true");
  expect(await screen.findByText("No unread messages from people.")).toBeTruthy();
  await waitFor(() => expect(lowTide.mock.calls.at(-1)?.[2]).toBe(false));
});
