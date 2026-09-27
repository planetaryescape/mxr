import { fireEvent } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { listActions } from "@/lib/actions/paneActions";
import { ActionRegistry } from "@/lib/actions/registry";
import type { Action, ActionContext, ActionScope } from "@/lib/actions/types";

import { setController } from "./controllers";
import { installKeyDispatcher, SEQUENCE_TIMEOUT_MS } from "./dispatcher";

const ALL_SCOPES: ActionScope[] = ["global", "sidebar", "list", "reader", "screener"];

function ctxWith(scopes: ActionScope[]): ActionContext {
  return {
    path: "/m/inbox",
    activePane: "mailbox",
    scopes,
    selectionCount: 0,
    accountCount: 1,
    hasFocusedThread: false,
    isFirstAccountOnly: true,
  };
}

function runAction(id: string, shortcut: string, extra: Partial<Action> = {}) {
  const run = vi.fn<() => void>();
  const action = { id, label: id, group: "Navigate", shortcut, run, ...extra } as Action;
  return { action, run };
}

let registry: ActionRegistry;
let scopes: ActionScope[];
let suspended: boolean;
let pending: (string | null)[];
let uninstall: () => void;

function press(keyInit: KeyboardEventInit, target: Element = document.body): boolean {
  return fireEvent.keyDown(target, keyInit);
}

beforeEach(() => {
  vi.useFakeTimers();
  registry = new ActionRegistry();
  scopes = ["global"];
  suspended = false;
  pending = [];
  uninstall = installKeyDispatcher(window, {
    registry,
    context: () => ctxWith(scopes),
    onPendingChange: (prefix) => pending.push(prefix),
    isSuspended: () => suspended,
    mac: false,
  });
});

afterEach(() => {
  uninstall();
  for (const scope of ALL_SCOPES) setController(scope, null);
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("installKeyDispatcher", () => {
  test("a single key runs its action and claims the event", () => {
    const compose = runAction("compose", "c");
    registry.define(compose.action);

    const notCancelled = press({ key: "c" });

    expect(compose.run).toHaveBeenCalledTimes(1);
    expect(notCancelled).toBe(false);
  });

  test("an unbound key is left alone", () => {
    expect(press({ key: "q" })).toBe(true);
  });

  test("g i waits for the second key, reports the prefix, then runs", () => {
    const inbox = runAction("inbox", "g i");
    registry.define(inbox.action);

    press({ key: "g" });
    expect(inbox.run).not.toHaveBeenCalled();
    expect(pending.at(-1)).toBe("g");

    press({ key: "i" });
    expect(inbox.run).toHaveBeenCalledTimes(1);
    expect(pending.at(-1)).toBeNull();
  });

  test("the prefix expires after the sequence timeout", () => {
    const inbox = runAction("inbox", "g i");
    registry.define(inbox.action);

    press({ key: "g" });
    vi.advanceTimersByTime(SEQUENCE_TIMEOUT_MS);
    expect(pending.at(-1)).toBeNull();

    press({ key: "i" });
    expect(inbox.run).not.toHaveBeenCalled();
  });

  test("a dead-end sequence drops the prefix and runs the key on its own", () => {
    const inbox = runAction("inbox", "g i");
    const toggle = runAction("toggle", "x");
    registry.defineMany([inbox.action, toggle.action]);

    press({ key: "g" });
    press({ key: "x" });

    expect(toggle.run).toHaveBeenCalledTimes(1);
    expect(inbox.run).not.toHaveBeenCalled();
  });

  test("a dead end that is itself a prefix starts a new sequence", () => {
    const inbox = runAction("inbox", "g i");
    const selectAll = runAction("select-all", "* a");
    registry.defineMany([inbox.action, selectAll.action]);

    press({ key: "g" });
    press({ key: "*" });
    press({ key: "a" });

    expect(selectAll.run).toHaveBeenCalledTimes(1);
    expect(inbox.run).not.toHaveBeenCalled();
  });

  test("a chord that is complete in one scope but a prefix in another runs after the timeout", () => {
    const star = runAction("global-star", "*");
    const selectAll = runAction("list-select-all", "* a", { scopes: ["list"] });
    registry.defineMany([star.action, selectAll.action]);
    scopes = ["list", "global"];

    press({ key: "*" });
    expect(star.run).not.toHaveBeenCalled();

    vi.advanceTimersByTime(SEQUENCE_TIMEOUT_MS);
    expect(star.run).toHaveBeenCalledTimes(1);
    expect(selectAll.run).not.toHaveBeenCalled();
  });

  describe("scoped commands", () => {
    let listArchive: ReturnType<typeof vi.fn<() => void>>;
    let readerArchive: ReturnType<typeof vi.fn<() => void>>;

    beforeEach(() => {
      listArchive = vi.fn<() => void>();
      readerArchive = vi.fn<() => void>();
      registry.define({
        id: "mail.archive",
        label: "Archive",
        group: "Mail",
        command: "archive",
        shortcut: "e",
        scopes: ["list", "reader"],
      });
    });

    test("fire on the innermost active scope's controller", () => {
      setController("list", { archive: listArchive });
      setController("reader", { archive: readerArchive });

      scopes = ["reader", "list", "global"];
      press({ key: "e" });
      expect(readerArchive).toHaveBeenCalledTimes(1);
      expect(listArchive).not.toHaveBeenCalled();

      scopes = ["list", "global"];
      press({ key: "e" });
      expect(listArchive).toHaveBeenCalledTimes(1);
      expect(readerArchive).toHaveBeenCalledTimes(1);
    });

    test("do nothing while the scope is inactive, even with a controller mounted", () => {
      setController("reader", { archive: readerArchive });
      scopes = ["list", "global"];

      expect(press({ key: "e" })).toBe(true);
      expect(readerArchive).not.toHaveBeenCalled();
    });

    test("do nothing when the active scope has no controller for the command", () => {
      setController("list", { down: () => undefined });
      scopes = ["list", "global"];

      expect(press({ key: "e" })).toBe(true);
    });
  });

  describe("text fields and overlays", () => {
    test("plain keys typed into an input are not actions", () => {
      const compose = runAction("compose", "c");
      registry.define(compose.action);
      const input = document.createElement("input");
      document.body.append(input);

      expect(press({ key: "c" }, input)).toBe(true);
      expect(compose.run).not.toHaveBeenCalled();
    });

    test("a global modifier chord still works from an input", () => {
      const palette = runAction("palette", "Mod+k");
      registry.define(palette.action);
      const input = document.createElement("input");
      document.body.append(input);

      expect(press({ key: "k", ctrlKey: true }, input)).toBe(false);
      expect(palette.run).toHaveBeenCalledTimes(1);
    });

    test("editing chords stay with the field even when bound", () => {
      const copy = runAction("copy-link", "Mod+c");
      registry.define(copy.action);
      const input = document.createElement("textarea");
      document.body.append(input);

      expect(press({ key: "c", ctrlKey: true }, input)).toBe(true);
      expect(copy.run).not.toHaveBeenCalled();
    });

    test("an open dialog owns its keys", () => {
      const compose = runAction("compose", "c");
      registry.define(compose.action);
      document.body.innerHTML = `<div role="dialog" data-state="open"><button>Ok</button></div>`;

      press({ key: "c" }, document.querySelector("button")!);
      expect(compose.run).not.toHaveBeenCalled();
    });

    test("a dialog animating closed does not swallow keys", () => {
      const compose = runAction("compose", "c");
      registry.define(compose.action);
      document.body.innerHTML = `<div role="dialog" data-state="closed"><span tabindex="0">x</span></div>`;

      press({ key: "c" }, document.querySelector("span")!);
      expect(compose.run).toHaveBeenCalledTimes(1);
    });

    test("Enter on a focused button keeps its native meaning", () => {
      const open = vi.fn<() => void>();
      registry.define({
        id: "list.open",
        label: "Open",
        group: "Move",
        command: "open",
        shortcut: "Enter",
        scopes: ["list"],
      });
      setController("list", { open });
      scopes = ["list", "global"];
      const button = document.createElement("button");
      document.body.append(button);

      expect(press({ key: "Enter" }, button)).toBe(true);
      expect(open).not.toHaveBeenCalled();

      press({ key: "Enter" });
      expect(open).toHaveBeenCalledTimes(1);
    });
  });

  // The list binds "Ctrl+d"/"Ctrl+u" (TUI half-page); off macOS Ctrl parses
  // as "Mod", and the dispatcher must still match the Ctrl spelling.
  test("Ctrl+d pages the list down on Windows and Linux", () => {
    registry.defineMany(listActions);
    const pageDown = vi.fn<() => void>();
    setController("list", { pageDown });
    scopes = ["list", "global"];

    press({ key: "d", ctrlKey: true });

    expect(pageDown).toHaveBeenCalledTimes(1);
  });

  test("isSuspended vetoes every key, modifier chords included", () => {
    const compose = runAction("compose", "c");
    const palette = runAction("palette", "Mod+k");
    registry.defineMany([compose.action, palette.action]);
    suspended = true;

    press({ key: "c" });
    press({ key: "k", ctrlKey: true });

    expect(compose.run).not.toHaveBeenCalled();
    expect(palette.run).not.toHaveBeenCalled();
  });

  test("uninstalling stops dispatch", () => {
    const compose = runAction("compose", "c");
    registry.define(compose.action);
    uninstall();

    press({ key: "c" });
    expect(compose.run).not.toHaveBeenCalled();
  });

  test("Escape after a pending prefix only cancels the prefix", () => {
    const inbox = runAction("inbox", "g i");
    const close = runAction("close", "Escape");
    registry.defineMany([inbox.action, close.action]);

    press({ key: "g" });
    press({ key: "Escape" });
    press({ key: "i" });

    expect(close.run).not.toHaveBeenCalled();
    expect(inbox.run).not.toHaveBeenCalled();
    expect(pending.at(-1)).toBeNull();
  });
});
