import { describe, expect, test } from "vitest";

import { useSelection } from "@/state/selectionStore";

import { rowKey } from "./rowKey";
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
