import { describe, expect, test } from "vitest";

import { formatChord, tokenFromEvent } from "./chord";

function key(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent("keydown", init);
}

describe("tokenFromEvent", () => {
  test("tells ? and / apart by the character typed, not the physical key", () => {
    expect(tokenFromEvent(key({ key: "?", code: "Slash", shiftKey: true }), false)).toBe("?");
    expect(tokenFromEvent(key({ key: "/", code: "Slash" }), false)).toBe("/");
    expect(tokenFromEvent(key({ key: "/", code: "Digit7", shiftKey: true }), false)).toBe("/");
  });

  test("Mod is Cmd on macOS and Ctrl elsewhere", () => {
    expect(tokenFromEvent(key({ key: "k", metaKey: true }), true)).toBe("Mod+k");
    expect(tokenFromEvent(key({ key: "k", ctrlKey: true }), false)).toBe("Mod+k");
    expect(tokenFromEvent(key({ key: "k", metaKey: true }), false)).toBe("Meta+k");
  });
});

describe("formatChord", () => {
  test("renders sequences and modifiers per platform", () => {
    expect(formatChord("g i", true)).toBe("g i");
    expect(formatChord("Mod+k", true)).toBe("⌘K");
    expect(formatChord("Mod+k", false)).toBe("Ctrl+K");
    expect(formatChord("Ctrl+d", true)).toBe("⌃D");
    expect(formatChord("Mod+Shift+p", true)).toBe("⌘⇧P");
    expect(formatChord("Mod+Shift+p", false)).toBe("Ctrl+Shift+P");
  });
});
