import { describe, expect, test } from "vitest";

import {
  canLetGoSource,
  cutLine,
  digestRows,
  displaySourceName,
  gistThread,
  numberNotInFact,
  openableLink,
  routineLine,
  splitSourceName,
  trackLabel,
  trailingItem,
  tuneToast,
} from "./digestView";
import { digestFixture, lineFixture } from "./testing";

describe("source names read as names", () => {
  test("an all-lowercase acronym is upper-cased; a longer one gets a capital", () => {
    expect(displaySourceName("ups")).toBe("UPS");
    expect(displaySourceName("usps")).toBe("USPS");
    expect(displaySourceName("dhl")).toBe("DHL");
    expect(displaySourceName("plausible")).toBe("Plausible");
    expect(displaySourceName("linear")).toBe("Linear");
    // Four letters exactly is still an acronym; five is a word.
    expect(displaySourceName("dell")).toBe("DELL");
    expect(displaySourceName("strav")).toBe("Strav");
  });

  test("a name with any capital is left exactly as it came", () => {
    expect(displaySourceName("eBay")).toBe("eBay");
    expect(displaySourceName("iCloud")).toBe("iCloud");
    expect(displaySourceName("GitHub acme/api")).toBe("GitHub acme/api");
    expect(displaySourceName("Amazon.com")).toBe("Amazon.com");
    expect(displaySourceName("Account Verification")).toBe("Account Verification");
  });

  test("an empty name stays empty, and a name with no letters is unchanged", () => {
    expect(displaySourceName("")).toBe("");
    expect(displaySourceName("1password")).toBe("1password");
  });
});

describe("the digest's layout", () => {
  test("Routine has no rows until opened, and its totals still count", () => {
    const digest = digestFixture();
    const collapsed = digestRows(digest, { routineOpen: false });
    expect(collapsed.rows.map((row) => row.section)).toEqual(["needs_a_look", "changed"]);
    expect(collapsed.rows.map((row) => row.index)).toEqual([0, 1]);
    // Six sources holding 1 to 6 messages.
    expect(collapsed.routine).toEqual({ sources: 6, messages: 21 });

    const open = digestRows(digest, { routineOpen: true });
    expect(open.bySection.routine).toHaveLength(6);
    expect(open.routine).toEqual({ sources: 6, messages: 21 });
  });

  test("the collapsed Routine line counts updates and sources in the daemon's words", () => {
    expect(routineLine({ sources: 16, messages: 31 })).toBe("Routine · 31 updates from 16 sources");
    expect(routineLine({ sources: 1, messages: 1 })).toBe("Routine · 1 update from 1 source");
  });

  test("hidden lines leave the Routine totals too", () => {
    const digest = digestFixture();
    const rows = digestRows(digest, {
      routineOpen: false,
      hidden: new Set([digest.routine[0]!.id]),
    });
    expect(rows.routine).toEqual({ sources: 5, messages: 20 });
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

describe("a row's facts", () => {
  test("a parcel's track says where it is and what comes next", () => {
    const steps = ["ordered", "shipped", "out for delivery", "delivered"];
    const parcel = (step: number | null | undefined) =>
      lineFixture({
        tracker: { kind: "parcel", state: "x", state_label: "x", outcome: "progress", steps, step },
      });
    expect(trackLabel(parcel(1))).toBe("Shipped · next: out for delivery");
    expect(trackLabel(parcel(0))).toBe("Ordered · next: shipped");
    // At the last step the fact already says it arrived.
    expect(trackLabel(parcel(3))).toBeNull();
    expect(trackLabel(parcel(null))).toBeNull();
    expect(trackLabel(parcel(undefined))).toBeNull();
    // A step the daemon sent outside the track shows nothing rather than a wrong one.
    expect(trackLabel(parcel(9))).toBeNull();
    expect(trackLabel(parcel(-1))).toBeNull();
    expect(trackLabel(parcel(1.5))).toBeNull();
    expect(trackLabel(lineFixture())).toBeNull();
  });

  test("the sending host splits off a Needs a look name only when it is the sender's", () => {
    const google = lineFixture({
      source_name: "Google (accounts.google.com)",
      sender_email: "no-reply@accounts.google.com",
    });
    expect(splitSourceName(google)).toEqual({ name: "Google", host: "accounts.google.com" });
    // A casing difference in the daemon's copy still splits; a mismatched host does not.
    expect(
      splitSourceName(
        lineFixture({
          source_name: "Google (Accounts.Google.com)",
          sender_email: "a@accounts.google.com",
        }),
      ),
    ).toEqual({ name: "Google", host: "accounts.google.com" });
    expect(
      splitSourceName(
        lineFixture({
          source_name: "Google (evil.example)",
          sender_email: "a@accounts.google.com",
        }),
      ),
    ).toEqual({ name: "Google (evil.example)", host: null });
    expect(
      splitSourceName(lineFixture({ source_name: "Strava", sender_email: undefined })),
    ).toEqual({
      name: "Strava",
      host: null,
    });
  });

  test("the amount shown is the first one the fact leaves out", () => {
    const line = lineFixture({
      fact: "Your scheduled payment of £33.33 is due",
      numbers: [
        { raw: "£33.33", value: 33.33, unit: "GBP" },
        { raw: "£1,204.33", value: 1204.33, unit: "GBP" },
      ],
    });
    expect(numberNotInFact(line)?.raw).toBe("£1,204.33");
    expect(
      numberNotInFact(lineFixture({ fact: "Your statement", numbers: undefined })),
    ).toBeUndefined();
  });

  test("one trailing item, in precedence order", () => {
    const delta = {
      raw: "21.3 km",
      previous_raw: "19.0 km",
      change: 12.1,
      text: "up 12%",
      against: "2026-09-30T05:00:00Z",
    };
    const amount = { raw: "£4.20", value: 4.2, unit: "GBP" };
    const everything = lineFixture({
      in_todo: "already in To do",
      todo_suggestion: "Suggested to-do",
      delta,
      numbers: [amount],
      section: "routine",
      count: 5,
    });
    expect(trailingItem(everything)).toEqual({ kind: "in_todo", text: "already in To do" });
    expect(trailingItem({ ...everything, in_todo: null })).toEqual({ kind: "todo_suggestion" });
    expect(trailingItem({ ...everything, in_todo: null, todo_suggestion: null })).toEqual({
      kind: "delta",
      text: "up 12%",
      title: "Computed from 21.3 km and 19.0 km",
    });
    const noDelta = { ...everything, in_todo: null, todo_suggestion: null, delta: null };
    expect(trailingItem(noDelta)).toEqual({ kind: "amount", text: "£4.20" });
    expect(trailingItem({ ...noDelta, numbers: [] })).toEqual({ kind: "count", text: "5 emails" });
    // A count is a routine thing and a single message needs no count.
    expect(trailingItem({ ...noDelta, numbers: [], count: 1 })).toBeNull();
    expect(trailingItem({ ...noDelta, numbers: [], section: "changed" })).toBeNull();
  });
});

describe("which lines ask for a gist", () => {
  test("a subject-only fact from the line's latest email asks about that thread", () => {
    expect(gistThread(lineFixture())).toBe("t1");
    expect(gistThread(lineFixture({ fact_message_id: "m1", latest_message_id: "m1" }))).toBe("t1");
  });

  test("a fact the rules took from the body, a fact from an older email, or no thread does not", () => {
    expect(gistThread(lineFixture({ fact_source: "body" }))).toBeNull();
    expect(gistThread(lineFixture({ fact_source: "rule" }))).toBeNull();
    expect(gistThread(lineFixture({ fact_message_id: "m0", latest_message_id: "m1" }))).toBeNull();
    expect(gistThread(lineFixture({ latest_thread_id: null }))).toBeNull();
  });
});
