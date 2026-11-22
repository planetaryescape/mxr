/* @vitest-environment jsdom */
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router";
import { useState, type ReactNode } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { useComposeUi } from "@/features/compose/composeUiStore";
import { MessagesRoute, type MessagesSearch } from "./MessagesRoute";
import type { Conversation, MessagesData, MessagesRow, MessagesTopic, PersonPage } from "./api";
import { setActiveQueryClient } from "@/lib/queryClient";
import { useModeDone, type ModeDoneOutcome } from "@/features/modes/modeDone";
import { useUndo } from "@/state/undoStore";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { optionalEnum, optionalFlag, optionalString } from "@/lib/searchParams";

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
  MessagesList: ({ data, onSelect }: { data: MessagesData; onSelect: (id: string) => void }) => (
    <ul>
      {[...data.your_turn, ...data.recent, ...data.quiet].map((listed) => (
        <li key={listed.id} data-testid="listed-row" onClick={() => onSelect(listed.id)}>
          {listed.id}
        </li>
      ))}
    </ul>
  ),
}));
vi.mock("./PersonPane", () => ({
  PersonPane: ({
    page,
    onBack,
    onDone,
  }: {
    page: PersonPage;
    onBack: () => void;
    onDone: () => void;
  }) => {
    const [topicsOpen, setTopicsOpen] = useState(false);
    return (
      <div data-testid="person-page">
        <span data-testid="shown-topic">{page.conversation?.thread_id}</span>
        <button aria-label="Back to the people" onClick={onBack}>
          Back
        </button>
        <button data-testid="topics-trigger" onClick={() => setTopicsOpen(true)}>
          Contract renewal
        </button>
        {topicsOpen ? (
          <button data-testid="topic" aria-current="true">
            Contract renewal
          </button>
        ) : null}
        <button onClick={onDone}>Done</button>
      </div>
    );
  },
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
function validateMessagesSearch(search: Record<string, unknown>): MessagesSearch {
  return {
    person: optionalString(search.person),
    topic: optionalString(search.topic),
    turn: optionalEnum(search.turn, ["all", "mine", "theirs"] as const),
    unread: optionalFlag(search.unread),
    pace: optionalFlag(search.pace),
  };
}

async function mountMessages(initial: string) {
  const root = createRootRoute();
  const route = createRoute({
    getParentRoute: () => root,
    path: "/messages",
    validateSearch: validateMessagesSearch,
    component: () => <MessagesRoute search={route.useSearch()} />,
  });
  const threadRoute = createRoute({
    getParentRoute: () => root,
    path: "/messages/$threadId",
    validateSearch: validateMessagesSearch,
    component: () => {
      const { threadId } = threadRoute.useParams();
      return <MessagesRoute search={threadRoute.useSearch()} threadId={threadId} />;
    },
  });
  const router = createRouter({
    routeTree: root.addChildren([route, threadRoute]),
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
  act(() => useComposeUi.setState({ intent: null, surface: "overlay" }));
  vi.restoreAllMocks();
});

function withDraft() {
  const drafted = {
    ...row("person:a", [topic("a-first"), topic("a-drafted")]),
    draft: {
      draft_id: "draft-a",
      thread_id: "a-drafted",
      text: "Friday works for me",
      reply_all: true,
      updated_at: "2026-10-19T00:00:00Z",
    },
  };
  const other = row("person:b", [topic("b-live")]);
  const list = listOf(other, drafted);
  client.setQueryData(listKey, list);
  cachePage(drafted, "a-drafted", "a-drafted");
  cachePage(drafted, null, "a-first");
  cachePage(other, "b-live", "b-live");
  http.send.mockImplementation(async (path) => {
    const url = new URL(path, "http://synthetic.local");
    if (url.pathname.endsWith("/people")) return { messages: list };
    throw new Error(`Unexpected synthetic request: ${path}`);
  });
  return { drafted, other };
}

test("opening a person with an unsent reply opens its conversation and resumes the reply inline", async () => {
  const { drafted } = withDraft();
  const router = await mountMessages("/messages?turn=mine");

  const listed = await screen.findAllByTestId("listed-row");
  fireEvent.click(listed.find((item) => item.textContent === drafted.id)!);

  await waitFor(() =>
    expect(router.state.location.search).toMatchObject({ person: drafted.id, topic: "a-drafted" }),
  );
  await waitFor(() =>
    expect(useComposeUi.getState().intent).toMatchObject({
      kind: "reply_all",
      messageId: "m-a-drafted",
      contextThreadId: "a-drafted",
    }),
  );
  expect(useComposeUi.getState().surface).toBe("inline");
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
  await act(async () => {
    done.resolve({ items: [] });
  });
});

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
  expect(await screen.findByText("1 shown · 1 need attention")).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
  await waitFor(() => expect(router.state.location.search).not.toHaveProperty("unread"));
  expect(router.state.location.search).not.toHaveProperty("pace");
  expect(router.state.location.search).not.toHaveProperty("turn");
  expect(screen.queryByRole("button", { name: "Clear filters" })).toBeNull();
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
