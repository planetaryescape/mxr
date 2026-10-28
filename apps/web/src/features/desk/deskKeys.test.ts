/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import { definedKeys, movedOverlayKey } from "@/lib/actions/definition";

describe("the desk's keys", () => {
  // `w` ran Done on the desk until it became Wake (blueprint 35). The desk
  // says where Done went while the definition announces the move.
  test("w names Done's key, e, and no longer runs anything there", () => {
    const moved = movedOverlayKey("desk", "w");
    expect(moved?.id).toBe("mail.archive");
    expect(moved?.keys?.[0]).toBe("e");
    expect(definedKeys("list.row-action")).toEqual(["w"]);
  });
});
