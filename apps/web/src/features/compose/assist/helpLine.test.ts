import { describe, expect, test } from "vitest";

import { helpAction, helpPlaceholder } from "./helpLine";

describe("helpPlaceholder", () => {
  test("names the action from the draft's state", () => {
    expect(helpPlaceholder({ hasText: false, hasSelection: false, answersMessage: true })).toBe(
      "Draft a reply, or say what it should say",
    );
    expect(helpPlaceholder({ hasText: true, hasSelection: false, answersMessage: true })).toBe(
      "Finish my draft, or say what to change",
    );
    expect(helpPlaceholder({ hasText: true, hasSelection: true, answersMessage: false })).toBe(
      "Edit the selection: say how",
    );
  });

  test("a new email is not offered a reply", () => {
    expect(helpPlaceholder({ hasText: false, hasSelection: false, answersMessage: false })).toBe(
      "Draft this email, or say what it should say",
    );
  });
});

describe("helpAction", () => {
  test("a selection is edited, text is finished, and an empty draft is written", () => {
    expect(helpAction({ hasText: true, hasSelection: true, answersMessage: true })).toBe(
      "edit_selection",
    );
    expect(helpAction({ hasText: true, hasSelection: false, answersMessage: true })).toBe("finish");
    expect(helpAction({ hasText: false, hasSelection: false, answersMessage: true })).toBe("write");
  });
});
