/* @vitest-environment jsdom */

import { describe, expect, test } from "vitest";

import { useUiPrefs } from "./uiPrefsStore";

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
