/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import { normalizeSegments, splitMessageText } from "./textSegments";

describe("splitMessageText", () => {
  test("folds a top-posted reply under its Gmail attribution", () => {
    const text = [
      "Sounds good, ship it.",
      "",
      "On Mon, Sep 21, 2026 at 10:02 AM Ada Lovelace <ada@example.com> wrote:",
      "> Here is the plan.",
      "> Step one.",
    ].join("\n");
    const segments = normalizeSegments(splitMessageText(text));
    expect(segments.map((s) => s.kind)).toEqual(["text", "quote"]);
    expect(segments[0]?.text.trim()).toBe("Sounds good, ship it.");
    expect(segments[1]?.text).toMatch(/^On Mon/);
  });
});
