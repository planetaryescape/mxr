import { describe, expect, test } from "vitest";

import type { ConversationMessage, MessagesData, MessagesRow, MessagesTopic } from "./api";
import {
  bands,
  countdownLabel,
  cursorRows,
  initials,
  letterLead,
  paragraphBlocks,
  rowForThread,
  splitAsk,
  stepTopic,
  topicLabel,
  topicStateLabel,
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
    const view = bands(
      data({ your_turn: [row("samir")], quiet: [row("leo")], quiet_total: 12 }),
    );
    expect(view.map((band) => band.band)).toEqual(["your_turn", "quiet"]);
    expect(view[1]?.more).toBe(11);
  });

  test("the cursor walks Your turn then Recent, and Quiet only when open", () => {
    const d = data({ your_turn: [row("a")], pinned: [row("p")], recent: [row("b")], quiet: [row("c")] });
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
    expect(topicStateLabel(topic("t", { state: "your_turn", last_at: "2026-10-01T23:00:00Z" }), NOW)).toBe(
      "your turn · 16h",
    );
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
    const text = "Thanks for the draft.\n\nTwo things changed.\n\nCan you reply with the next step?";
    const lead = letterLead(message(text, { ask_quote: "Can you reply with the next step?", paragraphs: 3 }));
    expect(lead).toEqual({ lead: "Can you reply with the next step?", hidden: 2 });
    expect(letterLead(message(text, { paragraphs: 3 })).lead).toBe("Thanks for the draft.");
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
