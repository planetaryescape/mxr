import { describe, expect, test } from "vitest";

import type { DeskRow } from "@/features/desk/api";
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

function owed(threadId: string, extra: Partial<DeskRow> = {}): DeskRow {
  return {
    lane: "owed",
    account_id: "acct-1",
    thread_id: threadId,
    message_id: `${threadId}-msg`,
    message_ids: [`${threadId}-msg`],
    counterparty_email: `${threadId}@example.com`,
    subject: `About ${threadId}`,
    reason: "replied to your message",
    since: "2026-09-25T09:00:00Z",
    age_seconds: 2 * 86_400,
    usual_seconds: 4 * 3600,
    usual_samples: 5,
    overdue: true,
    unread: true,
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
  test("the desk's You owe lane first in its order, then reply-later, one entry per conversation", () => {
    const queue = buildFocusQueue(
      [owed("a", { counterparty_name: "Theo Nash" }), owed("b")],
      [later("b"), later("c")],
      null,
    );
    expect(queue.map((entry) => entry.threadId)).toEqual(["a", "b", "c"]);
    expect(queue[0]).toMatchObject({
      messageId: "a-msg",
      sender: "Theo Nash",
      reason: "Replied to your message · 2d · usually 4h",
      source: "owed",
    });
    expect(queue[1]!.source).toBe("owed");
    expect(queue[2]).toMatchObject({
      messageId: "c-later",
      sender: "Nora",
      reason: "In your reply-later queue",
    });
  });
});

describe("focusReducer", () => {
  test("the conversation on screen stays when a refetch drops it; others go", () => {
    const session = run(
      { kind: "sync", items: [item("a"), item("b"), item("c")] },
      { kind: "sync", items: [item("c")] },
    );
    expect(session.queue).toEqual(["a", "c"]);
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
});
