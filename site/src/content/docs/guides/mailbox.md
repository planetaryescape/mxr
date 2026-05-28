---
title: Mailbox workflow
description: How to navigate, triage, and work through mail in the mxr TUI.
---

## Mailbox model

The mailbox screen is the default TUI workspace:

- Left: sidebar
- Center: mail list
- Right: preview, single-message, or thread view

The list is thread-first by default. mxr can also show message rows, but the default is one row per conversation.

## Sidebar

The sidebar has three sections:

- System labels
- User labels
- Saved searches

Selecting a label or saved search changes the active mailbox scope. The mail-list header reflects the current scope.

## Mail list

A thread row shows:

- Unread/star state
- Sender summary
- Subject and snippet
- Date metadata
- Attachment marker when any message in the row has attachments
- Link marker (`🔗`) when the body contains external links. A muted glyph
  means "has some links"; a brighter/accent-coloured glyph means "link-heavy"
  (newsletter-shaped). Trackers / unsubscribe URLs / list-management hostnames
  are filtered out, so the marker reflects useful links — calendar invites,
  shared docs, receipts, video calls — rather than every embedded image
  tracker. Filter for these in search with `has:link`, `has:link-heavy`, or
  `has:link-none`.
- Thread count with distinct styling when the conversation has multiple messages

Long subjects are truncated so date and metadata remain visible.

## Open and close the right pane

- `Enter` or `o`: open selected row
- `Esc`: dismiss the right pane back to the two-pane layout
- `Tab`: switch panes
- `F`: toggle fullscreen

## Thread view

Inside a thread:

- `j` / `k` move the focused message
- Reply, reply-all, forward, archive, label, snooze, and other actions target the focused message
- Message headers show label chips and attachment metadata
- Reader mode and attachment actions work from the focused message

## Body loading

Message bodies are synced eagerly with envelopes, so opening a synced
message in the TUI should read from local SQLite. The TUI batches body
loads through the daemon's `ListBodies` path; that path does not repair
missing rows by calling the provider while you wait.

```bash
id="$(mxr search 'label:inbox' --format ids --limit 1)"
mxr cat "$id" --view reader
```

What you get: the same cached body content the TUI expects to render,
with reader mode applied.

If a body row is missing, that is local-store drift rather than the
normal opening flow. The single-message CLI read path may repair a
missing body, but the TUI preview path stays local so mailbox navigation
does not turn into a hidden network repair job.

## Search in the TUI

Two search flows:

- `/`: jump into Search and query the full local index
- `Ctrl-f`: quick filter for the current mailbox only

- From Search:

- Results search every synced account and label, not just what is loaded on screen
- `Enter`, `o`, or `l` opens the selected result in the Search preview
- `Esc` moves preview -> results -> mailbox

## Selection and bulk actions

- `x`: toggle one row into selection
- `V`: visual line selection mode
- `Esc`: clear selection

When selection is active:

- Hint bar switches to selection-aware actions
- Command palette still works
- Destructive or broad mutations use a confirmation modal first

Common bulk actions:

- Archive
- Trash
- Spam
- Mark read/unread
- Star
- Apply label
- Move to label

## Mutation feedback

The TUI applies mailbox mutations optimistically: rows move, flags flip,
and labels update before the daemon reply returns. Transient daemon or
SQLite pool failures are retried briefly before the UI reconciles.

```bash
mxr archive --search 'from:noreply older:30d' --dry-run
mxr archive --search 'from:noreply older:30d' --yes
```

What you get: a dry-run count first, then an explicit mutation that can
retry transient failures and still reports a real failure if retries are
exhausted.

Preview auto-mark-read is quieter because it is background behavior. It
retries transient failures too, but if it cannot land, the mailbox
refreshes without a blocking mutation-failed modal.

## Attachments

When a message has attachments:

- The mail list shows an attachment marker
- The thread/message header shows attachment info
- `A` opens the attachment modal
- `Enter` / `o` opens the selected attachment
- `d` downloads it

## Snooze

`Z` opens the snooze modal with presets such as tomorrow morning, tonight, weekend, and next Monday.

Snoozing is local-first but also updates provider state where supported. For Gmail that means removing `INBOX` when snoozed and restoring it when the snooze wakes.

## Help and discovery

- `?`: help modal with all keybindings
- `Ctrl-p`: command palette
- `o` from Help: reopen the onboarding walkthrough

The help modal is context-aware. The command palette exposes mailbox, search, rules, diagnostics, account actions, config edit, and logs.

## In real life

- **First thing in the morning:** open mxr (`mxr`), press `g i` for
  Inbox, scan with `j/k`, `e` to archive, `b` to bookmark for reply
  later, `Z` to snooze. You'll be at inbox-zero before your coffee.
- **Backlog cleanup on a slow Friday:** `mxr stale --mine
  --older-than-days 30 --format ids | xargs -n1 mxr cat | $PAGER` —
  scan everything you've been ignoring, then bulk-archive what you
  decide to drop.
- **Pin an Owed lens to your sidebar:** `mxr saved add owed
  'is:owed-reply'`. The lens lists threads where you're the bottleneck,
  ranked by overdue score. Same set as `mxr owed`; whichever surface
  you prefer.
- **Switching contexts mid-day:** `g 1`–`g 9` jump straight to your
  saved-search lenses. Set up "VIP", "Today", "Waiting on me", and
  hop between them with one keystroke.

## Agent prompts that work

```text
"What's in my inbox right now? Group by sender, count unreads, and
show me the noisiest 5. Use `mxr search 'is:unread' --format json | jq`."
```

```text
"Help me hit inbox zero. For each unread, suggest archive / reply
later / snooze and explain why. Read with `mxr cat --view reader`,
classify in batches of 10. Don't actually mutate yet — I'll approve
each batch."
```

## See also

- [Unsubscribe](/guides/unsubscribe/)
- [Triage flow](/guides/triage-flow/)
- [Search workflow](/guides/search/)
- [Recipes — interactive pickers](/guides/recipes/#with-fzf--interactive-picker)
- [Keybindings reference](/reference/keybindings/)
