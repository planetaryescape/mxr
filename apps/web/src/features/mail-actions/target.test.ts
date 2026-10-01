import { describe, expect, test } from "vitest";

import type { MessageRowView } from "@/features/mailbox/types";

import { targetFromRows } from "./target";

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
