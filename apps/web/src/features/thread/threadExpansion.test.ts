import { describe, expect, test } from "vitest";

import { initialLanding } from "./threadExpansion";

function thread(unread: boolean[]) {
  return { messages: unread.map((isUnread, index) => ({ id: `m${index}`, unread: isUnread })) };
}

describe("initialLanding", () => {
  test("a cited message opens expanded and focused, whatever else is unread", () => {
    const landing = initialLanding(thread([false, true, false, false]), "m0");
    expect(landing.focusIndex).toBe(0);
    expect(landing.cited).toBe(true);
    expect([...landing.expanded].toSorted()).toEqual(["m0", "m1", "m3"]);
  });

  test("without one, or with one from another conversation, it lands as before", () => {
    const data = thread([false, true, false]);
    expect(initialLanding(data).focusIndex).toBe(1);
    expect(initialLanding(data, "elsewhere").focusIndex).toBe(1);
    expect(initialLanding(data, "elsewhere").cited).toBe(false);
    expect(initialLanding(data, "elsewhere").expanded.has("elsewhere")).toBe(false);
  });
});
