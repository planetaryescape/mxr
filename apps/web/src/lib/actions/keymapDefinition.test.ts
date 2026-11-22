/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import "./catalog";
import { isOverlayKey, MXR_VERSION, withoutExpiredMoves, type DefinedAction } from "./definition";
import keymap from "./keymap.generated.json";
import { getRegistry } from "./registry";

const defined = withoutExpiredMoves(keymap.action as DefinedAction[], MXR_VERSION).filter(
  (action) => (!action.clients || action.clients.includes("web")) && !isOverlayKey(action),
);

describe("the web registry and the key definition", () => {
  test("define the same actions", () => {
    const registered = getRegistry()
      .all()
      .map((action) => action.id)
      .toSorted();
    expect(registered).toEqual(defined.map((action) => action.id).toSorted());
  });
});
