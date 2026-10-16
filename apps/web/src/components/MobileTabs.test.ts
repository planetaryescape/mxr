import { describe, expect, test } from "vitest";

import { TABS } from "./MobileTabs";
import { phoneTabFor, type PhoneTab } from "./navMap";

describe("phone tabs", () => {
  test("are Now, Messages, To do, Reading and More", () => {
    expect(TABS.map((tab) => tab.label)).toEqual(["Now", "Messages", "To do", "Reading", "More"]);
  });

  // One row per route in src/routes: the lit tab always matches the page.
  const CASES: [string, PhoneTab, unknown?][] = [
    ["/now", "now"],
    ["/messages", "messages"],
    ["/messages/t1", "messages"],
    ["/messages", "more", "theirs"],
    ["/todo", "todo"],
    ["/todo/t1", "todo"],
    ["/reading", "reading"],
    ["/reading/t1", "reading"],
    ["/reading/item/abc", "reading"],
    ["/updates", "more"],
    ["/updates/t1", "more"],
    ["/archive", "more"],
    ["/archive/t1", "more"],
    ["/paper-trail", "more"],
    ["/paper-trail/t1", "more"],
    ["/m/inbox", "more"],
    ["/m/inbox/t1", "more"],
    ["/m/starred", "more"],
    ["/m/label/work", "more"],
    ["/m/label/work/t1", "more"],
    ["/m/category/c1", "more"],
    ["/m/saved/follow-ups/t1", "more"],
    ["/search", "more"],
    ["/search/t1", "more"],
    ["/find", "more"],
    ["/drafts", "more"],
    ["/snoozed", "more"],
    ["/snoozed/t1", "more"],
    ["/reply-queue", "more"],
    ["/reply-queue/t1", "more"],
    ["/owed", "more"],
    ["/owed/t1", "more"],
    ["/screener", "more"],
    ["/subscriptions", "more"],
    ["/invites", "more"],
    ["/deliveries", "more"],
    ["/analytics", "more"],
    ["/analytics/cadence", "more"],
    ["/rules", "more"],
    ["/rules/r1", "more"],
    ["/accounts", "more"],
    ["/accounts/a1", "more"],
    ["/activity", "more"],
    ["/jobs", "more"],
    ["/diagnostics", "more"],
    ["/settings/theme", "more"],
    ["/settings/all", "more"],
    ["/sender/a@b.example", "more"],
    ["/compose/new", "more"],
    ["/focus", "more"],
    ["/desk", "more"],
    ["/arrivals", "more"],
  ];

  test.each(CASES)("%s lights the %s tab", (path, tab, lane) => {
    expect(phoneTabFor(path, lane)).toBe(tab);
  });

  test("a path that only starts like a mode does not light it", () => {
    expect(phoneTabFor("/nowhere")).toBe("more");
    expect(phoneTabFor("/messagesx")).toBe("more");
  });
});
