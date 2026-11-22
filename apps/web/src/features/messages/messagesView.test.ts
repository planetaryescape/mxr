import { describe, expect, test } from "vitest";

import type { MessagesData, MessagesRow, MessagesTopic } from "./api";
import { afterDone, cursorRows, openThreads } from "./messagesView";

const NOW = new Date("2026-10-02T15:00:00Z");

function topic(id: string, extra: Partial<MessagesTopic> = {}): MessagesTopic {
  return {
    account_id: "acc",
    thread_id: id,
    subject: `Topic ${id}`,
    shape: "one_to_one",
    state: "quiet",
    last_at: "2026-10-01T15:00:00Z",
    message_count: 2,
    message_ids: [],
    reply_to_message_id: `m-${id}`,
    ...extra,
  };
}

function row(id: string, extra: Partial<MessagesRow> = {}): MessagesRow {
  return {
    id,
    kind: "person",
    account_id: "acc",
    band: "recent",
    title: id,
    closeness: "regular",
    your_turn: false,
    last_at: "2026-10-01T15:00:00Z",
    topics: [topic(`t-${id}`)],
    overdue: false,
    pinned: false,
    unread: false,
    why: "",
    ...extra,
  };
}

function data(extra: Partial<MessagesData> = {}): MessagesData {
  return {
    generated_at: NOW.toISOString(),
    header: "People you talk with, one row each. Reply or mark done.",
    your_turn: [],
    pinned: [],
    recent: [],
    quiet: [],
    recent_total: 0,
    quiet_total: 0,
    row_count: 0,
    thread_count: 0,
    merge_suggestion_count: 0,
    ...extra,
  };
}

describe("bands and the cursor", () => {
  test("the cursor walks Your turn then Recent, and Quiet only when open", () => {
    const d = data({
      your_turn: [row("a")],
      pinned: [row("p")],
      recent: [row("b")],
      quiet: [row("c")],
    });
    expect(cursorRows(d, false).map((r) => r.id)).toEqual(["a", "b"]);
    expect(cursorRows(d, true).map((r) => r.id)).toEqual(["a", "b", "c"]);
  });
});

describe("done here moves on", () => {
  const samir = row("person:samir", {
    title: "Samir Patel",
    topics: [
      topic("contract", { state: "your_turn", subject: "Contract renewal" }),
      topic("launch", { state: "waiting", subject: "Launch checklist" }),
      topic("old", { state: "done" }),
    ],
  });
  const jon = row("person:jon", { title: "Jon Bell" });
  const iris = row("person:iris", { title: "Iris Chen" });
  const list = data({ your_turn: [samir, jon], recent: [iris] });
  const page = [...samir.topics, topic("history", { subject: "Old invoice" })];

  test("the person's next topic still in Messages, never history", () => {
    const open = openThreads(samir, new Set());
    const next = afterDone({
      topics: page,
      thread: "contract",
      open,
      rows: cursorRows(list, false),
      person: samir.id,
    });
    expect(next).toEqual({ kind: "topic", topic: samir.topics[1] });
  });

  test("nothing left: the next person, the previous at the end, else none", () => {
    const open = openThreads(samir, new Set(["launch"]));
    const rows = cursorRows(list, false);
    expect(afterDone({ topics: page, thread: "contract", open, rows, person: samir.id })).toEqual({
      kind: "person",
      row: jon,
    });
    const irisTopics = iris.topics;
    expect(
      afterDone({
        topics: irisTopics,
        thread: irisTopics[0]!.thread_id,
        open,
        rows,
        person: iris.id,
      }),
    ).toEqual({ kind: "person", row: jon });
    expect(
      afterDone({
        topics: irisTopics,
        thread: irisTopics[0]!.thread_id,
        open,
        rows: [iris],
        person: iris.id,
      }),
    ).toEqual({ kind: "none" });
  });

  test("never opens a person whose own done is still on its way", () => {
    const open = openThreads(samir, new Set(["launch"]));
    const rows = cursorRows(list, false);
    expect(
      afterDone({
        topics: page,
        thread: "contract",
        open,
        rows,
        person: samir.id,
        hidden: new Set(["launch", "t-person:jon"]),
      }),
    ).toEqual({ kind: "person", row: iris });
    expect(
      afterDone({
        topics: page,
        thread: "contract",
        open,
        rows,
        person: samir.id,
        hidden: new Set(["launch", "t-person:jon", "t-person:iris"]),
      }),
    ).toEqual({ kind: "none" });
  });
});
