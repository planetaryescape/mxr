import { describe, expect, test } from "vitest";

import { toastLift } from "./toastClearance";

const VIEWPORT = 900;
// A desktop stack: bottom-right, 356 wide, one toast 72 tall.
const STACK = { left: 1068, right: 1424, height: 72 };
const box = (left: number, right: number, top: number, bottom: number) => ({
  left,
  right,
  top,
  bottom,
});

describe("toastLift", () => {
  test("rests at the toaster's own offset when nothing is in the way", () => {
    expect(toastLift(VIEWPORT, STACK, [], 40)).toBe(40);
    // A bar on the other side of the screen doesn't move it.
    expect(toastLift(VIEWPORT, STACK, [box(0, 800, 800, 870)], 40)).toBe(40);
  });

  test("sits above a bar it would cover", () => {
    // Focus mode's queue keys at the bottom of the reply column.
    expect(toastLift(VIEWPORT, STACK, [box(845, 1440, 800, 871)], 40)).toBe(VIEWPORT - 800 + 8);
  });

  test("keeps lifting past a second bar it lands on", () => {
    const keys = box(845, 1440, 800, 871);
    const send = box(862, 1423, 720, 755);
    expect(toastLift(VIEWPORT, STACK, [keys, send], 40)).toBe(VIEWPORT - 720 + 8);
  });

  test("stays put rather than leave the screen", () => {
    expect(toastLift(VIEWPORT, STACK, [box(0, 1440, 40, 880)], 40)).toBe(40);
  });
});
