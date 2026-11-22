import { describe, expect, test } from "vitest";

import { phoneTabFor, type PhoneTab } from "./navMap";

describe("phone tabs", () => {
  // One row per route in src/routes: the lit tab always matches the page.
  const CASES: [string, PhoneTab, unknown?][] = [
    ["/now", "now"],
    ["/messages", "messages"],
    ["/messages", "more", "theirs"],
    ["/todo", "todo"],
    ["/reading", "reading"],
    ["/reading/item/abc", "reading"],
    ["/updates", "more"],
    ["/archive", "more"],
    ["/m/inbox", "more"],
    ["/m/saved/follow-ups/t1", "more"],
    ["/search", "more"],
    ["/analytics", "more"],
    ["/rules", "more"],
    ["/accounts", "more"],
    ["/settings/theme", "more"],
    ["/sender/a@b.example", "more"],
    ["/compose/new", "more"],
  ];

  test.each(CASES)("%s lights the %s tab", (path, tab, lane) => {
    expect(phoneTabFor(path, lane)).toBe(tab);
  });

  test("a path that only starts like a mode does not light it", () => {
    expect(phoneTabFor("/nowhere")).toBe("more");
    expect(phoneTabFor("/messagesx")).toBe("more");
  });
});
