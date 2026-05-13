/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import { ActionRegistry } from "./registry";
import type { Action, ActionContext } from "./types";

const baseCtx: ActionContext = {
  path: "/m/inbox",
  activePane: "mailbox",
  selectionCount: 0,
  accountCount: 1,
  hasFocusedThread: false,
  hasFocusedMessage: false,
  isFirstAccountOnly: true,
};

function action(overrides: Partial<Action>): Action {
  return {
    id: "test.action",
    label: "Test action",
    group: "Mail",
    run: () => {},
    ...overrides,
  };
}

describe("ActionRegistry", () => {
  test("defineAction rejects duplicate ids", () => {
    const reg = new ActionRegistry();
    reg.define(action({ id: "mail.archive", label: "Archive" }));
    expect(() =>
      reg.define(action({ id: "mail.archive", label: "Archive again" })),
    ).toThrow(/duplicate.*id.*mail\.archive/i);
  });

  test("defineAction rejects duplicate shortcuts across actions", () => {
    const reg = new ActionRegistry();
    reg.define(action({ id: "a", shortcut: "g i" }));
    expect(() => reg.define(action({ id: "b", shortcut: "g i" }))).toThrow(
      /duplicate.*shortcut.*g i/i,
    );
  });

  test("getVisibleActions filters by when predicate against context", () => {
    const reg = new ActionRegistry();
    reg.define(action({ id: "always" }));
    reg.define(
      action({
        id: "in-archive",
        when: (c) => c.path.startsWith("/m/archive"),
      }),
    );
    reg.define(
      action({
        id: "with-selection",
        when: (c) => c.selectionCount > 0,
      }),
    );

    const inboxIds = reg.getVisibleActions({ ...baseCtx, path: "/m/inbox" }).map((a) => a.id);
    expect(inboxIds).toEqual(["always"]);

    const archiveIds = reg.getVisibleActions({ ...baseCtx, path: "/m/archive" }).map((a) => a.id);
    expect(archiveIds).toEqual(["always", "in-archive"]);

    const selectedIds = reg
      .getVisibleActions({ ...baseCtx, selectionCount: 3 })
      .map((a) => a.id);
    expect(selectedIds).toEqual(["always", "with-selection"]);
  });

  test("getShortcutMap omits paletteOnly actions", () => {
    const reg = new ActionRegistry();
    reg.define(action({ id: "go-inbox", shortcut: "g i" }));
    reg.define(
      action({ id: "show-help", shortcut: "Shift+Slash", paletteOnly: true }),
    );
    reg.define(action({ id: "no-shortcut" }));

    const map = reg.getShortcutMap();
    expect(map).toEqual({ "g i": "go-inbox" });
  });

  test("getShortcutMap includes aliases pointing to the same action id", () => {
    const reg = new ActionRegistry();
    reg.define(action({ id: "go-inbox", shortcut: "g i", aliases: ["1", "Digit1"] }));

    expect(reg.getShortcutMap()).toEqual({
      "g i": "go-inbox",
      "1": "go-inbox",
      Digit1: "go-inbox",
    });
  });

  test("defineAction rejects when an alias collides with another action's shortcut", () => {
    const reg = new ActionRegistry();
    reg.define(action({ id: "search", shortcut: "/" }));
    expect(() =>
      reg.define(action({ id: "go-inbox", shortcut: "g i", aliases: ["/"] })),
    ).toThrow(/duplicate.*shortcut.*\//i);
  });

  test("getActionForShortcut resolves both primary chord and aliases", () => {
    const reg = new ActionRegistry();
    reg.define(action({ id: "go-inbox", shortcut: "g i", aliases: ["1"] }));
    expect(reg.getActionForShortcut("g i")?.id).toBe("go-inbox");
    expect(reg.getActionForShortcut("1")?.id).toBe("go-inbox");
    expect(reg.getActionForShortcut("nope")).toBeUndefined();
  });
});
