/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import "@/lib/actions/catalog";
import { getRegistry } from "@/lib/actions/registry";

/** Reading's effective keys: its own scope over the global one. */
function readingKeys(): Map<string, string> {
  const keys = new Map<string, string>();
  for (const scope of ["global", "reading"]) {
    for (const binding of getRegistry().bindings()) {
      if (binding.scope === scope && !binding.retired) keys.set(binding.chord, binding.action.id);
    }
  }
  return keys;
}

describe("Reading's keys", () => {
  test("follow the blueprint's one key map in the edition", () => {
    const keys = readingKeys();
    expect(keys.get("Enter")).toBe("reading.read");
    expect(keys.get("L")).toBe("reading.article");
    expect(keys.get("b")).toBe("reading.later");
    expect(keys.get("e")).toBe("reading.let-go");
    expect(keys.get("D")).toBe("reading.unsubscribe");
    expect(keys.get("R")).toBe("reading.original");
    expect(keys.get("E")).toBe("reading.let-go-all");
    expect(keys.get("A")).toBeUndefined();
    expect(keys.get("o")).toBe("reading.open-email");
    expect(keys.get("j")).toBe("reading.down");
    // g r opens Reading from anywhere.
    expect(keys.get("g r")).toBe("nav.reading");
  });

  test('the reader highlights with " and fetches the article with L', () => {
    const reader = getRegistry()
      .bindings()
      // A moved key stays bound only to announce its new key.
      .filter((binding) => binding.scope === "reading-reader" && !binding.moved)
      .map((binding) => [binding.chord, binding.action.id]);
    expect(reader).toEqual(
      expect.arrayContaining([
        ['"', "reading.reader-highlight"],
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
