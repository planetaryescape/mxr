import { describe, expect, test } from "vitest";

import { TABS } from "./MobileTabs";

const owner = (path: string) => TABS.filter((tab) => tab.owns(path)).map((tab) => tab.key);

describe("phone tabs", () => {
  test("are the five the research settled on", () => {
    expect(TABS.map((tab) => tab.label)).toEqual(["Now", "Messages", "To do", "Reading", "Find"]);
  });

  test("Find holds Archive, search and the Inbox", () => {
    expect(owner("/archive")).toEqual(["find"]);
    expect(owner("/search")).toEqual(["find"]);
    expect(owner("/m/inbox/t1")).toEqual(["find"]);
    expect(owner("/find")).toEqual(["find"]);
  });

  test("each page belongs to one tab at most", () => {
    expect(owner("/messages/t1")).toEqual(["messages"]);
    expect(owner("/now")).toEqual(["now"]);
    expect(owner("/updates")).toEqual([]);
  });
});
