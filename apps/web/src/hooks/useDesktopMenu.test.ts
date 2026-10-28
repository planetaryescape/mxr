import { renderHook } from "@testing-library/react";
import { toast } from "sonner";
import { afterEach, describe, expect, test, vi } from "vitest";

import {
  ActionRegistry,
  getRegistry,
  onActionInvoked,
  resetRegistry,
} from "@/lib/actions/registry";
import type { Action, ActionContext, ActionScope } from "@/lib/actions/types";

import { MENU_ACTION_EVENT, runMenuAction, useDesktopMenu } from "./useDesktopMenu";

vi.mock("sonner", () => ({ toast: vi.fn<(message: string) => void>() }));

function ctx(scopes: ActionScope[]): () => ActionContext {
  return () => ({
    path: "/m/inbox",
    activePane: "mailbox",
    scopes,
    selectionCount: 0,
    accountCount: 1,
    hasFocusedThread: false,
    isFirstAccountOnly: true,
  });
}

function registryWith(run: () => void): ActionRegistry {
  const registry = new ActionRegistry();
  registry.define({
    id: "mail.reply",
    label: "Reply",
    group: "Mail",
    shortcut: "r",
    scopes: ["list"],
    run,
  } as Action);
  return registry;
}

afterEach(() => vi.clearAllMocks());

describe("desktop menu commands", () => {
  test("run through the registry where their key would", () => {
    const run = vi.fn<() => void>();
    const sources: string[] = [];
    const stop = onActionInvoked((_action, source) => sources.push(source));
    runMenuAction("mail.reply", registryWith(run), ctx(["list", "global"]), () => false);
    stop();
    expect(run).toHaveBeenCalledOnce();
    // A menu run teaches the key, as a click does.
    expect(sources).toEqual(["pointer"]);
  });

  test("say so where the action has nothing to act on", () => {
    const run = vi.fn<() => void>();
    runMenuAction("mail.reply", registryWith(run), ctx(["now", "global"]), () => false);
    expect(run).not.toHaveBeenCalled();
    expect(toast).toHaveBeenCalledWith("Reply isn't available here");
  });

  test("wait while a dialog or compose owns the keyboard, and ignore unknown ids", () => {
    const run = vi.fn<() => void>();
    runMenuAction("mail.reply", registryWith(run), ctx(["list", "global"]), () => true);
    runMenuAction("mail.nope", registryWith(run), ctx(["list", "global"]), () => false);
    expect(run).not.toHaveBeenCalled();
    expect(toast).not.toHaveBeenCalled();
  });

  test("listen only while the app is mounted, and say when they can be sent", () => {
    const run = vi.fn<() => void>();
    getRegistry().define({ id: "test.menu", label: "Menu", group: "Mail", run } as Action);
    const send = () =>
      window.dispatchEvent(new CustomEvent(MENU_ACTION_EVENT, { detail: "test.menu" }));
    const { unmount } = renderHook(() => useDesktopMenu());
    expect(window.mxrMenuReady).toBe(true);
    send();
    expect(run).toHaveBeenCalledOnce();
    unmount();
    expect(window.mxrMenuReady).toBe(false);
    send();
    expect(run).toHaveBeenCalledOnce();
    resetRegistry();
  });
});
