import { describe, expect, test } from "vitest";

import { NONE_HIDDEN, type HiddenByMode } from "@/features/modes/modeDone";

import type { Now } from "./api";
import { itemTarget, nowItems } from "./nowItems";

function person(thread: string, name: string) {
  return {
    why: `From Messages: your turn with ${name}, 3h.`,
    row: {
      lane: "owed",
      account_id: "acct",
      thread_id: thread,
      message_id: `m-${thread}`,
      message_ids: [`m-${thread}`],
      counterparty_email: `${name.toLowerCase()}@example.com`,
      counterparty_name: name,
      subject: "Launch checklist",
      reason: "asked you a question",
      since: "2026-10-03T06:00:00Z",
      age_seconds: 10_800,
      usual_samples: 0,
      overdue: false,
      unread: true,
    },
  } as unknown as Now["people"]["rows"][number];
}

function todo(id: string, thread: string | null) {
  return {
    why: "From To do: act by Wed 7 Oct · due Fri 9 Oct.",
    todo: { id, thread_id: thread, title: `Pay ${id}`, when_label: "act by Wed 7" },
  } as unknown as Now["due_soon"]["todos"][number];
}

const now = {
  generated_at: "2026-10-03T09:00:00Z",
  header: "The few things that need you now, from every mode.",
  headline: "Saturday morning. 2 people, 2 things to act on.",
  people: {
    rows: [person("t1", "Maya"), person("t2", "Sam")],
    total: 5,
    more_line: "and 3 more in Messages",
  },
  due_soon: { todos: [todo("a", "t3"), todo("b", null)], total: 2 },
  updates: {
    message_count: 4,
    source_count: 2,
    top_sources: [],
    line: "4 updates from 2 sources.",
    early: true,
    since: "2026-10-02T15:30:00Z",
    thread_ids: ["u1", "u2"],
  },
  reading: {
    account_id: "acct",
    message_id: "r-m",
    thread_id: "r1",
    sender_email: "digest@longreads.example",
    subject: "The quiet death of the three-pane layout",
    date: "2026-10-02T08:00:00Z",
    why: "From Reading: the newest from Long Reads.",
  },
  item_count: 6,
  first_run: { complete: true, scanned: 100 },
} as unknown as Now;

describe("Now's rows", () => {
  test("keep the fixed section order: People, Due soon, the card, the pick", () => {
    expect(nowItems(now, NONE_HIDDEN).map((item) => item.key)).toEqual([
      "person:t1",
      "person:t2",
      "todo:a",
      "todo:b",
      "updates",
      "reading:r1",
    ]);
  });

  test("leave out rows whose done is in flight, in that row's own mode", () => {
    const hidden: HiddenByMode = {
      ...NONE_HIDDEN,
      messages: new Set(["t1"]),
      // Done in another mode doesn't hide the row here: t2 is a person row.
      todo: new Set(["t3", "t2"]),
      updates: new Set(["u1"]),
    };
    expect(nowItems(now, hidden).map((item) => item.key)).toEqual([
      "person:t2",
      "todo:b",
      "updates",
      "reading:r1",
    ]);
    const allLetGo = { ...NONE_HIDDEN, updates: new Set(["u1", "u2"]) };
    expect(nowItems(now, allLetGo).some((item) => item.kind === "updates")).toBe(false);
  });

  test("open in their own mode, on that item", () => {
    const [maya, , due, dueWithoutEmail, card, pick] = nowItems(now, NONE_HIDDEN);
    expect(itemTarget(maya!)).toEqual({ to: "/messages/t1" });
    expect(itemTarget(due!)).toEqual({ to: "/todo/t3", search: { row: "a" } });
    expect(itemTarget(dueWithoutEmail!)).toEqual({ to: "/todo", search: { row: "b" } });
    // A card without lines can only open Updates.
    expect(itemTarget(card!)).toEqual({ to: "/updates" });
    expect(itemTarget(pick!)).toEqual({
      to: "/reading/item/r-m%3A0",
      search: { row: "r-m:0" },
    });
    expect(itemTarget(pick!, { email: true })).toEqual({ to: "/reading/r1" });
  });

  test("the Updates card opens on its first line", () => {
    const withLines = {
      ...now,
      updates: { ...now.updates, lines: [{ id: "line-1" }, { id: "line-2" }] },
    } as unknown as Now;
    const card = nowItems(withLines, NONE_HIDDEN).find((item) => item.kind === "updates");
    expect(itemTarget(card!)).toEqual({ to: "/updates", search: { row: "line-1" } });
  });

  test("nothing loaded is nothing to show", () => {
    expect(nowItems(undefined, NONE_HIDDEN)).toEqual([]);
  });
});
