/* @vitest-environment node */

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { afterEach, describe, expect, test } from "vitest";

import { isOverlayKey, type DefinedAction } from "@/lib/actions/definition";
import keymap from "@/lib/actions/keymap.generated.json";
import { useKeyScope } from "@/state/keyScopeStore";
import { announced } from "@/test/movedKeys";

import { announceMovedOverlayKey, optionLetter, pressedOverlayKey } from "./overlayKeys";

const press = (name: string, modifiers: Partial<KeyboardEvent> = {}) => ({
  key: name,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...modifiers,
});

afterEach(() => useKeyScope.setState({ moved: null }));

describe("overlay keys", () => {
  test("are the definition's keys", () => {
    expect(pressedOverlayKey(press("j"), "overlay.down", true)).toBe(true);
    expect(pressedOverlayKey(press("ArrowDown"), "overlay.down", false)).toBe(true);
    expect(pressedOverlayKey(press("k"), "overlay.down", true)).toBe(false);
    expect(pressedOverlayKey(press("E", { shiftKey: true }), "unsubscribe.and-archive")).toBe(true);
  });

  test("take the TUI's Ctrl as Mod off macOS, and mac-only keys on macOS only", () => {
    expect(pressedOverlayKey(press("j", { ctrlKey: true }), "search-palette.down", false)).toBe(
      true,
    );
    expect(pressedOverlayKey(press("n", { ctrlKey: true }), "overlay.filter-down", true)).toBe(
      true,
    );
    // Ctrl+N is the browser's New Window off macOS.
    expect(pressedOverlayKey(press("n", { ctrlKey: true }), "overlay.filter-down", false)).toBe(
      false,
    );
  });

  test("a key that moved away says where it went while announced", () => {
    const live = announced("unsubscribe.and-archive", "a", "unsubscribe");
    expect(announceMovedOverlayKey(press("a"), "unsubscribe", true)).toBe(live);
    expect(useKeyScope.getState().moved).toEqual(
      live ? { label: "Unsubscribe and archive their mail", key: "e" } : null,
    );
    expect(announceMovedOverlayKey(press("u"), "unsubscribe", true)).toBe(false);
  });

  test("an option prints its letter", () => {
    expect(optionLetter("unsubscribe.confirm")).toBe("u");
    expect(optionLetter("sender-ask.yes")).toBe("y");
  });

  // A dialog reads each of its keys by id, so no key the definition gives a
  // dialog can go unread, unless the browser or the dialog primitive gives it.
  test("every web overlay key the browser doesn't give is read by a dialog", () => {
    const src = join(__dirname, "../..");
    const files = (dir: string): string[] =>
      readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
        const path = join(dir, entry.name);
        if (entry.isDirectory()) return files(path);
        return /\.tsx?$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name) ? [path] : [];
      });
    const source = files(src)
      .map((path) => readFileSync(path, "utf8"))
      .join("\n");
    const unread = (keymap.action as DefinedAction[])
      .filter(
        (action) =>
          (!action.clients || action.clients.includes("web")) &&
          isOverlayKey(action) &&
          !action.web_native,
      )
      .map((action) => action.id)
      .filter((id) => !source.includes(`"${id}"`));
    expect(unread).toEqual([]);
  });
});
