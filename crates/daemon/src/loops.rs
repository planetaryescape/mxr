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
use mxr_core::MailSyncProvider;
use mxr_protocol::*;
use mxr_rules::{Rule, RuleAction, RuleEngine, RuleExecutionLog};
use mxr_store::{SyncRuntimeStatusUpdate, SyncStatus as StoreSyncStatus};
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::{interval, Duration};

/// Spawn sync loops for all configured accounts.
pub fn spawn_sync_loops(state: Arc<AppState>) {
    for (account_id, provider) in state.sync_provider_entries() {
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
            let watcher_provider = provider.clone();
            let handle = tokio::spawn(async move {
                let shutdown_rx = loop_state.shutdown_receiver();
                idle_loop_for_account(
                    loop_state.clone(),
                    watcher_account_id.clone(),
                    watcher_provider,
                    shutdown_rx,
                )
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
    provider: Arc<dyn mxr_core::MailSyncProvider>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let notify = state.idle_notify_for_account(&account_id);
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
    let mut deferred_relationship_contacts = BTreeSet::<String>::new();
    // Phase 3.1: wake the sleep early when an IDLE watcher signals.
    let idle_notify = state.idle_notify_for_account(&account_id);

    loop {
        if *shutdown_rx.borrow() {
            tracing::info!(account = %account_id, "Sync loop exiting: daemon shutdown requested");
            break;
        }
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

        let started_at = chrono::Utc::now();
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
                    last_attempt_at: Some(started_at),
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

        match state
            .sync_engine
            .sync_account_with_outcome(provider.as_ref())
            .await
        {
            Ok(outcome) => {
                let count = outcome.synced_count;
                backoff_secs = 0;
                let idle_for = chrono::Utc::now() - last_message_sync_at;
                if count > 0 {
                    last_message_sync_at = chrono::Utc::now();
                }
                let post_sync_cursor = state
                    .store
                    .get_sync_cursor(&account_id)
                    .await
                    .ok()
                    .flatten();
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
                            sync_in_progress: Some(false),
                            current_cursor_summary: Some(Some(describe_sync_cursor(
                                provider.as_ref(),
                                post_sync_cursor.as_ref(),
                            ))),
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
                let _ = state
                    .store
                    .insert_event(
                        "info",
                        "sync",
                        &format!("Sync completed for {account_id}"),
                        Some(&account_id),
                        Some(&format!(
                            "messages_synced={count}; cursor={}",
                            describe_sync_cursor(provider.as_ref(), post_sync_cursor.as_ref())
                        )),
                    )
                    .await;
                if count > 0 {
                    let was_initial_backfill = pre_sync_cursor
                        .as_ref()
                        .is_some_and(|c| provider.is_backfill_cursor(c));
                    let initial_backfill_in_progress = post_sync_cursor
                        .as_ref()
                        .is_some_and(|c| provider.is_backfill_cursor(c));

                    // Critical: the post-sync fan-out (semantic ingest,
                    // contacts refresh, relationship profile, rules
                    // engine, analytics backfill) used to run inline
                    // here. That kept the sync loop blocked until every
                    // downstream worker had ack'd the enqueue and any
                    // network call inside the rules engine had
                    // returned. On a busy mailbox that meant 10+
                    // minutes between "Gmail has new mail" and "mxr
                    // shows new mail". Move the fan-out to a detached
                    // task so the loop returns immediately to its
                    // periodic sleep / IDLE wait. Each downstream
                    // worker has its own bounded channel; if a worker
                    // is slow, only that worker backs up.
                    let upserted_ids = outcome.upserted_message_ids.clone();
                    let mut handover_relationship_contacts = Vec::new();
                    if initial_backfill_in_progress
                        || was_initial_backfill
                        || !deferred_relationship_contacts.is_empty()
                    {
                        match state
                            .store
                            .relationship_contacts_for_messages(&upserted_ids)
                            .await
                        {
                            Ok(contacts) => {
                                deferred_relationship_contacts
                                    .extend(contacts.into_iter().map(|(_, email)| email));
                            }
                            Err(error) => {
                                tracing::warn!(account = %account_id, %error, "relationship backfill contact lookup failed");
                            }
                        }
                        if initial_backfill_in_progress {
                            tracing::debug!(account = %account_id, "relationship profile refresh deferred during initial backfill");
                        } else {
                            handover_relationship_contacts = deferred_relationship_contacts
                                .iter()
                                .cloned()
                                .map(|email| (account_id.clone(), email))
                                .collect::<Vec<_>>();
                            deferred_relationship_contacts.clear();
                        }
                    }

                    let fanout_state = state.clone();
                    let fanout_account = account_id.clone();
                    let fanout_provider = provider.clone();
                    let fanout_initial_in_progress = initial_backfill_in_progress;
                    tokio::spawn(async move {
                        post_sync_fanout(
                            fanout_state,
                            fanout_account,
                            fanout_provider,
                            upserted_ids,
                            handover_relationship_contacts,
                            fanout_initial_in_progress,
                        )
                        .await;
                    });
                    // No automatic summary backfill: even gated by
                    // `llm.enabled`, this previously spawned unbounded
                    // tokio tasks (one per changed thread) on every sync
                    // tick. On a 100k-message initial backfill that
                    // saturates the tokio runtime + LLM and the TUI
                    // grinds to a halt. Summaries are now generated
                    // strictly on demand when the user opens a thread.
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

                tracing::info!(account = %account_id, "Sync completed: {count} messages");
                let event = IpcMessage {
                    id: 0,
                    source: ::mxr_protocol::ClientKind::default(),
                    payload: IpcPayload::Event(DaemonEvent::SyncCompleted {
                        account_id: account_id.clone(),
                        messages_synced: count,
                    }),
                };
                let _ = state.event_tx.send(event);

                if let Ok(labels) = state.store.list_labels_by_account(&account_id).await {
                    let counts: Vec<_> = labels
                        .iter()
                        .map(|l| LabelCount {
                            label_id: l.id.clone(),
                            unread_count: l.unread_count,
                            total_count: l.total_count,
                        })
                        .collect();
                    let counts_event = IpcMessage {
                        id: 0,
                        source: ::mxr_protocol::ClientKind::default(),
                        payload: IpcPayload::Event(DaemonEvent::LabelCountsUpdated { counts }),
                    };
                    let _ = state.event_tx.send(counts_event);
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
            Err(e) => {
                let err_str = e.to_string();
                let failure_class = classify_sync_error(&err_str);
                let consecutive_failures = existing_status
                    .as_ref()
                    .map_or(1, |status| status.consecutive_failures.saturating_add(1));
                let mut backoff_until = None;
                if let mxr_core::MxrError::RateLimited { retry_after_secs } = &e {
                    backoff_secs = retry_after_secs.saturating_add(10);
                    backoff_until =
                        Some(chrono::Utc::now() + chrono::Duration::seconds(backoff_secs as i64));
                } else {
                    backoff_secs = (backoff_secs * 2).clamp(30, 300);
                }
                let post_error_cursor = state
                    .store
                    .get_sync_cursor(&account_id)
                    .await
                    .ok()
                    .flatten();
                let _ = state
                    .store
                    .upsert_sync_runtime_status(
                        &account_id,
                        &SyncRuntimeStatusUpdate {
                            last_error: Some(Some(err_str.clone())),
                            failure_class: Some(Some(failure_class.to_string())),
                            consecutive_failures: Some(consecutive_failures),
                            backoff_until: Some(backoff_until),
                            sync_in_progress: Some(false),
                            current_cursor_summary: Some(Some(describe_sync_cursor(
                                provider.as_ref(),
                                post_error_cursor.as_ref(),
                            ))),
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
                let _ = state
                    .store
                    .insert_event(
                        "error",
                        "sync",
                        &format!("Sync failed for {account_id}"),
                        Some(&account_id),
                        Some(&format!(
                            "class={failure_class}; error={err_str}; cursor={}",
                            describe_sync_cursor(provider.as_ref(), post_error_cursor.as_ref())
                        )),
                    )
                    .await;
                tracing::error!(account = %account_id, "Sync error: {err_str}");
                let event = IpcMessage {
                    id: 0,
                    source: ::mxr_protocol::ClientKind::default(),
                    payload: IpcPayload::Event(DaemonEvent::SyncError {
                        account_id: account_id.clone(),
                        error: err_str,
                    }),
                };
                let _ = state.event_tx.send(event);
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
    relationship_handover_contacts: Vec<(AccountId, String)>,
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
    if !relationship_handover_contacts.is_empty() {
        if let Err(error) = state
            .relationship
            .enqueue_contacts(relationship_handover_contacts)
            .await
        {
            tracing::warn!(account = %account_id, %error, "relationship handover enqueue failed");
        }
    } else if !initial_backfill_in_progress {
        if let Err(error) = state
            .relationship
            .enqueue_contacts_from_messages(&upserted_message_ids)
            .await
        {
            tracing::warn!(account = %account_id, %error, "relationship profile enqueue failed");
        }
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

    // Self-heal analytics derived data. Each step is a `WHERE column IS
    // NULL / = 'unknown'` filter so it costs near-zero on healthy data
    // and silently backfills the rest. Runs in the fan-out task — not
    // critical for next-tick sync responsiveness.
    let backfill = crate::handler::diagnostics_impl::incremental_analytics_backfill(&state).await;
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

fn classify_sync_error(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("rate limit") || lower.contains("retry after") {
        "rate_limit"
    } else if lower.contains("auth") || lower.contains("oauth") || lower.contains("login") {
        "auth"
    } else if lower.contains("timeout")
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
fn describe_sync_cursor(
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
        let event = IpcMessage {
            id: 0,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Event(DaemonEvent::ReminderTriggered {
                sent_message_id: id,
            }),
        };
        let _ = state.event_tx.send(event);
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
        // Clear the scheduled flag before sending so a retry from a
        // crashed prior attempt doesn't re-fire indefinitely.
        if let Err(e) = state.store.cancel_scheduled_send(&draft_id).await {
            tracing::warn!(
                draft_id = %draft_id,
                "scheduled-send: failed to clear send_at before send: {e}"
            );
            continue;
        }
        match crate::handler::send_stored_draft(state, &draft_id, None).await {
            Ok(_) => tracing::debug!(draft_id = %draft_id, "scheduled-send: sent"),
            Err(e) => {
                if e.to_string().contains("draft safety blocked send") {
                    // Per docs/ai-email/01-pre-send-safety.md: keep the
                    // draft, clear the schedule, log a warning event so
                    // the user notices on next sync. Without this the
                    // flusher would retry every tick forever.
                    if let Err(clear_err) = state.store.cancel_scheduled_send(&draft_id).await {
                        tracing::warn!(
                            draft_id = %draft_id,
                            "scheduled-send: failed to clear schedule after safety block: {clear_err}"
                        );
                    }
                    tracing::warn!(
                        draft_id = %draft_id,
                        "scheduled-send: blocked by safety pipeline; schedule cleared. Use --check to inspect, then resend with --override-safety <token>: {e}"
                    );
                } else {
                    tracing::warn!(
                        draft_id = %draft_id,
                        "scheduled-send: send failed: {e}"
                    );
                }
            }
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
                    let event = IpcMessage {
                        id: 0,
                        source: ::mxr_protocol::ClientKind::default(),
                        payload: IpcPayload::Event(DaemonEvent::MessageUnsnoozed { message_id }),
                    };
                    let _ = state.event_tx.send(event);
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
    use mxr_protocol::{IpcMessage, IpcPayload, Request, Response, ResponseData};

    /// Phase 3.1 / Behavior 4: a provider whose `idle_watch` returns
    /// `Ok(None)` (the default) does NOT keep an IDLE loop running.
    /// The sync loop continues with its periodic poll. Catches
    /// regressions where the watcher is spawned unconditionally and
    /// busy-loops re-attempting `idle_watch` on a Gmail / SMTP account.
    #[tokio::test]
    async fn idle_loop_exits_immediately_when_provider_does_not_support_idle() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_provider().account_id().clone();
        let provider = state.default_provider().clone();
        let (_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        // Default fake provider has `idle_trigger = None` → idle_watch
        // returns Ok(None) → loop returns immediately.
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            idle_loop_for_account(state.clone(), account_id, provider, shutdown_rx),
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
        // the test can simulate a server-pushed event.
        let mut fake = mxr_provider_fake::FakeProvider::new(account_id.clone());
        let trigger = fake.enable_idle();
        let provider: StdArc<dyn mxr_core::MailSyncProvider> = StdArc::new(fake);

        // The notify that the sync loop awaits.
        let notify = state.idle_notify_for_account(&account_id);

        let (_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let watcher_state = state.clone();
        let watcher_account = account_id.clone();
        let watcher_handle = tokio::spawn(async move {
            idle_loop_for_account(watcher_state, watcher_account, provider, shutdown_rx).await;
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
