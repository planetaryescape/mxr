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
    { chord: "g f", next: ["f"], label: "Messages filter" },
    { chord: "g i", next: ["i"], label: "Inbox" },
  ]);
  expect(shortcutContinuations(registry, "*", context, true)).toEqual([]);
});

test("places and modes come before tools and saved searches, and an alias shares its action's row", () => {
  const registry = new ActionRegistry();
  const base = { group: "Navigate", run: vi.fn<() => void>() } as const;
  // Registry order: tools and saved searches first, To do last, as the
  // feature modules happen to define them.
  registry.define({ ...base, id: "nav.analytics", label: "Analytics", shortcut: "g A" });
  registry.define({ ...base, id: "nav.saved-search-1", label: "Saved search 1", shortcut: "g 1" });
  registry.define({ ...base, id: "nav.trash", label: "Trash", shortcut: "g #" });
  registry.define({
    ...base,
    id: "nav.inbox",
    label: "Inbox",
    shortcut: "g i",
    aliases: ["g 0"],
  });
  registry.define({ ...base, id: "nav.todo", label: "To do", shortcut: "g x" });

  expect(shortcutContinuations(registry, "g", context, true)).toEqual([
    { chord: "g i", next: ["i", "0"], label: "Inbox" },
    { chord: "g x", next: ["x"], label: "To do" },
    { chord: "g #", next: ["#"], label: "Trash" },
    { chord: "g A", next: ["A"], label: "Analytics" },
    { chord: "g 1", next: ["1"], label: "Saved search 1" },
  ]);
});
