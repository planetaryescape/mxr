/* @vitest-environment jsdom */

import { fireEvent } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { fromDefinition } from "@/lib/actions/definition";
import { ActionRegistry } from "@/lib/actions/registry";
import type { ActionContext } from "@/lib/actions/types";
import { installKeyDispatcher } from "@/lib/keys/dispatcher";

import { newSenderActions } from "./newSenderActions";

const moved = vi.fn<(label: string, key: string | null) => void>();

const context = (): ActionContext => ({
  path: "/now",
  activePane: "mailbox",
  scopes: ["now", "global"],
  selectionCount: 0,
  accountCount: 1,
  hasFocusedThread: false,
  isFirstAccountOnly: true,
});

let uninstall: () => void;

beforeEach(() => {
  moved.mockReset();
  const registry = new ActionRegistry();
  registry.defineMany(newSenderActions("now").map(fromDefinition));
  uninstall = installKeyDispatcher(window, {
    registry,
    context,
    mac: false,
    onMoved: (action, key) => moved(action.shortLabel ?? action.label, key),
  });
});

afterEach(() => {
  uninstall();
  document.body.innerHTML = "";
});

describe("1 to 4 on Now", () => {
  test("say where a new sender's answers went, and answer nothing", () => {
    fireEvent.keyDown(document.body, { key: "1" });
    expect(moved).toHaveBeenCalledExactlyOnceWith("New sender", "X");
  });

  test("never fire from a text field, which owns its keys", () => {
    const input = document.createElement("input");
    document.body.append(input);
    input.focus();
    fireEvent.keyDown(input, { key: "1" });
    expect(moved).not.toHaveBeenCalled();
  });
});
