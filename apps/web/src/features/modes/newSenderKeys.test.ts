/* @vitest-environment jsdom */

import { fireEvent } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { ActionRegistry } from "@/lib/actions/registry";
import type { Action, ActionContext } from "@/lib/actions/types";
import { setController } from "@/lib/keys/controllers";
import { installKeyDispatcher } from "@/lib/keys/dispatcher";

import { newSenderActions } from "./newSenderActions";

const answer = vi.fn<() => void>();
const tab = vi.fn<() => void>();

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
  answer.mockReset();
  tab.mockReset();
  const registry = new ActionRegistry();
  // SAFETY: a minimal RunAction; the registry reads only id, shortcut and run.
  registry.define({
    id: "nav.tab-mail",
    label: "Mail",
    group: "Navigate",
    shortcut: "1",
    run: tab,
  } as Action);
  registry.defineMany(newSenderActions("now"));
  setController("now", { senderChoice1: answer });
  uninstall = installKeyDispatcher(window, { registry, context, mac: false });
});

afterEach(() => {
  uninstall();
  setController("now", null);
  document.body.innerHTML = "";
});

describe("1 to 4 while typing", () => {
  test("answer the lit new sender from the page", () => {
    fireEvent.keyDown(document.body, { key: "1" });
    expect(answer).toHaveBeenCalledOnce();
    expect(tab).not.toHaveBeenCalled();
  });

  test("never fire from a text field, which owns its keys", () => {
    const input = document.createElement("input");
    document.body.append(input);
    input.focus();
    fireEvent.keyDown(input, { key: "1" });
    const area = document.createElement("textarea");
    document.body.append(area);
    fireEvent.keyDown(area, { key: "1" });
    expect(answer).not.toHaveBeenCalled();
    expect(tab).not.toHaveBeenCalled();
  });

  test("a held key answers once, not once per repeat", () => {
    fireEvent.keyDown(document.body, { key: "1" });
    fireEvent.keyDown(document.body, { key: "1", repeat: true });
    fireEvent.keyDown(document.body, { key: "1", repeat: true });
    expect(answer).toHaveBeenCalledOnce();
  });
});
