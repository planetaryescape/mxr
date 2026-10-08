import { describe, expect, test } from "vitest";

import {
  canLetGoSource,
  cutLine,
  digestRows,
  openableLink,
  ROUTINE_SHOWN,
  tuneToast,
} from "./digestView";
import { digestFixture, lineFixture } from "./testing";

describe("the digest's layout", () => {
  test("sections keep their order and Routine folds after a few sources", () => {
    const digest = digestFixture();
    const folded = digestRows(digest, { routineOpen: false });
    expect(folded.rows.map((row) => row.section)).toEqual([
      "needs_a_look",
      "changed",
      ...Array<string>(ROUTINE_SHOWN).fill("routine"),
    ]);
    expect(folded.rows.map((row) => row.index)).toEqual(folded.rows.map((_, at) => at));
    // Sources 4 and 5 hold 5 and 6 messages.
    expect(folded.quieter).toEqual({ sources: 2, messages: 11 });

    const open = digestRows(digest, { routineOpen: true });
    expect(open.bySection.routine).toHaveLength(6);
    expect(open.quieter).toEqual({ sources: 0, messages: 0 });
  });

  test("lines being let go leave the cursor's order", () => {
    const digest = digestFixture();
    const rows = digestRows(digest, {
      routineOpen: true,
      hidden: new Set([digest.changed[0]!.id]),
    });
    expect(rows.bySection.changed).toHaveLength(0);
    expect(rows.rows[1]?.section).toBe("routine");
  });

  test("the cut line names the digest, its time and its counts", () => {
    expect(cutLine(digestFixture())).toBe(
      "This morning's digest · 08:00 · 23 updates from 8 sources",
    );
    expect(cutLine(digestFixture({ message_count: 1, source_count: 1 }))).toBe(
      "This morning's digest · 08:00 · 1 update from 1 source",
    );
  });
});

describe("what a line offers", () => {
  test("a line that needs you never opens a link", () => {
    const link = { url: "https://myaccount.google.com/", domain: "google.com" };
    expect(openableLink(lineFixture({ link }))).toEqual(link);
    expect(openableLink(lineFixture({ link, signal: "needs_you" }))).toBeUndefined();
    expect(openableLink(lineFixture({ link, section: "needs_a_look" }))).toBeUndefined();
  });

  test("a parcel leaves on its own, so it has no let go", () => {
    expect(canLetGoSource(lineFixture())).toBe(true);
    const parcel = lineFixture({
      message_ids: [],
      tracker: {
        kind: "parcel",
        state: "in_transit",
        state_label: "on its way",
        outcome: "progress",
      },
    });
    expect(canLetGoSource(parcel)).toBe(false);
  });

  test("tuning says what the source does now", () => {
    expect(tuneToast("Strava", "changes_only")).toBe("Strava: only when something changes.");
    expect(tuneToast("Vercel", "muted")).toBe(
      "Vercel: muted, its mail stays in Archive and search.",
    );
  });
});
