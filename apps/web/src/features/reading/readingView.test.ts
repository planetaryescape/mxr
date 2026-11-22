import { describe, expect, test } from "vitest";

import type { components } from "@/api/generated";

import type { ReadingBand, ReadingItem, ReadingLink } from "./api";

type UnsubscribeMethod = components["schemas"]["UnsubscribeMethod"];
import {
  editionEntries,
  LINKS_SHOWN,
  letGoAllPlan,
  shownLinks,
  unsubscribePreview,
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

const purgePreview = (method: UnsubscribeMethod, token: string | null = "tok") => ({
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

describe("unsubscribe", () => {
  test("takes the method from the daemon's preview, not the edition", () => {
    const preview = unsubscribePreview({
      status: "success",
      data: purgePreview({ Mailto: { address: "x@y" } }),
    });
    expect(preview).toEqual({ method: "mailto", token: "tok", count: 11 });
    expect(
      unsubscribePreview({ status: "success", data: purgePreview({ OneClick: { url: "u" } }) })
        ?.method,
    ).toBe("one_click");
    expect(
      unsubscribePreview({ status: "success", data: purgePreview({ BodyLink: { url: "u" } }) })
        ?.method,
    ).toBe("link");
  });
});
