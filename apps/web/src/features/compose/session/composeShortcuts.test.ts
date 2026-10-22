import type { KeyboardEvent } from "react";
import { describe, expect, it, vi } from "vitest";

import { definedKeys } from "@/lib/actions/definition";

import {
  codeMirrorKey,
  COMPOSE_CHORDS,
  composeCommandOf,
  composeKeyActions,
  handleComposeShortcut,
  isComposeActionChord,
  isComposeChord,
  type ComposeShortcutHandlers,
} from "./composeShortcuts";

function handlers(): ComposeShortcutHandlers {
  return {
    busy: false,
    canServerSave: true,
    revealCc: vi.fn<() => void>(),
    revealBcc: vi.fn<() => void>(),
    handleAttachShortcut: vi.fn<() => void>(),
    requestSendLater: vi.fn<() => void>(),
    openSignaturePicker: vi.fn<() => void>(),
    handleRefreshClick: vi.fn<() => Promise<void>>(async () => {}),
    handleServerSaveClick: vi.fn<() => Promise<void>>(async () => {}),
    openSnippetPicker: vi.fn<() => void>(),
    handleSaveClick: vi.fn<() => Promise<void>>(async () => {}),
    requestSendAndArchive: vi.fn<() => void>(),
    requestSend: vi.fn<() => void>(),
    requestDiscard: vi.fn<() => void>(),
  };
}

function keyEvent(init: Partial<KeyboardEvent<HTMLDivElement>>) {
  const preventDefault = vi.fn<() => void>();
  const event = {
    defaultPrevented: false,
    metaKey: true,
    ctrlKey: false,
    shiftKey: false,
    repeat: false,
    key: "Enter",
    preventDefault,
    stopPropagation: vi.fn<() => void>(),
    ...init,
  } as unknown as KeyboardEvent<HTMLDivElement>;
  return { event, preventDefault };
}

describe("compose shortcuts", () => {
  it("a held ⌘↵ sends once and claims its repeats", () => {
    const bag = handlers();
    handleComposeShortcut(keyEvent({}).event, bag);
    const repeat = keyEvent({ repeat: true });
    handleComposeShortcut(repeat.event, bag);
    handleComposeShortcut(keyEvent({ repeat: true }).event, bag);
    expect(bag.requestSend).toHaveBeenCalledTimes(1);
    expect(repeat.preventDefault).toHaveBeenCalled();
  });

  it("no compose chord runs on a repeat", () => {
    const bag = handlers();
    for (const [key, shiftKey] of [
      ["Enter", true],
      ["Backspace", false],
      ["s", false],
      ["s", true],
      ["l", true],
      [";", false],
    ] as const) {
      handleComposeShortcut(keyEvent({ key, shiftKey, repeat: true }).event, bag);
    }
    const calls = Object.values(bag).filter((value) => typeof value === "function");
    expect(calls.every((fn) => (fn as ReturnType<typeof vi.fn>).mock.calls.length === 0)).toBe(
      true,
    );
  });

  it("editor chords keep repeating: held ⌘Z, ⌘⇧Z, ⌘B, ⌘C and ⌘A pass through", () => {
    const bag = handlers();
    for (const [key, shiftKey] of [
      ["z", false],
      ["z", true],
      ["b", false],
      ["i", false],
      ["c", false],
      ["a", false],
      ["ArrowLeft", false],
    ] as const) {
      const held = keyEvent({ key, shiftKey, repeat: true });
      handleComposeShortcut(held.event, bag);
      expect(held.preventDefault).not.toHaveBeenCalled();
      expect(isComposeChord({ key, shiftKey, metaKey: true, ctrlKey: false })).toBe(false);
    }
  });

  it("names exactly mxr's own chords", () => {
    const chord = (key: string, shiftKey = false) =>
      isComposeChord({ key, shiftKey, metaKey: false, ctrlKey: true });
    expect([chord("Enter"), chord("Enter", true), chord("s"), chord("Backspace")]).toEqual([
      true,
      true,
      true,
      true,
    ]);
    expect([chord("C", true), chord("B", true), chord("L", true), chord(";")]).toEqual([
      true,
      true,
      true,
      true,
    ]);
    expect([chord("z"), chord("Z", true), chord("b"), chord("c"), chord("v")]).toEqual([
      false,
      false,
      false,
      false,
      false,
    ]);
    expect(isComposeChord({ key: "Enter", shiftKey: false, metaKey: false, ctrlKey: false })).toBe(
      false,
    );
  });

  it("the handler runs exactly the table's chords, each its own command", () => {
    const mismatches: string[] = [];
    for (const [chord, command] of Object.entries(COMPOSE_CHORDS)) {
      const bag = handlers();
      const shiftKey = chord.startsWith("shift+");
      const key = chord.replace("shift+", "");
      handleComposeShortcut(keyEvent({ key, shiftKey }).event, bag);
      const ran = Object.entries(bag)
        .filter(
          ([, fn]) => typeof fn === "function" && vi.isMockFunction(fn) && fn.mock.calls.length > 0,
        )
        .map(([name]) => name);
      if (ran.join() !== command) mismatches.push(`${chord}: ${ran.join() || "nothing"}`);
    }
    expect(mismatches).toEqual([]);
  });

  it("⌘⇧⌫ is a discard chord everywhere, so its repeats are swallowed too", () => {
    expect(
      composeCommandOf({ key: "Backspace", shiftKey: true, metaKey: true, ctrlKey: false }),
    ).toBe("requestDiscard");
    const bag = handlers();
    const held = keyEvent({ key: "Backspace", shiftKey: true, repeat: true });
    handleComposeShortcut(held.event, bag);
    expect(held.preventDefault).toHaveBeenCalled();
    expect(bag.requestDiscard).not.toHaveBeenCalled();
  });

  it("takes its chords from the key definition, one per defined key", () => {
    const definedCount = composeKeyActions
      .filter((action) => action.id !== "compose.fullscreen")
      .reduce((count, action) => count + definedKeys(action.id).length, 0);
    expect(Object.keys(COMPOSE_CHORDS)).toHaveLength(definedCount);
    expect(COMPOSE_CHORDS["shift+r"]).toBe("handleRefreshClick");
    expect(COMPOSE_CHORDS[";"]).toBe("openSnippetPicker");
  });

  it("fullscreen is its own chord, outside the handler table", () => {
    const event = { key: "F", shiftKey: true, metaKey: true, ctrlKey: false };
    expect(isComposeActionChord(event, "compose.fullscreen")).toBe(true);
    expect(
      isComposeActionChord({ ...event, metaKey: false, ctrlKey: true }, "compose.fullscreen"),
    ).toBe(true);
    expect(isComposeActionChord({ ...event, shiftKey: false }, "compose.fullscreen")).toBe(false);
    expect(isComposeChord(event)).toBe(false);
  });

  it("spells definition chords in CodeMirror's notation", () => {
    expect(codeMirrorKey(definedKeys("compose.save")[0] ?? "")).toBe("Mod-s");
    expect(codeMirrorKey("Mod+Enter")).toBe("Mod-Enter");
    expect(codeMirrorKey("Mod+Shift+Enter")).toBe("Mod-Shift-Enter");
    expect(codeMirrorKey("Mod++")).toBe("Mod-+");
  });
});
