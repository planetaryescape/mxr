# Backfilled mail triggers background model calls

**Status:** fixed (D194) · **Found:** 2026-11-25, checked at `origin/main` `96e8785f5` · **Severity:** medium

A fresh install downloads the whole mailbox, which is fine. But three background jobs then send that old mail to the model, as if it had just arrived. With a ten-year mailbox and a hosted model, that means a large bill or a spent plan limit, and hours of calls, for mail nobody is reading.

BK's rule (2026-11-25): sync everything, but background inference runs only on mail that arrives live. Backfilled mail gets inference only when the user asks for it.

## Where backfilled mail reaches the model

- **Relationship summaries.** During the initial backfill, each page's contacts are collected (`crates/daemon/src/loops.rs:595-614`). When backfill ends, the whole set is queued for summaries (`loops.rs:636-656`). Every contact in the mailbox who passes the history threshold gets a model call (`crates/relationship/src/service.rs:210`, `:242`).
- **Delivery extraction.** `scan_messages` runs on every synced message, backfill pages included (`crates/daemon/src/handler/deliveries.rs:77-94`). Each shortlisted message goes to the model when `llm.enabled` is on (`deliveries.rs:185-205`). It checks no age and no arrival kind.
- **"Looks resolved" checks.** The to-do first run, which goes over all history, passes its scanned message ids to `todo_resolution::scan_messages` (`crates/daemon/src/handler/todos.rs:141`), as does the post-sync scan (`todos.rs:71`). Open to-dos from old mail are then queued for model checks.

Other model features run on demand (summaries, drafts, ask, the assistant, briefings), or have no background caller (decision log, contact commitments), so they are not affected.

## The signal already exists

The sync engine marks each upsert live only when the account's initial sync is done and the cursor is not a backfill cursor (`crates/sync/src/engine.rs:386-397`). The flag is stored as `message_arrivals.is_live` (migration `111_message_arrivals.sql:16`). Alert to-dos already use it to hold backfilled mail (`crates/updates/src/trust.rs:135`, `Held::Backfill`).

## Change

- Gate the three jobs on arrival kind: background inference runs only for messages with `is_live = 1`.
  - Relationship handover at backfill end: hand over no contacts. A contact gets its first summary when live mail from or to them arrives, or when the user asks.
  - Delivery extraction: still run the heuristics on backfilled mail, but skip `extract::enrich` unless the message is live or the call is forced (`force_llm`, used by `mxr deliveries scan`).
  - To-do resolution: queue model checks only for to-dos whose evidence message is live. Rule-based resolution can still run.
- Keep the on-demand paths for older mail:
  - `mxr deliveries scan --since-days N` already exists.
  - `mxr decisions rebuild --since N` already exists.
  - `mxr profile <email> --rebuild` already exists, for one contact.
  - Missing: a bulk relationship rebuild, for example `mxr relationships rebuild --since 30d`, with `--dry-run` showing the number of contacts and calls.
- Show it. `mxr llm status` (and its JSON) should say that backfilled mail is excluded, and how many inputs were skipped because they were backfilled.

## Edge cases to decide

- **A restored workspace.** Backup copies `message_arrivals` (`crates/store/src/backup.rs:53`), so restored mail keeps the flags it had. Mail synced after activation is live.
- **Mail from before migration 111.** Its `is_live` defaults to 0, so it counts as backfill. That is the safe side.
- **A re-sync after re-authorisation or a cursor reset.** If the provider hands back a backfill cursor, that mail counts as backfill, which is the safe side.
- **Day one feels empty.** With live-only, a new install has no relationship summaries until mail flows. An optional window for recent backfilled mail, such as `llm.backfill_window_days` (default 0), would cover this. BK first suggested 30 days, then said the concern is inference on arrival of old mail, so the default stays 0 until BK decides.

## Done when

On a fresh install with `llm.enabled = true` and a counting model endpoint, a full backfill makes zero background model calls. A live message that arrives afterwards gets its delivery, relationship and to-do checks as today. Each on-demand command above makes exactly the calls its `--dry-run` predicted.

## Fix

Decision D194 in `docs/blueprint/15-decision-log.md` records the rule.

- **The signal is a new column, not `is_live`.** `is_live` also requires unread inbound Inbox mail, because alert to-dos need that. Gating on it would drop mail you send and mail read on your phone before mxr synced it, so those would lose their checks. Migration 115 adds `message_arrivals.arrived_live`: set when a live page stores a message for the first time, never changed later. It starts as 1 for rows already marked `is_live`.
- **Each job checks it where it would call the model.**
  - Delivery extraction (`crates/daemon/src/handler/deliveries.rs`): the post-sync scan skips `extract::enrich` for mail that did not arrive live. `mxr deliveries scan` and the re-run after an outage still send it.
  - Relationship summaries (`crates/relationship/src/summary.rs`): a background summary waits until the contact has mail that arrived live. The handover at backfill end still queues every contact, so the style profile (no model) still builds. `force` (the explicit rebuilds) skips the wait.
  - To-do resolution (`crates/daemon/src/handler/todo_resolution.rs`): queues a model check only when the later email arrived live. Rules still run.
- **On demand.** `mxr relationships rebuild --since N [--dry-run]` (`RebuildRelationshipProfiles`, bridge `POST /mail/relationships/rebuild`) rebuilds every contact with mail in the window. Its dry run and `mxr deliveries scan --dry-run` report `model_calls`, and a real run reports the calls it made.
- **Status.** `mxr llm status` prints that background AI is live-only and the inputs held back since the daemon started (`background_live_only`, `backfill_held` in JSON).

Proof: `crates/daemon/src/loops/live_only_inference_tests.rs` drives two backfill pages and one live page through the sync loop with a counting model: zero calls through the backfill, then one delivery call and one relationship call for the live mail. `crates/daemon/src/handler/tests/backfill_on_demand.rs` checks that the relationship rebuild and the delivery scan each make exactly the calls their dry runs predicted. `backfilled_mail_queues_no_model_checks_and_counts_each_pair_held_once` covers to-do resolution.

A backup from before schema 115 gets the same repair as the migration: rows marked `is_live` count as arrived live. Relationship summaries an outage put off re-run as rebuilds, so an explicit rebuild of a backfilled contact is not held back on retry.

Left open: `llm.backfill_window_days` is not built. To-do checks already queued before the upgrade still drain.
