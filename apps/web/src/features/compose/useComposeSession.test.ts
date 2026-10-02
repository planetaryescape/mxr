import { describe, expect, test } from "vitest";

import { saveStatusLabel } from "./useComposeSession";

describe("saveStatusLabel", () => {
  test("an untouched draft claims no save", () => {
    expect(saveStatusLabel({ saving: false, dirty: false, lastSavedAt: null })).toBe("");
  });

  test("typing shows unsaved changes, then a save says when it landed", () => {
    expect(saveStatusLabel({ saving: false, dirty: true, lastSavedAt: null })).toBe(
      "Unsaved changes",
    );
    expect(saveStatusLabel({ saving: true, dirty: true, lastSavedAt: null })).toBe("Saving…");
    expect(saveStatusLabel({ saving: false, dirty: false, lastSavedAt: new Date() })).toBe(
      "Saved just now",
    );
  });
});
