import { fireEvent } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { todoActions } from "@/features/todo/actions";
import { fromDefinition } from "@/lib/actions/definition";
import { ActionRegistry } from "@/lib/actions/registry";
import type { Action, ActionContext, ActionScope } from "@/lib/actions/types";

import { setController } from "./controllers";
import { installKeyDispatcher } from "./dispatcher";

const ALL_SCOPES: ActionScope[] = ["global", "sidebar", "list", "reader", "screener", "todo"];

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
let deadEnds: string[];
let movedNotices: string[];
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
  deadEnds = [];
  movedNotices = [];
  uninstall = installKeyDispatcher(window, {
    registry,
    context: () => ctxWith(scopes),
    onPendingChange: (prefix) => pending.push(prefix),
    onDeadEnd: (sequence) => deadEnds.push(sequence),
    onMoved: (action, key) => movedNotices.push(`${action.label} is now ${key}`),
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

  test("a key that is part of an IME composition is not a shortcut", () => {
    const compose = runAction("compose", "c");
    registry.define(compose.action);

    // WebKit reports the composition's edge keystrokes with isComposing
    // false and only keyCode 229 to tell them apart.
    expect(press({ key: "c", isComposing: false, keyCode: 229 })).toBe(true);
    expect(press({ key: "c", isComposing: true })).toBe(true);

    expect(compose.run).not.toHaveBeenCalled();
  });

  describe("text fields and overlays", () => {
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
  });

  test("To do shortcuts do not consume ordinary text typed into fields", () => {
    registry.defineMany(todoActions.map(fromDefinition));
    const done = vi.fn<() => void>();
    setController("todo", { done });
    scopes = ["todo", "global"];
    const input = document.createElement("input");
    const editor = document.createElement("div");
    editor.setAttribute("contenteditable", "true");
    document.body.append(input, editor);

    expect(press({ key: " " }, input)).toBe(true);
    expect(press({ key: "e" }, input)).toBe(true);
    expect(press({ key: "g" }, editor)).toBe(true);
    expect(press({ key: "x" }, editor)).toBe(true);

    expect(done).not.toHaveBeenCalled();
    expect(pending.at(-1)).not.toBe("g");
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

  test("a held key runs a state-changing action once, not once per repeat", () => {
    const archive = runAction("mail.archive", "e");
    const star = runAction("mail.star", "s");
    registry.defineMany([archive.action, star.action]);

    press({ key: "e" });
    for (let repeat = 0; repeat < 10; repeat += 1) {
      expect(press({ key: "e", repeat: true })).toBe(false);
    }
    press({ key: "s" });
    press({ key: "s", repeat: true });

    expect(archive.run).toHaveBeenCalledTimes(1);
    expect(star.run).toHaveBeenCalledTimes(1);
  });

  test("q closes a dialog as Esc does, but types in a field", () => {
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    dialog.innerHTML = "<button>Keep</button><input />";
    document.body.append(dialog);
    const escapes: EventTarget[] = [];
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") escapes.push(event.target!);
    };
    document.addEventListener("keydown", onKey);
    try {
      const button = dialog.querySelector("button")!;
      expect(press({ key: "q" }, button)).toBe(false);
      expect(escapes).toEqual([button]);
      press({ key: "q" }, dialog.querySelector("input")!);
      expect(escapes).toEqual([button]);
    } finally {
      document.removeEventListener("keydown", onKey);
    }
  });
});

describe("single-key shortcuts off", () => {
  let off: boolean;

  beforeEach(() => {
    uninstall();
    off = true;
    uninstall = installKeyDispatcher(window, {
      registry,
      context: () => ctxWith(scopes),
      singleKeysOff: () => off,
      mac: false,
    });
  });

  test("a character does nothing and is left to the browser; chords and named keys still run", () => {
    const archive = runAction("archive", "e");
    const palette = runAction("palette", "Mod+k");
    const down = runAction("down", "ArrowDown");
    const goInbox = runAction("go-inbox", "g i");
    registry.defineMany([archive.action, palette.action, down.action, goInbox.action]);

    expect(press({ key: "e" })).toBe(true);
    expect(press({ key: "g" })).toBe(true);
    press({ key: "i" });
    expect(archive.run).not.toHaveBeenCalled();
    expect(goInbox.run).not.toHaveBeenCalled();

    press({ key: "k", ctrlKey: true });
    press({ key: "ArrowDown" });
    expect(palette.run).toHaveBeenCalledTimes(1);
    expect(down.run).toHaveBeenCalledTimes(1);

    off = false;
    press({ key: "e" });
    expect(archive.run).toHaveBeenCalledTimes(1);
  });
});
