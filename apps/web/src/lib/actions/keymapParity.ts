/*
 * Web and TUI key parity. `docs/reference/tui-keymap.json` is the TUI's
 * dispatcher, probed key by key (crates/tui/src/runner/tests/keymap.rs).
 * `keymapParity.test.ts` lines it up with the action registry: a key must
 * do the same thing in both clients, or appear in KEYMAP_DIFFERENCES with
 * the reason it differs.
 */

import { getRegistry } from "./registry";
import type { ActionScope } from "./types";

export type ParityContext =
  | "list"
  | "reader"
  | "sidebar"
  | "screener"
  | "todo"
  | "now"
  | "messages"
  | "updates"
  | "archive"
  | "reading";

/** The web scopes live in each context, innermost first. */
const CONTEXT_SCOPES: Record<ParityContext, ActionScope[]> = {
  list: ["list", "global"],
  reader: ["reader", "global"],
  sidebar: ["sidebar", "global"],
  todo: ["todo", "global"],
  now: ["now", "global"],
  messages: ["messages", "global"],
  archive: ["archive", "global"],
  updates: ["updates", "global"],
  reading: ["reading", "global"],
  // The TUI's screener is a modal that swallows every other key.
  screener: ["screener"],
};

/**
 * Web action id → the TUI action (its Debug name in tui-keymap.json) that
 * is the same thing. `inline:` names are keys a TUI pane handles in place.
 */
export const SAME_ACTION: Record<string, string[]> = {
  "shell.command-palette": ["OpenCommandPalette"],
  "shell.search-palette": ["OpenGlobalSearch"],
  "shell.help": ["Help"],
  "shell.compose": ["Compose"],
  "mail.undo": ["UndoLastMutation"],
  "nav.now": ["OpenNow"],
  "nav.messages": ["OpenMessages"],
  "nav.updates": ["OpenUpdates"],
  "nav.archive-mode": ["OpenArchiveMode"],
  "nav.inbox": ["GoToInbox", "OpenSavedSearchByIndex(0)"],
  "nav.starred": ["GoToStarred"],
  "nav.sent": ["GoToSent"],
  "nav.drafts": ["GoToDrafts", "OpenStoredDrafts"],
  "nav.archive": ["GoToAllMail"],
  "nav.labels": ["GoToLabel"],
  "nav.analytics": ["OpenAnalyticsScreen"],
  "nav.activity": ["OpenActivityScreen"],
  "nav.logs": ["OpenLogs"],
  "nav.settings": ["EditConfig"],
  "nav.reply-queue": ["OpenReplyQueue"],
  "nav.owed": ["OpenOwedReplies"],
  "nav.invites": ["OpenCalendarInvites"],
  "nav.reading": ["OpenReading"],
  "nav.screener": ["OpenScreenerQueue"],
  "nav.tab-mail": ["OpenTab1"],
  "nav.search-page": ["OpenTab2"],
  "nav.rules": ["OpenTab3"],
  "nav.accounts": ["OpenTab4"],
  "nav.diagnostics": ["OpenTab5"],
  "nav.tab-analytics": ["OpenTab6"],
  "nav.deliveries": ["OpenTab7"],
  ...Object.fromEntries(
    Array.from({ length: 9 }, (_, index) => [
      `nav.saved-search-${index + 1}`,
      [`OpenSavedSearchByIndex(${index + 1})`],
    ]),
  ),
  "mail.archive": ["Archive"],
  "mail.read-archive": ["MarkReadAndArchive"],
  "mail.trash": ["Trash"],
  "mail.spam": ["Spam"],
  "mail.star": ["Star"],
  "mail.mark-read": ["MarkRead"],
  "mail.mark-unread": ["MarkUnread"],
  "mail.label": ["ApplyLabel"],
  "mail.move": ["MoveToLabel"],
  "mail.snooze": ["Snooze"],
  "mail.unsubscribe": ["Unsubscribe"],
  "mail.reply-later": ["FlagReplyLater"],
  "mail.reply": ["Reply"],
  "mail.reply-all": ["ReplyAll"],
  "mail.forward": ["Forward"],
  "mail.export": ["ExportThread"],
  "mail.briefing": ["OpenThreadBriefing"],
  "mail.whois": ["OpenWhoisOnFocusedSender"],
  "mail.sender-profile": ["OpenSenderView"],
  "mail.links": ["OpenLinks"],
  "mail.attachments": ["AttachmentList"],
  "invite.accept": ["RespondInvite(Accept)"],
  "invite.tentative": ["RespondInvite(Tentative)"],
  "invite.decline": ["RespondInvite(Decline)"],
  "invite.accept-comment": ["RespondInviteWithComment(Accept)"],
  "invite.tentative-comment": ["RespondInviteWithComment(Tentative)"],
  "invite.decline-comment": ["RespondInviteWithComment(Decline)"],
  "sidebar.down": ["inline:next_item"],
  "sidebar.up": ["inline:prev_item"],
  "sidebar.open": ["SidebarSelect"],
  "sidebar.collapse": ["inline:collapse_section"],
  "sidebar.expand": ["inline:expand_section"],
  "list.down": ["MoveDown"],
  "list.up": ["MoveUp"],
  "list.top": ["JumpTop"],
  "list.bottom": ["JumpBottom"],
  "list.page-down": ["PageDown"],
  "list.page-up": ["PageUp"],
  "list.viewport-top": ["ViewportTop"],
  "list.viewport-middle": ["ViewportMiddle"],
  "list.open": ["OpenSelected"],
  "list.sidebar": ["inline:focus_sidebar"],
  "list.toggle-select": ["ToggleSelect"],
  "list.select-all": ["PatternSelect(All)"],
  "list.select-none": ["PatternSelect(None)"],
  "list.select-read": ["PatternSelect(Read)"],
  "list.select-unread": ["PatternSelect(Unread)"],
  "list.select-starred": ["PatternSelect(Starred)"],
  "list.visual": ["VisualLineMode"],
  "list.escape": ["Back"],
  "list.filter": ["OpenMailboxFilter"],
  "reader.scroll-down": ["inline:scroll_down"],
  "reader.scroll-up": ["inline:scroll_up"],
  "reader.page-down": ["inline:page_down"],
  "reader.page-up": ["inline:page_up"],
  "reader.top": ["JumpTop"],
  "reader.bottom": ["inline:jump_bottom"],
  "reader.next-message": ["inline:next_message"],
  "reader.prev-message": ["inline:prev_message"],
  "reader.list": ["inline:focus_list"],
  "reader.close": ["Back"],
  "reader.view-reader": ["ToggleReaderMode"],
  "reader.view-html": ["ToggleHtmlView"],
  "reader.remote": ["ToggleRemoteContent"],
  "reader.signature": ["ToggleSignature"],
  "reader.open-original": ["OpenInBrowser"],
  "reader.summary": ["SummarizeCurrentThread"],
  "reader.fullscreen": ["ToggleFullscreen"],
  "screener.allow": ["ScreenerDisposeAllow"],
  "screener.deny": ["ScreenerDisposeDeny"],
  "screener.feed": ["ScreenerDisposeFeed"],
  "screener.paper-trail": ["ScreenerDisposePaperTrail"],
  "screener.down": ["ScreenerModalNext"],
  "screener.up": ["ScreenerModalPrev"],
  "nav.todo": ["OpenTodo"],
  "mail.make-todo": ["CreateTodoFromMessage"],
  "todo.down": ["MoveDown"],
  "todo.up": ["MoveUp"],
  "todo.primary": ["TodoPrimary"],
  "todo.done": ["TodoDone"],
  "todo.schedule": ["TodoSchedule"],
  "todo.edit": ["TodoEdit"],
  "todo.dismiss": ["TodoDismiss"],
  "todo.open-email": ["TodoOpenEmail"],
  "todo.details": ["TodoReviewResolution"],
  "todo.expired": ["TodoOpenExpired"],
  "todo.catchup": ["TodoOpenCatchup"],
  "now.down": ["MoveDown"],
  "now.up": ["MoveUp"],
  "now.open": ["NowOpen"],
  "now.done": ["NowDone"],
  "now.reply": ["Reply"],
  "now.to-do": ["CreateTodoFromMessage"],
  "now.open-email": ["NowOpenEmail"],
  "now.let-go-digest": ["NowLetGoDigest"],
  "now.close-hint": ["DismissHint"],
  "messages.down": ["MoveDown"],
  "messages.up": ["MoveUp"],
  "messages.open": ["MessagesOpen"],
  "messages.reply": ["Reply"],
  "messages.reply-all": ["ReplyAll"],
  "messages.got-it": ["MessagesAck"],
  "messages.done": ["MessagesDone"],
  "messages.to-do": ["CreateTodoFromMessage"],
  "messages.reply-later": ["FlagReplyLater"],
  "messages.pin": ["MessagesPin"],
  "messages.new-topic": ["MessagesNewTopic"],
  "messages.prev-topic": ["MessagesPrevTopic"],
  "messages.next-topic": ["MessagesNextTopic"],
  "messages.person-page": ["MessagesPersonPage"],
  "messages.as-sent": ["MessagesAsSent"],
  "messages.escape": ["MessagesBack"],
  "mail.pass-to-mode": ["PassToMode"],
  "archive.down": ["MoveDown"],
  "archive.up": ["MoveUp"],
  "archive.ask": ["RecordsAsk"],
  "archive.copy-reference": ["RecordsCopyReference"],
  "archive.copy-amount": ["RecordsCopyAmount"],
  "archive.open-document": ["RecordsOpenDocument"],
  "archive.open-email": ["RecordsOpenEmail"],
  "archive.issuer": ["RecordsIssuerPage"],
  "archive.prev-year": ["RecordsPrevYear"],
  "archive.next-year": ["RecordsNextYear"],
  "archive.edit": ["RecordsFix"],
  "archive.check": ["RecordsMarkChecked"],
  "archive.dismiss": ["RecordsDismiss"],
  "archive.export": ["RecordsExport"],
  "archive.make-todo": ["RecordsMakeTodo"],
  "archive.close": ["RecordsBack"],
  "updates.down": ["MoveDown"],
  "updates.up": ["MoveUp"],
  "updates.expand": ["UpdatesOpen"],
  "updates.let-go-all": ["UpdatesLetGoAll"],
  "updates.let-go-source": ["UpdatesLetGoSource"],
  "updates.needs-me": ["UpdatesNeedsMe"],
  "updates.tune": ["UpdatesTune"],
  "updates.link": ["UpdatesOpenLink"],
  "updates.open-email": ["UpdatesOpenEmail"],
  "updates.copy-code": ["UpdatesCopyCode"],
  "updates.pin": ["TogglePin"],
  "updates.sweep": ["UpdatesSweep"],
  "updates.close-hint": ["UpdatesClose"],
  "reading.down": ["MoveDown"],
  "reading.up": ["MoveUp"],
  "reading.read": ["ReadingRead"],
  "reading.article": ["ReadingArticle"],
  "reading.later": ["ReadingLater"],
  "reading.let-go": ["ReadingLetGo"],
  "reading.unsubscribe": ["ReadingUnsubscribe"],
  "reading.original": ["ReadingOriginal"],
  "reading.let-go-all": ["ReadingLetGoAll"],
  "reading.open-email": ["ReadingOpenEmail"],
  "reading.later-shelf": ["ReadingLaterShelf"],
  "reading.move-sender": ["OpenSenderKindMenu"],
  "reading.marginalia": ["ReadingMarginalia"],
  "modes.move": ["OpenMoveMenu"],
  "modes.move-sender": ["OpenSenderMoveMenu"],
};

export interface KeymapDifference {
  /** `*` covers every context but the screener modal. */
  context: ParityContext | "*";
  keys: string[];
  /** Which client binds the key: one of them, or both to different things. */
  bound: "web" | "tui" | "both";
  why: string;
}

/** Keys that deliberately differ between the clients, and why. */
export const KEYMAP_DIFFERENCES: KeymapDifference[] = [
  // Everywhere.
  {
    context: "*",
    keys: ["Mod+."],
    bound: "web",
    why: "Shows or hides the assistant in the browser; terminals don't pass Ctrl+. through reliably, so the TUI uses Ctrl+a",
  },
  {
    context: "*",
    keys: ["Ctrl+a"],
    bound: "tui",
    why: "Shows or hides the TUI's assistant pane; the browser keeps Ctrl+a for select-all and uses Mod+.",
  },
  {
    context: "*",
    keys: ["Mod+k", ":"],
    bound: "web",
    why: "Extra palette keys for the browser; Ctrl+p opens the palette in both",
  },
  {
    context: "*",
    keys: ["0", "8", "9"],
    bound: "web",
    why: "Web shortcuts to Settings, Screener and Invites (g c, g S, g v in both); the TUI's tabs stop at 7",
  },
  {
    context: "*",
    keys: ["g !", "g #", "g n", "g w"],
    bound: "web",
    why: "Spam, Trash, Snoozed and Waiting on are pages on the web; labels or a desk lane in the TUI",
  },
  {
    context: "*",
    keys: ["g F"],
    bound: "web",
    why: "Focus & reply is a page on the web; the TUI starts it from the reply queue (g q, then F)",
  },
  {
    context: "*",
    keys: ["z"],
    bound: "web",
    why: "z also undoes on the web, for Gmail muscle memory; in the TUI it starts z z",
  },
  {
    context: "*",
    keys: ["z z"],
    bound: "tui",
    why: "The TUI centers the row; the browser keeps it in view",
  },
  {
    context: "*",
    keys: ["Tab"],
    bound: "tui",
    why: "Tab moves browser focus; the TUI switches panes with it",
  },
  {
    context: "*",
    keys: ["q"],
    bound: "tui",
    why: "q quits the TUI at its root; a browser tab has nothing to quit",
  },
  {
    context: "*",
    keys: ["n", "N"],
    bound: "tui",
    why: "The TUI steps search matches with n and N; the web search page lists them",
  },
  // Mail list.
  {
    context: "list",
    keys: ["Mod+a"],
    bound: "web",
    why: "Cmd+a is the browser's select-all; the TUI selects all with * a",
  },
  {
    context: "*",
    keys: ["* a", "* n", "* r", "* s", "* t", "* u"],
    bound: "tui",
    why: "The web's pattern keys are list-only and it has no thread pattern; outside the TUI's mail list, pattern keys only say where they work",
  },
  {
    context: "list",
    keys: ["Home", "End", "PageUp", "PageDown", "Backspace", "Delete"],
    bound: "web",
    why: "Browser keyboard keys",
  },
  {
    context: "list",
    keys: ["g f"],
    bound: "web",
    why: "Ctrl+f is the browser's Find off macOS, so the web also filters with g f",
  },
  {
    context: "list",
    keys: ["w"],
    bound: "web",
    why: "The web list's row action (Done on the desk, Wake on Snoozed); the TUI desk uses e",
  },
  {
    context: "list",
    keys: ["F", "O", "R", "S", "y"],
    bound: "tui",
    why: "The TUI reader is a preview beside the list, so its keys work from the list; the web opens the reader first",
  },
  // Reader.
  {
    context: "reader",
    keys: ["o"],
    bound: "both",
    why: "The web collapses messages and o toggles one, as in Gmail; the TUI shows every message and o opens the original, like O",
  },
  {
    context: "reader",
    keys: ["n", "N"],
    bound: "both",
    why: "n and N step search matches in the TUI and conversations in the web reader",
  },
  {
    context: "reader",
    keys: ["Q", ";", "[", "]", "g H"],
    bound: "web",
    why: "Web reader features: quoted text, collapse all, archive and step, raw headers",
  },
  {
    context: "reader",
    // prettier-ignore
    keys: ["Home", "End", "PageUp", "PageDown", "Space", "Shift+Space", "Backspace", "Delete"],
    bound: "web",
    why: "Browser keyboard keys",
  },
  {
    context: "reader",
    keys: ["Ctrl+f", "Enter", "V", "x"],
    bound: "tui",
    why: "List keys that reach the TUI's list from its reader pane",
  },
  // Sidebar.
  {
    context: "sidebar",
    // prettier-ignore
    keys: ["A", "E", "F", "H", "L", "M", "V", "b", "x", "Escape", "Ctrl+d", "Ctrl+u", "Ctrl+f", "i a", "i m", "i d", "i A", "i M", "i D"],
    bound: "tui",
    why: "The TUI sidebar passes list keys to the mail list beside it; the web sidebar keeps its own",
  },
  {
    context: "sidebar",
    keys: ["G", "g g"],
    bound: "both",
    why: "The TUI moves the mail list to its end; the web moves within the sidebar",
  },
  {
    context: "sidebar",
    keys: ["n"],
    bound: "tui",
    why: "A new saved search from the TUI sidebar; the web makes them on the Search page",
  },
  {
    context: "sidebar",
    keys: ["c", "u"],
    bound: "web",
    why: "Compose and undo work everywhere on the web; the TUI sidebar takes only navigation",
  },
  // To do.
  {
    context: "todo",
    // prettier-ignore
    keys: ["G", "g g", "H", "L", "M", "Ctrl+d", "Ctrl+u", "h", "ArrowLeft"],
    bound: "tui",
    why: "The TUI lens shares the list's motions and pane keys; the web runway moves with j and k",
  },
  {
    context: "todo",
    keys: ["i a", "i m", "i d", "i A", "i M", "i D"],
    bound: "tui",
    why: "The TUI lens passes invite answers through; the web answers an invite from the reader",
  },
  {
    context: "todo",
    keys: ["Escape"],
    bound: "web",
    why: "Both close the To do card with Esc; the TUI's keymap probe can't see a key that needs the card on screen",
  },
  {
    context: "todo",
    keys: ["c"],
    bound: "web",
    why: "Compose works everywhere on the web; the TUI lens keeps c free",
  },
  {
    context: "todo",
    keys: ["Space"],
    bound: "web",
    why: "Space ticks off the focused web to-do; the TUI keeps its To do shortcut on e",
  },
  // Now.
  {
    context: "now",
    // prettier-ignore
    keys: ["G", "g g", "H", "L", "M", "Ctrl+d", "Ctrl+u", "h", "ArrowLeft"],
    bound: "tui",
    why: "The TUI lens shares the list's motions and pane keys; the web Now moves with j and k",
  },
  {
    context: "now",
    keys: ["i a", "i m", "i d", "i A", "i M", "i D"],
    bound: "tui",
    why: "The TUI lens passes invite answers through; the web answers an invite from the reader",
  },
  {
    context: "now",
    keys: ["c"],
    bound: "web",
    why: "Compose works everywhere on the web; the TUI lens keeps c free",
  },
  {
    context: "now",
    keys: ["1", "2", "3", "4"],
    bound: "both",
    why: "On a lit new sender the web answers its question with 1 to 4 (Messages, Updates, Reading, Block); everywhere else they stay the numbered tabs, and the TUI keeps them as tabs",
  },
  // Messages.
  {
    context: "messages",
    // prettier-ignore
    keys: ["G", "g g", "H", "L", "M", "Ctrl+d", "Ctrl+u", "h", "ArrowLeft", "z z", "q"],
    bound: "tui",
    why: "The TUI lens shares the list's motions and pane keys; the web Messages moves with j and k",
  },
  {
    context: "messages",
    keys: ["i a", "i m", "i d", "i A", "i M", "i D"],
    bound: "tui",
    why: "The TUI lens passes invite answers through; the web answers an invite from the reader",
  },
  {
    context: "messages",
    keys: ["1", "2", "3", "4"],
    bound: "both",
    why: "On a lit new sender the web answers its question with 1 to 4 (Messages, Updates, Reading, Block); everywhere else they stay the numbered tabs, and the TUI keeps them as tabs",
  },
  {
    context: "messages",
    keys: ["Mod+Enter"],
    bound: "web",
    why: "The web's reply box sends and moves to the next person; the TUI composes in $EDITOR",
  },
  // Archive.
  {
    context: "archive",
    keys: ["g f"],
    bound: "both",
    why: "The web opens every filter at once; the TUI steps through the kind chips",
  },
  {
    context: "archive",
    // prettier-ignore
    keys: ["G", "g g", "H", "L", "M", "Ctrl+d", "Ctrl+u", "h", "ArrowLeft", "ArrowRight"],
    bound: "tui",
    why: "The TUI lens shares the list's motions and pane keys, and Right opens the card; the web shows the card beside the ledger",
  },
  {
    context: "archive",
    keys: ["i a", "i m", "i d", "i A", "i M", "i D"],
    bound: "tui",
    why: "The TUI lens passes invite answers through; the web answers an invite from the reader",
  },
  {
    context: "archive",
    keys: ["c"],
    bound: "web",
    why: "Compose works everywhere on the web; the TUI lens keeps c free",
  },
  // Updates.
  {
    context: "updates",
    // prettier-ignore
    keys: ["G", "g g", "H", "M", "Ctrl+d", "Ctrl+u", "h", "ArrowLeft"],
    bound: "tui",
    why: "The TUI lens shares the list's motions and pane keys; the web briefing moves with j and k",
  },
  {
    context: "updates",
    keys: ["i a", "i m", "i d", "i A", "i M", "i D"],
    bound: "tui",
    why: "The TUI lens passes invite answers through; the web answers an invite from the reader",
  },
  {
    context: "updates",
    keys: ["c"],
    bound: "web",
    why: "Compose works everywhere on the web; the TUI lens keeps c free",
  },
  {
    context: "updates",
    keys: ["Space"],
    bound: "web",
    why: "Space opens the details under the focused web line; the TUI's Updates keymap binds no Space",
  },
  // Reading.
  {
    context: "reading",
    // prettier-ignore
    keys: ["G", "g g", "H", "Ctrl+d", "Ctrl+u", "h", "ArrowLeft"],
    bound: "tui",
    why: "The TUI lens shares the list's motions and pane keys; the web edition moves with j and k",
  },
  {
    context: "reading",
    keys: ["i a", "i m", "i d", "i A", "i M", "i D"],
    bound: "tui",
    why: "The TUI lens passes invite answers through; the web answers an invite from the reader",
  },
  {
    context: "reading",
    keys: ["t"],
    bound: "tui",
    why: "The TUI makes a to-do from an item's email; on the web, open the email and press t there",
  },
  {
    context: "reading",
    keys: ["Escape"],
    bound: "web",
    why: "Both close the Reading card with Esc; the TUI's keymap probe can't see a key that needs the card on screen",
  },
  {
    context: "reading",
    keys: ["c"],
    bound: "web",
    why: "Compose works everywhere on the web; the TUI lens keeps c free",
  },
  // Screener.
  {
    context: "screener",
    keys: ["1", "2", "3", "4"],
    bound: "web",
    why: "The lit row shows 1 Allow, 2 Deny, 3 Feed, 4 Paper trail, like a new sender's choice; the TUI modal keeps a, d, f, p",
  },
  {
    context: "screener",
    keys: ["Escape"],
    bound: "tui",
    why: "The TUI screener is a modal that Esc closes; the web screener is a page",
  },
];

export type TuiKeymap = Record<ParityContext, Record<string, string>>;

export interface KeyRow {
  context: ParityContext;
  key: string;
  web?: string;
  tui?: string;
}

/** The web's effective key → action id in each context (current keys only). */
export function webKeymap(context: ParityContext): Map<string, string> {
  const scopes = CONTEXT_SCOPES[context];
  const map = new Map<string, string>();
  for (const scope of scopes.toReversed()) {
    for (const binding of getRegistry().bindings()) {
      if (binding.scope !== scope) continue;
      if (binding.retired) continue;
      map.set(binding.chord, binding.action.id);
    }
  }
  return map;
}

/** Every key bound in either client, side by side. */
export function keyRows(tui: TuiKeymap): KeyRow[] {
  const rows: KeyRow[] = [];
  for (const context of Object.keys(CONTEXT_SCOPES) as ParityContext[]) {
    const web = webKeymap(context);
    const tuiMap = tui[context] ?? {};
    const keys = new Set([...web.keys(), ...Object.keys(tuiMap)]);
    for (const key of [...keys].toSorted()) {
      rows.push({ context, key, web: web.get(key), tui: tuiMap[key] });
    }
  }
  return rows;
}

export function sameAction(row: KeyRow): boolean {
  return (
    row.web !== undefined && row.tui !== undefined && !!SAME_ACTION[row.web]?.includes(row.tui)
  );
}

function boundBy(row: KeyRow): KeymapDifference["bound"] {
  if (row.web !== undefined && row.tui !== undefined) return "both";
  return row.web !== undefined ? "web" : "tui";
}

/** The listed difference that explains this row, if any. */
export function differenceFor(row: KeyRow): KeymapDifference | undefined {
  const bound = boundBy(row);
  const listed = (context: KeymapDifference["context"]) =>
    KEYMAP_DIFFERENCES.find(
      (entry) => entry.context === context && entry.bound === bound && entry.keys.includes(row.key),
    );
  return listed(row.context) ?? (row.context === "screener" ? undefined : listed("*"));
}
