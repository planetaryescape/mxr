/* @vitest-environment jsdom */

import { describe, expect, test } from "vitest";

import { migrateUiPrefs, useUiPrefs } from "./uiPrefsStore";

describe("compose editor preference", () => {
  test("someone with no saved prefs gets the CodeMirror + vim editor", () => {
    expect(useUiPrefs.getInitialState().composeEditor).toBe("codemirror-vim");
  });

  test("a saved Rich text choice is kept, not migrated to the new default", async () => {
    // Written through the store's own storage so the test holds whether the
    // runtime exposes a working localStorage or the in-memory fallback.
    await useUiPrefs.persist
      .getOptions()
      .storage?.setItem("mxr.uiPrefs", { state: { composeEditor: "tiptap" }, version: 2 });
    await useUiPrefs.persist.rehydrate();
    expect(useUiPrefs.getState().composeEditor).toBe("tiptap");
  });
});

describe("theme preference", () => {
  test("someone with no saved prefs follows the OS appearance", () => {
    expect(useUiPrefs.getInitialState().theme).toBe("system");
  });
});

describe("home and sidebar prefs", () => {
  test("new installs land on Now with only places unfolded", () => {
    const initial = useUiPrefs.getInitialState();
    expect(initial.home).toBe("now");
    expect(initial.collapsedSections).toEqual(["more", "labels", "tools"]);
  });

  test("saved v2 prefs keep their choices, fold More and Labels and land on Now", () => {
    expect(migrateUiPrefs({ theme: "paper", collapsedSections: ["tools", "saved"] }, 2)).toEqual({
      theme: "paper",
      collapsedSections: ["tools", "saved", "more", "labels"],
      home: "now",
    });
    expect(migrateUiPrefs(undefined, 1)).toEqual({
      collapsedSections: ["tools", "more", "labels"],
      home: "now",
    });
  });

  test("a saved desk home becomes Now and an inbox home stays", () => {
    expect(migrateUiPrefs({ home: "desk", collapsedSections: [] }, 3)).toEqual({
      home: "now",
      collapsedSections: [],
    });
    const inbox = { home: "inbox", collapsedSections: [] };
    expect(migrateUiPrefs(inbox, 3)).toBe(inbox);
  });

  test("v4 prefs pass through untouched", () => {
    const saved = { home: "now", collapsedSections: [] };
    expect(migrateUiPrefs(saved, 4)).toBe(saved);
  });
});
