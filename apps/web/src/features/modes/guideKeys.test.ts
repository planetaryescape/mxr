/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import "@/lib/actions/catalog";

import { keysForModeGuide } from "./guideKeys";

describe("mode guide keys", () => {

  test("leaves other mode guides unchanged", () => {
    const keys = [{ key: "e", verb: "Done" }];

    expect(keysForModeGuide({ mode: "now", keys })).toBe(keys);
  });
});
