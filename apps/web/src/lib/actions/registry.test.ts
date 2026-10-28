import { afterEach, describe, expect, test, vi } from "vitest";

import { setController } from "@/lib/keys/controllers";

import { ActionRegistry, chordsOf, invokeAction, isAvailable, onActionInvoked } from "./registry";
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

    invokeAction(archive, ctx(["reader", "list", "global"]), "key");
    expect(readerArchive).toHaveBeenCalledTimes(1);

    setController("reader", null);
    invokeAction(archive, ctx(["reader", "list", "global"]), "key");
    expect(listArchive).toHaveBeenCalledTimes(1);
  });

  test("a command with no live controller is a no-op", () => {
    const archive = command("mail.archive", "archive", "e", ["list"]);
    expect(() => invokeAction(archive, ctx(["list", "global"]), "key")).not.toThrow();
  });

  test("a run action receives the context", () => {
    const runner = vi.fn<(context: ActionContext) => void>();
    const context = ctx(["global"], { selectionCount: 3 });

    invokeAction(run({ id: "go", run: runner }), context, "palette");

    expect(runner).toHaveBeenCalledWith(context);
  });

  test("tells listeners where an action that ran came from", () => {
    const heard: [string, string][] = [];
    const stop = onActionInvoked((action, source) => heard.push([action.id, source]));
    setController("list", { archive: () => undefined });
    const archive = command("mail.archive", "archive", "e", ["list"]);
    invokeAction(archive, ctx(["list", "global"]), "palette");
    invokeAction(archive, ctx(["list", "global"]), "key");
    // Nothing ran, so nothing is heard.
    invokeAction(archive, ctx(["reader", "global"]), "pointer");
    stop();
    invokeAction(archive, ctx(["list", "global"]), "palette");
    expect(heard).toEqual([
      ["mail.archive", "palette"],
      ["mail.archive", "key"],
    ]);
  });
});

describe("a key pressed beside the pane that owns it", () => {
  test("reaches the action's own pane when that pane sits beside the focused one", () => {
    const reg = new ActionRegistry();
    const toggle = vi.fn<() => void>();
    const select = command("list.toggle-select", "toggleSelect", "x", [
      "list",
      "reader",
      "sidebar",
    ]);
    reg.define(select);
    setController("reader", { scrollDown: () => {} });
    setController("sidebar", { down: () => {} });

    // No list mounted: the key can't be swallowed by a pane that can't run it.
    expect(reg.resolve("x", ["reader", "global"])).toBeUndefined();

    setController("list", { toggleSelect: toggle });
    expect(reg.resolve("x", ["reader", "global"])?.action.id).toBe("list.toggle-select");
    invokeAction(select, ctx(["reader", "global"]), "key");
    invokeAction(select, ctx(["sidebar", "global"]), "key");
    expect(toggle).toHaveBeenCalledTimes(2);
  });

  test("the list reaches the reader beside it, and the sidebar reaches both", () => {
    const fullscreen = vi.fn<() => void>();
    setController("list", {});
    setController("reader", { fullscreen });
    const wide = command("reader.fullscreen", "fullscreen", "F", ["reader", "list", "sidebar"]);
    invokeAction(wide, ctx(["list", "global"]), "key");
    invokeAction(wide, ctx(["sidebar", "global"]), "key");
    expect(fullscreen).toHaveBeenCalledTimes(2);
  });

  test("a pane's own command wins, and only the action's own pane is reached", () => {
    const listArchive = vi.fn<() => void>();
    const readerArchive = vi.fn<() => void>();
    setController("list", { archive: listArchive });
    setController("reader", { archive: readerArchive });
    // A read-only list has no archive; the reader beside it isn't archive's own pane.
    const archive = command("mail.archive", "archive", "e", ["list", "reader"]);
    invokeAction(archive, ctx(["reader", "global"]), "key");
    expect(readerArchive).toHaveBeenCalledTimes(1);
    setController("list", {});
    invokeAction(archive, ctx(["list", "global"]), "key");
    expect(readerArchive).toHaveBeenCalledTimes(1);
    expect(listArchive).not.toHaveBeenCalled();
  });

  test("a pane that isn't beside the action's own pane falls through to the next scope", () => {
    const reg = new ActionRegistry();
    const pageDown = vi.fn<() => void>();
    const page = command("list.page-down", "pageDown", "Ctrl+d", ["list", "now", "sidebar"]);
    reg.define(page);
    setController("sidebar", { down: () => {} });
    setController("now", { pageDown });
    expect(reg.resolve("Ctrl+d", ["sidebar", "now", "global"])?.scope).toBe("now");
    invokeAction(page, ctx(["sidebar", "now", "global"]), "key");
    expect(pageDown).toHaveBeenCalledTimes(1);
    setController("now", null);
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

describe("retired aliases", () => {
  test("still dispatch, and clash like any chord, but are not advertised", () => {
    const registry = new ActionRegistry();
    const reading = run({ id: "nav.reading", shortcut: "g r", retiredAliases: ["g R"] });
    registry.define(reading);
    expect(registry.resolve("g R", ["global"])?.action.id).toBe("nav.reading");
    expect(registry.resolve("g R", ["global"])?.retired).toBe(true);
    expect(registry.resolve("g r", ["global"])?.retired).toBe(false);
    expect(chordsOf(reading)).toEqual(["g r"]);
    expect(() => registry.define(run({ id: "nav.rules", shortcut: "g R" }))).toThrow(
      /duplicate shortcut/,
    );
  });
});
