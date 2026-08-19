#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "tests use panic and unwrap for direct fixture failures"
    )
)]

use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::types::SyncCursor;
use mxr_core::{MailSyncProvider, MxrError};
use mxr_protocol::*;
use mxr_rules::{Rule, RuleAction, RuleEngine, RuleExecutionLog};
use mxr_store::{SyncRuntimeStatusUpdate, SyncStatus as StoreSyncStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::{interval, timeout, Duration};

#[cfg(not(test))]
const SYNC_CYCLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
#[cfg(test)]
const SYNC_CYCLE_TIMEOUT: Duration = Duration::from_millis(50);

/// Upper bound on the envelopes a single `NewMessages` event carries.
///
/// The event is a notification: every client refetches on it (the web app
/// invalidates its queries, the TUI reloads its views) rather than treating the
/// payload as the source of truth. An initial backfill, though, upserts tens of
/// thousands of messages at once, and one event carrying all of them serialises
/// past the codec's 16 MiB frame cap — which used to cost the client its whole
/// connection, not just the event. Chunking instead of capping would fire one
/// new-mail chime per chunk, so cap it: 500 covers any realistic incremental
/// sync and keeps the frame in the low megabytes. The event's `total` still
/// reports how many messages actually arrived.
const NEW_MESSAGES_EVENT_LIMIT: usize = 500;

/// Test-only scheduler seam for the IDLE loop's provider-read → reload-snapshot
/// race window. The IDLE loop calls [`idle_race_hook::fire`] immediately after
/// reading the provider; a test installs a hook keyed by account to inject a
/// reload at exactly that point, making the otherwise-nondeterministic data race
/// deterministically reproducible. Compiled out entirely in release builds.
#[cfg(test)]
pub(crate) mod idle_race_hook {
    use mxr_core::id::AccountId;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    type Hook = Box<dyn FnMut() + Send>;

    fn hooks() -> &'static Mutex<HashMap<AccountId, Hook>> {
        static HOOKS: OnceLock<Mutex<HashMap<AccountId, Hook>>> = OnceLock::new();
        HOOKS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub(crate) fn install(account_id: AccountId, hook: Hook) {
        hooks().lock().unwrap().insert(account_id, hook);
    }

    /// Fire (once) the hook registered for `account_id`, if any. The hook is
    /// removed so it runs a single time.
    pub(crate) fn fire(account_id: &AccountId) {
        let hook = hooks().lock().unwrap().remove(account_id);
        if let Some(mut hook) = hook {
            hook();
        }
    }
}

/// Extra grace a detached sync gets to finish before the reaper aborts
/// it. The detach path exists for syncs that outlive their caller's wait
/// limit but are still making progress; this caps how long a genuinely
/// *wedged* sync can hold the per-account provider lock. Without it, a
/// sync stuck on an await that never resolves keeps the lock forever and
/// every later sync blocks on `acquire_provider_operation`.
#[cfg(not(test))]
const DETACHED_SYNC_GRACE: Duration = Duration::from_secs(10 * 60);
#[cfg(test)]
const DETACHED_SYNC_GRACE: Duration = Duration::from_millis(500);

/// A sync pass in flight, with everything needed to finalize it — by the
/// caller that started it, or by the reaper when it outlives the caller's
/// patience. One value, one finalizer: before this existed the sync loop, the
/// manual-sync handler, and the reaper each finalized their own way, and the
/// reaper's way quietly skipped the whole post-sync fan-out.
pub(crate) struct SyncPass {
    pub state: Arc<AppState>,
    pub account_id: AccountId,
    pub provider: Arc<dyn MailSyncProvider>,
    pub provider_guard: tokio::sync::OwnedMutexGuard<()>,
    pub sync_log_id: Option<i64>,
    pub prior_consecutive_failures: u32,
    pub pre_sync_cursor: Option<SyncCursor>,
    /// Set when a client is streaming this pass's progress events.
    pub operation: Option<SyncOperation>,
    pub started_by: SyncStarter,
    /// Cleared as the pass is finalized. The engine runs on its own task, and
    /// an aborted task keeps running until it reaches an await point — long
    /// enough to report one more milestone and resurrect a run that has just
    /// been wound up, leaving stale progress on the status row.
    progress_live: Arc<AtomicBool>,
    /// Set by the reaper so the log and the event trail say which pass this
    /// was.
    detached: bool,
}

/// Who started a pass, and therefore who is still waiting on it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncStarter {
    /// The account's own sync loop, which re-polls itself while pages remain.
    AccountLoop,
    /// A client holding its request open until the pass is done.
    BlockingClient,
    /// A client that has already been acked and is watching the status row.
    BackgroundClient,
}

/// The client-visible operation a pass reports its progress under.
#[derive(Clone)]
pub(crate) struct SyncOperation {
    pub operation_id: String,
    pub operation: String,
    pub account_id: Option<AccountId>,
}

/// What a caller needs back from a pass it finalized.
pub(crate) struct SyncPassOutcome {
    pub synced_count: u32,
    pub has_more: bool,
}

/// Result of waiting on a sync pass for up to a wall-clock limit.
#[expect(
    clippy::large_enum_variant,
    reason = "one of these exists per sync pass, and a pass runs for seconds to minutes; boxing would buy nothing but ceremony at the call sites"
)]
pub(crate) enum SyncWait {
    /// The pass finished within the limit and is the caller's to finalize.
    Finished {
        pass: SyncPass,
        result: Result<mxr_sync::SyncOutcome, MxrError>,
    },
    /// The limit elapsed while the sync was still running. The sync keeps
    /// running and a reaper task now owns the pass — the caller must not
    /// touch the provider guard, sync log row, or runtime status.
    Detached,
}

/// Open a sync pass: take the account's provider lock, open its sync-log row,
/// and mark the account as syncing. Shared by the sync loop and the manual
/// sync handler so both accounts start identically bookkept.
pub(crate) async fn begin_sync_pass(
    state: &Arc<AppState>,
    provider: Arc<dyn MailSyncProvider>,
    operation: Option<SyncOperation>,
    started_by: SyncStarter,
) -> SyncPass {
    let account_id = provider.account_id().clone();
    let provider_guard = state.acquire_provider_operation(&account_id).await;
    let existing_status = state
        .store
        .get_sync_runtime_status(&account_id)
        .await
        .ok()
        .flatten();
    let pre_sync_cursor = state
        .store
        .get_sync_cursor(&account_id)
        .await
        .ok()
        .flatten();
    let sync_log_id = state
        .store
        .insert_sync_log(&account_id, &StoreSyncStatus::Running)
        .await
        .ok();
    let _ = state
        .store
        .upsert_sync_runtime_status(
            &account_id,
            &SyncRuntimeStatusUpdate {
                last_attempt_at: Some(chrono::Utc::now()),
                last_error: Some(None),
                failure_class: Some(None),
                sync_in_progress: Some(true),
                current_cursor_summary: Some(Some(describe_sync_cursor(
                    provider.as_ref(),
                    pre_sync_cursor.as_ref(),
                ))),
                ..Default::default()
            },
        )
        .await;
    SyncPass {
        state: state.clone(),
        account_id,
        provider,
        provider_guard,
        sync_log_id,
        prior_consecutive_failures: existing_status
            .as_ref()
            .map_or(0, |status| status.consecutive_failures),
        pre_sync_cursor,
        operation,
        started_by,
        progress_live: Arc::new(AtomicBool::new(true)),
        detached: false,
    }
}

/// Turn the engine's milestones into live status and, when someone is
/// listening, `OperationProgress` events. Only the "stored" milestone adds to
/// the run's message count — the same page is reported three times, and
/// counting each one would treble it.
fn progress_sink(pass: &SyncPass) -> impl Fn(mxr_sync::SyncProgress) + Send + Sync + 'static {
    let state = pass.state.clone();
    let account_id = pass.account_id.clone();
    let operation = pass.operation.clone();
    let live = pass.progress_live.clone();
    move |milestone| {
        if !live.load(Ordering::Relaxed) {
            return;
        }
        // A page that knows how much is left turns into the run's
        // denominator, added to what the run has already stored.
        if let mxr_sync::SyncProgress::PageFetched {
            remaining_estimate: Some(remaining),
            ..
        } = milestone
        {
            state.record_sync_remaining(&account_id, remaining);
        }
        let (stored, message) = match milestone {
            mxr_sync::SyncProgress::PageFetched {
                messages, has_more, ..
            } => (
                0,
                if has_more {
                    format!("Fetched {messages} messages; more pages to come")
                } else {
                    format!("Fetched {messages} messages")
                },
            ),
            mxr_sync::SyncProgress::PageStored { messages } => {
                (messages, format!("Stored {messages} messages"))
            }
            mxr_sync::SyncProgress::PageIndexed { messages } => {
                (0, format!("Indexed {messages} messages"))
            }
            mxr_sync::SyncProgress::Restarted => {
                state.reset_sync_progress(&account_id);
                (0, "Rebuilding label associations".to_string())
            }
        };
        state.record_sync_progress(&account_id, stored, message);
        let Some(operation) = &operation else {
            return;
        };
        let Some(progress) = state.sync_progress(&account_id) else {
            return;
        };
        crate::handler::diagnostics_impl::emit_operation_event(
            &state,
            DaemonEvent::OperationProgress {
                operation_id: operation.operation_id.clone(),
                operation: operation.operation.clone(),
                account_id: operation.account_id.clone(),
                current: progress.current,
                total: progress.total,
                message: progress.message,
            },
        );
    }
}

/// Run `pass` on its own task and wait up to `limit`. Wrapping the sync future
/// directly in `tokio::time::timeout` cancels it at an arbitrary await point
/// on timeout — dropping an IMAP session mid-command and abandoning
/// cursor/status writes partway through. Spawning means the sync always runs
/// to completion; the limit only bounds how long the caller waits before
/// handing finalization to a reaper task.
pub(crate) async fn run_sync_pass(mut pass: SyncPass, limit: Duration) -> SyncWait {
    let task_state = pass.state.clone();
    let task_provider = pass.provider.clone();
    let sink = progress_sink(&pass);
    let mut task = tokio::spawn(async move {
        task_state
            .sync_engine
            .sync_account_reporting(task_provider.as_ref(), &sink)
            .await
    });
    match timeout(limit, &mut task).await {
        Ok(joined) => {
            let result = joined.unwrap_or_else(|join_error| {
                Err(MxrError::Provider(format!(
                    "sync task failed: {join_error}"
                )))
            });
            SyncWait::Finished { pass, result }
        }
        Err(_) => {
            // Tell whoever was waiting that their sync outlived the wait but
            // is still alive. `sync_in_progress` stays true — it genuinely is.
            //
            // Only for a caller that *was* waiting: `last_error` is what makes
            // an account read as unhealthy, and a background sync nobody is
            // blocked on outrunning its limit is a long backfill, not a fault.
            if pass.started_by != SyncStarter::BackgroundClient {
                let _ = pass
                    .state
                    .store
                    .upsert_sync_runtime_status(
                        &pass.account_id,
                        &SyncRuntimeStatusUpdate {
                            last_error: Some(Some(format!(
                                "sync still running after {limit:?}; continuing in background"
                            ))),
                            failure_class: Some(Some("timeout".to_string())),
                            ..Default::default()
                        },
                    )
                    .await;
            }
            // A blocking caller reports the detach to its own client itself;
            // leaving the operation on the pass would have the reaper emit a
            // second terminal event for the same operation id.
            if pass.started_by == SyncStarter::BlockingClient {
                pass.operation = None;
            }
            tokio::spawn(reap_detached_sync(pass, task));
            SyncWait::Detached
        }
    }
}

/// Wait for a detached sync to finish, then finalize it exactly as the caller
/// would have.
///
/// Normally we let it run to completion — aborting at an arbitrary await point
/// risks dropping a session mid-command. But a sync that never completes would
/// hold the provider guard forever and wedge every later sync on the account,
/// so it is aborted once it stops *reporting progress* for the grace period.
/// Elapsed time is the wrong test: a 50k backfill legitimately takes longer
/// than any grace we would want to give a wedged one.
async fn reap_detached_sync(
    mut pass: SyncPass,
    mut task: tokio::task::JoinHandle<Result<mxr_sync::SyncOutcome, MxrError>>,
) {
    pass.detached = true;
    let state = pass.state.clone();
    let account_id = pass.account_id.clone();
    let result = loop {
        let before = state.sync_progress_revision(&account_id);
        match timeout(DETACHED_SYNC_GRACE, &mut task).await {
            Ok(joined) => {
                break joined.unwrap_or_else(|join_error| {
                    Err(MxrError::Provider(format!(
                        "sync task failed: {join_error}"
                    )))
                })
            }
            Err(_) if state.sync_progress_revision(&account_id) != before => {
                tracing::info!(
                    account = %account_id,
                    "detached sync is still making progress; continuing to wait"
                );
            }
            Err(_) => {
                task.abort();
                tracing::error!(
                    account = %account_id,
                    "detached sync reported no progress for {DETACHED_SYNC_GRACE:?}; aborting to release the provider lock"
                );
                // `abort` only takes effect when the task next reaches an
                // await point, so joining it is what actually makes the engine
                // stop. Skipping the join would release the provider guard
                // below while the old sync was still writing, and the next
                // sync would take the lock and run alongside it.
                let _ = (&mut task).await;
                break Err(MxrError::Provider(format!(
                    "sync aborted: no progress for {DETACHED_SYNC_GRACE:?}"
                )));
            }
        }
    };
    let _ = finalize_sync_pass(pass, result).await;
}

/// Re-assert the account as busy when a background sync was claimed while this
/// pass was finalizing.
///
/// The finalizer reads the claim, then writes the status row. A claim taken in
/// between is a client that has already been acked and whose own pre-ack
/// `sync_in_progress = true` this write would silently undo, leaving the client
/// waiting on a row that says idle. Re-checking after the write closes it: the
/// claim outlives the window, so the later of the two writers always wins with
/// "busy".
async fn reassert_busy_if_background_sync_queued(state: &Arc<AppState>, account_id: &AccountId) {
    if !state.background_sync_queued(account_id) {
        return;
    }
    let _ = state
        .store
        .upsert_sync_runtime_status(
            account_id,
            &SyncRuntimeStatusUpdate {
                sync_in_progress: Some(true),
                ..Default::default()
            },
        )
        .await;
}

/// Record a finished pass and fan out from it: runtime status, sync log,
/// provider guard release, client events, and the detached post-sync work.
///
/// The pass's own error is handed back so the caller can decide what it means
/// for its schedule (backoff, retry), which is the only part that differs
/// between the sync loop, the manual handler, and the reaper.
pub(crate) async fn finalize_sync_pass(
    pass: SyncPass,
    result: Result<mxr_sync::SyncOutcome, MxrError>,
) -> Result<SyncPassOutcome, MxrError> {
    let SyncPass {
        state,
        account_id,
        provider,
        provider_guard,
        sync_log_id,
        prior_consecutive_failures,
        pre_sync_cursor,
        operation,
        started_by,
        progress_live,
        detached,
    } = pass;
    progress_live.store(false, Ordering::Relaxed);
    let late_note = if detached {
        " after exceeding its wait limit"
    } else {
        ""
    };

    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            let err_str = error.to_string();
            let failure_class = classify_sync_error(&err_str);
            let post_error_cursor = state
                .store
                .get_sync_cursor(&account_id)
                .await
                .ok()
                .flatten();
            let cursor_summary =
                describe_sync_cursor(provider.as_ref(), post_error_cursor.as_ref());
            let _ = state
                .store
                .upsert_sync_runtime_status(
                    &account_id,
                    &SyncRuntimeStatusUpdate {
                        last_error: Some(Some(err_str.clone())),
                        failure_class: Some(Some(failure_class.to_string())),
                        consecutive_failures: Some(prior_consecutive_failures.saturating_add(1)),
                        backoff_until: Some(None),
                        sync_in_progress: Some(state.background_sync_queued(&account_id)),
                        current_cursor_summary: Some(Some(cursor_summary.clone())),
                        ..Default::default()
                    },
                )
                .await;
            if let Some(log_id) = sync_log_id {
                let _ = state
                    .store
                    .complete_sync_log(log_id, &StoreSyncStatus::Error, 0, Some(&err_str))
                    .await;
            }
            reassert_busy_if_background_sync_queued(&state, &account_id).await;
            drop(provider_guard);
            state.finish_sync_run(&account_id, false);
            let _ = state
                .store
                .insert_event(
                    "error",
                    "sync",
                    &format!("Sync failed for {account_id}{late_note}"),
                    Some(&account_id),
                    Some(&format!(
                        "class={failure_class}; error={err_str}; cursor={cursor_summary}"
                    )),
                )
                .await;
            tracing::error!(account = %account_id, "Sync error: {err_str}");
            // Both events, always. `SyncError` is how a client that is not
            // watching this particular operation hears about it — which now
            // includes the client that asked for a background sync and got its
            // ack long before this. `OperationFailed` carries the operation id
            // so a client that *is* streaming can tie it to its request.
            crate::chimes::emit_daemon_event(
                &state,
                DaemonEvent::SyncError {
                    account_id: account_id.clone(),
                    error: err_str.clone(),
                },
            );
            if let Some(operation) = operation {
                crate::handler::diagnostics_impl::emit_operation_event(
                    &state,
                    DaemonEvent::OperationFailed {
                        operation_id: operation.operation_id,
                        operation: operation.operation,
                        account_id: operation.account_id,
                        error: err_str,
                        retryable: true,
                    },
                );
            }
            return Err(error);
        }
    };

    let count = outcome.synced_count;
    let post_sync_cursor = state
        .store
        .get_sync_cursor(&account_id)
        .await
        .ok()
        .flatten();
    let cursor_summary = describe_sync_cursor(provider.as_ref(), post_sync_cursor.as_ref());
    let _ = state
        .store
        .upsert_sync_runtime_status(
            &account_id,
            &SyncRuntimeStatusUpdate {
                last_success_at: Some(chrono::Utc::now()),
                last_error: Some(None),
                failure_class: Some(None),
                consecutive_failures: Some(0),
                backoff_until: Some(None),
                // Two reasons the account is still busy. A batch that
                // reports `has_more` is one page of a backfill, not a
                // finished sync — the account keeps re-polling. And a
                // background sync that has been acked but is still queued
                // behind the provider lock is work the client is waiting for;
                // this pass finishing says nothing about it. Reporting "idle"
                // in either gap makes a waiting client (`mxr demo`, `mxr sync
                // --wait`) stop short.
                sync_in_progress: Some(
                    outcome.has_more || state.background_sync_queued(&account_id),
                ),
                current_cursor_summary: Some(Some(cursor_summary.clone())),
                last_synced_count: Some(count),
                ..Default::default()
            },
        )
        .await;
    if let Some(log_id) = sync_log_id {
        let _ = state
            .store
            .complete_sync_log(log_id, &StoreSyncStatus::Success, count, None)
            .await;
    }
    reassert_busy_if_background_sync_queued(&state, &account_id).await;
    drop(provider_guard);
    state.finish_sync_run(&account_id, outcome.has_more);
    // A pass returns after one page. Without a wake, the account would sit at
    // `sync_in_progress = true` until the sync loop's next tick, a full
    // interval away: the loop only re-polls by itself when it is still
    // mid-cycle, which it is not once it has given up on a detached pass and
    // gone to sleep on its backoff. `notify_one` keeps a permit if the loop is
    // busy, so the wake is never lost.
    if outcome.has_more && (detached || started_by != SyncStarter::AccountLoop) {
        state.idle_notify_for_account(&account_id).notify_one();
    }
    if let Err(error) = crate::handler::reconcile_provider_drafts(&state, &account_id).await {
        tracing::warn!(account = %account_id, %error, "provider draft reconciliation failed");
    }
    let _ = state
        .store
        .insert_event(
            "info",
            "sync",
            &format!("Sync completed for {account_id}{late_note}"),
            Some(&account_id),
            Some(&format!("messages_synced={count}; cursor={cursor_summary}")),
        )
        .await;

    let was_initial_backfill = pre_sync_cursor
        .as_ref()
        .is_some_and(|c| provider.is_backfill_cursor(c));
    let initial_backfill_in_progress = post_sync_cursor
        .as_ref()
        .is_some_and(|c| provider.is_backfill_cursor(c));

    // The whole-table analytics repair is settled per backfill, not per page,
    // and the decision cannot live behind "this page carried messages": a
    // provider ends a backfill by handing back an empty final page (Gmail when
    // the page after a `nextPageToken` turns out empty, IMAP when the last UID
    // chunk is all deleted), and that page is the one that owes the repair.
    let analytics_due = if outcome.has_more || initial_backfill_in_progress {
        if count > 0 {
            state.owe_analytics_repair(&account_id);
        }
        false
    } else {
        // An idle tick that synced nothing and owes nothing has no reason to
        // rescan the whole mailbox.
        let owed = state.take_analytics_repair_debt(&account_id);
        count > 0 || owed
    };
    if analytics_due {
        let repair_state = state.clone();
        let repair_account = account_id.clone();
        tokio::spawn(async move {
            run_analytics_repair(&repair_state, &repair_account).await;
        });
    }

    let upserted_ids = outcome.upserted_message_ids;
    if count > 0 {
        let initial_backfill_finished = was_initial_backfill && !initial_backfill_in_progress;
        match state.warm_lexical_search(initial_backfill_finished).await {
            Ok(true) => tracing::info!(account = %account_id, "Lexical search index warmed"),
            Ok(false) => {}
            Err(error) => tracing::warn!(
                account = %account_id,
                %error,
                "lexical search warm-up failed after sync"
            ),
        }

        // Collect this page's contacts into the account's deferred set. The
        // relationship worker does per-message reads, so running it per page
        // during a backfill would put a query storm behind every page; the set
        // is handed over in one go when the backfill ends.
        if initial_backfill_in_progress
            || was_initial_backfill
            || state.has_deferred_relationship_contacts(&account_id)
        {
            match state
                .store
                .relationship_contacts_for_messages(&upserted_ids)
                .await
            {
                Ok(contacts) => state.defer_relationship_contacts(
                    &account_id,
                    contacts.into_iter().map(|(_, email)| email),
                ),
                Err(error) => {
                    tracing::warn!(account = %account_id, %error, "relationship backfill contact lookup failed");
                }
            }
            if initial_backfill_in_progress {
                tracing::debug!(account = %account_id, "relationship profile refresh deferred during initial backfill");
            }
        }

        // Slice before the query, not after: an initial backfill upserts tens
        // of thousands of ids and loading every envelope only to throw most
        // away costs a large read for nothing. The sync engine appends ids in
        // the order it processed the page, so the tail is the most recently
        // handled; the loaded slice is then sorted newest-first so consumers
        // that show `envelopes[0]` (the web app's new-mail notification) name
        // the newest message.
        let event_ids =
            &upserted_ids[upserted_ids.len().saturating_sub(NEW_MESSAGES_EVENT_LIMIT)..];
        match state.store.list_envelopes_by_ids(event_ids).await {
            Ok(mut envelopes) if !envelopes.is_empty() => {
                envelopes.sort_by_key(|envelope| std::cmp::Reverse(envelope.date));
                crate::chimes::emit_daemon_event(
                    &state,
                    DaemonEvent::NewMessages {
                        envelopes,
                        total: upserted_ids.len(),
                    },
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(account = %account_id, %error, "new-message event lookup failed");
            }
        }
    }

    // Hand the deferred contacts to the relationship worker on the same edge
    // as the analytics repair, and outside `count > 0` for the same reason: a
    // provider ends a backfill with an empty final page, and a handover gated
    // on "this page carried messages" would never happen for it — leaving the
    // whole backfill's contacts stranded in memory until the process exits.
    let contacts_handed_over = if initial_backfill_in_progress {
        false
    } else {
        let deferred = state.take_deferred_relationship_contacts(&account_id);
        if deferred.is_empty() {
            false
        } else {
            let contacts = deferred
                .into_iter()
                .map(|email| (account_id.clone(), email))
                .collect::<Vec<_>>();
            if let Err(error) = state.relationship.enqueue_contacts(contacts).await {
                tracing::warn!(account = %account_id, %error, "relationship handover enqueue failed");
            }
            true
        }
    };

    if count > 0 {
        // Critical: the post-sync fan-out (semantic ingest, contacts refresh,
        // relationship profile, rules engine, analytics backfill) used to run
        // inline here. That kept the sync loop blocked until every downstream
        // worker had ack'd the enqueue and any network call inside the rules
        // engine had returned. On a busy mailbox that meant 10+ minutes
        // between "Gmail has new mail" and "mxr shows new mail". Move the
        // fan-out to a detached task so the loop returns immediately to its
        // periodic sleep / IDLE wait. Each downstream worker has its own
        // bounded channel; if a worker is slow, only that worker backs up.
        //
        // No automatic summary backfill: even gated by `llm.enabled`, that
        // previously spawned unbounded tokio tasks (one per changed thread) on
        // every sync tick. On a 100k-message initial backfill that saturates
        // the tokio runtime + LLM and the TUI grinds to a halt. Summaries are
        // generated strictly on demand when the user opens a thread.
        let fanout_state = state.clone();
        let fanout_account = account_id.clone();
        let fanout_provider = provider.clone();
        tokio::spawn(async move {
            post_sync_fanout(
                fanout_state,
                fanout_account,
                fanout_provider,
                upserted_ids,
                contacts_handed_over,
                initial_backfill_in_progress,
            )
            .await;
        });
    }

    tracing::info!(account = %account_id, "Sync completed: {count} messages");
    crate::chimes::emit_daemon_event(
        &state,
        DaemonEvent::SyncCompleted {
            account_id: account_id.clone(),
            messages_synced: count,
        },
    );
    if let Ok(labels) = state.store.list_labels_by_account(&account_id).await {
        let counts: Vec<_> = labels
            .iter()
            .map(|l| LabelCount {
                label_id: l.id.clone(),
                unread_count: l.unread_count,
                total_count: l.total_count,
            })
            .collect();
        crate::chimes::emit_daemon_event(&state, DaemonEvent::LabelCountsUpdated { counts });
    }
    if let Some(operation) = operation {
        crate::handler::diagnostics_impl::emit_operation_event(
            &state,
            DaemonEvent::OperationCompleted {
                operation_id: operation.operation_id,
                operation: operation.operation,
                account_id: operation.account_id,
                message: format!("Sync complete: {count} message(s) updated"),
            },
        );
    }

    Ok(SyncPassOutcome {
        synced_count: count,
        has_more: outcome.has_more,
    })
}

/// Clear `sync_in_progress` rows left behind by a daemon that died
/// mid-sync. Runs once at startup, before any sync loop spawns, so a
/// stale flag can't survive into the new process.
pub async fn reconcile_interrupted_syncs(state: &AppState) {
    let statuses = match state.store.list_sync_runtime_statuses().await {
        Ok(statuses) => statuses,
        Err(error) => {
            tracing::warn!("startup sync-status reconciliation skipped: {error}");
            return;
        }
    };
    for status in statuses.iter().filter(|status| status.sync_in_progress) {
        let _ = state
            .store
            .upsert_sync_runtime_status(
                &status.account_id,
                &SyncRuntimeStatusUpdate {
                    last_error: Some(Some("sync interrupted by daemon restart".to_string())),
                    failure_class: Some(Some("interrupted".to_string())),
                    sync_in_progress: Some(false),
                    ..Default::default()
                },
            )
            .await;
        tracing::info!(
            account = %status.account_id,
            "cleared stale sync_in_progress from previous daemon run"
        );
    }
}

/// Spawn sync loops for all configured accounts.
pub fn spawn_sync_loops(state: Arc<AppState>) {
    for (account_id, _) in state.sync_provider_entries() {
        if state.mark_sync_loop_spawned(&account_id) {
            let loop_state = state.clone();
            let task_state = state.clone();
            let task_account_id = account_id.clone();
            let handle = tokio::spawn(async move {
                let shutdown_rx = loop_state.shutdown_receiver();
                sync_loop_for_account(loop_state, task_account_id.clone(), shutdown_rx).await;
                task_state.finish_sync_loop(&task_account_id);
            });
            state.register_sync_loop_handle(account_id.clone(), handle);
        }

        // Phase 3.1: spawn the IDLE watcher iff the provider returns a
        // watcher from `idle_watch`. Default impl returns Ok(None) so
        // poll-only providers (Gmail, SMTP, fake-with-no-trigger) skip.
        if state.mark_idle_loop_spawned(&account_id) {
            let loop_state = state.clone();
            let watcher_account_id = account_id.clone();
            let handle = tokio::spawn(async move {
                let shutdown_rx = loop_state.shutdown_receiver();
                idle_loop_for_account(loop_state.clone(), watcher_account_id.clone(), shutdown_rx)
                    .await;
                loop_state.finish_idle_loop(&watcher_account_id);
            });
            state.register_idle_loop_handle(account_id.clone(), handle);
        }
    }
}

/// Phase 3.1: per-account IDLE watcher. Calls
/// `MailSyncProvider::idle_watch` once; if the provider returns a
/// real watcher, loops calling `next_event`. Each event signals the
/// per-account `Notify` so the sync loop wakes early instead of
/// waiting for its periodic timer. On dropped connection, backs off
/// then re-acquires the watcher (next call to `idle_watch`).
async fn idle_loop_for_account(
    state: Arc<AppState>,
    account_id: mxr_core::id::AccountId,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let notify = state.idle_notify_for_account(&account_id);
    // Re-fetch the provider from the runtime each cycle and reconnect whenever
    // the providers are reloaded, so a rotated credential takes effect without a
    // daemon restart (the watcher would otherwise keep an old provider Arc and a
    // long-lived connection using the stale password).
    let mut reload_rx = state.reload_receiver();
    let mut backoff_secs: u64 = 0;
    loop {
        if *shutdown_rx.borrow() {
            return;
        }
        if backoff_secs > 0 {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                changed = shutdown_rx.changed() => {
                    if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                        return;
                    }
                }
            }
        }

        // Snapshot the reload generation as seen BEFORE reading the provider.
        // `reload_accounts_from_disk` swaps the provider runtime and THEN bumps
        // this generation, so observing a bump happens-after the swap. Marking
        // first is what closes the race: any reload landing after this point
        // leaves `reload_rx` observably changed (caught by the re-check below or
        // the select arm), instead of being masked. Marking AFTER the provider
        // read would let a concurrent reload (multi-threaded runtime) swap in a
        // fresh provider while we open a long-lived watcher on the stale one and
        // never notice — the exact stale-credential bug this loop exists to fix.
        reload_rx.mark_unchanged();
        let Some(provider) = state.sync_provider_for_account(&account_id) else {
            tracing::info!(account = %account_id, "IDLE loop exiting: account removed from runtime");
            return;
        };

        // Test-only seam: inject a reload at exactly the read→snapshot window so
        // the race guard is deterministic. No-op (and compiled out) in release.
        #[cfg(test)]
        idle_race_hook::fire(&account_id);

        // A reload landed between the snapshot and the provider read: re-fetch
        // before opening a long-lived watcher, so the watcher we block on always
        // reflects any reload already signaled. `mark_unchanged` at the top of
        // the next iteration clears the flag, so this cannot busy-spin.
        if reload_rx.has_changed().unwrap_or(false) {
            continue;
        }

        let mut watcher = match provider.idle_watch().await {
            Ok(Some(w)) => w,
            Ok(None) => {
                // Provider doesn't support IDLE — exit; the sync loop
                // continues its periodic poll.
                return;
            }
            Err(error) => {
                tracing::warn!(account = %account_id, %error, "idle_watch failed; backing off");
                backoff_secs = (backoff_secs.saturating_mul(2)).clamp(15, 300);
                continue;
            }
        };
        backoff_secs = 0;

        loop {
            tokio::select! {
                event = watcher.next_event() => {
                    match event {
                        Ok(()) => {
                            tracing::debug!(account = %account_id, "idle event; waking sync loop");
                            notify.notify_one();
                        }
                        Err(error) => {
                            tracing::warn!(account = %account_id, %error, "idle watcher dropped; reconnecting");
                            backoff_secs = backoff_secs.saturating_add(5).clamp(5, 300);
                            break;
                        }
                    }
                }
                changed = shutdown_rx.changed() => {
                    if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                        return;
                    }
                }
                changed = reload_rx.changed() => {
                    if changed.is_ok() {
                        tracing::info!(account = %account_id, "providers reloaded; dropping IDLE connection to reconnect with refreshed credentials");
                        // Drop the current watcher (and its connection) and let the
                        // outer loop re-fetch the refreshed provider.
                        break;
                    }
                }
            }
        }
    }
}

async fn sync_loop_for_account(
    state: Arc<AppState>,
    account_id: AccountId,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut backoff_secs: u64 = 0;
    let mut skip_sleep = true;
    let mut consecutive_has_more: u32 = 0;
    let mut last_message_sync_at = chrono::Utc::now();
    // Phase 3.1: wake the sleep early when an IDLE watcher signals.
    let idle_notify = state.idle_notify_for_account(&account_id);

    loop {
        if *shutdown_rx.borrow() {
            tracing::info!(account = %account_id, "Sync loop exiting: daemon shutdown requested");
            break;
        }
        // Re-fetched each iteration, so a reload's refreshed provider is picked
        // up on the next cycle. A reload mid-cycle can cause at most ONE stale
        // sync cycle (the provider was read before the sleep); that self-corrects
        // next cycle and sync is retried/idempotent, so it needs no signal here.
        let Some(provider) = state.sync_provider_for_account(&account_id) else {
            tracing::info!(account = %account_id, "Sync loop exiting: account removed from runtime");
            break;
        };
        let base_interval = state.sync_interval_secs().max(30);

        if skip_sleep {
            skip_sleep = false;
        } else {
            let wait = if backoff_secs > 0 {
                tracing::info!(account = %account_id, "Rate limited, backing off {backoff_secs}s");
                backoff_secs
            } else {
                base_interval
            };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(wait)) => {}
                _ = idle_notify.notified() => {
                    tracing::debug!(account = %account_id, "sync loop woken by IDLE notification");
                }
                changed = shutdown_rx.changed() => {
                    if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                        tracing::info!(account = %account_id, "Sync loop exiting during backoff: daemon shutdown requested");
                        break;
                    }
                }
            }
        }

        let pass = begin_sync_pass(&state, provider.clone(), None, SyncStarter::AccountLoop).await;
        let (pass, result) = match run_sync_pass(pass, SYNC_CYCLE_TIMEOUT).await {
            SyncWait::Finished { pass, result } => (pass, result),
            SyncWait::Detached => {
                tracing::warn!(
                    account = %account_id,
                    "sync exceeded {SYNC_CYCLE_TIMEOUT:?}; leaving it to finish in background"
                );
                backoff_secs = (backoff_secs * 2).clamp(30, 300);
                continue;
            }
        };
        match finalize_sync_pass(pass, result).await {
            Ok(outcome) => {
                let count = outcome.synced_count;
                backoff_secs = 0;
                let idle_for = chrono::Utc::now() - last_message_sync_at;
                if count > 0 {
                    last_message_sync_at = chrono::Utc::now();
                }

                if count == 0
                    && state.config_snapshot().search.semantic.enabled
                    && idle_for >= chrono::Duration::minutes(30)
                {
                    let semantic = state.semantic.clone();
                    tokio::spawn(async move {
                        match semantic.backfill_active_limited(200).await {
                            Ok(record) if record.progress_completed > 0 => {
                                tracing::info!(
                                    profile = record.profile.as_str(),
                                    completed = record.progress_completed,
                                    total = record.progress_total,
                                    "semantic idle backfill processed missing messages"
                                );
                            }
                            Ok(_) => {}
                            Err(error) => {
                                tracing::warn!("semantic idle backfill failed: {error}");
                            }
                        }
                    });
                    last_message_sync_at = chrono::Utc::now();
                }

                if outcome.has_more {
                    consecutive_has_more = consecutive_has_more.saturating_add(1);
                    if consecutive_has_more >= 50 {
                        tracing::warn!(
                            account = %account_id,
                            consecutive_has_more,
                            "has_more cap reached — forcing one sleep cycle"
                        );
                        consecutive_has_more = 0;
                    } else {
                        tracing::info!(
                            account = %account_id,
                            "provider has more — re-polling immediately"
                        );
                        skip_sleep = true;
                        continue;
                    }
                } else {
                    consecutive_has_more = 0;
                }
            }
            Err(mxr_core::MxrError::RateLimited { retry_after_secs }) => {
                // Same ceiling as every other backoff arm. A provider is free
                // to send a Retry-After measured in days, and honouring it
                // literally parks the account until the daemon restarts — on
                // top of overflowing the doubling and the i64 conversion
                // below. Re-polling a still-limited provider costs one 429.
                backoff_secs = retry_after_secs.saturating_add(10).clamp(30, 300);
                let backoff_until = chrono::Utc::now()
                    + chrono::Duration::seconds(i64::try_from(backoff_secs).unwrap_or(300));
                let _ = state
                    .store
                    .upsert_sync_runtime_status(
                        &account_id,
                        &SyncRuntimeStatusUpdate {
                            backoff_until: Some(Some(backoff_until)),
                            ..Default::default()
                        },
                    )
                    .await;
            }
            Err(_) => {
                backoff_secs = (backoff_secs * 2).clamp(30, 300);
            }
        }
    }
}

/// Run the work that used to sit inline at the end of each successful
/// sync cycle. Lives in a detached `tokio::spawn` so the sync loop
/// itself can return immediately and start sleeping for the next
/// interval (or wake on IDLE). Each step's failure is logged and
/// swallowed — none of this work blocks the user-facing path, and a
/// transient failure on one step shouldn't prevent the others from
/// running.
async fn post_sync_fanout(
    state: Arc<AppState>,
    account_id: AccountId,
    provider: Arc<dyn MailSyncProvider>,
    upserted_message_ids: Vec<mxr_core::MessageId>,
    // The caller already enqueued the account's deferred backfill contacts, so
    // this page's own contacts are part of that batch.
    contacts_handed_over: bool,
    initial_backfill_in_progress: bool,
) {
    if let Err(error) = state
        .semantic
        .enqueue_ingest_messages(&upserted_message_ids)
        .await
    {
        tracing::error!(account = %account_id, %error, "semantic indexing enqueue failed");
    }
    if let Err(error) = state
        .contacts_refresh
        .enqueue_accounts(std::slice::from_ref(&account_id))
        .await
    {
        tracing::warn!(account = %account_id, %error, "contacts refresh enqueue failed");
    }
    if !contacts_handed_over && !initial_backfill_in_progress {
        if let Err(error) = state
            .relationship
            .enqueue_contacts_from_messages(&upserted_message_ids)
            .await
        {
            tracing::warn!(account = %account_id, %error, "relationship profile enqueue failed");
        }
    }

    // Keep the user's voice profile current so AI drafts can write in their
    // voice. `rebuild_user_voice_profile` self-skips when there are <20 sent
    // messages or the source hash is unchanged, so this is naturally debounced.
    if !initial_backfill_in_progress {
        let voice_state = Arc::clone(&state);
        let voice_account = account_id.clone();
        tokio::spawn(async move {
            if let Err(error) = mxr_relationship::service::rebuild_user_voice_profile(
                &voice_state.store,
                &voice_account,
            )
            .await
            {
                tracing::warn!(account = %voice_account, %error, "user voice profile rebuild failed");
            }
        });
    }

    if let Err(error) = apply_rules_to_messages(
        &state,
        &account_id,
        provider.as_ref(),
        &upserted_message_ids,
    )
    .await
    {
        tracing::error!(account = %account_id, %error, "rule execution failed");
    }

    // Scan newly-upserted mail for deliveries (heuristic + optional LLM).
    // Respects `[deliveries].enabled`; failures are logged, never propagated.
    let delivery_summary =
        crate::handler::deliveries::scan_messages(&state, &upserted_message_ids).await;
    if delivery_summary.created > 0 || delivery_summary.updated > 0 {
        tracing::info!(
            account = %account_id,
            created = delivery_summary.created,
            updated = delivery_summary.updated,
            shortlisted = delivery_summary.shortlisted,
            "post-sync delivery scan"
        );
    }
}

/// Self-heal analytics derived data.
///
/// Every step is a whole-table `WHERE column IS NULL / = 'unknown'` scan, so
/// its cost tracks the size of the mailbox and not the size of the page that
/// triggered it: measured at 82-106s per call on a 50k database whether the
/// page held 2,500 rows or 10,000. Running it after every page put ten minutes
/// of writer contention behind a 26s seed, and a real Gmail account paging 500
/// messages at a time paid it every page. Deferring costs nothing precisely
/// because the steps filter the whole table: the run after the last page
/// repairs every row the earlier pages left. `finalize_sync_pass` owns the
/// decision of when that is.
async fn run_analytics_repair(state: &Arc<AppState>, account_id: &AccountId) {
    let backfill = crate::handler::diagnostics_impl::incremental_analytics_backfill(state).await;
    if backfill.did_work() || backfill.startup_repair_ran {
        tracing::info!(
            account = %account_id,
            directions = backfill.directions_reclassified,
            list_ids = backfill.list_ids_backfilled,
            reply_pairs = backfill.reply_pairs_resolved,
            business_hours = backfill.business_hours_backfilled,
            startup_repair = backfill.startup_repair_ran,
            "post-sync analytics backfill"
        );
    }
}

pub(crate) fn classify_sync_error(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("rate limit") || lower.contains("retry after") {
        "rate_limit"
    } else if lower.contains("auth") || lower.contains("oauth") || lower.contains("login") {
        "auth"
    } else if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("dns")
        || lower.contains("connection")
        || lower.contains("network")
        || lower.contains("tls")
    {
        "network"
    } else if lower.contains("lockbusy")
        || lower.contains("tantivy")
        || lower.contains("sqlite")
        || lower.contains("index")
    {
        "store_index"
    } else if lower.contains("imap") || lower.contains("smtp") || lower.contains("gmail") {
        "protocol"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod classify_sync_error_tests {
    use super::classify_sync_error;

    #[test]
    fn maps_common_sync_error_classes_for_event_payloads() {
        assert_eq!(
            classify_sync_error("rate limit: retry after 60s"),
            "rate_limit"
        );
        assert_eq!(classify_sync_error("oauth login failed"), "auth");
        assert_eq!(classify_sync_error("TLS connection timeout"), "network");
        assert_eq!(classify_sync_error("sqlite index lockbusy"), "store_index");
        assert_eq!(classify_sync_error("imap protocol violation"), "protocol");
        assert_eq!(classify_sync_error("unexpected sync failure"), "unknown");
    }
}

/// Delegate cursor display to the provider — each adapter owns its
/// cursor schema (MSP Phase B).
pub(crate) fn describe_sync_cursor(
    provider: &dyn mxr_core::MailSyncProvider,
    cursor: Option<&SyncCursor>,
) -> String {
    let empty = SyncCursor::empty();
    provider.describe_cursor(cursor.unwrap_or(&empty))
}

async fn apply_rules_to_messages(
    state: &AppState,
    account_id: &AccountId,
    provider: &dyn MailSyncProvider,
    message_ids: &[mxr_core::MessageId],
) -> Result<(), String> {
    let rows = state.store.list_rules().await.map_err(|e| e.to_string())?;
    if rows.is_empty() || message_ids.is_empty() {
        return Ok(());
    }

    let rules: Vec<Rule> = rows
        .iter()
        .map(|row| {
            serde_json::from_value(mxr_store::row_to_rule_json(row)).map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    let engine = RuleEngine::new(rules.clone());
    let labels = state
        .store
        .list_labels_by_account(account_id)
        .await
        .map_err(|e| e.to_string())?;

    for message_id in message_ids {
        let Some(envelope) = state
            .store
            .get_envelope(message_id)
            .await
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        let body = state
            .store
            .get_body(message_id)
            .await
            .map_err(|e| e.to_string())?;
        let label_ids = state
            .store
            .get_message_label_ids(message_id)
            .await
            .map_err(|e| e.to_string())?;
        let label_provider_ids = labels
            .iter()
            .filter(|label| label_ids.iter().any(|id| id == &label.id))
            .map(|label| label.provider_id.clone())
            .collect();
        let message = RuleMessage::from_parts(envelope.clone(), body, label_provider_ids);
        let message_id_str = message_id.as_str();
        let result = engine.evaluate(&message, &message_id_str);
        if result.actions.is_empty() {
            continue;
        }

        let mut action_names = Vec::new();
        let mut error = None;
        for action in &result.actions {
            action_names.push(format!("{action:?}"));
            if let Err(err) =
                execute_rule_action(state, account_id, provider, message_id, action, &labels).await
            {
                error = Some(err);
                break;
            }
        }

        for matched_rule_id in result.matched_rules {
            if let Some(rule) = rules.iter().find(|rule| rule.id == matched_rule_id) {
                let entry = RuleExecutionLog::entry(
                    &rule.id,
                    &rule.name,
                    &message_id_str,
                    &action_names,
                    error.is_none(),
                    error.as_deref(),
                );
                let actions_json =
                    serde_json::to_string(&entry.actions_applied).map_err(|e| e.to_string())?;
                state
                    .store
                    .insert_rule_log(mxr_store::RuleLogInput {
                        rule_id: &entry.rule_id.0,
                        rule_name: &entry.rule_name,
                        message_id: &entry.message_id,
                        actions_applied_json: &actions_json,
                        timestamp: entry.timestamp,
                        success: entry.success,
                        error: entry.error.as_deref(),
                    })
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }

        let _ = state
            .store
            .insert_event(
                if error.is_some() { "error" } else { "info" },
                "rule",
                &format!("Applied rules to {}", message.subject),
                Some(account_id),
                error.as_deref(),
            )
            .await;

        // Yield between messages so a large rule fan-out doesn't
        // monopolize the single writer connection. Sync mutations,
        // snooze wake, activity-log writes — anything else that needs
        // the writer — gets to interleave instead of waiting for the
        // entire batch to finish.
        tokio::task::yield_now().await;
    }

    Ok(())
}

async fn execute_rule_action(
    state: &AppState,
    account_id: &AccountId,
    provider: &dyn MailSyncProvider,
    message_id: &mxr_core::MessageId,
    action: &RuleAction,
    labels: &[mxr_core::Label],
) -> Result<(), String> {
    let provider_message_id = state
        .store
        .get_provider_id(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Provider ID not found for message {message_id}"))?;

    // Rule-driven actions get their own mutation_id per execution so a
    // re-run of the same rule (e.g. after a daemon restart mid-batch)
    // dedupes against the existing apply within the 24h window.
    let mutation_id = uuid::Uuid::now_v7().to_string();
    let mutation_to_apply = match action {
        RuleAction::AddLabel { label } => Some(mxr_core::Mutation::ModifyLabels {
            provider_message_id: provider_message_id.clone(),
            add: vec![label.clone()],
            remove: vec![],
        }),
        RuleAction::RemoveLabel { label } => Some(mxr_core::Mutation::ModifyLabels {
            provider_message_id: provider_message_id.clone(),
            add: vec![],
            remove: vec![label.clone()],
        }),
        RuleAction::Archive => Some(mxr_core::Mutation::ModifyLabels {
            provider_message_id: provider_message_id.clone(),
            add: vec![],
            remove: vec!["INBOX".to_string()],
        }),
        RuleAction::Trash => Some(mxr_core::Mutation::Trash {
            provider_message_id: provider_message_id.clone(),
        }),
        RuleAction::Star => Some(mxr_core::Mutation::SetStarred {
            provider_message_id: provider_message_id.clone(),
            starred: true,
        }),
        RuleAction::MarkRead => Some(mxr_core::Mutation::SetRead {
            provider_message_id: provider_message_id.clone(),
            read: true,
        }),
        RuleAction::MarkUnread => Some(mxr_core::Mutation::SetRead {
            provider_message_id: provider_message_id.clone(),
            read: false,
        }),
        RuleAction::Snooze { .. } | RuleAction::ShellHook { .. } => None,
    };
    if let Some(mutation) = mutation_to_apply {
        provider
            .apply_mutation(&mutation_id, &mutation)
            .await
            .map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().timestamp();
        if let Err(error) = state
            .store
            .record_mutation_applied(&mutation_id, &provider_message_id, account_id, now)
            .await
        {
            tracing::warn!(%error, mutation_id, "rule engine failed to record mutation dedup row");
        }
    }
    match action {
        RuleAction::AddLabel { label } => {
            if let Some(found) = labels
                .iter()
                .find(|candidate| candidate.provider_id == *label || candidate.name == *label)
            {
                state
                    .store
                    .add_message_label(message_id, &found.id, mxr_core::EventSource::RuleEngine)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        RuleAction::RemoveLabel { label } => {
            if let Some(found) = labels
                .iter()
                .find(|candidate| candidate.provider_id == *label || candidate.name == *label)
            {
                state
                    .store
                    .remove_message_label(message_id, &found.id, mxr_core::EventSource::RuleEngine)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        RuleAction::Archive => {}
        RuleAction::Trash => {
            state
                .store
                .move_to_trash(message_id, mxr_core::EventSource::RuleEngine)
                .await
                .map_err(|e| e.to_string())?;
        }
        RuleAction::Star => {
            state
                .store
                .set_starred(message_id, true, mxr_core::EventSource::RuleEngine)
                .await
                .map_err(|e| e.to_string())?;
        }
        RuleAction::MarkRead => {
            state
                .store
                .set_read(message_id, true, mxr_core::EventSource::RuleEngine)
                .await
                .map_err(|e| e.to_string())?;
        }
        RuleAction::MarkUnread => {
            state
                .store
                .set_read(message_id, false, mxr_core::EventSource::RuleEngine)
                .await
                .map_err(|e| e.to_string())?;
        }
        RuleAction::Snooze { duration } => {
            let wake_at = match duration {
                mxr_rules::SnoozeDuration::Hours { count } => {
                    chrono::Utc::now() + chrono::Duration::hours(*count as i64)
                }
                mxr_rules::SnoozeDuration::Days { count } => {
                    chrono::Utc::now() + chrono::Duration::days(*count as i64)
                }
                mxr_rules::SnoozeDuration::Until { date } => *date,
            };
            let original_labels = state
                .store
                .get_message_label_ids(message_id)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .insert_snooze(&mxr_core::types::Snoozed {
                    message_id: message_id.clone(),
                    account_id: account_id.clone(),
                    snoozed_at: chrono::Utc::now(),
                    wake_at,
                    original_labels,
                })
                .await
                .map_err(|e| e.to_string())?;
        }
        RuleAction::ShellHook { command } => {
            let payload = serde_json::json!({
                "message_id": message_id.as_str(),
                "provider_message_id": provider_message_id,
            });
            mxr_rules::shell_hook::execute_shell_hook(
                command,
                &mxr_rules::shell_hook::ShellHookPayload {
                    id: payload["message_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    from: mxr_rules::shell_hook::ShellHookAddress {
                        name: None,
                        email: String::new(),
                    },
                    subject: String::new(),
                    date: chrono::Utc::now().to_rfc3339(),
                    body_text: None,
                    attachments: Vec::new(),
                },
                Some(Duration::from_secs(state.hook_timeout_secs())),
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

struct RuleMessage {
    subject: String,
    from: String,
    to: Vec<String>,
    labels: Vec<String>,
    has_attachment: bool,
    size_bytes: u64,
    date: chrono::DateTime<chrono::Utc>,
    is_unread: bool,
    is_starred: bool,
    has_unsubscribe: bool,
    body_text: Option<String>,
    link_count: u32,
    body_word_count: u32,
}

impl RuleMessage {
    fn from_parts(
        envelope: mxr_core::Envelope,
        body: Option<mxr_core::MessageBody>,
        labels: Vec<String>,
    ) -> Self {
        Self {
            subject: envelope.subject,
            from: envelope.from.email,
            to: envelope.to.into_iter().map(|addr| addr.email).collect(),
            labels,
            has_attachment: envelope.has_attachments,
            size_bytes: envelope.size_bytes,
            date: envelope.date,
            is_unread: !envelope.flags.contains(mxr_core::MessageFlags::READ),
            is_starred: envelope.flags.contains(mxr_core::MessageFlags::STARRED),
            has_unsubscribe: !matches!(
                envelope.unsubscribe,
                mxr_core::types::UnsubscribeMethod::None
            ),
            body_text: body.and_then(|body| body.text_plain.or(body.text_html)),
            link_count: envelope.link_count,
            body_word_count: envelope.body_word_count,
        }
    }
}

impl mxr_rules::MessageView for RuleMessage {
    fn sender_email(&self) -> &str {
        &self.from
    }
    fn to_emails(&self) -> &[String] {
        &self.to
    }
    fn subject(&self) -> &str {
        &self.subject
    }
    fn labels(&self) -> &[String] {
        &self.labels
    }
    fn has_attachment(&self) -> bool {
        self.has_attachment
    }
    fn size_bytes(&self) -> u64 {
        self.size_bytes
    }
    fn date(&self) -> chrono::DateTime<chrono::Utc> {
        self.date
    }
    fn is_unread(&self) -> bool {
        self.is_unread
    }
    fn is_starred(&self) -> bool {
        self.is_starred
    }
    fn has_unsubscribe(&self) -> bool {
        self.has_unsubscribe
    }
    fn body_text(&self) -> Option<&str> {
        self.body_text.as_deref()
    }
    fn link_density_inputs(&self) -> (u32, u32) {
        (self.link_count, self.body_word_count)
    }
}

/// Periodic reconciler that resolves `reply_pair_pending` rows whose parent
/// has since arrived. Mirrors the snooze loop's shape: 60-second tick,
/// shutdown-aware, errors logged and swallowed (next tick retries).
pub async fn reply_pair_reconciler_loop(
    state: Arc<AppState>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut ticker = interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                    tracing::info!("Reply-pair reconciler exiting: shutdown requested");
                    break;
                }
                continue;
            }
        }
        match state.store.reconcile_reply_pair_pending().await {
            Ok(0) => {}
            Ok(n) => {
                tracing::debug!(resolved = n, "reply-pair reconciler migrated rows");
            }
            Err(e) => {
                tracing::warn!("Reply-pair reconcile error: {e}");
            }
        }
    }
}

/// Periodic refresh of the materialized `contacts` table. 5-minute cadence
/// matches the plan; full-table aggregate is fine for typical mailboxes.
/// Past ~100k messages, switch to incremental by `messages.id > last_seen_id`.
pub async fn contacts_refresher_loop(state: Arc<AppState>, mut shutdown_rx: watch::Receiver<bool>) {
    let mut ticker = interval(Duration::from_secs(300));
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                    tracing::info!("Contacts refresher exiting: shutdown requested");
                    break;
                }
                continue;
            }
        }
        match state.store.refresh_contacts().await {
            Ok(n) => tracing::debug!(rows = n, "contacts refresher updated table"),
            Err(e) => tracing::warn!("Contacts refresh error: {e}"),
        }
    }
}

/// Pre-compute the default Wrapped (current YTD, default account) on
/// startup and keep it warm. Wrapped runs ~10 SQL queries against the
/// store; on large mailboxes a cold call is multi-second, sometimes
/// minutes. Warming once at startup means opening the Wrapped tab in
/// the TUI is normally instant.
///
/// Cadence (15m) is shorter than `WRAPPED_CACHE_TTL` (30m) so the cache
/// never expires under steady-state — every tick re-primes the entry
/// before it would naturally roll over. Errors are logged and the
/// loop keeps running; warming is best-effort.
pub async fn wrapped_warmer_loop(state: Arc<AppState>, mut shutdown_rx: watch::Receiver<bool>) {
    // Prime once immediately at startup — the whole point is to absorb
    // the first cold call before the user gets to the Wrapped tab.
    warm_default_wrapped(&state).await;

    let mut ticker = interval(Duration::from_secs(15 * 60));
    // The first tick of `interval` fires immediately; we already
    // warmed once above, so consume that first tick.
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                    tracing::info!("Wrapped warmer exiting: shutdown requested");
                    break;
                }
                continue;
            }
        }
        warm_default_wrapped(&state).await;
    }
}

async fn warm_default_wrapped(state: &Arc<AppState>) {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    let Some(start) = Utc.with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0).single() else {
        return;
    };
    let since_unix = start.timestamp();
    let until_unix = now.timestamp();
    let label = format!("{} year-to-date", now.year());

    // Account scopes to warm: the implicit "all accounts" key (which the
    // TUI sends when no `--account` filter is set) plus every configured
    // account. This way the first Wrapped open after switching accounts
    // is also instant. Each scope is its own cache key so they don't
    // collide.
    let mut scopes: Vec<Option<mxr_core::id::AccountId>> = vec![None];
    for (account_id, _) in state.sync_provider_entries() {
        scopes.push(Some(account_id));
    }

    for account_id in scopes {
        let cache_key = crate::state::WrappedCacheKey {
            account_id: account_id.clone(),
            label: label.clone(),
        };
        let started = std::time::Instant::now();
        match state
            .store
            .wrapped_summary(account_id.as_ref(), since_unix, until_unix, &label)
            .await
        {
            Ok(summary) => {
                state.wrapped_cache_put(cache_key, Arc::new(summary));
                tracing::debug!(
                    label = %label,
                    account = ?account_id.as_ref().map(mxr_core::AccountId::as_str),
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "wrapped warmer primed cache"
                );
            }
            Err(e) => {
                tracing::warn!(
                    label = %label,
                    account = ?account_id.as_ref().map(mxr_core::AccountId::as_str),
                    "wrapped warmer failed: {e}"
                );
            }
        }
    }
}

/// Process all auto-reminders due by `now`: mark each as triggered,
/// emit a `ReminderTriggered` event so clients can refresh views.
/// Returns the number of reminders that fired.
///
/// Factored out of `auto_reminders_loop` so it can be exercised
/// directly in tests with a virtual `now` — no clock plumbing needed
/// in the test harness.
pub async fn process_due_reminders(
    state: &AppState,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<u32, String> {
    let due = state
        .store
        .get_due_auto_reminders(now)
        .await
        .map_err(|e| e.to_string())?;
    let count = due.len() as u32;
    for reminder in due {
        let id = reminder.sent_message_id.clone();
        if let Err(e) = state.store.mark_auto_reminder_triggered(&id, now).await {
            tracing::warn!(
                message_id = %id.as_str(),
                "auto-reminder mark-triggered failed: {e}"
            );
            continue;
        }
        if let Err(e) = crate::handler::reply_later::set_reply_later_at(state, &id, true, now).await
        {
            tracing::warn!(
                message_id = %id.as_str(),
                "auto-reminder reply-later marker failed: {e}"
            );
        }
        crate::chimes::emit_daemon_event(
            state,
            DaemonEvent::ReminderTriggered {
                sent_message_id: id,
            },
        );
    }
    Ok(count)
}

/// Process all scheduled drafts due by `now`: invoke the existing
/// send pipeline (`send_stored_draft`) for each. Returns the number of
/// drafts that fired (regardless of send outcome — we count attempts).
///
/// Factored out for direct test access; the surrounding loop just
/// calls this on each tick.
pub async fn process_due_scheduled_sends(
    state: &AppState,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<u32, String> {
    let due = state
        .store
        .get_due_scheduled_drafts(now)
        .await
        .map_err(|e| e.to_string())?;
    let count = due.len() as u32;
    for draft_id in due {
        // Clear `send_at` and record a durable attempt marker in one
        // transaction, BEFORE sending — so a retry from a crashed prior
        // attempt can't re-fire indefinitely (at-most-once), and a crash
        // mid-send leaves an unresolved marker we can surface at startup
        // rather than a silent loss.
        if let Err(e) = state
            .store
            .clear_send_at_and_record_attempt(&draft_id, now)
            .await
        {
            tracing::warn!(
                draft_id = %draft_id,
                "scheduled-send: failed to clear send_at / record attempt before send: {e}"
            );
            continue;
        }
        let outcome = match crate::handler::send_stored_draft(state, &draft_id, None).await {
            Ok(_) => {
                tracing::debug!(draft_id = %draft_id, "scheduled-send: sent");
                "sent"
            }
            Err(e) => {
                if e.to_string().contains("draft safety blocked send") {
                    // Keep the draft; the schedule is already cleared by
                    // clear_send_at_and_record_attempt above. The error
                    // already carries a freshly-minted single-use override
                    // token and the exact resend command, so just surface it.
                    tracing::warn!(
                        draft_id = %draft_id,
                        "scheduled-send: blocked by safety pipeline; schedule cleared. {e}"
                    );
                    "blocked"
                } else {
                    tracing::warn!(draft_id = %draft_id, "scheduled-send: send failed: {e}");
                    "failed"
                }
            }
        };
        // Resolve the attempt marker so it isn't surfaced as a lost send.
        if let Err(e) = state
            .store
            .record_scheduled_send_outcome(&draft_id, now, outcome)
            .await
        {
            tracing::warn!(
                draft_id = %draft_id,
                "scheduled-send: failed to record outcome '{outcome}': {e}"
            );
        }
    }
    Ok(count)
}

/// Background loop: flush due scheduled sends on a 60-second cadence.
pub async fn scheduled_sends_loop(state: Arc<AppState>, mut shutdown_rx: watch::Receiver<bool>) {
    let mut ticker = interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                    tracing::info!("Scheduled-sends loop exiting: shutdown requested");
                    break;
                }
                continue;
            }
        }
        match process_due_scheduled_sends(&state, chrono::Utc::now()).await {
            Ok(0) => {}
            Ok(n) => tracing::debug!(fired = n, "scheduled-sends loop fired"),
            Err(e) => tracing::warn!("Scheduled-sends loop error: {e}"),
        }
    }
}

/// Background loop: scan auto-reminders on a 60-second cadence and
/// fire any whose window has elapsed.
/// Daily sweep that hard-deletes activity rows older than the per-tier
/// retention windows. Mirrors `auto_reminders_loop` shape. The recorder
/// also writes a synthesized `activity.pruned` marker for each tier that
/// produced deletions so users can audit retention behavior.
pub async fn activity_prune_loop(state: Arc<AppState>, mut shutdown_rx: watch::Receiver<bool>) {
    use mxr_protocol::ClientKind;
    use mxr_store::Tier;

    const DAY_MS: i64 = 86_400_000;
    // Run once shortly after startup, then every 24 h.
    let mut ticker = interval(Duration::from_secs(86_400));
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                    tracing::info!("Activity prune loop exiting: shutdown requested");
                    break;
                }
                continue;
            }
        }
        let cfg = state.config_snapshot().activity.retention;
        let now_ms = chrono::Utc::now().timestamp_millis();
        for (tier, days) in [
            (Tier::Ephemeral, cfg.ephemeral_days),
            (Tier::Standard, cfg.standard_days),
            (Tier::Important, cfg.important_days),
        ] {
            let cutoff = now_ms - (days as i64) * DAY_MS;
            match state.store.prune_activity_before(cutoff, Some(tier)).await {
                Ok(0) => {}
                Ok(n) => {
                    tracing::debug!(
                        rows = n,
                        tier = tier.as_str(),
                        "activity prune deleted rows"
                    );
                    state.activity.record(crate::activity::OwnedEntry {
                        ts: now_ms,
                        account_id: None,
                        source: ClientKind::Daemon,
                        action: "activity.pruned".into(),
                        target_kind: None,
                        target_id: None,
                        tier: Tier::Important,
                        context: Some(serde_json::json!({
                            "tier": tier.as_str(),
                            "before_ts": cutoff,
                            "deleted": n,
                        })),
                    });
                }
                Err(e) => {
                    tracing::warn!(error = %e, tier = tier.as_str(), "activity prune failed");
                }
            }
        }
    }
}

pub async fn mutation_dedup_prune_loop(
    state: Arc<AppState>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    // Mutation dedup rows have a 24h TTL; prune hourly so the table
    // stays bounded under heavy mutation traffic. The undo log gets
    // pruned alongside since both tables share the daemon's
    // maintenance cadence.
    let mut ticker = interval(Duration::from_secs(3600));
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                    tracing::info!("Mutation dedup prune loop exiting: shutdown requested");
                    break;
                }
                continue;
            }
        }
        let now = chrono::Utc::now().timestamp();
        match state.store.prune_expired_mutation_dedup(now).await {
            Ok(0) => {}
            Ok(n) => tracing::debug!(rows = n, "mutation dedup prune deleted rows"),
            Err(e) => tracing::warn!(error = %e, "mutation dedup prune failed"),
        }
        match state.store.prune_expired_undo_entries(now).await {
            Ok(0) => {}
            Ok(n) => tracing::debug!(rows = n, "mutation undo prune deleted rows"),
            Err(e) => tracing::warn!(error = %e, "mutation undo prune failed"),
        }
    }
}

pub async fn auto_reminders_loop(state: Arc<AppState>, mut shutdown_rx: watch::Receiver<bool>) {
    let mut ticker = interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                    tracing::info!("Auto-reminders loop exiting: shutdown requested");
                    break;
                }
                continue;
            }
        }
        match process_due_reminders(&state, chrono::Utc::now()).await {
            Ok(0) => {}
            Ok(n) => tracing::debug!(fired = n, "auto-reminders loop fired reminders"),
            Err(e) => tracing::warn!("Auto-reminders loop error: {e}"),
        }
    }
}

pub async fn snooze_loop(state: Arc<AppState>, mut shutdown_rx: watch::Receiver<bool>) {
    let mut ticker = interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                    tracing::info!("Snooze loop exiting: daemon shutdown requested");
                    break;
                }
                continue;
            }
        }
        match state.store.get_due_snoozes(chrono::Utc::now()).await {
            Ok(snoozed) => {
                for item in snoozed {
                    let message_id = item.message_id.clone();
                    if let Err(e) = crate::handler::restore_snoozed_message(&state, &item).await {
                        tracing::error!(message_id = %message_id, "Snooze wake error: {e}");
                        continue;
                    }
                    crate::chimes::emit_daemon_event(
                        &state,
                        DaemonEvent::MessageUnsnoozed { message_id },
                    );
                }
            }
            Err(e) => {
                tracing::error!("Snooze check error: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::{Label, MxrError, SyncBatch, SyncCapabilities};
    use mxr_protocol::{IpcMessage, IpcPayload, Request, Response, ResponseData};

    struct HangingSyncProvider {
        account_id: AccountId,
    }

    #[async_trait::async_trait]
    impl MailSyncProvider for HangingSyncProvider {
        fn name(&self) -> &str {
            "hanging-sync"
        }

        fn account_id(&self) -> &AccountId {
            &self.account_id
        }

        fn capabilities(&self) -> SyncCapabilities {
            SyncCapabilities::default()
        }

        async fn authenticate(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn refresh_auth(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
            std::future::pending().await
        }

        async fn sync_messages(&self, _cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
            unreachable!("sync_labels hangs before sync_messages")
        }

        async fn fetch_attachment(
            &self,
            _provider_message_id: &str,
            _provider_attachment_id: &str,
        ) -> Result<Vec<u8>, MxrError> {
            Err(MxrError::NotFound("no attachment".into()))
        }

        async fn apply_mutation(
            &self,
            _mutation_id: &str,
            _mutation: &mxr_core::Mutation,
        ) -> Result<(), MxrError> {
            Ok(())
        }
    }

    /// A provider that never returns must not be cancelled mid-flight.
    /// The loop's wait times out, the status row stays honest
    /// (`sync_in_progress=true` + "still running" marker), and the loop
    /// itself keeps running and shuts down cleanly.
    /// Succeeds with an empty batch after `delay`. Slow enough to
    /// outlive a short caller wait, fast enough for the reaper path to
    /// be observable in tests.
    struct SlowThenEmptySyncProvider {
        account_id: AccountId,
        delay: Duration,
    }

    #[async_trait::async_trait]
    impl MailSyncProvider for SlowThenEmptySyncProvider {
        fn name(&self) -> &str {
            "slow-then-empty-sync"
        }

        fn account_id(&self) -> &AccountId {
            &self.account_id
        }

        fn capabilities(&self) -> SyncCapabilities {
            SyncCapabilities::default()
        }

        async fn authenticate(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn refresh_auth(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
            tokio::time::sleep(self.delay).await;
            Ok(Vec::new())
        }

        async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
            Ok(SyncBatch {
                upserted: Vec::new(),
                deleted_provider_ids: Vec::new(),
                label_changes: Vec::new(),
                next_cursor: cursor.clone(),
                has_more: false,
                threads_changed: Vec::new(),
                remaining_estimate: None,
            })
        }

        async fn fetch_attachment(
            &self,
            _provider_message_id: &str,
            _provider_attachment_id: &str,
        ) -> Result<Vec<u8>, MxrError> {
            Err(MxrError::NotFound("no attachment".into()))
        }

        async fn apply_mutation(
            &self,
            _mutation_id: &str,
            _mutation: &mxr_core::Mutation,
        ) -> Result<(), MxrError> {
            Ok(())
        }
    }

    /// IDLE-capable provider double: records the label of every provider whose
    /// `idle_watch` is acquired, so a test can prove the IDLE loop re-fetches
    /// the refreshed provider after a reload. Its watcher blocks forever, so the
    /// only way the loop acquires a *second* watcher is by reconnecting.
    struct IdleReloadProvider {
        account_id: AccountId,
        label: &'static str,
        acquired: Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl IdleReloadProvider {
        fn new(
            account_id: AccountId,
            label: &'static str,
            acquired: Arc<std::sync::Mutex<Vec<&'static str>>>,
        ) -> Self {
            Self {
                account_id,
                label,
                acquired,
            }
        }
    }

    struct PendingWatcher;

    #[async_trait::async_trait]
    impl mxr_core::IdleWatcher for PendingWatcher {
        async fn next_event(&mut self) -> Result<(), MxrError> {
            std::future::pending().await
        }
    }

    #[async_trait::async_trait]
    impl MailSyncProvider for IdleReloadProvider {
        fn name(&self) -> &str {
            "idle-reload"
        }

        fn account_id(&self) -> &AccountId {
            &self.account_id
        }

        fn capabilities(&self) -> SyncCapabilities {
            SyncCapabilities::default()
        }

        async fn authenticate(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn refresh_auth(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
            Ok(Vec::new())
        }

        async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
            Ok(SyncBatch {
                upserted: Vec::new(),
                deleted_provider_ids: Vec::new(),
                label_changes: Vec::new(),
                next_cursor: cursor.clone(),
                has_more: false,
                threads_changed: Vec::new(),
                remaining_estimate: None,
            })
        }

        async fn fetch_attachment(
            &self,
            _provider_message_id: &str,
            _provider_attachment_id: &str,
        ) -> Result<Vec<u8>, MxrError> {
            Err(MxrError::NotFound("no attachment".into()))
        }

        async fn apply_mutation(
            &self,
            _mutation_id: &str,
            _mutation: &mxr_core::Mutation,
        ) -> Result<(), MxrError> {
            Ok(())
        }

        async fn idle_watch(&self) -> Result<Option<Box<dyn mxr_core::IdleWatcher>>, MxrError> {
            self.acquired
                .lock()
                .expect("acquired lock")
                .push(self.label);
            Ok(Some(Box::new(PendingWatcher)))
        }
    }

    /// Regression test for the consulting stale-credential bug: the persistent
    /// IDLE loop captured its provider once, so a rotated password stayed cached
    /// until a full daemon restart. After `reload_accounts_from_disk` swaps the
    /// provider runtime and signals a reload, the IDLE loop must drop its
    /// connection and reconnect using the REFRESHED provider.
    #[tokio::test]
    async fn idle_loop_reconnects_with_refreshed_provider_after_reload() {
        let account_id = AccountId::new();
        let acquired = Arc::new(std::sync::Mutex::new(Vec::<&'static str>::new()));

        let old_provider = Arc::new(IdleReloadProvider::new(
            account_id.clone(),
            "old",
            acquired.clone(),
        ));
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, old_provider, None)
                .await
                .unwrap(),
        );

        let shutdown_rx = state.shutdown_receiver();
        let loop_state = state.clone();
        let loop_account_id = account_id.clone();
        let handle = tokio::spawn(async move {
            idle_loop_for_account(loop_state, loop_account_id, shutdown_rx).await;
        });

        // The loop acquires the OLD provider's watcher first.
        wait_until(&acquired, |log| log.contains(&"old")).await;

        // Swap the runtime provider (as reload does) and signal the reload.
        let new_provider = Arc::new(IdleReloadProvider::new(
            account_id.clone(),
            "new",
            acquired.clone(),
        ));
        state.add_sync_provider_for_test(new_provider);
        state.signal_providers_reloaded();

        // The loop must drop the old watcher and reconnect with the NEW provider.
        wait_until(&acquired, |log| log.contains(&"new")).await;

        state.request_shutdown();
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;

        let log = acquired.lock().expect("acquired lock").clone();
        assert_eq!(
            log.first(),
            Some(&"old"),
            "idle loop should start on the original provider"
        );
        assert!(
            log.contains(&"new"),
            "idle loop must reconnect with the refreshed provider after reload, got {log:?}"
        );
    }

    /// Deterministic guard for the exact read→mark race Codex flagged. The
    /// `idle_reload_race_hook` fires precisely in the window between the provider
    /// read and the reload-generation snapshot; the hook swaps in the NEW
    /// provider and signals the reload right there. With the fix (snapshot BEFORE
    /// the read + `has_changed` re-check) the loop notices and opens its watcher
    /// on the NEW provider — never touching OLD. Reverting the fix (snapshot
    /// AFTER the read, no re-check) masks the signal, the loop blocks on OLD
    /// forever, and this test times out. See the revert-check in the PR notes.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn idle_loop_opens_watcher_on_new_provider_when_reload_lands_in_read_window() {
        let account_id = AccountId::new();
        let acquired = Arc::new(std::sync::Mutex::new(Vec::<&'static str>::new()));

        let old_provider = Arc::new(IdleReloadProvider::new(
            account_id.clone(),
            "old",
            acquired.clone(),
        ));
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, old_provider, None)
                .await
                .unwrap(),
        );

        // Inject the reload at exactly the read→mark window. Fires once.
        let hook_state = state.clone();
        let hook_account_id = account_id.clone();
        let hook_acquired = acquired.clone();
        idle_race_hook::install(
            account_id.clone(),
            Box::new(move || {
                let new_provider = Arc::new(IdleReloadProvider::new(
                    hook_account_id.clone(),
                    "new",
                    hook_acquired.clone(),
                ));
                hook_state.add_sync_provider_for_test(new_provider);
                hook_state.signal_providers_reloaded();
            }),
        );

        let shutdown_rx = state.shutdown_receiver();
        let loop_state = state.clone();
        let loop_account_id = account_id.clone();
        let handle = tokio::spawn(async move {
            idle_loop_for_account(loop_state, loop_account_id, shutdown_rx).await;
        });

        // The loop must open its watcher on NEW, having re-fetched after the
        // injected reload — it must never establish a watcher on OLD.
        wait_until(&acquired, |log| log.contains(&"new")).await;

        state.request_shutdown();
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;

        let log = acquired.lock().expect("acquired lock").clone();
        assert_eq!(
            log,
            vec!["new"],
            "loop must open its watcher only on the refreshed provider, got {log:?}"
        );
    }

    async fn wait_until(
        acquired: &Arc<std::sync::Mutex<Vec<&'static str>>>,
        predicate: impl Fn(&[&'static str]) -> bool,
    ) {
        for _ in 0..200 {
            if predicate(&acquired.lock().expect("acquired lock")) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "condition not met within timeout; acquired={:?}",
            acquired.lock().expect("acquired lock")
        );
    }

    /// Stale `sync_in_progress=true` rows from a daemon that died
    /// mid-sync are cleared by the startup reconciliation.
    #[tokio::test]
    async fn reconcile_interrupted_syncs_clears_stale_in_progress_rows() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_provider().account_id().clone();
        let _ = state
            .store
            .upsert_sync_runtime_status(
                &account_id,
                &SyncRuntimeStatusUpdate {
                    sync_in_progress: Some(true),
                    ..Default::default()
                },
            )
            .await;

        reconcile_interrupted_syncs(&state).await;

        let status = state
            .store
            .get_sync_runtime_status(&account_id)
            .await
            .unwrap()
            .expect("status row should exist");
        assert!(!status.sync_in_progress);
        assert_eq!(status.failure_class.as_deref(), Some("interrupted"));
        assert!(status
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("daemon restart")));
    }

    #[tokio::test]
    async fn sync_loop_detaches_stuck_provider_and_marks_status_still_running() {
        let account_id = AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let provider = std::sync::Arc::new(HangingSyncProvider {
            account_id: account_id.clone(),
        });
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, provider, None)
                .await
                .unwrap(),
        );
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let loop_state = state.clone();
        let loop_account_id = account_id.clone();
        let handle = tokio::spawn(async move {
            sync_loop_for_account(loop_state, loop_account_id, shutdown_rx).await;
        });

        let mut observed = None;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let status = state
                .store
                .get_sync_runtime_status(&account_id)
                .await
                .unwrap();
            if status
                .as_ref()
                .is_some_and(|status| status.sync_in_progress && status.last_error.is_some())
            {
                observed = status;
                break;
            }
        }

        shutdown_tx.send_replace(true);
        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .unwrap()
            .unwrap();

        let status = observed.expect("sync timeout should mark runtime status");
        // The sync was detached, not cancelled: it is genuinely still
        // running, and the row says so.
        assert!(status.sync_in_progress);
        assert_eq!(status.failure_class.as_deref(), Some("timeout"));
        assert!(
            status
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("still running")),
            "expected still-running marker, got {:?}",
            status.last_error
        );
    }

    /// A sync that finishes *after* the wait limit must still be
    /// finalized by the reaper: status row cleared, success recorded,
    /// provider guard released so the next sync can run.
    #[tokio::test]
    async fn detached_sync_is_finalized_by_reaper_after_late_completion() {
        let account_id = AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let provider = std::sync::Arc::new(SlowThenEmptySyncProvider {
            account_id: account_id.clone(),
            delay: Duration::from_millis(200),
        });
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, provider.clone(), None)
                .await
                .unwrap(),
        );

        let pass = begin_sync_pass(
            &state,
            provider as std::sync::Arc<dyn MailSyncProvider>,
            None,
            SyncStarter::BlockingClient,
        )
        .await;
        let wait = run_sync_pass(pass, Duration::from_millis(20)).await;
        assert!(
            matches!(wait, SyncWait::Detached),
            "200ms sync must outlive a 20ms wait"
        );

        // The reaper finalizes once the sync completes: poll for the
        // cleared flag, then confirm the guard was released.
        let mut finalized = None;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let status = state
                .store
                .get_sync_runtime_status(&account_id)
                .await
                .unwrap();
            if status.as_ref().is_some_and(|s| !s.sync_in_progress) {
                finalized = status;
                break;
            }
        }
        let status = finalized.expect("reaper should clear sync_in_progress after late completion");
        assert!(status.last_success_at.is_some());
        assert_eq!(status.last_error, None);
        let reacquired = tokio::time::timeout(
            Duration::from_millis(500),
            state.acquire_provider_operation(&account_id),
        )
        .await;
        assert!(
            reacquired.is_ok(),
            "provider guard must be released by the reaper"
        );
    }

    /// A detached sync that *never* completes must not hold the provider
    /// guard forever. After the grace period the reaper aborts it, records
    /// the failure, and releases the lock so the next sync can run.
    /// Regression for the 7-day wedge: a stuck sync held the per-account
    /// lock indefinitely and every later sync blocked on it.
    #[tokio::test]
    async fn detached_sync_that_never_finishes_is_aborted_and_releases_lock() {
        let account_id = AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let provider = std::sync::Arc::new(HangingSyncProvider {
            account_id: account_id.clone(),
        });
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, provider.clone(), None)
                .await
                .unwrap(),
        );

        let pass = begin_sync_pass(
            &state,
            provider as std::sync::Arc<dyn MailSyncProvider>,
            None,
            SyncStarter::BlockingClient,
        )
        .await;
        let wait = run_sync_pass(pass, Duration::from_millis(20)).await;
        assert!(
            matches!(wait, SyncWait::Detached),
            "a never-finishing sync must outlive the caller wait"
        );

        // The reaper aborts after DETACHED_SYNC_GRACE: poll well past it
        // for the cleared flag, then confirm the guard was released.
        let mut finalized = None;
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let status = state
                .store
                .get_sync_runtime_status(&account_id)
                .await
                .unwrap();
            if status.as_ref().is_some_and(|s| !s.sync_in_progress) {
                finalized = status;
                break;
            }
        }
        let status =
            finalized.expect("reaper should clear sync_in_progress after aborting a wedged sync");
        assert!(
            status
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("aborted")),
            "expected abort marker, got {:?}",
            status.last_error
        );
        let reacquired = tokio::time::timeout(
            Duration::from_millis(500),
            state.acquire_provider_operation(&account_id),
        )
        .await;
        assert!(
            reacquired.is_ok(),
            "provider guard must be released after the reaper aborts a wedged sync"
        );
    }

    /// Test double for a provider that ends a backfill with an empty page.
    /// Gmail does this whenever the page after a `nextPageToken` turns out to
    /// hold nothing, and IMAP when the last UID chunk is all deleted — the
    /// case that used to leave the whole-table analytics repair unrun, because
    /// the fan-out that owned it only ran for a page that carried messages.
    struct PagesThenEmptySyncProvider {
        account_id: AccountId,
        pages_served: std::sync::atomic::AtomicU32,
        pages: u32,
    }

    #[async_trait::async_trait]
    impl MailSyncProvider for PagesThenEmptySyncProvider {
        fn name(&self) -> &str {
            "pages-then-empty-sync"
        }

        fn account_id(&self) -> &AccountId {
            &self.account_id
        }

        fn capabilities(&self) -> SyncCapabilities {
            SyncCapabilities::default()
        }

        async fn authenticate(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn refresh_auth(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
            Ok(Vec::new())
        }

        /// Mid-backfill cursors are marked, so the daemon defers the
        /// relationship refresh exactly as it does for a real provider.
        fn is_backfill_cursor(&self, cursor: &SyncCursor) -> bool {
            cursor.as_bytes().starts_with(b"page:")
        }

        async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
            use std::sync::atomic::Ordering;
            let served = self.pages_served.fetch_add(1, Ordering::SeqCst);
            let has_more = served + 1 < self.pages;
            let next_cursor = if has_more {
                SyncCursor::from_bytes(format!("page:{}", served + 1).into_bytes())
            } else {
                SyncCursor::from_bytes(b"done".to_vec())
            };
            let upserted = if has_more {
                let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
                    .account_id(self.account_id.clone())
                    .provider_id(format!("page-{served}"))
                    .build();
                let body = crate::test_fixtures::make_empty_body(&envelope.id);
                vec![mxr_core::types::SyncedMessage { envelope, body }]
            } else {
                // The empty final page: nothing synced, nothing left to page.
                Vec::new()
            };
            let _ = cursor;
            Ok(SyncBatch {
                upserted,
                deleted_provider_ids: Vec::new(),
                label_changes: Vec::new(),
                next_cursor,
                has_more,
                threads_changed: Vec::new(),
                remaining_estimate: None,
            })
        }

        async fn fetch_attachment(
            &self,
            _provider_message_id: &str,
            _provider_attachment_id: &str,
        ) -> Result<Vec<u8>, MxrError> {
            Err(MxrError::NotFound("no attachment".into()))
        }

        async fn apply_mutation(
            &self,
            _mutation_id: &str,
            _mutation: &mxr_core::Mutation,
        ) -> Result<(), MxrError> {
            Ok(())
        }
    }

    /// Poll for the whole-table analytics repair having run.
    /// `analytics_startup_repair_done` is the cheapest probe: the repair swaps
    /// it on its first call and nothing else touches it. The repair is
    /// spawned, so give it a moment.
    async fn analytics_repair_ran(state: &Arc<AppState>) -> bool {
        use std::sync::atomic::Ordering;
        for _ in 0..100 {
            if state.analytics_startup_repair_done.load(Ordering::SeqCst) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    /// The repair is a whole-table scan, so it belongs at the end of a
    /// backfill rather than after every page — but it must still happen when
    /// the backfill's last page carries no messages at all. Nothing else in
    /// the daemon runs it: there is no startup call, so a skipped repair is a
    /// mailbox whose analytics stay wrong until the next sync that happens to
    /// land messages.
    #[tokio::test]
    async fn analytics_repair_is_deferred_through_a_backfill_and_run_on_its_empty_last_page() {
        let account_id = AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let provider = std::sync::Arc::new(PagesThenEmptySyncProvider {
            account_id: account_id.clone(),
            pages_served: std::sync::atomic::AtomicU32::new(0),
            pages: 2,
        });
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, provider.clone(), None)
                .await
                .unwrap(),
        );
        let provider = provider as std::sync::Arc<dyn MailSyncProvider>;

        // Page one: one message, more to come. The repair waits.
        let pass = begin_sync_pass(&state, provider.clone(), None, SyncStarter::AccountLoop).await;
        let SyncWait::Finished { pass, result } = run_sync_pass(pass, Duration::from_secs(5)).await
        else {
            panic!("a fast sync must not detach");
        };
        let outcome = finalize_sync_pass(pass, result).await.unwrap();
        assert!(outcome.has_more, "the first page has more to come");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !state
                .analytics_startup_repair_done
                .load(std::sync::atomic::Ordering::SeqCst),
            "a page with more to come must not pay for the whole-table repair"
        );
        assert!(
            state.has_deferred_relationship_contacts(&account_id),
            "a mid-backfill page defers its contacts instead of enqueueing them"
        );

        // Page two: empty, and the end of the backfill. The debt falls due
        // even though this page synced nothing.
        let pass = begin_sync_pass(&state, provider, None, SyncStarter::AccountLoop).await;
        let SyncWait::Finished { pass, result } = run_sync_pass(pass, Duration::from_secs(5)).await
        else {
            panic!("a fast sync must not detach");
        };
        let outcome = finalize_sync_pass(pass, result).await.unwrap();
        assert_eq!(outcome.synced_count, 0, "the last page is empty");
        assert!(!outcome.has_more);
        assert!(
            analytics_repair_ran(&state).await,
            "the repair must run when the backfill ends, even on an empty page"
        );
        assert!(
            !state.has_deferred_relationship_contacts(&account_id),
            "the deferred contacts must be handed over when the backfill ends, \
             even though the page that ended it carried no messages"
        );
    }

    /// A background sync claimed while a pass is finalizing must survive that
    /// pass's status write. The window is two lines wide, so rather than race
    /// it this pins the repair: the finalizer re-checks after writing, and a
    /// claim held across the check leaves the account marked busy.
    #[tokio::test]
    async fn a_claim_taken_during_finalization_leaves_the_account_marked_busy() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_provider().account_id().clone();
        let _ = state
            .store
            .upsert_sync_runtime_status(
                &account_id,
                &SyncRuntimeStatusUpdate {
                    sync_in_progress: Some(false),
                    ..Default::default()
                },
            )
            .await;

        let claim = AppState::claim_background_sync(&state, &account_id)
            .expect("the account starts unclaimed");
        reassert_busy_if_background_sync_queued(&state, &account_id).await;
        let status = state
            .store
            .get_sync_runtime_status(&account_id)
            .await
            .unwrap()
            .expect("sync status row");
        assert!(
            status.sync_in_progress,
            "a queued claim must be re-asserted over a finalizer's write"
        );

        drop(claim);
        let _ = state
            .store
            .upsert_sync_runtime_status(
                &account_id,
                &SyncRuntimeStatusUpdate {
                    sync_in_progress: Some(false),
                    ..Default::default()
                },
            )
            .await;
        reassert_busy_if_background_sync_queued(&state, &account_id).await;
        let status = state
            .store
            .get_sync_runtime_status(&account_id)
            .await
            .unwrap()
            .expect("sync status row");
        assert!(
            !status.sync_in_progress,
            "with no claim outstanding the account stays idle"
        );
    }

    /// An idle tick that synced nothing and owes nothing has no reason to
    /// rescan the whole mailbox — the mirror of the case above, and the reason
    /// the debt is tracked rather than "run it whenever a pass ends".
    #[tokio::test]
    async fn analytics_repair_does_not_run_for_a_sync_that_owes_nothing() {
        let account_id = AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let provider = std::sync::Arc::new(PagesThenEmptySyncProvider {
            account_id: account_id.clone(),
            pages_served: std::sync::atomic::AtomicU32::new(0),
            pages: 1,
        });
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, provider.clone(), None)
                .await
                .unwrap(),
        );

        let pass = begin_sync_pass(
            &state,
            provider as std::sync::Arc<dyn MailSyncProvider>,
            None,
            SyncStarter::AccountLoop,
        )
        .await;
        let SyncWait::Finished { pass, result } = run_sync_pass(pass, Duration::from_secs(5)).await
        else {
            panic!("a fast sync must not detach");
        };
        finalize_sync_pass(pass, result).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            !state
                .analytics_startup_repair_done
                .load(std::sync::atomic::Ordering::SeqCst),
            "an empty tick with no deferred work must not rescan the mailbox"
        );
    }

    /// A pass that finishes while a background sync is still queued behind the
    /// provider lock must not report the account idle: the client that was
    /// acked is waiting for its own sync, and `mxr sync --wait` would
    /// otherwise return having waited for somebody else's.
    #[tokio::test]
    async fn a_pass_finishing_under_a_queued_background_sync_keeps_the_account_busy() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_provider().account_id().clone();
        let claim = AppState::claim_background_sync(&state, &account_id)
            .expect("the account starts unclaimed");

        let pass = begin_sync_pass(
            &state,
            state.default_provider(),
            None,
            SyncStarter::AccountLoop,
        )
        .await;
        let SyncWait::Finished { pass, result } = run_sync_pass(pass, Duration::from_secs(5)).await
        else {
            panic!("a fast sync must not detach");
        };
        let outcome = finalize_sync_pass(pass, result).await.unwrap();
        assert!(!outcome.has_more, "the fake provider drains in one page");

        let status = state
            .store
            .get_sync_runtime_status(&account_id)
            .await
            .unwrap()
            .expect("sync status row");
        assert!(
            status.sync_in_progress,
            "a queued background sync keeps the account busy"
        );

        // And once the claim goes, the next pass is free to report idle.
        drop(claim);
        let pass = begin_sync_pass(
            &state,
            state.default_provider(),
            None,
            SyncStarter::AccountLoop,
        )
        .await;
        let SyncWait::Finished { pass, result } = run_sync_pass(pass, Duration::from_secs(5)).await
        else {
            panic!("a fast sync must not detach");
        };
        finalize_sync_pass(pass, result).await.unwrap();
        let status = state
            .store
            .get_sync_runtime_status(&account_id)
            .await
            .unwrap()
            .expect("sync status row");
        assert!(!status.sync_in_progress);
    }

    /// A detached sync is aborted for being wedged, not for being slow.    /// A detached sync is aborted for being wedged, not for being slow. This
    /// one runs for three grace periods while reporting progress throughout,
    /// and must be allowed to finish: aborting on total elapsed time would
    /// kill every large backfill.
    #[tokio::test]
    async fn detached_sync_that_keeps_reporting_progress_is_not_aborted() {
        let account_id = AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let provider = std::sync::Arc::new(SlowThenEmptySyncProvider {
            account_id: account_id.clone(),
            delay: DETACHED_SYNC_GRACE * 3,
        });
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, provider.clone(), None)
                .await
                .unwrap(),
        );

        let heartbeat_state = state.clone();
        let heartbeat_account = account_id.clone();
        let heartbeat = tokio::spawn(async move {
            loop {
                tokio::time::sleep(DETACHED_SYNC_GRACE / 5).await;
                heartbeat_state.record_sync_progress(
                    &heartbeat_account,
                    1,
                    "still working".to_string(),
                );
            }
        });

        let pass = begin_sync_pass(
            &state,
            provider as std::sync::Arc<dyn MailSyncProvider>,
            None,
            SyncStarter::BlockingClient,
        )
        .await;
        let wait = run_sync_pass(pass, Duration::from_millis(20)).await;
        assert!(
            matches!(wait, SyncWait::Detached),
            "a slow sync must outlive a 20ms wait"
        );

        let mut finalized = None;
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let status = state
                .store
                .get_sync_runtime_status(&account_id)
                .await
                .unwrap();
            if status.as_ref().is_some_and(|s| s.last_success_at.is_some()) {
                finalized = status;
                break;
            }
        }
        heartbeat.abort();
        let status = finalized.expect("a progressing detached sync must finish, not be aborted");
        assert_eq!(
            status.last_error, None,
            "it must not be recorded as aborted"
        );
    }

    /// Phase 3.1 / Behavior 4: a provider whose `idle_watch` returns
    /// `Ok(None)` (the default) does NOT keep an IDLE loop running.
    /// The sync loop continues with its periodic poll. Catches
    /// regressions where the watcher is spawned unconditionally and
    /// busy-loops re-attempting `idle_watch` on a Gmail / SMTP account.
    #[tokio::test]
    async fn idle_loop_exits_immediately_when_provider_does_not_support_idle() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_provider().account_id().clone();
        let (_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        // Default fake provider has `idle_trigger = None` → idle_watch
        // returns Ok(None) → loop returns immediately.
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            idle_loop_for_account(state.clone(), account_id, shutdown_rx),
        )
        .await;
        assert!(
            result.is_ok(),
            "idle loop must exit promptly when provider has no IDLE"
        );
    }

    /// Phase 3.1 / Behavior 1 (TUI-side proxy): an IDLE event from the
    /// watcher signals the per-account `Notify`, which the sync loop's
    /// select branch wakes on. We can't run the full sync loop here
    /// without bringing in the entire sync engine fixture, so this
    /// test verifies the wake-up plumbing — the watcher fires the
    /// notification, and the same Notify the sync loop awaits is the
    /// one that gets fired.
    #[tokio::test]
    async fn idle_event_wakes_per_account_notify() {
        use std::sync::Arc as StdArc;
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_provider().account_id().clone();

        // Enable IDLE on the fake provider. Get the trigger handle so
        // the test can simulate a server-pushed event. Install it in the runtime
        // (overwriting the default fake) so the loop re-fetches it from state —
        // the loop no longer takes a provider argument.
        let mut fake = mxr_provider_fake::FakeProvider::new(account_id.clone());
        let trigger = fake.enable_idle();
        let provider: StdArc<dyn mxr_core::MailSyncProvider> = StdArc::new(fake);
        state.add_sync_provider_for_test(provider);

        // The notify that the sync loop awaits.
        let notify = state.idle_notify_for_account(&account_id);

        let (_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let watcher_state = state.clone();
        let watcher_account = account_id.clone();
        let watcher_handle = tokio::spawn(async move {
            idle_loop_for_account(watcher_state, watcher_account, shutdown_rx).await;
        });

        // Race: fire the trigger; the watcher's next_event awaits resolve;
        // notify.notify_one() is called; our notified() future returns.
        trigger.notify_one();

        let woken = tokio::time::timeout(std::time::Duration::from_secs(2), notify.notified())
            .await
            .is_ok();
        assert!(woken, "Notify must fire within 2s after trigger");

        watcher_handle.abort();
    }

    /// Phase 3.1: `idle_notify_for_account` returns the same handle on
    /// repeated calls so watcher and sync loop see the same Notify.
    /// Catches "each call creates a new Notify" regressions where
    /// the wake-up never reaches the sync loop.
    #[tokio::test]
    async fn idle_notify_for_account_returns_stable_handle() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_provider().account_id().clone();
        let a = state.idle_notify_for_account(&account_id);
        let b = state.idle_notify_for_account(&account_id);
        assert!(
            std::sync::Arc::ptr_eq(&a, &b),
            "idle_notify_for_account must return the same Arc"
        );
    }

    #[tokio::test]
    async fn apply_rules_to_messages_marks_message_read_and_logs_history() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let outcome = state
            .sync_engine
            .sync_account_with_outcome(state.default_provider().as_ref())
            .await
            .unwrap();
        let mut unread_id = None;
        for message_id in &outcome.upserted_message_ids {
            let envelope = state.store.get_envelope(message_id).await.unwrap().unwrap();
            if !envelope.flags.contains(mxr_core::MessageFlags::READ) {
                unread_id = Some(message_id.clone());
                break;
            }
        }
        let unread_id = unread_id.expect("expected unread fixture message");
        let now = chrono::Utc::now();
        let rule = serde_json::json!({
            "id": "rule-1",
            "name": "Mark unread as read",
            "enabled": true,
            "priority": 10,
            "conditions": {"type":"field","field":"is_unread"},
            "actions": [{"type":"mark_read"}],
            "created_at": now,
            "updated_at": now
        });
        let _ = crate::handler::handle_request(
            &state,
            &IpcMessage {
                id: 1,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Request(Request::UpsertRule { rule }),
            },
        )
        .await;

        apply_rules_to_messages(
            &state,
            state.default_provider().account_id(),
            state.default_provider().as_ref(),
            std::slice::from_ref(&unread_id),
        )
        .await
        .unwrap();

        let envelope = state.store.get_envelope(&unread_id).await.unwrap().unwrap();
        assert!(envelope.flags.contains(mxr_core::MessageFlags::READ));

        let history = crate::handler::handle_request(
            &state,
            &IpcMessage {
                id: 2,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Request(Request::ListRuleHistory {
                    rule: Some("rule-1".to_string()),
                    limit: 10,
                }),
            },
        )
        .await;
        match history.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::RuleHistory { entries },
            }) => assert_eq!(entries.len(), 1),
            other => panic!("expected rule history, got {other:?}"),
        }
    }
}
