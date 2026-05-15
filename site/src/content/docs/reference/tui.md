---
title: TUI
description: Reference for screens, panes, modals, and interaction model in the mxr terminal UI.
---

## Screens

The TUI has six top-level screens:

- Mailbox (`1`)
- Search (`2`)
- Rules (`3`)
- Accounts (`4`)
- Diagnostics (`5`)
- Analytics (no digit; open via `Ctrl-p` → "Analytics")

Open the first five with `1`-`5`. Analytics has no default digit key today — open it from the command palette (`Ctrl-p` then type "Analytics") or rebind the open action in `~/.config/mxr/keys.toml` (see [Custom keybindings](/reference/config/#custom-keybindings)).

## Discoverability

Three places to find a key without memorising:

- **`?`** opens the help modal — context-aware, shows every key bound in the current view.
- **`Ctrl-p`** opens the command palette — fuzzy-matches every action by name.
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

- **Storage** — sender / mimetype / label rollups (`m` toggles to
  Largest Messages mode; `g` cycles `group_by` while in Breakdown).
- **Stale Threads** — threads waiting on a reply (`p` toggles
  perspective, `[`/`]` adjusts `older_than_days`, `{`/`}` adjusts
  `within_days`).
- **Contacts** — asymmetry vs decay (`m` toggles sub-mode, `R`
  refreshes the materialized contacts table).
- **Response Time** — reply-latency percentiles (`d` toggles
  direction).
- **Subscriptions** — list-sender ROI table (`o` toggles open-rate
  ranking; `u` opens the unsubscribe-confirm modal for the selected
  row).
- **Wrapped** — Spotify-style yearly summary as a 7-tile dashboard
  grid (`h`/`j`/`k`/`l` move between tiles, `y`/`Y` step year,
  `t` cycles window kind: YTD → Year → SinceDays).

Two cross-view interactions:

- `Enter` drills down. Storage senders/labels and Contacts emails
  jump to a Search filter; Stale Threads, Largest Messages, and
  Subscriptions rows open the underlying conversation directly via
  `Request::GetEnvelope` (no search round-trip).
- `f` opens the **filter modal** — a per-view form with all CLI
  flags exposed as editable fields. `Tab`/`Shift-Tab` to navigate,
  `Enter` to apply, `Esc` to cancel.

## Modals and overlays

- Command palette
- Help modal
- Label picker
- Compose confirmation — `[s]` sends, `[a]` schedules send-later,
  `[n]` sends and sets a follow-up reminder, `[d]` saves a draft,
  `[r]` refines, `[e]` reopens `$EDITOR`, `Esc` discards
- Bulk confirmation
- Attachment modal
- Snooze modal — preset list plus a **Custom…** entry that opens a
  text prompt parsed by the same `in 2h` / `tomorrow 9am` /
  `monday 17:00` / RFC3339 grammar as `mxr snooze --until`
- Unsubscribe confirmation
- Analytics filter modal
- Reply-later queue browser — list of flagged messages and due reminders,
  opened with `Ctrl-p → Reply Queue`; cancel a pending reminder from the
  focused sent message with `Ctrl-p → Cancel Reminder`
- Snippets browser — read-only list with body preview; CRUD flows
  through `mxr snippets`
- Sender profile — volume, cadence, open commitments, and other recent
  emails from the focused message's sender, opened with
  `Ctrl-p → Sender View`. Inside the modal, `j` / `k` selects another
  email and `Enter` / `o` opens it.
- Screener queue — triage list with `a`/`d`/`f`/`p` disposition keys
  (allow / deny / feed / paper-trail) wired to `Request::SetScreenerDecision`
- Thread summary — LLM-generated Markdown summary and next steps, opened with
  `Ctrl-p → Summarize Thread`
- Welcome / setup — first-launch modal with `d` (demo), `g` (Gmail),
  `i` (IMAP) shortcuts; `Enter` opens the new-account form
- Doctor findings — surfaced inside the Diagnostics Status pane with
  per-finding glyph (`✗` / `!` / `·`), category, message, and indented
  remediation commands

## Mailbox semantics

- Actions in thread view target the focused message
- Labels appear in message headers
- Attachment indicators appear in the mail list and message/thread header
- Thread counts are styled separately from sender names
- Label and saved-search scopes drive the mail list header and query state

## Discovery model

- First-run onboarding walkthrough
- Command palette: broad action surface
- Help modal: context-aware keybinding reference
- Hint bar: context-sensitive shortcuts, including selection-aware actions
- `gc`: edit config globally
