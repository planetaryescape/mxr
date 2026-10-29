# A bill's to-do changes identity when its due date crosses a month

Status: open, needs a decision. Found 2026-10-29 while fixing the clock dependence in `handler::tests::todo::html_evidence`.

## What happens

`mxr_todo::pass::stored_record` keys a bill's to-do by its due month: `asks|payment|<fingerprint>|due:%Y-%m`. If a rescan of the same email finds a due date in a different month, for example a correction that moves the due date from 31 October to 1 November, the rescan:

- creates a new to-do row,
- retires the old one,
- and loses any snooze, edit or note on the old to-do.

## Why it's built that way

The code comment says "next month's bill is a new one". The month in the key keeps a monthly bill from one sender as separate to-dos, so it's deliberate. The month-boundary move for the same email looks unplanned.

## Options

- Key by the source email as well as the month, so a re-dated bill from the same email keeps its to-do and only a new email starts a new one.
- Keep the key as it is, and carry snooze and edits over when a rescan of the same email retires a to-do for a new month.
- Leave it. It's rare, because it needs a due date that moves across a month boundary on rescan.

The test now pins its clock to 2026-10-26 (commit b3165134d), so it no longer fails on dates whose 3-days-out falls on the 1st of a month.
