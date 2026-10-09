/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import "@/lib/actions/catalog";

import { keysForModeGuide } from "./guideKeys";

describe("mode guide keys", () => {
  test("shows every web To do completion shortcut while preserving the guide", () => {
    const keys = keysForModeGuide({
      mode: "todo",
      keys: [
        { key: "j", verb: "next" },
        { key: "e", verb: "tick off" },
      ],
    });

    expect(keys).toEqual([
      { key: "j", verb: "next" },
      { key: "Space", verb: "tick off" },
      { key: "e", verb: "tick off" },
    ]);
  });

  test("leaves other mode guides unchanged", () => {
    const keys = [{ key: "e", verb: "Done" }];

    expect(keysForModeGuide({ mode: "now", keys })).toBe(keys);
  });
});
