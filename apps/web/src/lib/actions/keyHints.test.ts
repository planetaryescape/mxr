import { describe, expect, it } from "vitest";

import {
  createHintTracker,
  HINT_GAP_MS,
  KEYBOARD_QUIET_MS,
  parseHintMemory,
  type HintStore,
} from "./keyHints";
import type { Action } from "./types";

const archive: Action = {
  id: "mail.archive",
  command: "archive",
  label: "Archive",
  shortcut: "e",
  group: "Mail",
  scopes: ["list", "reader"],
};
const trash: Action = { ...archive, id: "mail.trash", label: "Move to Trash", shortcut: "#" };
const keyless: Action = { ...archive, id: "mail.keyless", shortcut: undefined };

function memoryStore(): HintStore & { raw: () => unknown } {
  let saved: ReturnType<HintStore["load"]> = null;
  return {
    load: () => structuredClone(saved) ?? { counts: {}, shown: [], lastShownAt: 0 },
    save: (memory) => {
      saved = structuredClone(memory);
    },
    raw: () => saved,
  };
}

function setup() {
  let time = 1_000_000;
  const store = memoryStore();
  const tracker = createHintTracker(store, () => time);
  return {
    tracker,
    store,
    advance: (ms: number) => {
      time += ms;
    },
  };
}

describe("key hints", () => {
  it("names the key after the third pointer use, once ever", () => {
    const { tracker, advance } = setup();
    expect(tracker.pointer(archive)).toBeNull();
    expect(tracker.pointer(archive)).toBeNull();
    const hint = tracker.pointer(archive);
    expect(hint?.text).toBe("Tip: press e for Archive.");
    advance(HINT_GAP_MS * 10);
    for (let use = 0; use < 10; use += 1) expect(tracker.pointer(archive)).toBeNull();
  });

  it("never shows while the keyboard is in use, and shows once it rests", () => {
    const { tracker, advance } = setup();
    tracker.pointer(archive);
    tracker.pointer(archive);
    tracker.keyboard();
    expect(tracker.pointer(archive)).toBeNull();
    advance(KEYBOARD_QUIET_MS);
    expect(tracker.pointer(archive)?.actionId).toBe("mail.archive");
  });

  it("shows at most one hint every few minutes", () => {
    const { tracker, advance } = setup();
    for (let use = 0; use < 3; use += 1) tracker.pointer(archive);
    for (let use = 0; use < 3; use += 1) expect(tracker.pointer(trash)).toBeNull();
    advance(HINT_GAP_MS);
    expect(tracker.pointer(trash)?.text).toBe("Tip: press # for Move to Trash.");
  });

  it("ignores actions without a key, and survives missing storage", () => {
    const { tracker } = setup();
    for (let use = 0; use < 5; use += 1) expect(tracker.pointer(keyless)).toBeNull();
    const blind = createHintTracker({ load: () => null, save: () => undefined }, () => 0);
    for (let use = 0; use < 5; use += 1) expect(blind.pointer(archive)).toBeNull();
  });

  it("uses the button's own words when the action's label is generic", () => {
    const { tracker } = setup();
    const rowAction: Action = {
      ...archive,
      id: "list.row-action",
      label: "This list's row action",
      shortcut: "w",
    };
    tracker.pointer(rowAction, "Done");
    tracker.pointer(rowAction, "Done");
    expect(tracker.pointer(rowAction, "Done")?.text).toBe("Tip: press w for Done.");
  });

  it("starts over from stored memory that isn't what it wrote", () => {
    expect(parseHintMemory("nonsense")).toEqual({ counts: {}, shown: [], lastShownAt: 0 });
    expect(
      parseHintMemory({ counts: { a: "3", b: 2 }, shown: "mail.archive", lastShownAt: null }),
    ).toEqual({
      counts: { b: 2 },
      shown: [],
      lastShownAt: 0,
    });
  });
});
