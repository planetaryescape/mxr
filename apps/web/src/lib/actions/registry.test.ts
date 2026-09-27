import { afterEach, describe, expect, test, vi } from "vitest";

import { setController } from "@/lib/keys/controllers";

import { ActionRegistry, invokeAction, isAvailable } from "./registry";
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

  test("allows the same chord in different scopes", () => {
    const reg = new ActionRegistry();
    reg.define(command("list.down", "down", "j", ["list"]));
    expect(() => reg.define(command("reader.down", "scrollDown", "j", ["reader"]))).not.toThrow();
  });

  test("rejects a chord that is a prefix of another in the same scope, in either order", () => {
    const reg = new ActionRegistry();
    reg.define(run({ id: "inbox", shortcut: "g i" }));
    expect(() => reg.define(run({ id: "g", shortcut: "g" }))).toThrow(/clashes with "g i"/);

    const reversed = new ActionRegistry();
    reversed.define(run({ id: "g", shortcut: "g" }));
    expect(() => reversed.define(run({ id: "inbox", shortcut: "g i" }))).toThrow(
      /chord "g i" \(inbox\) clashes with "g"/,
    );
  });

  test("a prefix in another scope is not a clash", () => {
    const reg = new ActionRegistry();
    reg.define(run({ id: "star", shortcut: "*" }));
    expect(() => reg.define(command("select-all", "selectAll", "* a", ["list"]))).not.toThrow();
  });

  test("palette-only actions don't reserve their chord", () => {
    const reg = new ActionRegistry();
    reg.define(run({ id: "palette-only", shortcut: "x", paletteOnly: true }));
    expect(() => reg.define(run({ id: "keyed", shortcut: "x" }))).not.toThrow();
    expect(reg.resolve("x", ["global"])?.action.id).toBe("keyed");
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

  test("skips a scoped command with no live controller", () => {
    const reg = new ActionRegistry();
    reg.define(run({ id: "global-e", shortcut: "e" }));
    reg.define(command("list-e", "archive", "e", ["list"]));

    expect(reg.resolve("e", ["list", "global"])?.action.id).toBe("global-e");
    setController("list", { somethingElse: () => {} });
    expect(reg.resolve("e", ["list", "global"])?.action.id).toBe("global-e");
  });

  test("normalizes extra whitespace in a typed chord", () => {
    const reg = new ActionRegistry();
    reg.define(run({ id: "inbox", shortcut: "g i" }));
    expect(reg.resolve(" g  i ", ["global"])?.action.id).toBe("inbox");
  });

  test("hasContinuation only counts live bindings in active scopes", () => {
    const reg = new ActionRegistry();
    reg.define(command("select-all", "selectAll", "* a", ["list"]));

    expect(reg.hasContinuation("*", ["list", "global"])).toBe(false);
    setController("list", { selectAll: () => {} });
    expect(reg.hasContinuation("*", ["list", "global"])).toBe(true);
    expect(reg.hasContinuation("*", ["global"])).toBe(false);
  });
});

describe("invokeAction", () => {
  test("runs a command on the innermost scope that implements it", () => {
    const listArchive = vi.fn<() => void>();
    const readerArchive = vi.fn<() => void>();
    setController("list", { archive: listArchive });
    setController("reader", { archive: readerArchive });
    const archive = command("mail.archive", "archive", "e", ["list", "reader"]);

    invokeAction(archive, ctx(["reader", "list", "global"]));
    expect(readerArchive).toHaveBeenCalledTimes(1);

    setController("reader", null);
    invokeAction(archive, ctx(["reader", "list", "global"]));
    expect(listArchive).toHaveBeenCalledTimes(1);
  });

  test("a command with no live controller is a no-op", () => {
    const archive = command("mail.archive", "archive", "e", ["list"]);
    expect(() => invokeAction(archive, ctx(["list", "global"]))).not.toThrow();
  });

  test("a run action receives the context", () => {
    const runner = vi.fn<(context: ActionContext) => void>();
    const context = ctx(["global"], { selectionCount: 3 });

    invokeAction(run({ id: "go", run: runner }), context);

    expect(runner).toHaveBeenCalledWith(context);
  });
});

describe("getVisibleActions", () => {
  test("hides command actions until their view registers the command", () => {
    const reg = new ActionRegistry();
    reg.define(run({ id: "compose" }));
    reg.define(command("mail.archive", "archive", "e", ["list", "reader"]));
    const inList = ctx(["list", "global"]);

    expect(reg.getVisibleActions(inList).map((action) => action.id)).toEqual(["compose"]);

    setController("list", { archive: () => {} });
    expect(reg.getVisibleActions(inList).map((action) => action.id)).toEqual([
      "compose",
      "mail.archive",
    ]);
  });

  test("filters by the when predicate", () => {
    const reg = new ActionRegistry();
    reg.define(run({ id: "always" }));
    reg.define(run({ id: "with-selection", when: (c) => c.selectionCount > 0 }));

    expect(reg.getVisibleActions(ctx(["global"])).map((action) => action.id)).toEqual(["always"]);
    expect(
      reg.getVisibleActions(ctx(["global"], { selectionCount: 2 })).map((action) => action.id),
    ).toEqual(["always", "with-selection"]);
  });

  test("a run action scoped to a pane is only available while that pane is active", () => {
    const screener = run({ id: "screener.approve", scopes: ["screener"] });
    expect(isAvailable(screener, ctx(["global"]))).toBe(false);
    expect(isAvailable(screener, ctx(["screener", "global"]))).toBe(true);
  });
});
