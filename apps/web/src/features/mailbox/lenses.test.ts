import { describe, expect, test } from "vitest";

import { lensesFromShell, resolveLens, savedSearchLenses, slugify } from "./lenses";
import type { ShellResponse, SidebarItem } from "./types";

// Mirrors the bridge's sidebar (crates/web/src/chrome.rs): item ids are the
// ASCII slug of the name, user labels carry their provider label id.
function bridgeSlug(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "-")
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

  test("gives each system lens the identity projections use", () => {
    const identity = (key: string) => lenses.find((lens) => lens.key === key)?.identity;
    expect(identity("inbox")).toEqual({ kind: "inbox" });
    expect(identity("archive")).toEqual({ kind: "all_mail" });
    expect(identity("starred")).toEqual({ kind: "starred" });
    expect(identity("trash")).toEqual({ kind: "trash" });
    expect(identity("sent")).toEqual({ kind: "label", labelName: "SENT" });
  });

  test("lists saved searches separately", () => {
    expect(savedSearchLenses(lenses).map((lens) => [lens.label, lens.path])).toEqual([
      ["Unread invoices", "/m/saved/unread-invoices"],
    ]);
  });
});

describe("resolveLens", () => {
  test("resolves system mailboxes, with all-mail as an alias of archive", () => {
    expect(resolveLens({ kind: "system", mailbox: "inbox" }, lenses)?.key).toBe("inbox");
    expect(resolveLens({ kind: "system", mailbox: "archive" }, lenses)?.label).toBe("All Mail");
    expect(resolveLens({ kind: "system", mailbox: "all-mail" }, lenses)?.key).toBe("archive");
  });

  test("inbox works before the shell has loaded; other mailboxes don't guess", () => {
    expect(resolveLens({ kind: "system", mailbox: "inbox" }, [])?.path).toBe("/m/inbox");
    expect(resolveLens({ kind: "system", mailbox: "starred" }, [])).toBeNull();
  });

  test("unknown lenses resolve to null rather than another mailbox", () => {
    expect(resolveLens({ kind: "system", mailbox: "nope" }, lenses)).toBeNull();
    expect(resolveLens({ kind: "label", name: "deleted-label" }, lenses)).toBeNull();
    expect(resolveLens({ kind: "saved", slug: "gone" }, lenses)).toBeNull();
  });

  test("resolves a label by its URL slug or its real name, keeping the real name", () => {
    const bySlug = resolveLens({ kind: "label", name: "follow-up" }, lenses);
    const byName = resolveLens({ kind: "label", name: encodeURIComponent("Follow Up") }, lenses);

    expect(bySlug?.labelName).toBe("Follow Up");
    expect(bySlug?.params).toEqual({ lens_kind: "label", label_id: "Label_2" });
    expect(bySlug?.identity).toEqual({ kind: "label", labelName: "Follow Up" });
    expect(byName?.key).toBe(bySlug?.key);
  });

  test("round-trips a label with accented characters through its path", () => {
    const reunion = lenses.find((lens) => lens.label === "Réunion")!;
    const name = decodeURIComponent(reunion.path.replace("/m/label/", ""));

    expect(resolveLens({ kind: "label", name }, lenses)?.labelName).toBe("Réunion");
  });

  // Bug: label paths come from the bridge's ASCII-only slug, so names that
  // differ only in non-ASCII characters share a path and the second label
  // can never be opened (a fully non-ASCII name gets an empty slug).
  test.fails("labels whose names are non-ASCII get distinct, resolvable paths", () => {
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

  test("resolves saved searches by slug", () => {
    expect(resolveLens({ kind: "saved", slug: "unread-invoices" }, lenses)?.params).toEqual({
      lens_kind: "saved_search",
      saved_search: "Unread invoices",
    });
  });
});

describe("slugify", () => {
  test("lowercases and joins words with single dashes", () => {
    expect(slugify("  Follow Up / Later ")).toBe("follow-up-later");
    expect(slugify("--Work--")).toBe("work");
  });
});
