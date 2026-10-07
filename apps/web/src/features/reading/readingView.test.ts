import { describe, expect, test } from "vitest";

import type { ReadingBand, ReadingEdition, ReadingItem, ReadingLink } from "./api";
import {
  domainLabel,
  editionEntries,
  highlightPayload,
  LINKS_SHOWN,
  letGoAllPlan,
  minutesLeft,
  scrollProgress,
  shownLinks,
  unsubscribeMethodLine,
  unsubscribePreview,
  visibleBands,
} from "./readingView";

function link(n: number, extra: Partial<ReadingLink> = {}): ReadingLink {
  return {
    item_key: `m1:${n}`,
    title: `Link ${n}`,
    url: `https://example.com/${n}`,
    domain: "example.com",
    tracked: false,
    on_later: false,
    article_cached: false,
    ...extra,
  };
}

function item(key: string, thread: string, extra: Partial<ReadingItem> = {}): ReadingItem {
  return {
    item_key: `${key}:0`,
    account_id: "a",
    message_id: key,
    thread_id: thread,
    kind: "issue",
    shape: "single",
    title: `Title ${key}`,
    source: "Long Reads Weekly",
    sender_email: "essays@longreads.example",
    words: 900,
    minutes: 4,
    arrived_at: "2026-10-06T08:00:00Z",
    why: "Here because: you subscribed (rule).",
    on_later: false,
    progress: 0,
    opened: false,
    finished: false,
    article_cached: false,
    ...extra,
  };
}

function band(name: ReadingBand["band"], items: ReadingItem[]): ReadingBand {
  return { band: name, label: name, items };
}

describe("digest links", () => {
  const links = Array.from({ length: 7 }, (_, i) => link(i + 1));

  test("show four with a count of the rest, or all once opened", () => {
    expect(shownLinks(links, false).shown).toHaveLength(LINKS_SHOWN);
    expect(shownLinks(links, false).more).toBe(3);
    expect(shownLinks(links, true)).toEqual({ shown: links, more: 0 });
    expect(shownLinks(links.slice(0, 3), false).more).toBe(0);
  });

  test("a tracked link names the tracker, not a site it can't know", () => {
    expect(domainLabel("sqlite.org")).toBe("sqlite.org");
    expect(domainLabel("substack.com", true)).toBe("via substack.com");
    expect(domainLabel(null)).toBe("");
  });
});

describe("the cursor order", () => {
  test("goes item by item with each digest's shown links after it", () => {
    const digest = item("d", "t2", {
      shape: "digest",
      links: [link(1), link(2), link(3), link(4), link(5)],
    });
    const bands = [
      band("since_last_visit", [item("e", "t1"), digest]),
      band("fading", [item("f", "t3")]),
    ];
    const keys = editionEntries(bands, new Set()).map((entry) => entry.key);
    expect(keys).toEqual(["e:0", "d:0", "m1:1", "m1:2", "m1:3", "m1:4", "f:0"]);
    const opened = editionEntries(bands, new Set(["d:0"])).map((entry) => entry.key);
    expect(opened).toContain("m1:5");
  });

  test("an item let go of leaves its band, and an empty band goes", () => {
    const edition = {
      bands: [band("since_last_visit", [item("e", "t1")]), band("earlier", [item("g", "t4")])],
    } as ReadingEdition;
    const bands = visibleBands(edition, new Set(["t1"]));
    expect(bands.map((b) => b.band)).toEqual(["earlier"]);
  });
});

describe("let go of all", () => {
  test("covers every conversation shown, once, in order", () => {
    const bands = [
      band("since_last_visit", [item("e", "t1"), item("h", "t1")]),
      band("fading", [item("f", "t3")]),
    ];
    expect(letGoAllPlan(bands)).toEqual({
      threadIds: ["t1", "t3"],
      titles: ["Title e", "Title f"],
    });
  });
});

describe("time left", () => {
  test("counts down at the reader's pace and says nothing is left at the end", () => {
    expect(minutesLeft(2300, 0, 230)).toBe(10);
    expect(minutesLeft(2300, 0.62, 230)).toBe(4);
    expect(minutesLeft(2300, 0.999, 230)).toBe(1);
    expect(minutesLeft(2300, 1, 230)).toBe(0);
    expect(minutesLeft(0, 0, 230)).toBe(0);
  });

  test("progress is how far down the column is, and a column that fits is read", () => {
    expect(scrollProgress(0, 2000, 1000)).toBe(0);
    expect(scrollProgress(500, 2000, 1000)).toBe(0.5);
    expect(scrollProgress(1200, 2000, 1000)).toBe(1);
    expect(scrollProgress(0, 800, 1000)).toBe(1);
  });
});

describe("h", () => {
  test("saves the selection tidied, and nothing when nothing is selected", () => {
    expect(highlightPayload("  A delete is\n\n  the absence   of a row ", "m:0", "issue")).toEqual({
      itemKey: "m:0",
      view: "issue",
      quote: "A delete is\nthe absence of a row",
    });
    expect(highlightPayload("   \n ", "m:0", "article")).toBeNull();
    expect(highlightPayload("x".repeat(5000), "m:0", "article")?.quote).toHaveLength(4000);
  });
});

describe("unsubscribe", () => {
  test("says how the sender is told", () => {
    expect(unsubscribeMethodLine("one_click")).toMatch(/told directly/);
    expect(unsubscribeMethodLine("link")).toMatch(/page/);
    expect(unsubscribeMethodLine("mailto")).toMatch(/email/);
    expect(unsubscribeMethodLine("none")).toMatch(/No unsubscribe method/);
  });

  const result = (method: unknown, token: string | null = "tok") => ({
    ok: true,
    result: {
      address: "digest@growth.example",
      status: "preview",
      method,
      message_count: 11,
      archived_count: 0,
      preview_token: token,
    },
  });

  test("takes the method from the daemon's preview, not the edition", () => {
    const preview = unsubscribePreview({
      status: "success",
      data: result({ Mailto: { address: "x@y" } }),
    });
    expect(preview).toEqual({ method: "mailto", token: "tok", count: 11 });
    expect(
      unsubscribePreview({ status: "success", data: result({ OneClick: { url: "u" } }) })?.method,
    ).toBe("one_click");
    expect(
      unsubscribePreview({ status: "success", data: result({ BodyLink: { url: "u" } }) })?.method,
    ).toBe("link");
  });

  test("is not ready while loading, after a failure, without a token or a method", () => {
    expect(unsubscribePreview({ status: "pending", data: undefined })).toBeNull();
    expect(unsubscribePreview({ status: "error", data: undefined })).toBeNull();
    expect(
      unsubscribePreview({ status: "success", data: result({ OneClick: { url: "u" } }, null) }),
    ).toBeNull();
    expect(unsubscribePreview({ status: "success", data: result("None") })).toBeNull();
    expect(unsubscribePreview({ status: "success", data: { ok: false } })).toBeNull();
  });
});
