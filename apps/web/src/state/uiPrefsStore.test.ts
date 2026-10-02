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
  test("new installs land on the desk with only places unfolded", () => {
    const initial = useUiPrefs.getInitialState();
    expect(initial.home).toBe("desk");
    expect(initial.collapsedSections).toEqual(["more", "labels", "tools"]);
  });

  test("saved v2 prefs keep their choices and fold More and Labels", () => {
    expect(migrateUiPrefs({ theme: "paper", collapsedSections: ["tools", "saved"] }, 2)).toEqual({
      theme: "paper",
      collapsedSections: ["tools", "saved", "more", "labels"],
    });
    expect(migrateUiPrefs(undefined, 1)).toEqual({
      collapsedSections: ["tools", "more", "labels"],
    });
  });

  test("v3 prefs pass through untouched, including an inbox home", () => {
    const saved = { home: "inbox", collapsedSections: [] };
    expect(migrateUiPrefs(saved, 3)).toBe(saved);
  });
});
