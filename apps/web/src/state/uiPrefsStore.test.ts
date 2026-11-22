/* @vitest-environment jsdom */

import { describe, expect, test } from "vitest";

import {
  DEFAULT_READING_DISPLAY,
  readReadingDisplay,
  migrateUiPrefs,
  useUiPrefs,
} from "./uiPrefsStore";

describe("compose editor preference", () => {
  test("a saved Rich text choice is kept, not migrated to the new default", async () => {
    await useUiPrefs.persist
      .getOptions()
      .storage?.setItem("mxr.uiPrefs", { state: { composeEditor: "tiptap" }, version: 2 });
    await useUiPrefs.persist.rehydrate();
    expect(useUiPrefs.getState().composeEditor).toBe("tiptap");
  });
});

describe("theme preference", () => {
  test("new installs use Auto and saved explicit canvas choices survive migration", () => {
    expect(useUiPrefs.getInitialState().emailHtmlTheme).toBe("auto");
    expect(migrateUiPrefs({ emailHtmlTheme: "dark" }, 4)).toEqual({
      emailHtmlTheme: "dark",
      contrast: "system",
    });
    expect(migrateUiPrefs({ emailHtmlTheme: "original" }, 4)).toEqual({
      emailHtmlTheme: "original",
      contrast: "system",
    });
    expect(migrateUiPrefs({ emailHtmlTheme: "unexpected" }, 4)).toEqual({
      contrast: "system",
    });
  });
});

describe("reading display preference", () => {
  test("repairs stored values field by field", () => {
    expect(readReadingDisplay(null)).toEqual(DEFAULT_READING_DISPLAY);
    expect(readReadingDisplay([])).toEqual(DEFAULT_READING_DISPLAY);
    expect(
      readReadingDisplay({
        theme: "sepia",
        typeface: "removed",
        size: 999,
        spacing: "airy",
        pen: "x",
      }),
    ).toEqual({ theme: "sepia", typeface: "serif", size: 20, spacing: "airy" });
  });
});
