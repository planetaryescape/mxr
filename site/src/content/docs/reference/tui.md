---
title: TUI
description: Reference for screens, panes, modals, and interaction model in the mxr terminal UI.
---

## Screens

The TUI has seven top-level screens, one per digit key:

- Mailbox (`1`)
- Search (`2`)
- Rules (`3`)
- Accounts (`4`)
- Diagnostics (`5`)
- Analytics (`6`)
- Deliveries (`7`)

The command palette (`Ctrl-p`) opens each by name too. Rebind the keys in
`keys.toml` next to the file printed by `mxr config path` (see
[Custom keybindings](/reference/config/#custom-keybindings)).

## Discoverability

Three places to find a key without memorising:

- **`?`** opens the help modal, which shows every key bound in the current view.
- **`Ctrl-p`** opens the command palette, which fuzzy-matches every action by name.
- **The hint bar at the bottom** of every screen surfaces the most relevant shortcuts for the current selection.

The keybindings reference page lists every default; the help modal is faster while you're using the TUI.

## Mailbox screen

Layout:

- Sidebar
- Mail list
- Message or thread pane

Behavior:

- Thread-first list by default
- Optional message-list mode
- Focused-thread-message targeting for reply and mutations
- Explicit right-pane dismissal with `Esc`
- Bulk selection with confirmation modals
- Sidebar lenses replace the mail list in place: **Desk**, **Reading**,
  **Paper trail**, **Subscriptions**, **Owed replies**, and **Calendar
  invites** (every detected invite with inline RSVP; see
  [keybindings](/reference/keybindings/#calendar-invites-lens))

## Desk

**Desk** is the first lens in the sidebar, and **Go to the Desk** opens it.
It shows the same lanes as `mxr desk` ([Clear the desk](/guides/now/)).
Move across the lanes and open a conversation beside the desk as in any
list. The archive and mark-read-and-archive keys are **Done** for the row
under the cursor, from the list or from that conversation, and undo reverses
it. A held key repeats in the TUI, so press Done once per row. When the desk
or the owed-replies list clears, the status bar says **Low tide. Nobody's
waiting on you.** The keys are in the
[keybindings reference](/reference/keybindings/#go-to).

## Reading and Paper trail

Reading and Paper trail are sidebar lenses
([Clear Reading and Paper trail](/guides/reading-and-paper-trail/)). Mail is
bundled by sender, each bundle with its "here because" line and no unread
count. From a bundle you can pin a message, move the sender to another
place, sweep the sender's bundle or the whole place, and load more senders
or more of one sender's messages. Every sweep shows its preview first; the
whole-place sweep opens on **Cancel**, so confirming it takes a deliberate
extra step. Undo reverses every chunk of the last sweep. Mail keys (archive,
star, trash) act on the message under the cursor. The keys are in the
[keybindings reference](/reference/keybindings/#reading-and-paper-trail).

## Archive

Archive (`ge`) is a records lens: receipts, orders, bookings, bills and
documents, one row per record ([Archive](/guides/archive/)). The answer box
sits at the top; `/` focuses it and `Enter` asks, and the answer card shows
the field, where it came from and whether it is checked. Below it are the
"Coming up" lines and the ledger by month, each month with its count and
totals; `·?` marks an amount nobody has confirmed. `Enter` on a row opens
the record's document, and the record card shows every field with its
source. `?` explains the mode; the first time the cursor reaches the first
record, or an answer appears, a one-line hint in the status line names its
key, and `Esc` dismisses it for good. `T` on a conversation files
it as a record after showing the card. The keys are in the
[keybindings reference](/reference/keybindings/#archive).

## Thread context

When a thread opens, the message pane leads with its context: the gist and
what they ask of you (with the quoted sentence) when a model is configured,
then who the thread is with and how you know them, whether you owe a reply,
open promises both ways, and which model wrote the gist. The facts arrive at
once; the gist waits out a short debounce so scrolling through the list does
not ask the model about every row. The CLI equivalent is
`mxr briefing context THREAD_ID`.

## Thread summaries

Press `y` or run `Ctrl-p` → **Summarize Thread** from the mailbox, message, or
thread view. The LLM request runs in the background, so navigation, body loads,
and other mail actions keep working.

Cached summaries appear above the message body when a thread opens. If a long
uncached thread is worth summarizing, the TUI may also start the same background
request after a short debounce. While it runs, the message pane shows a
**Summary** block with a refreshing state; when it finishes, the Markdown
summary replaces that loading text. The CLI equivalent is:

```bash
mxr summarize THREAD_ID
```

## Search screen

- Fixed query input at the top
- Result list on the left
- Preview pane on the right
- Live search against the full local index
- `Ctrl-f` is separate and only filters the current mailbox
- `Enter`, `o`, or `l` opens the selected result in preview
- `Esc` moves preview -> results -> mailbox

## Rules screen

- Rule list on the left
- Guided workspace on the right
- Overview, history, dry-run, and edit states
- Textarea-driven condition/action editing for supported rule fields

## Diagnostics screen

- Status summary
- Doctor output
- Recent events
- Recent logs
- Bug-report generation
- Config edit and log-open shortcuts from inside the page

## Accounts screen

- Details on the left
- Runtime account list on the right
- Add IMAP/SMTP account
- Test connectivity
- Set default account
- Edit config without leaving the TUI
- Inspect runtime-only accounts such as browser-auth Gmail setups

## Analytics screen

Same surface as the CLI analytics commands (`mxr storage`,
`mxr contacts`, `mxr stale`, `mxr response-time`, `mxr subscriptions`,
`mxr wrapped`), without leaving the TUI. Cycle views with
`Tab` / `Shift-Tab`; refresh the active view with `r`.

Six views:

- **Storage**: sender / mimetype / label rollups (`m` toggles to
  Largest Messages mode; `g` cycles `group_by` while in Breakdown).
- **Stale Threads**: threads waiting on a reply (`p` toggles
  perspective, `[`/`]` adjusts `older_than_days`, `{`/`}` adjusts
  `within_days`).
- **Contacts**: asymmetry vs decay (`m` toggles sub-mode, `R`
  refreshes the materialized contacts table).
- **Response Time**: reply-latency percentiles (`d` toggles
  direction).
- **Subscriptions**: list-sender ROI table (`o` toggles open-rate
  ranking; `u` opens the unsubscribe-confirm modal for the selected
  row).
- **Wrapped**: Spotify-style yearly summary as a 7-tile dashboard
  grid (`h`/`j`/`k`/`l` move between tiles, `y`/`Y` step year,
  `t` cycles window kind: YTD → Year → SinceDays).

Two cross-view interactions:

- `Enter` drills down. Storage senders/labels and Contacts emails
  jump to a Search filter; Stale Threads, Largest Messages, and
  Subscriptions rows open the underlying conversation directly via
  `Request::GetEnvelope` (no search round-trip).
- `f` opens the **filter modal**, a per-view form with all CLI
  flags exposed as editable fields. `Tab`/`Shift-Tab` to navigate,
  `Enter` to apply, `Esc` to cancel.

## Modals and overlays

- Command palette
- Help modal
- Label picker
- Compose confirmation: `[s]` sends, `[a]` schedules send-later,
  `[n]` sends and sets a follow-up reminder, `[d]` saves a draft,
  `[r]` refines, `[e]` reopens `$EDITOR`, `Esc` discards
- Bulk confirmation
- Attachment modal
- Snooze modal: preset list plus a **Custom…** entry that opens a
  text prompt with a live preview of the resolved time, using the same
  [time phrases](/reference/time-phrases/) as `mxr snooze --until`.
  Tab switches between readings of an ambiguous phrase such as `fri 3`;
  the send-at and remind prompts work the same way
- Unsubscribe confirmation
- Analytics filter modal
- Reply-later queue browser: list of flagged messages and due reminders,
  opened with **Reply queue** (`gq`) or `Ctrl-p → Reply Queue`. `F` there replies to each queued
  message in turn, from the selected one: sending one reply opens the next
  ([Focus & reply](/guides/focus-and-reply/)). Cancel a pending reminder
  from the focused sent message with `Ctrl-p → Cancel Reminder`
- Promise prompt: after a send whose message promises something with a
  date, a toast asks "Remind me …?"; `y` keeps it as a reminder, `n` or
  `Esc` lets it go
- Snippets browser: read-only list with body preview; CRUD flows
  through `mxr snippets`
- Sender profile: volume, cadence, open commitments, and other recent
  emails from the focused message's sender, opened with
  `Ctrl-p → Sender View`. Inside the modal, `j` / `k` selects another
  email and `Enter` / `o` opens it.
- Screener queue: triage list with `a`/`d`/`f`/`p` disposition keys
  (allow / deny / feed / paper-trail) wired to `Request::SetScreenerDecision`
- Welcome / setup: first-launch modal with `d` (demo), `g` (Gmail),
  `i` (IMAP) shortcuts; `Enter` opens the new-account form
- Doctor findings: surfaced inside the Diagnostics Status pane with
  per-finding glyph (`✗` / `!` / `·`), category, message, and indented
  remediation commands

## Mailbox semantics

- Actions in thread view target the focused message
- Labels appear in message headers
- Attachment indicators appear in the mail list and message/thread header
- Thread counts are styled separately from sender names
- Label and saved-search scopes drive the mail list header and query state
- Message bodies are fetched through bulk `ListBodies` reads and should
  already be cached from sync. The TUI does not use body preview as a
  provider repair path.
- Mailbox mutations are optimistic. Transient IPC/database failures get
  bounded retry while the optimistic UI stays in place; explicit
  mutations surface an error after retries are exhausted. Preview
  auto-mark-read is best-effort and reconciles quietly.

## Discovery model

- Hints: the first time the cursor reaches an element a mode explains
  (Now's first why line, To do's first runway bar, Messages' Got it, an
  Archive answer), one sentence naming its key shows in the status line.
  `Esc` or using that key dismisses it in every client; `mxr modes hint ID
  --show` brings one back. There is no tour.
- Command palette: broad action surface
- Help modal: context-aware keybinding reference
- Hint bar: context-sensitive shortcuts, including selection-aware actions
- `gc`: edit config globally
