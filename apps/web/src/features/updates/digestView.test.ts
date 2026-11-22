import { describe, expect, test } from "vitest";

import { digestRows, openableLink, trackLabel } from "./digestView";
import { digestFixture, lineFixture } from "./testing";

describe("the digest's layout", () => {
  test("Routine has no rows until opened, and its totals still count", () => {
    const digest = digestFixture();
    const collapsed = digestRows(digest, { routineOpen: false });
    expect(collapsed.rows.map((row) => row.section)).toEqual(["needs_a_look", "changed"]);
    expect(collapsed.rows.map((row) => row.index)).toEqual([0, 1]);
    expect(collapsed.routine).toEqual({ sources: 6, messages: 21 });

    const open = digestRows(digest, { routineOpen: true });
    expect(open.bySection.routine).toHaveLength(6);
    expect(open.routine).toEqual({ sources: 6, messages: 21 });
  });
});

describe("what a line offers", () => {
  test("a line that needs you never opens a link", () => {
    const link = { url: "https://myaccount.google.com/", domain: "google.com" };
    expect(openableLink(lineFixture({ link }))).toEqual(link);
    expect(openableLink(lineFixture({ link, signal: "needs_you" }))).toBeUndefined();
    expect(openableLink(lineFixture({ link, section: "needs_a_look" }))).toBeUndefined();
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
    expect(trackLabel(parcel(3))).toBeNull();
    expect(trackLabel(parcel(null))).toBeNull();
    expect(trackLabel(parcel(undefined))).toBeNull();
    expect(trackLabel(parcel(9))).toBeNull();
    expect(trackLabel(parcel(-1))).toBeNull();
    expect(trackLabel(parcel(1.5))).toBeNull();
    expect(trackLabel(lineFixture())).toBeNull();
  });
});
