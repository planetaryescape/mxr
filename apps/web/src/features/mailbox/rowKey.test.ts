import { describe, expect, test } from "vitest";

import { useSelection } from "@/state/selectionStore";

import { conversationKey, holdsThread, rowKey, selectedKey, threadAccounts } from "./rowKey";
import type { MessageRowView } from "./types";

function row(patch: Partial<MessageRowView>): MessageRowView {
  return {
    id: "m1",
    kind: "message",
    thread_id: "t1",
    provider_id: "",
    sender: "",
    subject: "",
    snippet: "",
    date: "",
    date_label: "",
    date_full: "",
    date_relative: "",
    unread: false,
    starred: false,
    has_attachments: false,
    ...patch,
  };
}

describe("row identity", () => {
  test("a conversation keeps its key when a different message comes to represent it", () => {
    expect(rowKey(row({ kind: "thread", id: "m1" }))).toBe(
      rowKey(row({ kind: "thread", id: "m2" })),
    );
  });

  test("messages and attachments keep message-level keys", () => {
    expect(rowKey(row({}))).toBe("m1");
    expect(rowKey(row({ kind: "attachment", attachment_id: "a1" }))).toBe("m1:a1");
  });

  test("two accounts sharing a thread id are two rows, selected independently", () => {
    const first = row({ kind: "thread", thread_id: "shared", account_id: "acc-a", id: "m-a" });
    const second = row({ kind: "thread", thread_id: "shared", account_id: "acc-b", id: "m-b" });
    expect(rowKey(first)).not.toBe(rowKey(second));

    useSelection.getState().setScope("scope-shared-thread");
    useSelection.getState().toggle(rowKey(first));
    expect(useSelection.getState().has(rowKey(first))).toBe(true);
    expect(useSelection.getState().has(rowKey(second))).toBe(false);
    useSelection.getState().clear();
  });
});

describe("a thread id held by two accounts", () => {
  test("names each account's conversation apart, and a bare id for data with no account", () => {
    expect(conversationKey("t", "A")).not.toBe(conversationKey("t", "B"));
    expect(conversationKey("t")).toBe("t");
  });

  test("a qualified entry covers only its own account; a bare one covers both", () => {
    const onlyA = new Set([conversationKey("t", "A")]);
    expect(holdsThread(onlyA, "t", "A")).toBe(true);
    expect(holdsThread(onlyA, "t", "B")).toBe(false);
    const bare = new Set(["t"]);
    expect(holdsThread(bare, "t", "A")).toBe(true);
    expect(holdsThread(bare, "t", "B")).toBe(true);
  });

  test("the URL's account picks the listed copy; an older link without one takes the first", () => {
    const listed = [
      { thread_id: "t", account_id: "A" },
      { thread_id: "t", account_id: "B" },
    ];
    expect(selectedKey(listed, "t", "B")).toBe(conversationKey("t", "B"));
    expect(selectedKey(listed, "t", null)).toBe(conversationKey("t", "A"));
    expect(selectedKey([], "t", null)).toBe("t");
  });

  test("thread_accounts pairs name both accounts and leave out rows with no account", () => {
    expect(
      threadAccounts([
        { thread_id: "t", account_id: "A" },
        { thread_id: "t", account_id: "B" },
        { thread_id: "t", account_id: "A" },
        { thread_id: "old" },
      ]),
    ).toEqual([
      { thread_id: "t", account_id: "A" },
      { thread_id: "t", account_id: "B" },
    ]);
  });
});
