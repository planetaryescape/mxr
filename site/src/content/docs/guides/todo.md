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
`Coming up`, `Later`, `Whenever` and `Done this week`. The act-by date is the
last date to act and still meet the due date; mxr can surface the task earlier
to give you time. For a bank-transfer bill, act-by is three working days before
the due date. Use a row's ID from the JSON output for the CLI commands below.

## Check the source and act

```bash
mxr todo why TODO_ID --format json
```

Inspect where the title, amount and dates came from. In the web app, press
`o` for the source email or `Enter` for the selected row's primary action.
Check extracted details against that source before relying on them. Use the
row's edit control, or `,` on the web, to correct what mxr read.

Bills, failed payments, sign or verification requests, promises and annual
renewal reviews can become tasks automatically. An alert in Updates offers
`t` to create a task when you choose; it does not automatically create one
merely because it appears in `Needs a look`.

## Tick off finished work

```bash
mxr todo done TODO_ID --dry-run
mxr todo done TODO_ID
```

On the web, use the visible checkbox on the left, or select the row and press
`Space` or `e`. In the TUI, `e` ticks it off. App shortcuts do not interpret
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
