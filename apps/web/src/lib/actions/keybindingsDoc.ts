/*
 * The web keybindings reference, rendered from the action registry. A test
 * compares it with site/src/content/docs/reference/keybindings.md, so the
 * published table cannot drift from what the dispatcher does.
 */

import { parseChord } from "@/lib/keys/chord";

import "./catalog";
import { chordsOf, getRegistry, scopesOf } from "./registry";
import type { Action, ActionScope } from "./types";

const SECTIONS: { title: string; intro: string; match: (action: Action) => boolean }[] = [
  {
    title: "Everywhere",
    intro: "Work on every page except while typing in a field or compose.",
    match: (action) => isGlobal(action) && !isGoTo(action),
  },
  {
    title: "Go to",
    intro: "`g` then a letter, as in the TUI. Digits match the TUI's tabs.",
    match: (action) => isGlobal(action) && isGoTo(action),
  },
  {
    title: "Mail actions",
    intro:
      "In the mail list they act on the selection or the row under the cursor; in the reader, on the open conversation.",
    match: (action) => scopesOf(action).includes("list") && scopesOf(action).includes("reader"),
  },
  { title: "Mail list", intro: "", match: (action) => only(action, "list") },
  { title: "Reader", intro: "", match: (action) => only(action, "reader") },
  { title: "Sidebar", intro: "", match: (action) => only(action, "sidebar") },
  { title: "Screener", intro: "", match: (action) => only(action, "screener") },
  {
    title: "Focus & reply",
    intro:
      "One conversation at a time. In the reply, ⌘Enter sends and moves on; Tab out of the reply (or Esc in the rich-text editor) to use these keys.",
    match: (action) => only(action, "focus"),
  },
  {
    title: "Reading and Paper trail",
    intro: "Bundles of mail that isn't from people. The reader keeps its own keys.",
    match: (action) => only(action, "place"),
  },
];

function isGlobal(action: Action): boolean {
  return scopesOf(action).includes("global");
}

function isGoTo(action: Action): boolean {
  return action.id.startsWith("nav.");
}

function only(action: Action, scope: ActionScope): boolean {
  const scopes = scopesOf(action);
  return scopes.length === 1 && scopes[0] === scope;
}

/** Platform-neutral key label for docs: "Mod+k" → "⌘K / Ctrl+K". */
export function docChord(chord: string): string {
  return parseChord(chord)
    .map((token) => {
      if (token.startsWith("Mod+")) {
        const key = token.slice(4).toUpperCase();
        return `⌘${key} / Ctrl+${key}`;
      }
      if (token.startsWith("Ctrl+")) return `Ctrl+${token.slice(5).toUpperCase()}`;
      if (token === "Escape") return "Esc";
      return token;
    })
    .join(" ");
}

function escapeCell(value: string): string {
  return value.replaceAll("|", "\\|");
}

export function renderKeybindingsMarkdown(): string {
  const actions = getRegistry()
    .all()
    .filter((action) => !action.paletteOnly && chordsOf(action).length > 0);
  const out: string[] = [];
  for (const section of SECTIONS) {
    const rows = actions.filter(section.match);
    if (rows.length === 0) continue;
    out.push(`### ${section.title}`, "");
    if (section.intro) out.push(section.intro, "");
    out.push("| Key | Action | Note |", "|-----|--------|------|");
    const savedJumps = rows.filter((action) => action.id.startsWith("nav.saved-search-"));
    if (savedJumps.length > 0) {
      out.push(
        `| \`g 1\` … \`g ${savedJumps.length}\` | Open saved search 1 to ${savedJumps.length} | In sidebar order, as the TUI's tab strip |`,
      );
    }
    for (const action of rows.filter((row) => !row.id.startsWith("nav.saved-search-"))) {
      const keys = chordsOf(action)
        .map((chord) => `\`${escapeCell(docChord(chord))}\``)
        .join(", ");
      out.push(`| ${keys} | ${escapeCell(action.label)} | ${escapeCell(action.tuiNote ?? "")} |`);
    }
    out.push("");
  }
  return out.join("\n").trimEnd();
}
