import { fireEvent } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { installKeyDispatcher } from "@/lib/keys/dispatcher";
import { useKeymapStore } from "@/state/keymapStore";

import { reregisterCatalog } from "./catalog";
import { loadDefinition, setServedKeys } from "./definition";
import { getRegistry, onActionInvoked, resetRegistry } from "./registry";
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
    resetRegistry();
    expect(getRegistry().all()).toHaveLength(0);
    const app = install();
    try {
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
});
