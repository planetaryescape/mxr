import { describe, expect, test } from "vitest";

import type { PlaceBundle, PlaceMessage, SweepPreview } from "./api";
import {
  correctionMessage,
  isStalePreview,
  KIND_OPTIONS,
  kindOptionForKey,
  paperTrailItems,
  readingIssues,
  sweepConfirmLabel,
  sweepNote,
  sweepTitle,
  whyHere,
} from "./placeCopy";

function message(id: string, date: string, extra: Partial<PlaceMessage> = {}): PlaceMessage {
  return {
    message_id: id,
    thread_id: `t-${id}`,
    subject: `Issue ${id}`,
    snippet: "",
    date,
    unread: true,
    pinned: false,
    starred: false,
    ...extra,
  };
}

function bundle(email: string, messages: PlaceMessage[], name?: string): PlaceBundle {
  return {
    account_id: "acct",
    sender_email: email,
    sender_name: name,
    kind: {
      kind: "reading",
      rule: "list_unsubscribe",
      reason: "has List-Unsubscribe",
      corrected: false,
    },
    message_count: messages.length,
    unread_count: messages.length,
    pinned_count: messages.filter((m) => m.pinned).length,
    newest_at: messages[0]?.date ?? "2026-01-01T00:00:00Z",
    newest_subject: messages[0]?.subject ?? "",
    messages,
  };
}

function sweptSender(email: string, count: number) {
  return { account_id: "acct", sender_email: email, count };
}

function preview(extra: Partial<SweepPreview> = {}): SweepPreview {
  return {
    place: "paper_trail",
    count: 13,
    pinned_excluded: 1,
    senders: [],
    sample_subjects: [],
    preview_token: "token",
    ...extra,
  };
}

describe("Reading feed", () => {
  test("issues from every sender, newest first, each with its bundle", () => {
    const weekly = bundle("weekly@x.example", [
      message("a", "2026-09-20T08:00:00Z"),
      message("b", "2026-09-13T08:00:00Z"),
    ]);
    const daily = bundle("daily@y.example", [message("c", "2026-09-22T08:00:00Z")]);
    const issues = readingIssues([weekly, daily]);
    expect(issues.map((issue) => issue.message.message_id)).toEqual(["c", "a", "b"]);
    expect(issues[0]?.bundle).toBe(daily);
  });
});

describe("Paper trail rows", () => {
  test("a closed bundle is one row; an open one lists its messages under it", () => {
    const shop = bundle("receipts@shop.example", [
      message("a", "2026-09-20T08:00:00Z"),
      message("b", "2026-09-19T08:00:00Z", { pinned: true }),
    ]);
    const bank = bundle("alerts@bank.example", [message("c", "2026-09-18T08:00:00Z")]);
    expect(paperTrailItems([shop, bank], new Set()).map((item) => item.type)).toEqual([
      "bundle",
      "bundle",
    ]);
    const open = paperTrailItems([shop, bank], new Set(["acct|receipts@shop.example"]));
    expect(
      open.map((item) => (item.type === "message" ? item.message.message_id : item.type)),
    ).toEqual(["bundle", "a", "b", "bundle"]);
    expect(new Set(open.map((item) => item.key)).size).toBe(open.length);
  });
});

describe("copy", () => {
  test("why here quotes the daemon's reason", () => {
    expect(whyHere({ reason: "automated sender, has List-Unsubscribe" })).toBe(
      "Here because: automated sender, has List-Unsubscribe.",
    );
  });

  test("the whole-place confirm names its scope and size; a bundle's stays short", () => {
    const all = preview({
      count: 143,
      senders: [sweptSender("a@x.example", 100), sweptSender("b@y.example", 43)],
    });
    expect(sweepConfirmLabel(all, true)).toBe("Archive all 143 from 2 senders");
    expect(sweepConfirmLabel(preview(), false)).toBe("Archive 13 messages");
  });

  test("sweep titles name the bundle or the place, and what stays", () => {
    expect(sweepTitle(preview())).toBe("Archive 13 messages from Paper trail?");
    expect(sweepTitle(preview({ count: 1 }), "Shop")).toBe("Archive 1 message from Shop?");
    expect(sweepTitle(preview({ count: 0 }))).toBe("Nothing to sweep in Paper trail");
    expect(sweepNote(preview())).toMatch(/^1 pinned message stays\. /);
    expect(sweepNote(preview({ pinned_excluded: 2 }))).toMatch(/^2 pinned messages stay\. /);
    expect(sweepNote(preview({ pinned_excluded: 0 }))).not.toMatch(/pinned/);
    // Past what is on screen, the note says how far the sweep reaches.
    expect(sweepNote(preview({ count: 143 }), 20)).toMatch(
      /^Archives 143 messages, 20 shown here\. /,
    );
    expect(sweepNote(preview(), 13)).not.toMatch(/shown here/);
    expect(sweepNote(preview({ count: 3, pinned_excluded: 0 }), 0)).toMatch(/^Only what/);
  });

  test("an expired preview is told apart from other failures", () => {
    expect(
      isStalePreview(new Error("sweep: that preview expired or was already used; preview again")),
    ).toBe(true);
    expect(isStalePreview(new Error("bridge unreachable"))).toBe(false);
  });

  test("corrections say where the sender went", () => {
    expect(correctionMessage("Hacker News", "people")).toBe("Moved Hacker News to People");
    expect(correctionMessage("Hacker News", null)).toBe("Hacker News is automatic again");
  });
});

describe("kind menu", () => {
  test("one key per kind, and automatic is the null choice", () => {
    expect(KIND_OPTIONS.map((option) => option.kind)).toEqual([
      "people",
      "reading",
      "paper_trail",
      "screened_out",
      null,
    ]);
    expect(new Set(KIND_OPTIONS.map((option) => option.key)).size).toBe(KIND_OPTIONS.length);
    expect(kindOptionForKey("t")?.kind).toBe("paper_trail");
    expect(kindOptionForKey("a")?.kind).toBeNull();
    expect(kindOptionForKey("z")).toBeUndefined();
  });
});
