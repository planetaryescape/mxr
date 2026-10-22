import { describe, expect, it } from "vitest";

import { getRegistry } from "@/lib/actions";

import { REPEATABLE_ACTION_IDS } from "./dispatcher";

describe("repeatable keys", () => {
  it("names only real actions, and only movement", () => {
    for (const id of REPEATABLE_ACTION_IDS) {
      const action = getRegistry().get(id);
      expect(action?.id).toBe(id);
      expect(["Move", "Read"]).toContain(action?.group);
    }
    for (const id of ["mail.archive", "mail.trash", "mail.star", "mail.snooze", "focus.send"]) {
      expect(REPEATABLE_ACTION_IDS.has(id)).toBe(false);
    }
  });

  it("holding j or k moves in every mode, not one row", () => {
    const moves = getRegistry()
      .all()
      .map((action) => action.id)
      .filter((id) => /\.(down|up)$/.test(id));
    expect(moves).toEqual(expect.arrayContaining(["now.down", "messages.up", "screener.down"]));
    expect(moves.filter((id) => !REPEATABLE_ACTION_IDS.has(id))).toEqual([]);
  });
});
