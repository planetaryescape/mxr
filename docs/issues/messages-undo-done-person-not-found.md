# Undoing a Done in Messages once showed "Couldn't find this person"

Status: open, seen once. Found 2026-10-28 while rewriting the Messages e2e journey (branch `test/e2e-high-signal-only`).

## What happened

One local Chromium run marked a conversation Done in Messages, then pressed `u`. The daemon restored the conversation, and the toast said "Restored: Reply acceptance". But the page showed "Couldn't find this person" where the restored conversation should have been.

The same journey passed in the three other full runs and in 10 repeated runs (`--repeat-each=5` for that test and the edited triage test).

## What's known

- The daemon state was right: the e2e test now asserts the restored attention through the daemon, and that passes.
- The empty state is the person view's not-found copy. So for a moment the page asked for a person the daemon didn't return. That could be a stale route parameter after the undo, or a race between the undo landing and the person query.

## What would settle it

Reproduce it with the person query and the undo's response logged in order: run the Messages journey with `--repeat-each=20` and keep the trace on failure. If it's a race, the person view should refetch, or wait, after an undo that touches the open person.
