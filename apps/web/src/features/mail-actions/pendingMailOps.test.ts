import { describe, expect, test } from "vitest";

import type { MessageRowView } from "@/features/mailbox/types";

import {
  projectRows,
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

  test("trash removes everywhere except Trash", () => {
    for (const lens of [INBOX, ALL_MAIL, WORK, SEARCH, STARRED, SPAM]) {
      expect(visible([op("trash", ["a"])], lens)).toEqual(["b"]);
    }
    expect(visible([op("trash", ["a"])], TRASH)).toEqual(["a", "b"]);
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
});

describe("projectRows: thread rows and op order", () => {
  test("a removal wins over a later flag change", () => {
    expect(visible([op("archive", ["a"]), op("star", ["a"])], INBOX)).toEqual(["b"]);
  });
});
