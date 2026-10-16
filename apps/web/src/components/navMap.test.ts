import { describe, expect, test } from "vitest";

import { isEntryActive, MAIL_ENTRIES, TOOL_ENTRIES, VIEW_ENTRIES } from "./navMap";

const find = (entries: typeof MAIL_ENTRIES, key: string) => entries.find((e) => e.key === key)!;

describe("the map's fixed groups", () => {
  test("Mail, Views and Tools are the ones D133 names, in order, with no daemon data", () => {
    expect(MAIL_ENTRIES.map((e) => e.label)).toEqual([
      "Starred",
      "Sent",
      "Drafts",
      "Snoozed",
      "Reply later",
      "All Mail",
      "Spam",
      "Trash",
    ]);
    expect(VIEW_ENTRIES.map((e) => e.label)).toEqual([
      "Waiting on",
      "Owed replies",
      "Screener",
      "Subscriptions",
      "Invites",
      "Deliveries",
    ]);
    expect(TOOL_ENTRIES.map((e) => e.label)).toEqual([
      "Search",
      "Analytics",
      "Rules",
      "Accounts",
      "Activity",
      "Jobs",
      "Diagnostics",
      "Settings",
    ]);
  });

  test("every item has a distinct key and path", () => {
    const all = [...MAIL_ENTRIES, ...VIEW_ENTRIES, ...TOOL_ENTRIES];
    expect(new Set(all.map((e) => e.key)).size).toBe(all.length);
    expect(new Set(all.map((e) => `${e.to}${JSON.stringify(e.search ?? "")}`)).size).toBe(
      all.length,
    );
  });
});

describe("which entry is the page on screen", () => {
  test("Waiting on and Messages share a path; the turn decides", () => {
    const waiting = find(VIEW_ENTRIES, "waiting");
    expect(isEntryActive("/messages", "theirs", waiting)).toBe(true);
    expect(isEntryActive("/messages", undefined, waiting)).toBe(false);
  });

  test("a conversation under a list keeps its entry lit, and a lookalike path does not", () => {
    const snoozed = find(MAIL_ENTRIES, "snoozed");
    expect(isEntryActive("/snoozed/t1", undefined, snoozed)).toBe(true);
    expect(isEntryActive("/snoozedx", undefined, snoozed)).toBe(false);
  });

  test("every settings page lights Settings", () => {
    const settings = find(TOOL_ENTRIES, "settings");
    expect(isEntryActive("/settings/all", undefined, settings)).toBe(true);
    expect(isEntryActive("/settings/llm", undefined, settings)).toBe(true);
    expect(isEntryActive("/setting", undefined, settings)).toBe(false);
  });
});
