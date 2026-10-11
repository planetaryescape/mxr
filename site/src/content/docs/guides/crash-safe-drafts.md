---
title: Crash-safe drafts
description: How to review and recover a send after a disconnect or daemon restart.
---

Review a saved draft before sending, and keep its operation ID until the
provider outcome is known. mxr records a durable outcome before calling the
provider so a lost response does not become an automatic retry.

## Review and send a saved draft

Run a dry-run preview, then submit the exact revision and operation identity it
returns:

```bash
mxr send DRAFT_ID --dry-run --format json
mxr send DRAFT_ID --expected-revision REVISION_FROM_PREVIEW \
  --operation-id OPERATION_ID_FROM_PREVIEW
mxr send-outcome OPERATION_ID_FROM_PREVIEW --account ACCOUNT_ID --format json
```

The send command prints the operation ID and its outcome-query command to
stderr before submission. Keep that ID if the connection fails. JSON and JSONL
output remain on stdout.

The outcome query distinguishes three results:

- **Accepted** means the provider accepted the message. Do not send it again,
  even if local Sent indexing has not finished.
- **NotAccepted** is positive evidence that this operation was not accepted.
  Review the current draft again before starting a new operation with a fresh ID.
- **Unknown** means acceptance may have happened. Do not resend, schedule, or
  try to clear it with `mxr drafts resume`; query the retained operation again
  or verify with the provider.

An accepted or unknown operation remains tied to its original draft and
account, including after draft cleanup and daemon restart. Reusing the same
operation ID with `mxr send-outcome` retrieves its result; a different ID does not make
an uncertain send safe to repeat.

## Find an interrupted send

`mxr drafts recover` lists drafts whose state needs inspection. It does not
prove that a send was not accepted or make an uncertain send retryable.
Query the operation ID printed before the original submission:

```bash
mxr drafts recover
mxr send-outcome OPERATION_ID --account ACCOUNT_ID --format json
```

Do not infer non-acceptance from an old `sending` status, a missing local Sent
message, or a restarted daemon. Only a durable `NotAccepted` result allows a
new, reviewed operation.

In the TUI, close the send error to return to the same compose review. After
restarting the TUI, use `Ctrl-p` and **Reopen last send draft** to open the
retained file in your editor. **Check last send outcome** queries the retained
operation. Sending again checks that outcome first; an accepted or uncertain
send stays blocked.

## Schedule a reviewed draft

Scheduling also requires the revision from a preview. Later edits do not
silently change the scheduled message:

```bash
mxr send DRAFT_ID --dry-run --format json
mxr send DRAFT_ID --at "tomorrow 9am" \
  --expected-revision REVISION_FROM_PREVIEW
```

The scheduler assigns a separate operation identity and records its attempt
before sending. Query the latest attempt with
`mxr send-outcome 'draft:DRAFT_ID' --account ACCOUNT_ID --format json`.
The ordinary send preview's ID does not identify a scheduled attempt.
An uncertain attempt must be resolved by its retained send outcome; it is not safe to schedule the draft
again under a new identity. The [automated follow-ups guide](/guides/automated-followups/)
shows the scheduling flow.
