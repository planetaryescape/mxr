import { describe, expect, test } from "vitest";

import type { ConversationMessage, MessagesData, MessagesRow, MessagesTopic } from "./api";
import {
  afterDone,
  bands,
  countdownLabel,
  cursorRows,
  doneLine,
  initials,
  letterLead,
  openThreads,
  paragraphBlocks,
  rowForThread,
  splitAsk,
  stepTopic,
  topicLabel,
  topicStateLabel,
  topicsLeft,
  waitLabel,
} from "./messagesView";

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

function message(text: string, extra: Partial<ConversationMessage> = {}): ConversationMessage {
  return {
    message_id: "m1",
    from: { email: "samir@launchpad.example", name: "Samir Patel" },
    from_me: false,
    date: NOW.toISOString(),
    text,
    trimmed: { quote: false, signature: false },
    layout: "letter",
    paragraphs: 1,
    ...extra,
  };
}

describe("bands and the cursor", () => {
  test("bands keep the daemon's order and drop empty ones", () => {
    const view = bands(data({ your_turn: [row("samir")], quiet: [row("leo")], quiet_total: 12 }));
    expect(view.map((band) => band.band)).toEqual(["your_turn", "quiet"]);
    expect(view[1]?.more).toBe(11);
  });

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

  test("a thread finds the row that holds it", () => {
    const d = data({ recent: [row("b", { topics: [topic("x"), topic("y")] })] });
    expect(rowForThread(d, "y")?.id).toBe("b");
    expect(rowForThread(d, "z")).toBeUndefined();
  });
});

describe("labels", () => {
  test("waits read in minutes, hours and days", () => {
    expect(waitLabel("2026-10-02T14:59:30Z", NOW)).toBe("now");
    expect(waitLabel("2026-10-02T14:13:00Z", NOW)).toBe("47m");
    expect(waitLabel("2026-10-01T23:00:00Z", NOW)).toBe("16h");
    expect(waitLabel("2026-09-29T15:00:00Z", NOW)).toBe("3d");
  });

  test("initials come from the first two words", () => {
    expect(initials("Samir Patel")).toBe("SP");
    expect(initials("Samir, Ruth")).toBe("SR");
    expect(initials("iris@meridian.example")).toBe("I");
  });

  test("a group topic names the others", () => {
    expect(topicLabel(topic("t", { with: ["Ruth"], subject: "Pricing copy" }))).toBe(
      "with Ruth: Pricing copy",
    );
    expect(
      topicStateLabel(topic("t", { state: "your_turn", last_at: "2026-10-01T23:00:00Z" }), NOW),
    ).toBe("your turn · 16h");
  });

  test("[ and ] step through topics without wrapping", () => {
    const topics = [topic("a"), topic("b"), topic("c")];
    expect(stepTopic(topics, "b", 1)?.thread_id).toBe("c");
    expect(stepTopic(topics, "c", 1)?.thread_id).toBe("c");
    expect(stepTopic(topics, "a", -1)?.thread_id).toBe("a");
    expect(stepTopic(topics, null, 1)?.thread_id).toBe("a");
    expect(stepTopic([], "a", 1)).toBeUndefined();
  });

  test("the countdown counts whole seconds down to sending", () => {
    expect(countdownLabel(5_000, 0)).toBe("Sending in 5s");
    expect(countdownLabel(5_000, 4_100)).toBe("Sending in 1s");
    expect(countdownLabel(5_000, 5_000)).toBe("Sending…");
  });
});

describe("length decides the shape", () => {
  test("a letter leads with the paragraph holding the ask", () => {
    const text =
      "Thanks for the draft.\n\nTwo things changed.\n\nCan you reply with the next step?";
    const lead = letterLead(
      message(text, { ask_quote: "Can you reply with the next step?", paragraphs: 3 }),
    );
    expect(lead).toEqual({ lead: "Can you reply with the next step?", hidden: 2 });
    expect(letterLead(message(text, { paragraphs: 3 })).lead).toBe("Thanks for the draft.");
    const greeted = "Hi Alex,\n\nThe draft is fine.\n\nOne change.";
    expect(letterLead(message(greeted, { paragraphs: 3 }))).toEqual({
      lead: "Hi Alex,\n\nThe draft is fine.",
      hidden: 1,
    });
  });

  test("a compact note shows whole", () => {
    expect(letterLead(message("Thanks, on it.", { layout: "compact" }))).toEqual({
      lead: "Thanks, on it.",
      hidden: 0,
    });
  });

  test("the ask is cut out of the text to highlight it", () => {
    expect(splitAsk("Hi. Can you sign? Thanks.", "Can you sign?")).toEqual([
      { at: 0, text: "Hi. ", ask: false },
      { at: 4, text: "Can you sign?", ask: true },
      { at: 17, text: " Thanks.", ask: false },
    ]);
    expect(splitAsk("No ask here.", "Missing")).toEqual([
      { at: 0, text: "No ask here.", ask: false },
    ]);
  });

  test("paragraphs keep where they start", () => {
    expect(paragraphBlocks("One.\n\nTwo.\n \nThree.")).toEqual([
      { at: 0, text: "One." },
      { at: 6, text: "Two." },
      { at: 13, text: "Three." },
    ]);
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
  // The page lists history the list doesn't: an old quiet thread.
  const page = [...samir.topics, topic("history", { subject: "Old invoice" })];

  test("a row's open threads leave out done and in-flight ones", () => {
    expect([...openThreads(samir, new Set())]).toEqual(["contract", "launch"]);
    expect([...openThreads(samir, new Set(["launch"]))]).toEqual(["contract"]);
    expect([...openThreads(undefined, new Set())]).toEqual([]);
    // The page lists history and group topics; only the row's count.
    expect(topicsLeft(page, openThreads(samir, new Set())).map((t) => t.thread_id)).toEqual([
      "contract",
      "launch",
    ]);
  });

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

  test("the toast names what was done and what opened, then where it went", () => {
    const next = { kind: "topic", topic: samir.topics[1]! } as const;
    expect(doneLine({ subject: "Contract renewal" }, "Done. Archived in Gmail.", next)).toBe(
      "Done: Contract renewal. Next: Launch checklist. Archived in Gmail.",
    );
    expect(
      doneLine({ person: "Samir Patel" }, "Done in Messages. Still in To do (due Wed).", {
        kind: "person",
        row: jon,
      }),
    ).toBe("Done with Samir Patel. Next: Jon Bell. Still in To do (due Wed).");
    expect(doneLine({ person: "Samir Patel" }, "Done.", { kind: "none" })).toBe(
      "Done with Samir Patel.",
    );
  });
});
