import { describe, expect, test } from "vitest";

import type { MessageGroupView, MessageRowView } from "@/features/mailbox/types";

import {
  projectGroups,
  projectRows,
  rowMessageIds,
  type LensIdentity,
  type MailAction,
  type MailActionPayload,
  type PendingMailOp,
} from "./pendingMailOps";

function row(id: string, overrides: Partial<MessageRowView> = {}): MessageRowView {
  return {
    id,
    kind: "message",
    thread_id: `thread-${id}`,
    provider_id: `provider-${id}`,
    sender: "Ada <ada@example.com>",
    subject: `Subject ${id}`,
    snippet: "",
    date: "2026-09-26T10:00:00Z",
    date_label: "",
    date_full: "",
    date_relative: "",
    unread: false,
    starred: false,
    has_attachments: false,
    ...overrides,
  };
}

let sequence = 0;
function op(action: MailAction, ids: string[], payload?: MailActionPayload): PendingMailOp {
  sequence += 1;
  return { id: `op-${sequence}`, action, messageIds: new Set(ids), payload };
}

const INBOX: LensIdentity = { kind: "inbox" };
const ALL_MAIL: LensIdentity = { kind: "all_mail" };
const SEARCH: LensIdentity = { kind: "search" };
const STARRED: LensIdentity = { kind: "starred" };
const TRASH: LensIdentity = { kind: "trash" };
const SPAM: LensIdentity = { kind: "spam" };
const WORK: LensIdentity = { kind: "label", labelName: "Work" };
const RECEIPTS: LensIdentity = { kind: "label", labelName: "Receipts" };

/** Ids left visible after projecting `ops` onto rows a, b in `lens`. */
function visible(ops: PendingMailOp[], lens: LensIdentity): string[] {
  return projectRows([row("a"), row("b")], ops, lens).map((item) => item.id);
}

describe("projectRows: which lenses an action removes rows from", () => {
  test.each([
    ["inbox", INBOX, ["b"]],
    ["all mail", ALL_MAIL, ["a", "b"]],
    ["a label", WORK, ["a", "b"]],
    ["search", SEARCH, ["a", "b"]],
  ] as const)("archive in %s", (_name, lens, expected) => {
    expect(visible([op("archive", ["a"])], lens)).toEqual(expected);
  });

  test("read-and-archive and snooze leave only the inbox", () => {
    expect(visible([op("read-and-archive", ["a"])], INBOX)).toEqual(["b"]);
    expect(visible([op("read-and-archive", ["a"])], ALL_MAIL)).toEqual(["a", "b"]);
    expect(visible([op("snooze", ["a"])], INBOX)).toEqual(["b"]);
    expect(visible([op("snooze", ["a"])], SEARCH)).toEqual(["a", "b"]);
  });

  test("trash removes everywhere except Trash", () => {
    for (const lens of [INBOX, ALL_MAIL, WORK, SEARCH, STARRED, SPAM]) {
      expect(visible([op("trash", ["a"])], lens)).toEqual(["b"]);
    }
    expect(visible([op("trash", ["a"])], TRASH)).toEqual(["a", "b"]);
  });

  test("spam removes everywhere except Spam", () => {
    expect(visible([op("spam", ["a"])], INBOX)).toEqual(["b"]);
    expect(visible([op("spam", ["a"])], TRASH)).toEqual(["b"]);
    expect(visible([op("spam", ["a"])], SPAM)).toEqual(["a", "b"]);
  });

  test("label-remove hides rows only in that label's lens, ignoring case", () => {
    const remove = op("label-remove", ["a"], { label: "work" });

    expect(visible([remove], WORK)).toEqual(["b"]);
    expect(visible([remove], RECEIPTS)).toEqual(["a", "b"]);
    expect(visible([remove], INBOX)).toEqual(["a", "b"]);
  });

  test("labels removes rows from the lens of any removed label, never on add", () => {
    const change = op("labels", ["a"], { add: ["Receipts"], remove: ["Work", "Later"] });

    expect(visible([change], WORK)).toEqual(["b"]);
    expect(visible([change], RECEIPTS)).toEqual(["a", "b"]);
    expect(visible([change], INBOX)).toEqual(["a", "b"]);
  });

  test("label-add never removes a row", () => {
    expect(visible([op("label-add", ["a"], { label: "Work" })], WORK)).toEqual(["a", "b"]);
  });

  test("unstar hides the row only in Starred", () => {
    expect(visible([op("unstar", ["a"])], STARRED)).toEqual(["b"]);
    expect(visible([op("unstar", ["a"])], INBOX)).toEqual(["a", "b"]);
  });

  test("move leaves the inbox and other label lenses but stays in its destination", () => {
    const move = op("move", ["a"], { label: "Receipts" });

    expect(visible([move], INBOX)).toEqual(["b"]);
    expect(visible([move], WORK)).toEqual(["b"]);
    expect(visible([move], RECEIPTS)).toEqual(["a", "b"]);
    expect(visible([move], ALL_MAIL)).toEqual(["a", "b"]);
  });

  test("route leaves the inbox and its queue label", () => {
    const route = op("route", ["a"], { label: "Receipts", fromQueueLabel: "Work" });

    expect(visible([route], INBOX)).toEqual(["b"]);
    expect(visible([route], WORK)).toEqual(["b"]);
    expect(visible([route], RECEIPTS)).toEqual(["a", "b"]);
  });

  test("route without archiving keeps the row in the inbox", () => {
    const route = op("route", ["a"], {
      label: "Receipts",
      fromQueueLabel: "Work",
      archive: false,
    });

    expect(visible([route], INBOX)).toEqual(["a", "b"]);
    expect(visible([route], WORK)).toEqual(["b"]);
  });
});

describe("projectRows: flag changes", () => {
  test("star, unstar, read and unread change flags in place", () => {
    const rows = [row("a"), row("b", { starred: true, unread: true })];

    const projected = projectRows(
      rows,
      [op("star", ["a"]), op("unread", ["a"]), op("unstar", ["b"]), op("read", ["b"])],
      INBOX,
    );

    expect(projected.map(({ id, starred, unread }) => ({ id, starred, unread }))).toEqual([
      { id: "a", starred: true, unread: true },
      { id: "b", starred: false, unread: false },
    ]);
  });

  test("read-and-archive marks read where the row stays", () => {
    const [projected] = projectRows(
      [row("a", { unread: true })],
      [op("read-and-archive", ["a"])],
      ALL_MAIL,
    );

    expect(projected?.unread).toBe(false);
  });

  test("the server rows are not mutated", () => {
    const original = row("a");
    projectRows([original], [op("star", ["a"])], INBOX);

    expect(original.starred).toBe(false);
  });
});

describe("projectRows: thread rows and op order", () => {
  test("a thread row is matched through any of its message_ids", () => {
    const thread = row("thread-row", { kind: "thread", message_ids: ["m1", "m2", "m3"] });

    expect(projectRows([thread], [op("archive", ["m2"])], INBOX)).toEqual([]);
    expect(projectRows([thread], [op("archive", ["thread-row"])], INBOX)).toEqual([thread]);
  });

  test("a thread row with an empty message_ids list falls back to its own id", () => {
    const thread = row("t", { kind: "thread", message_ids: [] });
    expect(rowMessageIds(thread)).toEqual(["t"]);
    expect(projectRows([thread], [op("archive", ["t"])], INBOX)).toEqual([]);
  });

  test("ops apply in order: star then unstar ends unstarred", () => {
    const [projected] = projectRows([row("a")], [op("star", ["a"]), op("unstar", ["a"])], INBOX);
    expect(projected?.starred).toBe(false);
  });

  test("a removal wins over a later flag change", () => {
    expect(visible([op("archive", ["a"]), op("star", ["a"])], INBOX)).toEqual(["b"]);
  });

  test("unrelated rows are returned as the same objects", () => {
    const rows = [row("a"), row("b")];
    const projected = projectRows(rows, [op("star", ["a"])], INBOX);
    expect(projected[1]).toBe(rows[1]);
  });
});

describe("projectGroups", () => {
  test("drops groups that the projection empties", () => {
    const groups: MessageGroupView[] = [
      { id: "today", label: "Today", rows: [row("a")] },
      { id: "older", label: "Older", rows: [row("b"), row("c")] },
    ];

    const projected = projectGroups(groups, [op("archive", ["a", "b"])], INBOX);

    expect(projected).toEqual([{ id: "older", label: "Older", rows: [row("c")] }]);
  });

  test("returns the input untouched with no pending ops", () => {
    const groups: MessageGroupView[] = [{ id: "today", label: "Today", rows: [row("a")] }];
    expect(projectGroups(groups, [], INBOX)).toBe(groups);
  });
});
