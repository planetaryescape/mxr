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
  --expected-workspace '<OWNER_JSON_FROM_PREVIEW>' \
  --operation-id OPERATION_ID_FROM_PREVIEW
mxr send-outcome OPERATION_ID_FROM_PREVIEW --account ACCOUNT_ID \
  --expected-workspace '<OWNER_JSON_FROM_PREVIEW>' --format json
```

The send command prints the operation ID and its outcome-query command to
stderr before submission. The preview returns `expected_workspace`, containing
the workspace ID and recovery generation. Keep that exact JSON with the
revision and operation ID, and pass it unchanged in single quotes to both send
and outcome queries. JSON and JSONL output remain on stdout.

If the selected daemon changes, do not replace the retained workspace JSON
with the new daemon's identity. A `workspace_conflict` means the reviewed work
still belongs to another workspace or recovery generation. Keep the private
draft and pending operation, return to their original workspace, and reopen or
review them explicitly before continuing. Do not submit the same private work
to the newly selected workspace.

The outcome query reports whether acceptance is known:

- **Accepted** means the provider accepted the message. Do not send it again,
  even if local Sent indexing has not finished.
- **Waiting** means mxr has proof that submission did not happen and retains
  your approved message for automatic retry. `recovery.retry_at` is the earliest
  retry time; the daemon checks every minute. No second Send press is needed.
- **NotAccepted** is positive evidence that this operation was not accepted.
  Review the current draft again before starting a new operation with a fresh ID.
- **Unknown** means acceptance may have happened. Do not resend, schedule, or
  try to clear it with `mxr drafts resume`; query the retained operation again
  or verify with the provider.
- **No outcome** means the daemon has no claim for that identity. If you retained
  the reviewed revision and operation ID, you may repeat that same submission
  with the same identity. Do not invent a new operation ID to retry an
  uncertain result.

An accepted or unknown operation remains tied to its original draft and
account, including after draft cleanup and daemon restart. Reusing the same
operation ID with `mxr send-outcome` retrieves its result. Reusing a reviewed
identity when no outcome exists preserves the same operation; a different ID
does not make an uncertain send safe to repeat.

## Wait through a provider cooldown

For saved-draft sends, Gmail's local cooldown, temporary token preparation
failures and explicit rate-limit refusals retain the same operation ID and
generated Message-ID. The approval includes the resolved sender, workspace,
revision, recipients, body and private, hashed copies of attachment bytes.
It survives daemon restart. Ordinary saved drafts are never enrolled by a
background scan.

Frozen approval copies have a 25 MiB per-file limit. Provider limits may be
lower, including limits on the whole message.

Portable backups keep canonical drafts and retained send outcomes, but exclude
runnable approvals, their frozen snapshots and operational attempt history.
Restored drafts stay held for explicit review; restoring a backup never resumes
the send queue.

Retries respect the provider's wait, exponential backoff and jitter, with at
most 12 attempts. Editing the draft, changing its workspace or sender, or
losing/corrupting a frozen attachment cancels the old approval. Review the
current draft before any new send. Query the retained result for its phase,
attempt count, deadline and reason:

```bash
mxr send-outcome OPERATION_ID --account ACCOUNT_ID \
  --expected-workspace '<OWNER_JSON_FROM_ORIGINAL_PREVIEW>' --format json
```

A timeout after possible submission remains `unknown`: mxr checks Gmail for a
Sent message with the exact Message-ID and sender, but never resubmits it.
An empty search does not prove rejection. A known acceptance can repair local
Sent indexing without sending again. SMTP/Outlook transport errors remain
uncertain when their submission phase cannot be established.

The web app shows retained operations under **Drafts → Send outcomes**. In the
TUI, use **Check last send outcome** in the command palette. Waiting is not
provider acceptance, and acceptance is not proof of recipient delivery.

Post-send reminders and archive-after-send are not part of the retained
approval. If a send waits, check acceptance before applying those actions
explicitly.

## Recover an older false-unknown cooldown record

First check whether you sent the message through another client. **Do not
resume the mxr operation if the follow-up was completed elsewhere.**

For a legacy Gmail operation, preview recovery using its original workspace and
operation ID:

```bash
mxr send-outcome OPERATION_ID --account ACCOUNT_ID \
  --expected-workspace '<OWNER_JSON_FROM_ORIGINAL_PREVIEW>' \
  --recover-known-not-submitted --dry-run --format json
```

Recovery requires a matching, timestamped structured daemon log proving that
the old local Gmail gate refused the call before dispatch, an unchanged
reviewed draft, the retained sender and Message-ID, and durable attachment
hashes. Arbitrary path-based legacy attachments cannot be recovered this way.
Missing logs, ambiguous transport evidence or an empty Sent search cannot
authorize a resend. The preview does not queue or send mail.

Only if `recoverable` is true and you still want that original approved send,
confirm the exact fingerprint returned by the preview:

```bash
mxr send-outcome OPERATION_ID --account ACCOUNT_ID \
  --expected-workspace '<OWNER_JSON_FROM_ORIGINAL_PREVIEW>' \
  --recover-known-not-submitted \
  --expected-evidence-sha256 EVIDENCE_SHA256_FROM_PREVIEW --format json
```

This retains the same identity as `waiting`; it does not erase uncertainty or
create a duplicate draft. The evidence fingerprint is kept with the local
approval. The TUI offers **Preview last send recovery**, then **Resume previewed
approved send**. The web Send outcomes row offers the same two steps. MCP
exposes `mxr_recover_send_outcome` with `dry_run` and
`expected_evidence_sha256`.

## Find an interrupted send

`mxr drafts recover` lists drafts whose state needs inspection. It does not
prove that a send was not accepted or make an uncertain send retryable.
Query the operation ID printed before the original submission:

```bash
mxr drafts recover
mxr send-outcome OPERATION_ID --account ACCOUNT_ID \
  --expected-workspace '<OWNER_JSON_FROM_ORIGINAL_PREVIEW>' --format json
```

For a legacy receipt with no retained operation ID, query the account-scoped
draft alias instead:

```bash
mxr send-outcome 'draft:DRAFT_ID' --account ACCOUNT_ID \
  --expected-workspace '<OWNER_JSON_FROM_ORIGINAL_PREVIEW>' --format json
```

Do not infer non-acceptance from an old `sending` status, a missing local Sent
message, or a restarted daemon. Only a durable `NotAccepted` result allows a
new, reviewed operation. When no outcome claim exists, replay only the exact
retained revision and operation ID. If an upgrade exposes a migration problem,
use a forward fix; older daemons cannot safely interpret the persisted outcome
records.

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
  --expected-revision REVISION_FROM_PREVIEW \
  --expected-workspace '<OWNER_JSON_FROM_PREVIEW>'
```

The scheduler assigns a separate operation identity and records its attempt
before sending. Query the scheduled draft by its account-scoped alias:
`mxr send-outcome 'draft:DRAFT_ID' --account ACCOUNT_ID --expected-workspace '<OWNER_JSON_FROM_SCHEDULE_PREVIEW>' --format json`.
Keep the exact `expected_workspace` JSON from the schedule preview and shell-quote it.
The ordinary send preview's ID does not identify a scheduled attempt, and the
global last-send query may still refer to another draft. An uncertain attempt
must be resolved by its retained send outcome; do not schedule it again under a
new identity. The [automated follow-ups guide](/guides/automated-followups/)
shows the scheduling flow.
