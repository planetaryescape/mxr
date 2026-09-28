import { describe, expect, it } from "vitest";

import { followDistance, lockAxis, pendingAt, releaseAt, SWIPE, type SwipeMap } from "./swipeRules";

const MAIL: SwipeMap = { right: "archive", rightLong: "trash", left: "snooze" };
const WAITING: SwipeMap = { right: "done", left: "snooze" };

describe("swipe rules", () => {
  it("waits a few pixels, then locks to one axis", () => {
    expect(lockAxis(3, 2)).toBe("pending");
    expect(lockAxis(12, 4)).toBe("x");
    expect(lockAxis(4, 12)).toBe("y");
    expect(lockAxis(-12, 3)).toBe("x");
  });

  it("shows nothing before the reveal distance, then the pending action", () => {
    expect(pendingAt(SWIPE.reveal - 1, MAIL)).toBeNull();
    expect(pendingAt(SWIPE.reveal, MAIL)).toEqual({
      action: "archive",
      side: "right",
      armed: false,
    });
    expect(pendingAt(SWIPE.commit, MAIL)).toEqual({
      action: "archive",
      side: "right",
      armed: true,
    });
    expect(pendingAt(SWIPE.long, MAIL)).toEqual({ action: "trash", side: "right", armed: true });
    expect(pendingAt(-SWIPE.reveal, MAIL)).toEqual({
      action: "snooze",
      side: "left",
      armed: false,
    });
  });

  it("commits on release past the threshold, and only there", () => {
    expect(releaseAt(SWIPE.commit - 1, 0, MAIL)).toBeNull();
    expect(releaseAt(SWIPE.commit, 0, MAIL)).toBe("archive");
    expect(releaseAt(-SWIPE.commit, 0, MAIL)).toBe("snooze");
    expect(releaseAt(SWIPE.long - 1, 0, MAIL)).toBe("archive");
    expect(releaseAt(SWIPE.long, 0, MAIL)).toBe("trash");
  });

  it("lets a fast flick commit the short action but never the destructive one", () => {
    expect(releaseAt(SWIPE.flickDistance, SWIPE.flickVelocity, MAIL)).toBe("archive");
    expect(releaseAt(-SWIPE.flickDistance, -SWIPE.flickVelocity, MAIL)).toBe("snooze");
    expect(releaseAt(SWIPE.flickDistance - 1, 5, MAIL)).toBeNull();
    // However hard the flick, trash needs the long throw.
    expect(releaseAt(SWIPE.long - 1, 10, MAIL)).toBe("archive");
  });

  it("gives a Waiting row done instead of archive, and no trash", () => {
    expect(pendingAt(SWIPE.long + 50, WAITING)?.action).toBe("done");
    expect(releaseAt(SWIPE.long + 50, 0, WAITING)).toBe("done");
    // Past the commit distance the row resists: nothing further waits there.
    expect(followDistance(SWIPE.commit + 100, WAITING)).toBeLessThan(SWIPE.commit + 100);
    expect(followDistance(SWIPE.commit + 100, MAIL)).toBe(SWIPE.commit + 100);
  });

  it.each([
    // [dx, velocity px/ms, expected]
    [189, 0, "archive"],
    [190, 0, "trash"],
    [300, 0, "trash"],
    [190, 0.49, "trash"],
    [190, 0.5, "archive"],
    [190, 10, "archive"],
    [300, 10, "archive"],
    [300, -10, "archive"],
    [89, 0, null],
    [89, 10, "archive"],
    [29, 10, null],
    [-90, 0, "snooze"],
    [-300, -10, "snooze"],
  ] as const)("releases at %i px moving %f px/ms as %s", (dx, velocity, expected) => {
    expect(releaseAt(dx, velocity, MAIL)).toBe(expected);
  });
});
