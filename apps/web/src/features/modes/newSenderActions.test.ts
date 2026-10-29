import { afterEach, describe, expect, test } from "vitest";

import "@/lib/actions/catalog";
import { getRegistry } from "@/lib/actions/registry";
import { setController } from "@/lib/keys/controllers";
import { announced } from "@/test/movedKeys";

afterEach(() => {
  setController("now", null);
  setController("messages", null);
});

describe("keys 1 to 4 on Now and Messages", () => {
  test("say a new sender's answers moved to the X picker, on both pages, while announced", () => {
    for (const scope of ["now", "messages"] as const) {
      setController(scope, { down: () => {}, up: () => {} });
      for (const digit of ["1", "2", "3", "4"]) {
        const id = `${scope}.sender-${digit}`;
        const binding = getRegistry().resolve(digit, [scope, "global"]);
        const answers =
          binding?.action.id === id ? { moved: binding.moved, movedTo: binding.movedTo } : null;
        expect(answers).toEqual(announced(id, digit, scope) ? { moved: true, movedTo: "X" } : null);
      }
    }
  });
});
