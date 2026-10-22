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
    match: isMailAction,
  },
  { title: "Mail list", intro: "", match: (action) => inScope(action, "list") },
  { title: "Reader", intro: "", match: (action) => inScope(action, "reader") },
  { title: "Sidebar", intro: "", match: (action) => inScope(action, "sidebar") },
  { title: "Screener", intro: "", match: (action) => inScope(action, "screener") },
  {
    title: "Focus & reply",
    intro:
      "One conversation at a time. In the reply, ⌘Enter sends and moves on; Tab out of the reply (or Esc in the rich-text editor) to use these keys.",
    match: (action) => inScope(action, "focus"),
  },
  {
    title: "Now",
    intro:
      "On a lit new sender, `1` to `4` answer its question; elsewhere they stay the numbered tabs.",
    match: (action) => inScope(action, "now"),
  },
  {
    title: "Messages",
    intro:
      "On a lit new sender, `1` to `4` answer its question; elsewhere they stay the numbered tabs.",
    match: (action) => inScope(action, "messages"),
  },
  { title: "To do", intro: "", match: (action) => inScope(action, "todo") },
  {
    title: "To do: catch-up",
    intro:
      "The first run's older undated tasks: keep each one, or let it go to the Expired list. `C` on To do opens it.",
    match: (action) => inScope(action, "catchup"),
  },
  {
    title: "To do: Expired list",
    intro: "Tasks that expired or were let go. `E` on To do opens it.",
    match: (action) => inScope(action, "expired"),
  },
  { title: "Updates", intro: "", match: (action) => inScope(action, "updates") },
  {
    title: "Reading edition",
    intro:
      "Newsletters as an edition. `b`, `e`, `D` and `A` work on the item or link under the cursor; `D` and `A` preview first.",
    match: (action) => inScope(action, "reading"),
  },
  {
    title: "Reading reader",
    intro:
      "One item in a 66-character column. The article is fetched only on `L` or the Article tab.",
    match: (action) => inScope(action, "reading-reader"),
  },
  { title: "Archive", intro: "", match: (action) => inScope(action, "archive") },
  {
    title: "Arrivals",
    intro:
      "The emails behind one of Now's arrival counts. `X` (in Mail actions) moves the email under the cursor.",
    match: (action) => inScope(action, "arrivals"),
  },
  {
    title: "Compose",
    intro: "While writing a message. ⌘ and Ctrl both work, on every platform.",
    match: (action) => inScope(action, "compose"),
  },
];

function isGlobal(action: Action): boolean {
  return scopesOf(action).includes("global");
}

function isGoTo(action: Action): boolean {
  return action.id.startsWith("nav.");
}

function isMailAction(action: Action): boolean {
  return scopesOf(action).includes("list") && scopesOf(action).includes("reader");
}

/** Live in `scope`; mail actions are left to their own table. */
function inScope(action: Action, scope: ActionScope): boolean {
  return scopesOf(action).includes(scope) && !isMailAction(action);
}

/** Platform-neutral key label for docs: "Mod+k" → "⌘K / Ctrl+K", "Mod+Shift+c" → "⌘⇧C / Ctrl+Shift+C". */
export function docChord(chord: string): string {
  return parseChord(chord)
    .map((token) => {
      if (token.startsWith("Mod+")) {
        const rest = token.slice(4);
        const shift = rest.startsWith("Shift+");
        const name = shift ? rest.slice(6) : rest;
        // Letters print as capitals; named keys (Enter, Backspace) as they are.
        const key = name.length === 1 ? name.toUpperCase() : name;
        return shift ? `⌘⇧${key} / Ctrl+Shift+${key}` : `⌘${key} / Ctrl+${key}`;
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

function boundActions(): Action[] {
  return getRegistry()
    .all()
    .filter((action) => !action.paletteOnly && chordsOf(action).length > 0);
}

/** Bound actions that no table lists, such as every key of a scope without a section. */
export function undocumentedActionIds(): string[] {
  return boundActions()
    .filter((action) => !SECTIONS.some((section) => section.match(action)))
    .map((action) => action.id);
}

export function renderKeybindingsMarkdown(): string {
  const actions = boundActions();
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
