/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import { definedKeys, movedOverlayKey } from "@/lib/actions/definition";
import { announced } from "@/test/movedKeys";

describe("the desk's keys", () => {
  // `w` ran Done on the desk until it became Wake (blueprint 35). The desk
  // says where Done went while the definition announces the move.
  test("w names Done's key, e, while announced, and no longer runs anything there", () => {
    const moved = movedOverlayKey("desk", "w");
    expect(moved && { id: moved.id, key: moved.keys?.[0] }).toEqual(
      announced("mail.archive", "w", "desk") ? { id: "mail.archive", key: "e" } : undefined,
    );
    expect(definedKeys("list.row-action")).toEqual(["w"]);
  });
});
