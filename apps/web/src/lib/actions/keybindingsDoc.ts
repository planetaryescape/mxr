/*
 * The web keybindings reference, rendered from the action registry. A test
 * compares it with site/src/content/docs/reference/keybindings.md, so the
 * published table cannot drift from what the dispatcher does.
 */

import { parseChord } from "@/lib/keys/chord";

import "./catalog";
import { isOverlayKey, type DefinedAction } from "./definition";
import keymap from "./keymap.generated.json";
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
    intro:
      "`g` then a letter, as in the TUI: lowercase for mail places and modes, a capital for tools.",
    match: (action) => isGlobal(action) && isGoTo(action),
  },
  {
    title: "Mail actions",
    intro:
      "In the mail list they act on the selection or the row under the cursor; in the reader, on the open conversation.",
    match: isMailAction,
  },
  {
    title: "Mail list",
    intro: "With a conversation open beside the list, the reader's view keys work from here too.",
    match: (action) => inScope(action, "list"),
  },
  {
    title: "Reader",
    intro: "Selection and filter keys act on the mail list beside the reader.",
    match: (action) => inScope(action, "reader"),
  },
  {
    title: "Sidebar",
    intro:
      "Keys for the mail list and the reader reach them from here, as in the TUI: mail verbs act on the list's selection or its lit row.",
    // Every key that works from the sidebar, mail verbs included.
    match: (action) => scopesOf(action).includes("sidebar"),
  },
  {
    title: "Page controls",
    intro:
      "On tool and settings pages, focus tabs and filters with `g f`; arrow keys then change the choice.",
    match: (action) => inScope(action, "page"),
  },
  { title: "Screener", intro: "", match: (action) => inScope(action, "screener") },
  {
    title: "Focus & reply",
    intro:
      "One conversation at a time. In the reply, ⌘Enter sends and moves on; Tab out of the reply (or Esc in the rich-text editor) to use these keys.",
    match: (action) => inScope(action, "focus"),
  },
  {
    title: "Now",
    intro: "On a lit new sender, `X` then `M`, `U`, `R` or `D` answers its question.",
    match: (action) => inScope(action, "now"),
  },
  {
    title: "Messages",
    intro: "On a lit new sender, `X` then `M`, `U`, `R` or `D` answers its question.",
    match: (action) => inScope(action, "messages") || inScope(action, "messages-filters"),
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

/**
 * The dialogs, in page order. An open dialog owns every key and reads its
 * own from the key definition (`isOverlayKey`), so these keys are no
 * registry binding; their tables come from the definition itself.
 */
const OVERLAYS: { scope: string; title: string; intro: string }[] = [
  { scope: "help", title: "Help", intro: "`?` opens it. Letters type into its filter." },
  {
    scope: "palette",
    title: "Command palette",
    intro: "`⌘K` / `Ctrl+K` or `:` opens it. Letters type into its filter.",
  },
  {
    scope: "search-palette",
    title: "Search palette",
    intro: "`/` opens it. Letters type the search.",
  },
  {
    scope: "label-picker",
    title: "Labels",
    intro: "`l` labels and `v` moves. Letters type into its filter, and `Enter` ticks a label.",
  },
  {
    scope: "categories",
    title: "Categories",
    intro:
      "Letters type into its filter, and `Enter` ticks or unticks a category; Preview shows the change before it applies.",
  },
  { scope: "links", title: "Links", intro: "`L` opens it." },
  { scope: "unsubscribe", title: "Unsubscribe", intro: "`D` opens it." },
  {
    scope: "move-picker",
    title: "Move to",
    intro: "`X` moves this email; a capital moves everything from its sender.",
  },
  {
    scope: "move-picker-sender",
    title: "Move the sender to",
    intro: "`P` moves everything from the sender. Either case picks.",
  },
  {
    scope: "new-sender-picker",
    title: "Move to: a new sender",
    intro:
      "`X` or `P` on a new sender: a capital answers its question (from `P`, either case does).",
  },
  {
    scope: "sender-kind-picker",
    title: "Move sender to",
    intro: "`P` in Reading. Either case picks.",
  },
  {
    scope: "not-now",
    title: "Not now",
    intro: "`b` opens it on Later. Letters type a time of your own.",
  },
  { scope: "correct-todo", title: "Correct a to-do", intro: "`X` on a to-do." },
  { scope: "sender-ask", title: "Always for this sender?", intro: "After a Not-sure answer." },
  {
    scope: "reply-later",
    title: "Reply later",
    intro:
      "Letters type when to bring it back; with nothing typed, `Enter` flags it for reply later.",
  },
  {
    scope: "delete-confirm",
    title: "Delete confirms",
    intro:
      "Deleting a draft, rule, saved search or label. It opens on Keep, so a bare `Enter` deletes nothing.",
  },
  {
    scope: "bulk-confirm",
    title: "Bulk action preview",
    intro: "A batch action over many conversations shows them first.",
  },
  {
    scope: "sweep-confirm",
    title: "Sweep preview",
    intro: "Sweeping a sender or a whole place. A whole place's sweep opens on Cancel.",
  },
  {
    scope: "let-go-preview",
    title: "Let go preview",
    intro: "Letting go of everything in To do's catch-up or in Updates.",
  },
  {
    scope: "send-confirm",
    title: "Send confirm",
    intro: "When the safety check found something. It opens on Cancel.",
  },
  {
    scope: "compose-discard",
    title: "Discard draft",
    intro: "Discarding a draft from the composer. It opens on Keep.",
  },
  {
    scope: "assistant-approval",
    title: "Assistant: a change waiting for approval",
    intro:
      "The proof card in the assistant, while it has focus. `Esc` in the message box moves there.",
  },
  {
    scope: "assistant-question",
    title: "Assistant: a question waiting for an answer",
    intro:
      "The question card in the assistant, while it has focus. Number keys answer the first question not yet answered.",
  },
  {
    scope: "assistant-history",
    title: "Assistant: past conversations",
    intro: "The assistant's history list.",
  },
  {
    scope: "popover",
    title: "Popovers and panels",
    intro:
      "A topic list, the people in a conversation, Search's filters, a link preview or the side rail. Menus close with `Esc` too.",
  },
];

/** The definition's keys for the web's dialogs, by dialog. */
function overlayActions(scope: string): DefinedAction[] {
  return (keymap.action as DefinedAction[]).filter(
    (action) =>
      (!action.clients || action.clients.includes("web")) &&
      isOverlayKey(action) &&
      (action.keys?.length ?? 0) > 0 &&
      action.scopes?.includes(scope),
  );
}

/** Dialogs with web keys that no table lists. */
export function undocumentedOverlays(): string[] {
  const scopes = new Set(
    (keymap.action as DefinedAction[])
      .filter(
        (action) => (!action.clients || action.clients.includes("web")) && isOverlayKey(action),
      )
      .flatMap((action) => action.scopes ?? []),
  );
  return [...scopes].filter((scope) => !OVERLAYS.some((overlay) => overlay.scope === scope));
}

function isGlobal(action: Action): boolean {
  return scopesOf(action).includes("global");
}

function isGoTo(action: Action): boolean {
  return action.id.startsWith("nav.");
}

/** A mail verb: in both the list and the reader, and not a pane's own key. */
function isMailAction(action: Action): boolean {
  if (action.id.startsWith("list.") || action.id.startsWith("reader.")) return false;
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
  out.push(
    "### Dialogs",
    "",
    "An open dialog takes every key until it closes. `Esc` closes any of them without changing anything, and so does `q` where nothing is typed; `Enter` presses the highlighted button, and in a yes-or-no question `y` is yes and `n` is no.",
    "",
  );
  for (const overlay of OVERLAYS) {
    const rows = overlayActions(overlay.scope);
    if (rows.length === 0) continue;
    out.push(`#### ${overlay.title}`, "");
    if (overlay.intro) out.push(overlay.intro, "");
    out.push("| Key | Action | Note |", "|-----|--------|------|");
    for (const action of rows) {
      const keys = [
        ...(action.keys ?? []).map((chord) => `\`${escapeCell(docChord(chord))}\``),
        ...(action.mac_keys ?? []).map((chord) => `\`${escapeCell(docChord(chord))}\` (macOS)`),
      ].join(", ");
      const note = action.note ?? (action.web_native ? "The browser's own key" : "");
      out.push(`| ${keys} | ${escapeCell(action.label)} | ${escapeCell(note)} |`);
    }
    out.push("");
  }
  return out.join("\n").trimEnd();
}
