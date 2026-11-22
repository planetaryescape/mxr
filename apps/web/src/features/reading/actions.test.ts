/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import "@/lib/actions/catalog";
import { getRegistry } from "@/lib/actions/registry";

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
    expect(keys.get("g r")).toBe("nav.reading");
  });

});
