import { describe, expect, test } from "vitest";

import type { MessageRowView, ThreadResponse } from "@/features/mailbox/types";

import { targetFromRows, targetFromThread } from "./target";

function row(overrides: Partial<MessageRowView>): MessageRowView {
  return {
    id: "a-2",
    kind: "thread",
    thread_id: "thread-a",
    provider_id: "",
    sender: "Maya <maya@example.com>",
    subject: "Launch checklist",
    snippet: "",
    date: "2026-10-01T10:00:00Z",
    date_label: "",
    date_full: "",
    date_relative: "",
    unread: false,
    starred: false,
    has_attachments: false,
    ...overrides,
  };
}

describe("targetFromRows", () => {
  test("a conversation starred outside the list unstars that message too", () => {
    // The inbox lists a-1 and a-2; the star is on the reply in Sent (a-sent).
    const target = targetFromRows(
      [row({ starred: true, message_ids: ["a-1", "a-2"], starred_message_ids: ["a-sent"] })],
      "list",
    );
    expect(target.messageIds).toEqual(["a-1", "a-2"]);
    expect(target.unstarIds).toEqual(["a-1", "a-2", "a-sent"]);
    expect(target.anyStarred).toBe(true);
  });

  test("a message row unstars just its message", () => {
    const target = targetFromRows([row({ kind: "message", id: "m-1", starred: true })], "list");
    expect(target.unstarIds).toEqual(["m-1"]);
  });
});

describe("targetFromThread", () => {
  test("a windowed reader still acts on the whole conversation", () => {
    const data: ThreadResponse = {
      thread: {
        account_id: "acct",
        id: "thread-1",
        latest_date: "2026-10-01T10:00:00Z",
        message_count: 4,
        message_ids: ["m-1", "m-2", "m-3", "m-4"],
        participants: [],
        snippet: "",
        subject: "Launch checklist",
        unread_count: 0,
      },
      // Only the newest page is loaded.
      messages: [row({ id: "m-3", thread_id: "thread-1" }), row({ id: "m-4", thread_id: "thread-1" })],
      bodies: [],
      total_messages: 4,
      has_earlier: true,
      earlier_before: "m-3",
    };
    const target = targetFromThread(data);
    expect(target.messageIds).toEqual(["m-1", "m-2", "m-3", "m-4"]);
    expect(target.unstarIds).toEqual(["m-1", "m-2", "m-3", "m-4"]);
    expect(target.rows.map((message) => message.id)).toEqual(["m-3", "m-4"]);
    expect(target.primary?.id).toBe("m-4");
  });
});
