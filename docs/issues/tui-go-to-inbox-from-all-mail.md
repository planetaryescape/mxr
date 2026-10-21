# `g i` and `g s` do nothing from All Mail on the fake provider

Observed 2026-10-21 in a real PTY against a fake-provider daemon, on both the published v0.6.118 binary and the G03 candidate: after `g a` (All Mail), pressing `g i` or `g s` leaves the list on All Mail. Evidence: `.orchestrate/mxr-blueprint-gaps-20261021/g03-probe-base-evidence/` (outside the repo).

`Action::GoToInbox` (`crates/tui/src/app/mailbox_actions.rs`) looks for a label whose `name` is `"INBOX"`. The fake provider's Inbox label is named `Inbox` with provider id `INBOX` (`crates/provider-fake/src/fixtures.rs:132`), so the lookup misses and the code falls back to `desired_system_mailbox`, which did not navigate in this state. The same happens for STARRED.

Not yet checked: whether Gmail or IMAP accounts hit the fallback too (that depends on their label display names), and why the fallback does not navigate. Settle it by matching on provider id or `LabelKind::System`, plus a test from All Mail.
