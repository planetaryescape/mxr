/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import { actionShortcutSections } from "./hints";
import type { ActionContext } from "./types";

const baseCtx: ActionContext = {
  path: "/m/inbox",
  activePane: "mailbox",
  selectionCount: 0,
  accountCount: 0,
  hasFocusedThread: false,
  hasFocusedMessage: false,
  isFirstAccountOnly: false,
};

describe("actionShortcutSections", () => {
  test("groups visible registry actions by action.group", () => {
    const sections = actionShortcutSections(baseCtx);
    const titles = sections.map((s) => s.title);
    expect(titles).toContain("Navigation");
    expect(titles).toContain("Search");
    expect(titles).toContain("Compose");
  });

  test("includes paletteOnly actions when they have a shortcut", () => {
    // shell.help is paletteOnly with shortcut "Shift+Slash" → "?"
    const sections = actionShortcutSections(baseCtx);
    const allHints = sections.flatMap((s) => s.hints);
    expect(allHints).toContainEqual({ key: "?", label: "Help" });
  });

  test("appends page hints after registry sections", () => {
    const pageSections = [
      { title: "Reader shortcuts", hints: [{ key: "j/k", label: "Scroll" }] },
    ];
    const sections = actionShortcutSections(baseCtx, pageSections);
    expect(sections.at(-1)?.title).toBe("Reader shortcuts");
  });

  test("formats $mod as command key in chip labels", () => {
    const sections = actionShortcutSections(baseCtx);
    const allHints = sections.flatMap((s) => s.hints);
    expect(allHints).toContainEqual({ key: "⌘k", label: "Command palette" });
  });
});
