/* @vitest-environment node */

import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, test } from "vitest";

import { docChord, renderKeybindingsMarkdown } from "./keybindingsDoc";

const DOC = resolve(__dirname, "../../../../../site/src/content/docs/reference/keybindings.md");
const START =
  "<!-- web-keys:start (generated from the action registry; run UPDATE_KEY_DOCS=1 npm test) -->";
const END = "<!-- web-keys:end -->";

describe("web keybindings reference", () => {
  test("the published table matches the action registry", () => {
    const doc = readFileSync(DOC, "utf8");
    const start = doc.indexOf(START);
    const end = doc.indexOf(END);
    // A missing marker means the generated section was deleted from keybindings.md.
    expect(start).toBeGreaterThan(-1);
    const generated = renderKeybindingsMarkdown();
    const current = doc.slice(start + START.length, end).trim();
    if (process.env.UPDATE_KEY_DOCS === "1" && current !== generated) {
      writeFileSync(
        DOC,
        `${doc.slice(0, start + START.length)}\n\n${generated}\n\n${doc.slice(end)}`,
      );
      return;
    }
    expect(current).toBe(generated);
  });

  test("renders modifier chords for both platforms", () => {
    expect(docChord("Mod+k")).toBe("⌘K / Ctrl+K");
    expect(docChord("g i")).toBe("g i");
    expect(docChord("Escape")).toBe("Esc");
  });
});
