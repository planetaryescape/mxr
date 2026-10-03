import { describe, expect, test } from "vitest";

import type { Rail } from "@/features/modes/rail";

import { railNavEntries } from "./sidebarRail";

const rail: Rail = {
  generated_at: "2026-10-03T09:00:00Z",
  more: [],
  entries: [
    { id: "now", name: "Now", key: "g h", group: "home", count: 3, badge: 3, status: "built" },
    {
      id: "messages",
      name: "Messages",
      key: "g m",
      group: "modes",
      count: 5,
      status: "early",
      early_note: "Early version: the desk's lanes.",
    },
    { id: "todo", name: "To do", key: "g x", group: "modes", count: 2, status: "built" },
    { id: "updates", name: "Updates", key: "g u", group: "modes", count: 40, status: "early" },
    { id: "reading", name: "Reading", key: "g r", group: "modes", count: 90, status: "early" },
    { id: "archive", name: "Archive", key: "g e", group: "modes", status: "early" },
    { id: "inbox", name: "Inbox", key: "g i", group: "lens", status: "built" },
  ],
};

describe("the sidebar's rail", () => {
  test("follows the daemon's order, keys and paths", () => {
    const entries = railNavEntries(rail);
    expect(entries.map((entry) => [entry.label, entry.shortcut, entry.to])).toEqual([
      ["Now", "g h", "/now"],
      ["Messages", "g m", "/messages"],
      ["To do", "g x", "/todo"],
      ["Updates", "g u", "/updates"],
      ["Reading", "g r", "/reading"],
      ["Archive", "g e", "/archive"],
      ["Inbox", "g i", "/m/inbox"],
    ]);
  });

  test("only Now has a badge; Updates and Reading show no count", () => {
    const byKey = new Map(railNavEntries(rail).map((entry) => [entry.key, entry]));
    expect(byKey.get("now")).toMatchObject({ count: 3, badge: true });
    expect(byKey.get("messages")).toMatchObject({ count: 5, badge: false });
    expect(byKey.get("todo")).toMatchObject({ count: 2, badge: false });
    expect(byKey.get("updates")?.count).toBeUndefined();
    expect(byKey.get("reading")?.count).toBeUndefined();
  });

  test("early modes carry their note", () => {
    const byKey = new Map(railNavEntries(rail).map((entry) => [entry.key, entry]));
    expect(byKey.get("messages")?.early).toBe("Early version: the desk's lanes.");
    expect(byKey.get("updates")?.early).toBe("Early version");
    expect(byKey.get("now")?.early).toBeUndefined();
  });

  test("before the rail loads, the same entries show without counts", () => {
    const entries = railNavEntries(undefined);
    expect(entries.map((entry) => entry.key)).toEqual(
      railNavEntries(rail).map((entry) => entry.key),
    );
    expect(entries.every((entry) => entry.count === undefined)).toBe(true);
  });
});
