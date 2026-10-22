import { describe, expect, test } from "vitest";

import { formatChord, parseChord, tokenFromEvent } from "./chord";

function key(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent("keydown", init);
}

describe("tokenFromEvent", () => {
  test("tells ? and / apart by the character typed, not the physical key", () => {
    // US layout: Shift+Slash types "?".
    expect(tokenFromEvent(key({ key: "?", code: "Slash", shiftKey: true }), false)).toBe("?");
    expect(tokenFromEvent(key({ key: "/", code: "Slash" }), false)).toBe("/");
    // A layout where "/" itself needs Shift still means "/".
    expect(tokenFromEvent(key({ key: "/", code: "Digit7", shiftKey: true }), false)).toBe("/");
  });

  test("keeps shifted printable characters as typed", () => {
    expect(tokenFromEvent(key({ key: "G", shiftKey: true }), false)).toBe("G");
    expect(tokenFromEvent(key({ key: "g" }), false)).toBe("g");
    expect(tokenFromEvent(key({ key: "#", shiftKey: true }), false)).toBe("#");
  });

  test("Mod is Cmd on macOS and Ctrl elsewhere", () => {
    expect(tokenFromEvent(key({ key: "k", metaKey: true }), true)).toBe("Mod+k");
    expect(tokenFromEvent(key({ key: "k", ctrlKey: true }), false)).toBe("Mod+k");
    // Cmd on a non-Mac keyboard is not Mod.
    expect(tokenFromEvent(key({ key: "k", metaKey: true }), false)).toBe("Meta+k");
  });

  test("Ctrl on macOS stays Ctrl so TUI chords don't steal Cmd", () => {
    expect(tokenFromEvent(key({ key: "d", ctrlKey: true }), true)).toBe("Ctrl+d");
  });

  test("spells Shift out with modifiers and lowercases the letter", () => {
    expect(tokenFromEvent(key({ key: "P", metaKey: true, shiftKey: true }), true)).toBe(
      "Mod+Shift+p",
    );
  });

  test("names Space and Shift on named keys", () => {
    expect(tokenFromEvent(key({ key: " " }), false)).toBe("Space");
    expect(tokenFromEvent(key({ key: " ", shiftKey: true }), false)).toBe("Shift+Space");
    expect(tokenFromEvent(key({ key: "Enter", shiftKey: true }), false)).toBe("Shift+Enter");
    expect(tokenFromEvent(key({ key: "Escape" }), false)).toBe("Escape");
  });

  test("a character typed with Option on macOS is the character, with no Alt", () => {
    // UK Mac: Option-3 types "#". German Mac: Option-5 and Option-8 type "[" and "{".
    expect(tokenFromEvent(key({ key: "#", code: "Digit3", altKey: true }), true)).toBe("#");
    expect(tokenFromEvent(key({ key: "[", code: "Digit5", altKey: true }), true)).toBe("[");
    expect(
      tokenFromEvent(key({ key: "{", code: "Digit8", altKey: true, shiftKey: true }), true),
    ).toBe("{");
  });

  test("a character typed with AltGr off macOS is the character, with no modifiers", () => {
    // Browsers report AltGr as Ctrl+Alt. German: AltGr-8 "[", AltGr-7 "{"; French: AltGr-3 "#".
    expect(
      tokenFromEvent(key({ key: "[", code: "Digit8", ctrlKey: true, altKey: true }), false),
    ).toBe("[");
    expect(
      tokenFromEvent(key({ key: "{", code: "Digit7", ctrlKey: true, altKey: true }), false),
    ).toBe("{");
    expect(
      tokenFromEvent(key({ key: "#", code: "Digit3", ctrlKey: true, altKey: true }), false),
    ).toBe("#");
  });

  test("real Alt and Mod chords keep their modifiers", () => {
    expect(tokenFromEvent(key({ key: "a", code: "KeyA", altKey: true }), false)).toBe("Alt+a");
    expect(tokenFromEvent(key({ key: "a", code: "KeyA", altKey: true }), true)).toBe("Alt+a");
    expect(tokenFromEvent(key({ key: "k", metaKey: true, altKey: true }), true)).toBe("Mod+Alt+k");
    expect(tokenFromEvent(key({ key: "k", ctrlKey: true, altKey: true }), false)).toBe("Mod+Alt+k");
    // Plain Alt off macOS is not AltGr: the chord stays a chord.
    expect(tokenFromEvent(key({ key: "[", altKey: true }), false)).toBe("Alt+[");
    expect(tokenFromEvent(key({ key: "F4", altKey: true }), false)).toBe("Alt+F4");
  });

  test("ignores a bare modifier press", () => {
    expect(tokenFromEvent(key({ key: "Shift", shiftKey: true }), false)).toBeNull();
    expect(tokenFromEvent(key({ key: "Meta", metaKey: true }), true)).toBeNull();
  });
});

describe("parseChord", () => {
  test("splits sequences on whitespace", () => {
    expect(parseChord("  g   i ")).toEqual(["g", "i"]);
    expect(parseChord("Mod+k")).toEqual(["Mod+k"]);
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

  test("uses short names for named keys", () => {
    expect(formatChord("Escape", false)).toBe("Esc");
    expect(formatChord("ArrowDown", false)).toBe("↓");
    expect(formatChord("Shift+Space", true)).toBe("⇧Space");
  });

  test("formats + as a key", () => {
    expect(formatChord("Mod++", true)).toBe("⌘+");
  });
});
