import { fireEvent } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { installKeyDispatcher } from "@/lib/keys/dispatcher";
import { useKeymapStore } from "@/state/keymapStore";

import { reregisterCatalog } from "./catalog";
import {
  definedShortcut,
  isDefinitionLoaded,
  loadDefinition,
  resetDefinitionForTests,
  setServedKeys,
} from "./definition";
import { getRegistry, onActionInvoked, resetRegistry } from "./registry";
import { applyServedKeymap } from "./servedKeymap";
import type { ActionContext } from "./types";

function ctx(): ActionContext {
  return {
    path: "/messages",
    activePane: "mailbox",
    scopes: ["global", "list"],
    selectionCount: 0,
    accountCount: 1,
    hasFocusedThread: false,
    isFirstAccountOnly: true,
  };
}

/** Watch every action the registry runs, and the events the dispatcher claims. */
function install() {
  const ran: string[] = [];
  const stop = onActionInvoked((action) => ran.push(action.id));
  const uninstall = installKeyDispatcher(window, {
    registry: getRegistry(),
    context: ctx,
    mac: false,
  });
  return {
    ran,
    press: (key: string) => fireEvent.keyDown(document.body, { key }),
    dispose: () => {
      uninstall();
      stop();
    },
  };
}

afterEach(async () => {
  setServedKeys(null);
  await loadDefinition();
  reregisterCatalog();
  useKeymapStore.setState({
    singleKeys: true,
    remapped: [],
    problems: [],
    version: 0,
    catalogLoaded: false,
  });
  document.body.innerHTML = "";
});

describe("the catalog is not on the first-paint path", () => {
  test("a key pressed before it lands runs nothing; the same key works after", async () => {
    // The app's boot leaves the registry empty; the catalog arrives after the
    // first paint and `scheduleActionCatalog` registers the actions.
    resetRegistry();
    expect(getRegistry().all()).toHaveLength(0);
    const app = install();
    try {
      // "c" opens compose once the registry exists. Before that the press is
      // left to the browser and claims nothing.
      expect(app.press("c")).toBe(true);
      expect(app.ran).toEqual([]);

      await loadDefinition();
      reregisterCatalog();

      app.press("c");
      expect(app.ran).toEqual(["shell.compose"]);
    } finally {
      app.dispose();
    }
  });

  test("keys served before the catalog lands are the ones that run when it does", async () => {
    // The order a returning app sees: the cached keys apply while the entry
    // is evaluated, the catalog arrives after the first paint.
    resetDefinitionForTests();
    resetRegistry();
    expect(isDefinitionLoaded()).toBe(false);

    applyServedKeymap({
      actions: [{ id: "shell.compose", keys: ["%"] }],
      remapped: ["shell.compose"],
    });

    // Labels are right at once, from the small entry index.
    expect(definedShortcut("shell.compose")).toBe("%");
    // Keys run nothing until the catalog lands.
    expect(getRegistry().all()).toHaveLength(0);

    await loadDefinition();
    reregisterCatalog();

    // The person's key is the binding, and the definition's default is gone.
    expect(definedShortcut("shell.compose")).toBe("%");
    expect(
      getRegistry()
        .bindings()
        .find((b) => b.action.id === "shell.compose")?.chord,
    ).toBe("%");

    const app = install();
    try {
      app.press("c");
      expect(app.ran).toEqual([]);
      app.press("%");
      expect(app.ran).toEqual(["shell.compose"]);
    } finally {
      app.dispose();
    }
  });

  test("a served keymap replaces the defaults without a reload", async () => {
    const before = useKeymapStore.getState().version;
    applyServedKeymap({
      actions: [{ id: "mail.archive", keys: ["%"] }],
      remapped: ["mail.archive"],
    });
    expect(definedShortcut("mail.archive")).toBe("%");
    expect(useKeymapStore.getState().version).toBe(before + 1);
    expect(
      getRegistry()
        .bindings()
        .find((b) => b.action.id === "mail.archive")?.chord,
    ).toBe("%");
  });
});

describe("the index answers the labels the first screen shows", () => {
  test("the keys navMap and sidebarRail read at module evaluation are right", () => {
    expect(definedShortcut("nav.now")).toBe("g h");
    expect(definedShortcut("nav.settings")).toBe("g C");
    expect(definedShortcut("shell.help")).toBe("?");
  });

  test("the registry cannot be built from the index alone", () => {
    resetDefinitionForTests();
    expect(isDefinitionLoaded()).toBe(false);
    expect(() => reregisterCatalog()).toThrow(/has not loaded/);
  });
});
