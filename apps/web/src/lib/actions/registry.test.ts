import { afterEach, describe, expect, test, vi } from "vitest";

import { setController } from "@/lib/keys/controllers";

import { ActionRegistry, invokeAction } from "./registry";
import type { Action, ActionContext, ActionScope } from "./types";

function ctx(scopes: ActionScope[], overrides: Partial<ActionContext> = {}): ActionContext {
  return {
    path: "/m/inbox",
    activePane: "mailbox",
    scopes,
    selectionCount: 0,
    accountCount: 1,
    hasFocusedThread: false,
    isFirstAccountOnly: true,
    ...overrides,
  };
}

function run(overrides: Partial<Action> & { id: string }): Action {
  return { label: overrides.id, group: "Mail", run: () => {}, ...overrides } as Action;
}

function command(
  id: string,
  commandName: string,
  shortcut: string,
  scopes: ActionScope[],
  extra: Partial<Action> = {},
): Action {
  return {
    id,
    label: id,
    group: "Mail",
    command: commandName,
    shortcut,
    scopes,
    ...extra,
  } as Action;
}

afterEach(() => {
  for (const scope of ["global", "sidebar", "list", "reader", "screener"] as const) {
    setController(scope, null);
  }
});

describe("ActionRegistry.define", () => {
  test("rejects a duplicate id", () => {
    const reg = new ActionRegistry();
    reg.define(run({ id: "mail.archive" }));
    expect(() => reg.define(run({ id: "mail.archive" }))).toThrow(/duplicate id "mail\.archive"/);
  });

  test("rejects a chord bound twice in one scope, aliases included", () => {
    const reg = new ActionRegistry();
    reg.define(run({ id: "search", shortcut: "/" }));
    expect(() => reg.define(run({ id: "other", shortcut: "/" }))).toThrow(
      /duplicate shortcut "\/" in scope "global" \(already bound to "search"\)/,
    );
    expect(() => reg.define(run({ id: "aliased", shortcut: "g i", aliases: ["/"] }))).toThrow(
      /duplicate shortcut "\/"/,
    );
  });
});

describe("ActionRegistry.resolve", () => {
  test("checks the innermost scope first", () => {
    const reg = new ActionRegistry();
    reg.define(run({ id: "global-e", shortcut: "e" }));
    reg.define(command("list-e", "archive", "e", ["list"]));
    setController("list", { archive: () => {} });

    expect(reg.resolve("e", ["list", "global"])?.action.id).toBe("list-e");
    expect(reg.resolve("e", ["global"])?.action.id).toBe("global-e");
  });
});

describe("invokeAction", () => {
  test("runs a command on the innermost scope that implements it", () => {
    const listArchive = vi.fn<() => void>();
    const readerArchive = vi.fn<() => void>();
    setController("list", { archive: listArchive });
    setController("reader", { archive: readerArchive });
    const archive = command("mail.archive", "archive", "e", ["list", "reader"]);

    invokeAction(archive, ctx(["reader", "list", "global"]), "key");
    expect(readerArchive).toHaveBeenCalledTimes(1);

    setController("reader", null);
    invokeAction(archive, ctx(["reader", "list", "global"]), "key");
    expect(listArchive).toHaveBeenCalledTimes(1);
  });
});
