import { describe, expect, test } from "vitest";

import type { MailboxResponse, MessageGroupView, MessageRowView } from "./types";
import { mergeMailboxPages } from "./useMailboxQuery";

function row(id: string, threadId: string): MessageRowView {
  return {
    id,
    kind: "thread",
    thread_id: threadId,
    provider_id: id,
    sender: "Ada",
    subject: id,
    snippet: "",
    date: "2026-09-26T10:00:00Z",
    date_label: "",
    date_full: "",
    date_relative: "",
    unread: false,
    starred: false,
    has_attachments: false,
  };
}

function page(groups: MessageGroupView[], hasMore: boolean, nextOffset: number | null) {
  return {
    mailbox: {
      lensLabel: "Inbox",
      view: "threads",
      counts: { total: 4 },
      has_more: hasMore,
      next_offset: nextOffset,
      groups,
    },
  } satisfies MailboxResponse;
}

function ids(merged: MailboxResponse | undefined) {
  return merged?.mailbox.groups.map((group) => [group.id, group.rows.map((item) => item.id)]);
}

describe("mergeMailboxPages", () => {
  test("a thread that shifts onto the next page appears once", () => {
    const first = page(
      [{ id: "today", label: "Today", rows: [row("m1", "t1"), row("m2", "t2")] }],
      true,
      2,
    );
    // New mail pushed t2 down; the second page repeats it with a newer message id.
    const second = page(
      [
        { id: "today", label: "Today", rows: [row("m2b", "t2"), row("m3", "t3")] },
        { id: "older", label: "Older", rows: [row("m4", "t4")] },
      ],
      false,
      null,
    );

    const merged = mergeMailboxPages([first, second], "threads");

    expect(ids(merged)).toEqual([
      ["today", ["m1", "m2", "m3"]],
      ["older", ["m4"]],
    ]);
    // Paging state comes from the last page.
    expect(merged?.mailbox.has_more).toBe(false);
    expect(merged?.mailbox.next_offset).toBeNull();
  });

  test("message view keeps every message of a thread, deduping only repeated messages", () => {
    const first = page([{ id: "today", label: "Today", rows: [row("m1", "t1")] }], true, 1);
    const second = page(
      [{ id: "today", label: "Today", rows: [row("m1", "t1"), row("m2", "t1")] }],
      false,
      null,
    );

    expect(ids(mergeMailboxPages([first, second], "messages"))).toEqual([["today", ["m1", "m2"]]]);
  });

  test("a group emptied by dedupe is dropped", () => {
    const first = page([{ id: "today", label: "Today", rows: [row("m1", "t1")] }], true, 1);
    const second = page([{ id: "older", label: "Older", rows: [row("m1b", "t1")] }], false, null);

    expect(ids(mergeMailboxPages([first, second], "threads"))).toEqual([["today", ["m1"]]]);
  });

  test("no pages means no data", () => {
    expect(mergeMailboxPages([], "threads")).toBeUndefined();
  });
});
