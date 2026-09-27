import { InfiniteQueryObserver, QueryClient, type InfiniteData } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { mergeMailboxPages } from "@/features/mailbox/useMailboxQuery";
import type { MailboxResponse, MessageRowView, MutationResponse } from "@/features/mailbox/types";
import { setActiveQueryClient } from "@/lib/queryClient";
import { useUndo } from "@/state/undoStore";

import { performMailAction, performUndo } from "./mailMutations";
import { projectGroups, usePendingMailOps, type LensIdentity } from "./pendingMailOps";

type Deferred<T> = { promise: Promise<T>; resolve: (value: T) => void; reject: (e: Error) => void };
function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (e: Error) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const api = vi.hoisted(() => ({
  archiveMessages: vi.fn<(ids: string[]) => Promise<MutationResponse>>(),
  starMessages: vi.fn<(ids: string[], starred: boolean) => Promise<MutationResponse>>(),
  snoozeMessage: vi.fn<(input: { messageId: string; until: string }) => Promise<unknown>>(),
  unsnoozeMessage: vi.fn<(id: string) => Promise<unknown>>(),
  undoMutation: vi.fn<(id: string) => Promise<unknown>>(),
}));

vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  ...api,
}));

interface ToastOptions {
  description?: string;
  action?: { label: string; onClick: () => void };
}
const toast = vi.hoisted(() => ({
  success: vi.fn<(message: string, options?: ToastOptions) => void>(),
  error: vi.fn<(message: string, options?: ToastOptions) => void>(),
  info: vi.fn<(message: string) => void>(),
}));
vi.mock("sonner", () => ({ toast }));

const INBOX: LensIdentity = { kind: "inbox" };
const MAILBOX_KEY = ["mailbox", { lens_kind: "inbox", view: "threads" }] as const;
const SEARCH_KEY = ["search", { q: "invoice" }] as const;

function row(id: string, overrides: Partial<MessageRowView> = {}): MessageRowView {
  return {
    id,
    kind: "thread",
    thread_id: `thread-${id}`,
    provider_id: `provider-${id}`,
    sender: "Ada <ada@example.com>",
    subject: `Subject ${id}`,
    snippet: "",
    date: "2026-09-26T10:00:00Z",
    date_label: "",
    date_full: "",
    date_relative: "",
    unread: true,
    starred: false,
    has_attachments: false,
    message_ids: [`${id}-1`, `${id}-2`],
    ...overrides,
  };
}

function page(rows: MessageRowView[]): MailboxResponse {
  return {
    mailbox: {
      lensLabel: "Inbox",
      view: "threads",
      counts: {},
      has_more: false,
      groups: [{ id: "today", label: "Today", rows }],
    },
  };
}

function ok(requested: number, mutationId?: string): MutationResponse {
  return {
    ok: true,
    result: { requested, succeeded: requested, skipped: 0, failed: 0, mutation_id: mutationId },
  };
}

let qc: QueryClient;
let refetch: ReturnType<typeof vi.fn<() => Promise<MailboxResponse>>>;
let unsubscribe: () => void;

/** What the inbox renders: the cached pages merged, with pending ops projected. */
function renderedInbox() {
  const data = qc.getQueryData<InfiniteData<MailboxResponse>>(MAILBOX_KEY);
  const merged = mergeMailboxPages(data?.pages ?? [], "threads");
  const groups = projectGroups(
    merged?.mailbox.groups ?? [],
    usePendingMailOps.getState().ops,
    INBOX,
  );
  return groups.flatMap((group) => group.rows);
}

beforeEach(() => {
  qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  setActiveQueryClient(qc);
  usePendingMailOps.setState({ ops: [] });
  useUndo.setState({ lastMutationId: null, lastUndo: null });
  qc.setQueryData<InfiniteData<MailboxResponse>>(MAILBOX_KEY, {
    pages: [page([row("a"), row("b")])],
    pageParams: [0],
  });
  qc.setQueryData(SEARCH_KEY, { groups: [] });
  refetch = vi.fn<() => Promise<MailboxResponse>>(async () => page([row("a"), row("b")]));
  // An on-screen mailbox: invalidation waits for this refetch.
  const observer = new InfiniteQueryObserver(qc, {
    queryKey: MAILBOX_KEY,
    queryFn: () => refetch(),
    initialPageParam: 0,
    getNextPageParam: () => undefined,
    staleTime: Infinity,
  });
  unsubscribe = observer.subscribe(() => undefined);
});

afterEach(() => {
  unsubscribe();
  qc.clear();
  vi.clearAllMocks();
});

describe("performMailAction", () => {
  test("the row leaves the view before the request resolves", async () => {
    const request = deferred<MutationResponse>();
    api.archiveMessages.mockReturnValue(request.promise);

    const done = performMailAction("archive", ["a-1", "a-2"]);

    expect(renderedInbox().map((item) => item.id)).toEqual(["b"]);
    request.resolve(ok(2));
    await done;
  });

  test("success toasts with Undo and keeps the op until the refetch lands", async () => {
    api.archiveMessages.mockResolvedValue(ok(2, "mut-1"));
    const serverAnswer = deferred<MailboxResponse>();
    refetch.mockReturnValueOnce(serverAnswer.promise);

    const done = performMailAction("archive", ["a-1", "a-2"]);

    await vi.waitFor(() => expect(refetch).toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve, 0));
    const [message, options] = toast.success.mock.calls[0]!;
    expect(message).toBe("Archived 2 messages");
    expect(options?.action?.label).toBe("Undo");
    expect(useUndo.getState().lastMutationId).toBe("mut-1");
    // The old cache still has row a; the op keeps it hidden meanwhile.
    expect(usePendingMailOps.getState().ops).toHaveLength(1);
    expect(renderedInbox().map((item) => item.id)).toEqual(["b"]);

    serverAnswer.resolve(page([row("b")]));
    await expect(done).resolves.toMatchObject({ ok: true });
    expect(usePendingMailOps.getState().ops).toEqual([]);
    expect(renderedInbox().map((item) => item.id)).toEqual(["b"]);
  });

  test("a failure brings back only its own rows while a later op stays pending", async () => {
    const archive = deferred<MutationResponse>();
    const star = deferred<MutationResponse>();
    api.archiveMessages.mockReturnValue(archive.promise);
    api.starMessages.mockReturnValue(star.promise);

    const first = performMailAction("archive", ["a-1", "a-2"]);
    const second = performMailAction("star", ["b-1", "b-2"]);
    expect(renderedInbox().map(({ id, starred }) => ({ id, starred }))).toEqual([
      { id: "b", starred: true },
    ]);

    archive.reject(new Error("daemon offline"));
    await expect(first).resolves.toMatchObject({ ok: false });

    expect(toast.error).toHaveBeenCalledWith(
      "Archived failed",
      expect.objectContaining({ description: "daemon offline" }),
    );
    expect(usePendingMailOps.getState().ops.map((item) => item.action)).toEqual(["star"]);
    expect(renderedInbox().map(({ id, starred }) => ({ id, starred }))).toEqual([
      { id: "a", starred: false },
      { id: "b", starred: true },
    ]);

    star.resolve(ok(2));
    await second;
    expect(usePendingMailOps.getState().ops).toEqual([]);
  });

  test("partial success is a failure that says how many changed", async () => {
    api.archiveMessages.mockResolvedValue({
      ok: true,
      result: {
        requested: 3,
        succeeded: 2,
        skipped: 0,
        failed: 1,
        accounts: [
          {
            account_id: "acc-1",
            account_name: "Work",
            succeeded: 2,
            skipped: 0,
            failed: 1,
            error: "rate limited",
          },
        ],
      },
    });

    const outcome = await performMailAction("archive", ["a-1", "a-2", "b-1"]);

    expect(outcome.ok).toBe(false);
    expect(toast.success).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith(
      "Archived failed",
      expect.objectContaining({
        description: "Only 2 of 3 messages changed. Work: rate limited",
      }),
    );
    expect(usePendingMailOps.getState().ops).toEqual([]);
  });

  test("an auth failure offers to re-authorize the account", async () => {
    api.archiveMessages.mockResolvedValue({
      ok: false,
      result: {
        requested: 1,
        succeeded: 0,
        skipped: 0,
        failed: 1,
        accounts: [
          {
            account_id: "acc-1",
            account_name: "Work",
            succeeded: 0,
            skipped: 0,
            failed: 1,
            error: "invalid_grant",
          },
        ],
      },
    });

    await performMailAction("archive", ["a-1"]);

    const [, options] = toast.error.mock.calls[0]!;
    expect(options?.description).toBe("No messages changed. Work: invalid_grant");
    expect(options?.action?.label).toBe("Re-authorize Work");
  });

  test("silent success skips the toast but still refreshes", async () => {
    api.starMessages.mockResolvedValue(ok(1));

    await performMailAction("star", ["a-1"], { silent: true });

    expect(toast.success).not.toHaveBeenCalled();
    expect(refetch).toHaveBeenCalled();
  });

  test("no ids is a no-op", async () => {
    await expect(performMailAction("archive", [])).resolves.toEqual({ ok: false });
    expect(api.archiveMessages).not.toHaveBeenCalled();
  });
});

describe("undo", () => {
  test("Undo from the toast reverses the mutation and refreshes search results too", async () => {
    api.archiveMessages.mockResolvedValue(ok(2, "mut-9"));
    api.undoMutation.mockResolvedValue({ ok: true });
    await performMailAction("archive", ["a-1", "a-2"]);
    expect(qc.getQueryState(SEARCH_KEY)?.isInvalidated).toBe(true);
    // Reset so we can see the undo invalidate it again.
    qc.setQueryData(SEARCH_KEY, { groups: [] });
    expect(qc.getQueryState(SEARCH_KEY)?.isInvalidated).toBe(false);

    const [, options] = toast.success.mock.calls[0]!;
    options?.action?.onClick();

    await vi.waitFor(() => expect(toast.success).toHaveBeenCalledWith("Undone"));
    expect(api.undoMutation).toHaveBeenCalledWith("mut-9");
    await vi.waitFor(() => expect(qc.getQueryState(SEARCH_KEY)?.isInvalidated).toBe(true));
    expect(useUndo.getState().lastMutationId).toBeNull();
  });

  test("a failed undo says so and keeps the undo available", async () => {
    useUndo.setState({ lastMutationId: "mut-old" });
    api.undoMutation.mockRejectedValue(new Error("undo window expired"));

    await expect(performUndo("mut-old")).resolves.toBe(false);

    expect(toast.error).toHaveBeenCalledWith("Undo failed", {
      description: "undo window expired",
    });
    expect(useUndo.getState().lastMutationId).toBe("mut-old");
  });

  test("undoing a snooze wakes each message", async () => {
    api.snoozeMessage.mockResolvedValue({});
    api.unsnoozeMessage.mockResolvedValue({});

    await performMailAction("snooze", ["a-1", "a-2"], { until: "tomorrow" });
    expect(api.snoozeMessage).toHaveBeenCalledWith({ messageId: "a-1", until: "tomorrow" });

    await useUndo.getState().lastUndo?.();

    expect(api.unsnoozeMessage.mock.calls.map(([id]) => id)).toEqual(["a-1", "a-2"]);
    expect(toast.success).toHaveBeenLastCalledWith("Snooze undone");
  });
});
