/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import { definedKeys, movedOverlayKey } from "@/lib/actions/definition";
import { announced } from "@/test/movedKeys";

describe("the desk's keys", () => {
  test("w names Done's key, e, while announced, and no longer runs anything there", () => {
    const moved = movedOverlayKey("desk", "w");
    expect(moved && { id: moved.id, key: moved.keys?.[0] }).toEqual(
      announced("mail.archive", "w", "desk") ? { id: "mail.archive", key: "e" } : undefined,
    );
    expect(definedKeys("list.row-action")).toEqual(["w"]);
  });
});
