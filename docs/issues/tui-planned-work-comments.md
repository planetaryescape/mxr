# TUI comments named work that was never done

## Where

- `crates/tui/src/app/message_actions.rs`, the Owed lens arm: "no per-row open affordance for now; a future slice can wire enter to the underlying thread". The `MailboxView::Owed` arm of the open-row handler is empty.
- `crates/tui/src/action.rs`, `Action::OpenReplyQueue`: opens "once the Tantivy operator `is:reply-later` lands; today opens via CLI".
- `crates/tui/src/app/state/arrivals.rs`, `not_sure_hint_shown`: "each hint once per session; moves to the daemon's hint-seen state when fix/contextual-hints lands". `SetHintSeen` exists, so check whether this is stale.
- `crates/tui/src/app/state/mailbox.rs`, the `MailboxView::CalendarInvites` variant, whose doc called it groundwork for future calendar/event features.

## Problem

Each comment promised a follow-up. None has an owner or a date. A reader cannot tell whether the behaviour is final.

## Smallest fix

Decide each one: build it, or confirm the current behaviour is final and say so in the docs. For the arrivals hint state, check whether `SetHintSeen` already covers it.
