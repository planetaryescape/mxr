/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import "@/lib/actions/catalog";
import { webKeymap } from "@/lib/actions/keymapParity";
import { getRegistry } from "@/lib/actions/registry";

describe("Reading's keys", () => {
  test("follow the blueprint's one key map in the edition", () => {
    const keys = webKeymap("reading");
    expect(keys.get("Enter")).toBe("reading.read");
    expect(keys.get("L")).toBe("reading.article");
    expect(keys.get("b")).toBe("reading.later");
    expect(keys.get("e")).toBe("reading.let-go");
    expect(keys.get("D")).toBe("reading.unsubscribe");
    expect(keys.get("R")).toBe("reading.original");
    expect(keys.get("A")).toBe("reading.let-go-all");
    expect(keys.get("o")).toBe("reading.open-email");
    expect(keys.get("j")).toBe("reading.down");
    // g r opens Reading from anywhere.
    expect(keys.get("g r")).toBe("nav.reading");
  });

  test("the reader highlights with h and fetches the article with L", () => {
    const reader = getRegistry()
      .bindings()
      .filter((binding) => binding.scope === "reading-reader")
      .map((binding) => [binding.chord, binding.action.id]);
    expect(reader).toEqual(
      expect.arrayContaining([
        ["h", "reading.reader-highlight"],
        ["L", "reading.reader-article"],
        ["b", "reading.reader-later"],
        ["e", "reading.reader-let-go"],
        ["R", "reading.reader-original"],
        ["n", "reading.reader-next"],
        ["Escape", "reading.reader-back"],
      ]),
    );
  });
});
