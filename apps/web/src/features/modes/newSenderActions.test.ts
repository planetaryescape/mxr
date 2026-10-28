import { afterEach, describe, expect, test } from "vitest";

import "@/lib/actions/catalog";
import { getRegistry } from "@/lib/actions/registry";
import { setController } from "@/lib/keys/controllers";

afterEach(() => {
  setController("now", null);
  setController("messages", null);
});

describe("keys 1 to 4 on Now and Messages", () => {
  test("say a new sender's answers moved to the X picker, on both pages", () => {
    for (const scope of ["now", "messages"] as const) {
      setController(scope, { down: () => {}, up: () => {} });
      for (const digit of ["1", "2", "3", "4"]) {
        const binding = getRegistry().resolve(digit, [scope, "global"]);
        expect(binding?.action.id).toBe(`${scope}.sender-${digit}`);
        expect(binding?.moved).toBe(true);
        expect(binding?.movedTo).toBe("X");
      }
    }
  });
});
