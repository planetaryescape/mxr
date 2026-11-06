import { afterEach, describe, expect, test } from "vitest";

import { setController } from "@/lib/keys/controllers";

import { shortcutSections } from "./hints";
import type { ActionContext, ActionScope } from "./types";

function ctx(scopes: ActionScope[]): ActionContext {
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

function allHints(scopes: ActionScope[]) {
  return shortcutSections(ctx(scopes)).flatMap((section) => section.hints);
}

afterEach(() => {
  setController("list", null);
  setController("reader", null);
});

describe("shortcutSections", () => {
  test("puts the active pane first, then mail actions, then global keys", () => {
    const titles = shortcutSections(ctx(["list", "global"])).map((section) => section.title);

    expect(titles[0]).toBe("Mail list (this view)");
    expect(titles[1]).toBe("Mail actions");
    expect(titles.slice(2, 5)).toEqual(["Everywhere", "Search", "Go to"]);
    // Other panes follow for discovery, without the "this view" tag.
    expect(titles).toContain("Reader");
    expect(titles.indexOf("Reader")).toBeGreaterThan(titles.indexOf("Go to"));
  });

  test("follows the reader when it is the active pane", () => {
    const titles = shortcutSections(ctx(["reader", "global"])).map((section) => section.title);

    expect(titles[0]).toBe("Reader (this view)");
    expect(titles).toContain("Mail list");
    expect(titles).not.toContain("Mail list (this view)");
  });

  test("in Now, lists its mail verbs as its own and puts Mail actions after the global keys", () => {
    const sections = shortcutSections(ctx(["now", "global"]));
    const titles = sections.map((section) => section.title);
    const now = sections.find((section) => section.id === "now")!;

    expect(titles[0]).toBe("Now (this view)");
    // b and X work in Now; help said nothing of them there.
    expect(now.hints.map((item) => item.id)).toEqual(
      expect.arrayContaining(["mail.snooze", "modes.move"]),
    );
    // Mail actions' e is Archive; in Now e is Done, so they come after Go to.
    expect(titles.indexOf("Mail actions")).toBeGreaterThan(titles.indexOf("Go to"));
  });

  test("lists ? as keyboard help, live everywhere", () => {
    const help = allHints(["global"]).find((hint) => hint.id === "shell.help");

    expect(help).toMatchObject({ keys: ["?"], label: "Keyboard help", live: true });
  });

  test("carries the TUI difference note", () => {
    const readArchive = allHints(["list", "global"]).find(
      (hint) => hint.id === "mail.read-archive",
    );
    const undo = allHints(["global"]).find((hint) => hint.id === "mail.undo");

    expect(readArchive?.note).toMatch(/read\/unread are I and U/);
    expect(undo).toMatchObject({ keys: ["u", "z"], note: expect.stringMatching(/Gmail/) });
  });

  test("marks pane keys live only while that view implements them", () => {
    const before = allHints(["list", "global"]).find((hint) => hint.id === "list.down");
    expect(before?.live).toBe(false);

    setController("list", { down: () => {} });
    const after = allHints(["list", "global"]).find((hint) => hint.id === "list.down");
    expect(after).toMatchObject({ keys: ["j", "↓"], live: true });
  });

  test("omits palette-only actions", () => {
    const ids = allHints(["list", "reader", "global"]).map((hint) => hint.id);

    expect(ids).not.toContain("mail.compose-to-sender");
    expect(ids).not.toContain("list.toggle-threads");
  });
});
