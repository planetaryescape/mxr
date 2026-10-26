import { expect, test, vi } from "vitest";
import { ActionRegistry } from "@/lib/actions/registry";
import type { ActionContext } from "@/lib/actions/types";
import { shortcutContinuations } from "./continuations";

const context: ActionContext = {
  path: "/messages",
  activePane: "mailbox",
  scopes: ["messages", "global"],
  selectionCount: 0,
  accountCount: 1,
  hasFocusedThread: false,
  isFirstAccountOnly: true,
};

test("continuations reflect scope precedence and omit unavailable and retired keys", () => {
  const registry = new ActionRegistry();
  const base = { group: "View", run: vi.fn<() => void>() } as const;
  registry.define({ ...base, id: "global-filter", label: "Global filter", shortcut: "g f" });
  registry.define({
    ...base,
    id: "messages-filter",
    label: "Messages filter",
    scopes: ["messages"],
    shortcut: "g f",
  });
  registry.define({
    ...base,
    id: "other-view",
    label: "Other view",
    scopes: ["reading"],
    shortcut: "g r",
  });
  registry.define({
    ...base,
    id: "unavailable",
    label: "Unavailable",
    shortcut: "g a",
    when: () => false,
  });
  registry.define({
    ...base,
    id: "inbox",
    label: "Inbox",
    shortcut: "g i",
    retiredAliases: ["g I"],
  });
  expect(shortcutContinuations(registry, "g", context, true)).toEqual([
    { chord: "g f", next: "f", label: "Messages filter" },
    { chord: "g i", next: "i", label: "Inbox" },
  ]);
  expect(shortcutContinuations(registry, "*", context, true)).toEqual([]);
});
