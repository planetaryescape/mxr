import { describe, expect, test } from "vitest";

import type { PlaceBundle, PlaceMessage, PlaceResponse } from "./api";
import { mapPlaceData, mergeBundles, messagesLeft, nextBundleOffset } from "./placePaging";

function message(id: string, pinned = false): PlaceMessage {
  return {
    message_id: id,
    thread_id: `t-${id}`,
    subject: id,
    snippet: "",
    date: "2026-09-20T08:00:00Z",
    unread: false,
    pinned,
    starred: false,
  };
}

function bundle(email: string, messages: PlaceMessage[], count = messages.length): PlaceBundle {
  return {
    account_id: "acct",
    sender_email: email,
    kind: {
      kind: "paper_trail",
      rule: "automated_address",
      reason: "automated sender",
      corrected: false,
    },
    message_count: count,
    unread_count: 0,
    pinned_count: messages.filter((m) => m.pinned).length,
    newest_at: "2026-09-20T08:00:00Z",
    newest_subject: "",
    messages,
  };
}

function page(bundles: PlaceBundle[], totalBundles: number): PlaceResponse {
  return {
    kind: "Place",
    place: "paper_trail",
    bundles,
    total_bundles: totalBundles,
    total_messages: 0,
    generated_at: "2026-09-20T08:00:00Z",
  };
}

function rename(response: PlaceResponse): PlaceResponse {
  return { ...response, total_messages: 7 };
}

describe("paging a place", () => {
  test("the next bundle page starts after what has loaded, until all have", () => {
    expect(nextBundleOffset([])).toBe(0);
    const first = page([bundle("a@x", []), bundle("b@x", [])], 3);
    expect(nextBundleOffset([first])).toBe(2);
    expect(nextBundleOffset([first, page([bundle("c@x", [])], 3)])).toBeUndefined();
    // An empty page ends paging even if the total says otherwise.
    expect(nextBundleOffset([first, page([], 5)])).toBeUndefined();
  });

  test("bundles merge once each, with later message pages appended once", () => {
    const shop = bundle("shop@x", [message("1", true), message("2")], 5);
    const pages = [
      page([shop], 2),
      page([bundle("shop@x", [message("9")]), bundle("bank@x", [])], 2),
    ];
    const merged = mergeBundles(pages, new Map([["acct|shop@x", [message("2"), message("3")]]]));
    expect(merged.map((b) => b.sender_email)).toEqual(["shop@x", "bank@x"]);
    expect(merged[0]!.messages.map((m) => m.message_id)).toEqual(["1", "2", "3"]);
    expect(messagesLeft(merged[0]!)).toBe(2);
  });

  test("cache patches reach a single response and every page of a paged list", () => {
    expect(mapPlaceData(page([], 0), rename)).toMatchObject({ total_messages: 7 });
    const paged = { pages: [page([], 0), page([], 0)], pageParams: [0, 50] };
    expect(mapPlaceData(paged, rename)).toMatchObject({
      pages: [{ total_messages: 7 }, { total_messages: 7 }],
      pageParams: [0, 50],
    });
    expect(mapPlaceData(undefined, rename)).toBeUndefined();
  });
});
