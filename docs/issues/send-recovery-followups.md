# Send recovery keeps uncertainty explicit while diagnostics improve

These are bounded follow-ups to the saved-draft send outcome contract at
`3b5648422e1d284cf369e54c1dfae99052ac93da`. They do not weaken the current
rule: `unknown` remains uncertain and must not trigger a retry. No release
promise or implementation change is implied.

## Distinguish proven pre-submission failures where the adapter can

The Gmail provider maps every error returned by `GmailClient::send_message` to
`Unknown` (`crates/provider-gmail/src/provider.rs`). That client awaits
`auth_header()` before calling HTTP `.send()` (`crates/provider-gmail/src/client.rs`),
so a failure in that step is known to occur before the request, but currently
shares the conservative classification with errors from the send request and
response handling. A focused adapter change could preserve definite
pre-submission evidence when the boundary is explicit.

SMTP and Outlook map `transport.send()` errors to `Unknown`
(`crates/provider-smtp/src/lib.rs`, `crates/provider-outlook/src/smtp.rs`). Do
not classify DNS, connection, or authentication failures from broad error text;
these call sites do not establish which phase completed. Keep them uncertain
unless the transport exposes precise phase evidence.

## Preserve diagnostics after a scheduled attempt

The scheduled-send loop clears `send_at` before attempting delivery and records
the attempt outcome (`crates/daemon/src/loops.rs::process_due_scheduled_sends`).
The pending-schedule query lists only rows with draft status and non-null
`send_at` (`crates/store/src/scheduled_sends.rs::list_pending_scheduled_sends`).
After a failed or uncertain attempt, the draft can therefore disappear from
the pending-sends view even though attempt/outcome evidence remains elsewhere.

S08c should decide how to surface this history and notify users without
suggesting a resend for `unknown`. Until then, keep the retained outcome query
as the recovery path; do not infer failure from a missing pending row.

## Recovery can lose context across later edits and failures

The following findings are pinned to review candidate
`7413df8716410b67b75c6051c920aa6e2060551a`. Recheck them against the candidate
that includes the TUI save-to-server data-loss fix before prioritizing; they do
not change the saved-send rule that `unknown` must not be retried.

- The TUI recognizes a committed update only when stored content still matches
  the submitted content (`crates/tui/src/send_recovery.rs::prepare_reviewed_send`).
  If an `UpdateDraft` reply is lost and the user edits again, the newer text
  remains in the compose file, but reopening it alone does not resolve the stale
  revision. Recovery stores no pending content hash.
- Each browser send adds an operation reference to local storage, and every
  retained row queries its outcome
  (`apps/web/src/features/drafts/sendOutcomes.ts::retainSendReference`,
  `apps/web/src/features/drafts/OrphanedDrafts.tsx`). Any future UI retention
  limit must preserve operation IDs needed to resolve uncertain sends.
- The TUI has one in-memory `failed_send` slot, which a later failure can
  replace. The recovery file separately retains compose paths and can reopen an
  earlier file (`crates/tui/src/app/state/compose.rs`,
  `crates/tui/src/send_recovery_state.rs`).
- The `draft:` alias query orders by `updated_at DESC, rowid DESC`
  (`crates/store/src/draft_send_outcomes.rs`). A backward clock step can make
  it show an older outcome; the send claim still blocks a duplicate operation.
- A malformed `tui-send-recovery.json` prevents reading the retained identity
  and can block TUI send recovery. The current error does not name the file or
  give repair guidance (`crates/tui/src/send_recovery_state.rs`,
  `crates/tui/src/runner.rs`).
- Outcome lookup distinguishes an operation belonging to another account
  (“not found for this account”) from a missing operation (no outcome)
  (`crates/daemon/src/handler/mutations.rs::get_send_outcome`).
- An unknown or already accepted outcome can appear under the web toast
  “Cannot retain send identity,” even when identity retention succeeded
  (`apps/web/src/features/compose/session/useComposeSend.ts`).
- If persisting `not_accepted` fails, the error bubbles from
  `reject_draft_send`; the transaction leaves the outcome `unknown`, but the
  surfaced message does not explain that uncertainty
  (`crates/daemon/src/handler/mutations.rs`,
  `crates/store/src/draft_send_outcomes.rs`).

Mail-based unsubscribe and calendar RSVP call providers outside the saved-draft
send path (`crates/daemon/src/handler/mutations.rs`,
`crates/daemon/src/handler/mailbox.rs`). They need separate review before any
broader send-outcome coverage claim.
