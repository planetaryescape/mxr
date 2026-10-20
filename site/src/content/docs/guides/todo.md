---
title: Act on To dos
description: Review task dates, finish work, and schedule what can wait.
---

Open To do to act on what an email asks you to do. Rows show an action title,
the counterparty, and any extracted amount or dates, so you can decide what
to do before opening the full email.

```bash
mxr todo
mxr todo --format json
```

In the web app and TUI, press `g x`. The runway groups tasks into `Now`,
`Coming up`, `Later`, `Whenever` and `Done this week`. A web row is a
checkbox, the task, who it is from ("Frank Billing · from Mon", or "from you"
for a promise you made) and its window in words ("act by Fri"). The window
turns amber on the day and after. The lit row's keys, `Space` done and `o`
email, take the window's place on the same line, and the same keys show in
the status bar with the reason the task is here. The act-by date is the
last date to act and still meet the due date; mxr can surface the task earlier
to give you time. For a bank-transfer bill, act-by is three working days before
the due date. Use a row's ID from the JSON output for the CLI commands below.

## Check the source and act

```bash
mxr todo why TODO_ID --format json
```

Inspect where the title, amount and dates came from. In the web app, press
`o` to open the source email beside the list, `v` for why the row is here and
where each field came from (a field mxr wasn't sure of reads "unchecked"),
or `Enter` for the selected row's primary action (the email with its link
marked, or a reply to a promise). Check extracted details against that source
before relying on them. Use the
row's edit control, or `,` on the web, to correct what mxr read.

Bills, failed payments, sign or verification requests, promises and annual
renewal reviews can become tasks automatically. An alert in Updates offers
`t` to create a task when you choose; it does not automatically create one
merely because it appears in `Needs a look`.

## Add or correct a task yourself

```bash
mxr todo add --from MESSAGE_ID --title "Send the signed form" --due "in 2d" --dry-run
mxr todo add --from MESSAGE_ID --title "Send the signed form" --due "in 2d"
mxr todo edit TODO_ID 'title=Send the completed form' 'due=in 3d' --dry-run
mxr todo edit TODO_ID 'title=Send the completed form' 'due=in 3d'
```

Use a source message ID from search or the reader for `--from`. The title
names the action, not the email's subject. `edit` accepts `title`, `due`,
`amount`, `counterparty` and `kind` as `field=value`; an empty value clears
due, amount or counterparty. A task you create or edit is yours: it no
longer expires automatically. Check the preview before applying either
change.

## Review the first-run catch-up

```bash
mxr todo catchup --format json
mxr todo catchup --keep TODO_ID --dry-run
mxr todo catchup --keep TODO_ID
mxr todo catchup --let-go TODO_ID --dry-run
mxr todo catchup --let-go TODO_ID
```

The first run offers older undated tasks in a bounded catch-up rather than
putting every historical task on your runway. Keep the ones that still
matter, or let them go to the Expired list without archiving their email.
Use `--undecide TODO_ID` to return a decision to this review, previewing it
with `--dry-run` first. `--let-go-all --dry-run` previews the remaining
batch before `--let-go-all` applies it.

## Tick off finished work

```bash
mxr todo done TODO_ID --dry-run
mxr todo done TODO_ID
```

On the web, use the checkbox on the left, or light the row and press
`Space` (`e` works too, as in the TUI). The row folds away and the next one
lights. In the TUI, `e` ticks it off. App shortcuts do not interpret
ordinary typing in a text input or composer as task actions.

To reopen a task you completed by mistake:

```bash
mxr todo undo TODO_ID --dry-run
mxr todo undo TODO_ID
```

## Bring a task back later

```bash
mxr todo snooze TODO_ID "in 3d" --dry-run
mxr todo snooze TODO_ID "in 3d"
```

Scheduling changes when the task appears; it leaves the due date intact.
On the web, press `Z` to schedule the selected task. Use
`mxr todo snooze TODO_ID --clear --dry-run` to preview returning to the date
mxr calculated, then repeat without `--dry-run` to apply it.

## Link a task with Microsoft To Do

```bash
mxr todo microsoft status --format json
mxr todo microsoft link TODO_ID --dry-run --format json
mxr todo microsoft link TODO_ID --fingerprint REVIEWED_FINGERPRINT --format json
```

Install the optional [ms-todo CLI](https://github.com/planetaryescape/ms-todo#installation)
on the machine running the Mixer daemon and sign in with `ms-todo auth login`.
Use a current schema-2 CLI with defer-date support. Mixer checks that contract
before creating a task and refuses older output rather than clearing a schedule.
The web app only shows its **Microsoft To Do** button when that CLI is on the
daemon's PATH. Open a row's **details**, choose **Microsoft To Do**, review the
title, dates and outgoing context, then choose **Link and sync both ways**.
Without the CLI, details offers an installation hint instead.

In the TUI, select a task and press `M` to preview the link, then `M` again to
confirm it (`Esc` cancels). These controls only appear when the CLI is installed.
Only tasks you explicitly link participate; Mixer does not import your other
Microsoft tasks.

For the CLI, review the dry run and copy its `fingerprint` into `--fingerprint`.
Linking without that value is refused. If the task changed, review a new dry run;
the link command never silently substitutes a newer preview. `--account ACCOUNT`
restricts both linking and bulk sync, and a task ID must belong to that account.
In the TUI, the link review takes over the runway: `j`/`k` or Page Up/Down scroll
long context without changing the selected task. An unusably small terminal
cannot confirm the link; enlarge it or press `Esc` to cancel.

After linking, the daemon checks for changes every 30 seconds, in bounded
batches. Title, due date, completion and reopening sync in either direction.
Scheduling syncs with ms-todo's **defer date**, which does not hide the task in
Microsoft's phone app and is not its My Day. Dates use the daemon's local time
zone; Microsoft dates are day-only. An unchanged day preserves Mixer's existing
time; a changed schedule uses Mixer's configured morning hour.

Microsoft-only notes, reminders, importance, steps, categories and list membership
are not overwritten by field edits. Completing or reopening still has Microsoft's
normal reminder behavior. The initial link sends the context shown in the
preview, not the full email or its attachments. Mixer-specific amounts and field
provenance remain in Mixer.

```bash
mxr todo microsoft sync TODO_ID --dry-run --format json
mxr todo microsoft sync TODO_ID --format json
```

Omit `TODO_ID` to check every linked task; with `--account`, only that account
participates. External MCP/agent profiles
with no explicit account check only their allowed enabled accounts.
A sync preview reads ms-todo's cache; a real sync asks ms-todo to refresh first.
In the web details, use **Preview sync**
or **Sync now**. In the TUI, `S` previews the selected link and a second `S` syncs it.

If both apps changed the same field differently, sync pauses and reports the
conflict. Set the field to the same value in both apps, then sync again. This
comparison uses ms-todo's current cache; an edit made on another device after
the refresh remains subject to ms-todo's own conflict handling. A recurring
Microsoft task pauses the link because Mixer cannot represent its occurrences.

Offline writes wait in ms-todo's outbox. If a handoff or write is **unconfirmed**,
do not create another task or retry the write. Inspect
`ms-todo outbox list --format json` and resolve it there. Mixer keeps a durable
receipt to prevent repeated clicks or restarts from creating a duplicate.
A deleted or missing Microsoft task pauses the link; it is not recreated, and
the Mixer task is not deleted.

## Review later evidence separately

```bash
mxr todo resolve TODO_ID SUGGESTION_ID preview
```

Use `resolution.id` from a task's JSON as `SUGGESTION_ID`. A later email can
produce a `Looks resolved` or revised-date offer with an exact quote. The
task stays unchanged until you accept it. Follow
[Review later evidence](/guides/todo-resolution/) to inspect, accept,
dismiss or undo that offer.

## See also

- [To do command reference](/reference/cli/todo/)
- [Annual renewal reviews](/guides/archive/#subscriptions-are-receipts-that-recur)
- [Keyboard reference](/reference/keybindings/#to-do)
- [Review later evidence](/guides/todo-resolution/)
