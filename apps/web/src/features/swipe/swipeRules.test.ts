import { describe, expect, it } from "vitest";

import { pendingAt, releaseAt, SWIPE, type SwipeMap } from "./swipeRules";

const MAIL: SwipeMap = { right: "archive", rightLong: "trash", left: "snooze" };
const WAITING: SwipeMap = { right: "done", left: "snooze" };

describe("swipe rules", () => {
  it("lets a fast flick commit the short action but never the destructive one", () => {
    expect(releaseAt(SWIPE.flickDistance, SWIPE.flickVelocity, MAIL)).toBe("archive");
    expect(releaseAt(-SWIPE.flickDistance, -SWIPE.flickVelocity, MAIL)).toBe("snooze");
    expect(releaseAt(SWIPE.flickDistance - 1, 5, MAIL)).toBeNull();
    expect(releaseAt(SWIPE.long - 1, 10, MAIL)).toBe("archive");
  });

  it("always arms exactly what a release at that instant would commit", () => {
    const mismatches: string[] = [];
    for (const map of [MAIL, WAITING]) {
      for (let dx = -320; dx <= 320; dx += 5) {
        for (const velocity of [-10, -1, -0.5, -0.49, -0.1, 0, 0.1, 0.49, 0.5, 1, 10]) {
          const pending = pendingAt(dx, velocity, map);
          const armed = pending?.armed ? pending.action : null;
          if (armed !== releaseAt(dx, velocity, map)) mismatches.push(`${dx}@${velocity}`);
        }
      }
    }
    expect(mismatches).toEqual([]);
  });
});
