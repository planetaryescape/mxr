---
title: Crash-safe drafts
description: How mxr keeps in-flight sends safe across crashes, daemon restarts, and provider hiccups.
---

mxr writes every draft to local SQLite before it ever crosses the
network. The send pipeline tracks lifecycle state explicitly so a
crash, a missed acknowledgement, or a provider timeout can never lose
your draft — and never silently double-send it either.

## Lifecycle states

Drafts move through three states:

- `'draft'` — being edited, ready to send.
- `'sending'` — handed to the provider; a send is in flight.
- `'sent'` — provider returned a receipt; the draft row is removed
  from the live table after the synthetic Sent envelope is ingested.

The `'sending'` state is the dangerous one. If the daemon dies between
"call the provider" and "receive the receipt", the draft sits in
`'sending'` forever — which would prevent retries (the CAS check
refuses to re-send a draft that's already in flight) without recovery.

## Heartbeats and the 1h cutoff

While a draft is in `'sending'`, the daemon writes a heartbeat
timestamp on the row. The send pipeline updates the heartbeat right
after the CAS-to-`'sending'` transition (mirrored by the live retry
path) so any actually-running send keeps a fresh timestamp.

On daemon startup, the recovery loop scans for drafts in `'sending'`
whose most recent activity (`last_heartbeat_at`, falling back to
`status_updated_at`) is older than **one hour**. Those rows are
auto-reset to `'draft'` so the user can retry through the normal send
pipeline. The choice of 1h is generous on purpose — even a slow OAuth
refresh or a multi-megabyte attachment never takes that long, so a
stale row is a real orphan, not a slow legitimate send.

## Acting earlier from the CLI

You don't have to wait for the startup loop. The CLI exposes the same
state directly:

```bash
mxr drafts recover                # list orphaned 'sending' drafts
mxr drafts resume DRAFT_ID        # force-reset to 'draft' so you can retry
mxr drafts discard DRAFT_ID       # permanently delete the draft
```

`recover` shows what the startup loop *would* reset, only available
immediately. `resume` is idempotent — running it on a draft already in
`'draft'` is a no-op. `discard` is the path you take when a recovered
draft is no longer wanted.

After `resume`, the draft is back in the normal flow:

```bash
mxr send DRAFT_ID
mxr send DRAFT_ID --dry-run       # preview before re-sending
```

## Why CAS, not just retry-on-fail

The send pipeline uses a compare-and-set transition (`'draft'` →
`'sending'`) so two simultaneous send attempts on the same draft can't
both invoke the provider. Only the unique transition is allowed
through; the second attempt fails fast with `draft is already being
sent`. After the provider returns, the draft moves to `'sent'` — also
via a status update, never via duplicate work.

This makes the worst case "draft sits in `'sending'` until recovery"
rather than "draft gets double-sent after a crash."

## Idempotent provider sends

The send pipeline persists the rendered `Message-ID` header before
calling the provider. If the next sync sees a message with the same
`Message-ID` we already attempted, mxr's IMAP/Gmail dedupe path skips
it. The combination — local CAS state + stable Message-ID — means a
recovered-and-resumed draft can be resent without producing two copies
on the wire even if the original send actually completed but the
acknowledgement was lost.

## Scheduled sends are at-most-once, and losses are surfaced

A scheduled send (`mxr send <draft> --at ...`) clears its `send_at`
*before* the daemon calls the provider, so a crash can never make it
re-fire on every tick. The trade-off is a narrow window — daemon dies
between clearing the schedule and the provider accepting — where the
send could be silently lost.

mxr makes that window visible. Clearing `send_at` and writing a
send-attempt marker happen in a single transaction; the marker's outcome
is resolved (`sent` / `blocked` / `failed`) once the send returns. If the
daemon restarts and finds an attempt with no recorded outcome, it logs a
warning and records an event (visible in `mxr events` / `mxr doctor`)
telling you a scheduled send may not have completed, so you can verify
and resend. Scheduled sends stay at-most-once; you just never lose one
silently.
