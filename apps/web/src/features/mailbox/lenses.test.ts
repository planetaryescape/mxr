import { describe, expect, test } from "vitest";

import { lensesFromShell, resolveLens } from "./lenses";
import type { ShellResponse, SidebarItem } from "./types";

function bridgeSlug(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "-")
    .replace(/^-+|-+$/g, "");
}

function label(name: string, labelId: string): SidebarItem {
  return { id: bridgeSlug(name), label: name, lens: { kind: "label", labelId } };
}

const shell: ShellResponse = {
  sidebar: {
    sections: [
      {
        id: "system",
        title: "System",
        items: [
          { id: "trash", label: "TRASH", lens: { kind: "label", labelId: "TRASH" } },
          { id: "inbox", label: "INBOX", unread: 4, lens: { kind: "inbox" } },
          { id: "starred", label: "STARRED", lens: { kind: "label", labelId: "STARRED" } },
          { id: "sent", label: "SENT", lens: { kind: "label", labelId: "SENT" } },
          { id: "all-mail", label: "All Mail", lens: { kind: "all_mail" } },
          { id: "subscriptions", label: "Subscriptions", lens: { kind: "subscription" } },
        ],
      },
      {
        id: "labels",
        title: "Labels",
        items: [
          label("Work", "Label_1"),
          label("Follow Up", "Label_2"),
          label("Réunion", "Label_3"),
        ],
      },
      {
        id: "saved-searches",
        title: "Saved Searches",
        items: [
          {
            id: "saved-search-unread-invoices",
            label: "Unread invoices",
            lens: { kind: "saved_search", savedSearch: "Unread invoices" },
          },
        ],
      },
    ],
  },
};

const lenses = lensesFromShell(shell);

describe("lensesFromShell", () => {
  test("orders system lenses like the TUI and skips subscriptions", () => {
    const system = lenses.filter((lens) => lens.section === "system");
    expect(system.map((lens) => [lens.key, lens.label, lens.path])).toEqual([
      ["inbox", "Inbox", "/m/inbox"],
      ["starred", "Starred", "/m/starred"],
      ["sent", "Sent", "/m/sent"],
      ["archive", "All Mail", "/m/archive"],
      ["trash", "Trash", "/m/trash"],
    ]);
  });
});

describe("resolveLens", () => {
  test("resolves a label by its URL slug or its real name, keeping the real name", () => {
    const bySlug = resolveLens({ kind: "label", name: "follow-up" }, lenses);
    const byName = resolveLens({ kind: "label", name: encodeURIComponent("Follow Up") }, lenses);

    expect(bySlug?.labelName).toBe("Follow Up");
    expect(bySlug?.params).toEqual({ lens_kind: "label", label_id: "Label_2" });
    expect(bySlug?.identity).toEqual({ kind: "label", labelName: "Follow Up" });
    expect(byName?.key).toBe(bySlug?.key);
  });

  test("labels whose names are non-ASCII get distinct, resolvable paths", () => {
    const cjk = lensesFromShell({
      sidebar: {
        sections: [
          { id: "labels", title: "Labels", items: [label("日本", "L1"), label("中文", "L2")] },
        ],
      },
    });

    const [japan, chinese] = cjk;
    expect(japan?.path).not.toBe(chinese?.path);
    const name = decodeURIComponent(chinese!.path.replace("/m/label/", ""));
    expect(resolveLens({ kind: "label", name }, cjk)?.labelName).toBe("中文");
  });
});
