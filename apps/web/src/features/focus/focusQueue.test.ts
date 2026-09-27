import { describe, expect, test } from "vitest";

import type { OwedReplyRow } from "@/features/owed/api";
import type { ReplyQueueMessage } from "@/features/reply-queue/api";

import {
  buildFocusQueue,
  emptyFocusSession,
  focusProgress,
  focusReducer,
  type FocusEvent,
  type FocusItem,
  type FocusSession,
} from "./focusQueue";

function owed(threadId: string, extra: Partial<OwedReplyRow> = {}): OwedReplyRow {
  return {
    thread_id: threadId,
    latest_inbound_msg_id: `${threadId}-msg`,
    from_email: `${threadId}@example.com`,
    from_name: null,
    subject: `About ${threadId}`,
    latest_inbound_at: "2026-09-20T09:00:00Z",
    waiting_days: 3.2,
    expected_days: 1,
    overdue_score: 2,
    ...extra,
  };
}

function later(threadId: string, accountId?: string): ReplyQueueMessage {
  return {
    id: `${threadId}-later`,
    thread_id: threadId,
    account_id: accountId,
    subject: `Later ${threadId}`,
    snippet: "",
    date: "2026-09-21T09:00:00Z",
    from: { name: "Nora", email: "nora@example.com" },
  };
}

function item(threadId: string): FocusItem {
  return {
    threadId,
    messageId: `${threadId}-msg`,
    sender: threadId,
    subject: threadId,
    reason: "",
    source: "owed",
  };
}

function run(...events: FocusEvent[]): FocusSession {
  return events.reduce(focusReducer, emptyFocusSession);
}

describe("buildFocusQueue", () => {
  test("owed first in the daemon's order, then reply-later, one entry per conversation", () => {
    const queue = buildFocusQueue(
      [owed("a", { from_name: "Theo Nash" }), owed("b")],
      [later("b"), later("c")],
      null,
    );
    expect(queue.map((entry) => entry.threadId)).toEqual(["a", "b", "c"]);
    expect(queue[0]).toMatchObject({
      messageId: "a-msg",
      sender: "Theo Nash",
      reason: "Waiting 3 days; you usually reply within 1 day",
      source: "owed",
    });
    expect(queue[1]!.source).toBe("owed");
    expect(queue[2]).toMatchObject({
      messageId: "c-later",
      sender: "Nora",
      reason: "In your reply-later queue",
    });
  });

  test("reply-later mail from another account stays out of a scoped queue", () => {
    const queue = buildFocusQueue([], [later("x", "acct-1"), later("y", "acct-2")], "acct-1");
    expect(queue.map((entry) => entry.threadId)).toEqual(["x"]);
  });
});

describe("focusReducer", () => {
  test("the order is fixed at the start; new arrivals join the end", () => {
    const session = run(
      { kind: "sync", items: [item("a"), item("b")] },
      { kind: "sync", items: [item("c"), item("b"), item("a")] },
    );
    expect(session.queue).toEqual(["a", "b", "c"]);
  });

  test("a refetch that changes nothing keeps the same session", () => {
    const first = run({ kind: "sync", items: [item("a"), item("b")] });
    expect(focusReducer(first, { kind: "sync", items: [item("a"), item("b")] })).toBe(first);
  });

  test("the conversation on screen stays when a refetch drops it; others go", () => {
    const session = run(
      { kind: "sync", items: [item("a"), item("b"), item("c")] },
      { kind: "sync", items: [item("c")] },
    );
    expect(session.queue).toEqual(["a", "c"]);
  });

  test("skip moves the current conversation to the end and keeps it", () => {
    const session = run(
      { kind: "sync", items: [item("a"), item("b"), item("c")] },
      { kind: "skip" },
    );
    expect(session.queue).toEqual(["b", "c", "a"]);
    expect(focusProgress(session)).toEqual({ position: 1, total: 3, done: 0 });
  });

  test("handled conversations count as progress and never come back from a refetch", () => {
    const session = run(
      { kind: "sync", items: [item("a"), item("b")] },
      { kind: "handled", threadId: "a" },
      { kind: "sync", items: [item("a"), item("b")] },
    );
    expect(session.queue).toEqual(["b"]);
    expect(focusProgress(session)).toEqual({ position: 2, total: 2, done: 1 });
  });

  test("an undone send puts the conversation back in front", () => {
    const session = run(
      { kind: "sync", items: [item("a"), item("b"), item("c")] },
      { kind: "handled", threadId: "a" },
      { kind: "restore", threadId: "a" },
    );
    expect(session.queue).toEqual(["a", "b", "c"]);
    expect(focusProgress(session).done).toBe(0);
  });

  test("finishing leaves an empty queue with everything handled", () => {
    const session = run({ kind: "sync", items: [item("a")] }, { kind: "handled", threadId: "a" });
    expect(session.queue).toEqual([]);
    expect(focusProgress(session)).toEqual({ position: 1, total: 1, done: 1 });
  });
});
