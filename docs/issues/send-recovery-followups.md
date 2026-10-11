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
