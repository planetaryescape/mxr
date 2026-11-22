# Invite chord arms and the message-view card hint must change together

## Where

- `crates/tui/src/app/input.rs`: the `i` prefix arms (`ia`/`im`/`id` auto-confirm with a one-second undo; `iA`/`iM`/`iD` open compose with a comment).
- `crates/tui/src/ui/message_view.rs`: the invite card hint text names the same chords.

## Problem

A comment told maintainers to keep the two in sync. Nothing checks it.

## Smallest fix

Build the card hint from the key registry, or add a test that every chord named in the hint resolves to the invite action.
