#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::types::SyncCursor;
use mxr_core::MailSyncProvider;
use mxr_protocol::*;
use mxr_rules::{Rule, RuleAction, RuleEngine, RuleExecutionLog};
use mxr_store::{SyncRuntimeStatusUpdate, SyncStatus as StoreSyncStatus};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::{interval, Duration};

/// Spawn sync loops for all configured accounts.
pub fn spawn_sync_loops(state: Arc<AppState>) {
    for (account_id, _) in state.sync_provider_entries() {
        if !state.mark_sync_loop_spawned(&account_id) {
            continue;
        }
        let loop_state = state.clone();
        let task_state = state.clone();
        let task_account_id = account_id.clone();
        let handle = tokio::spawn(async move {
            let shutdown_rx = loop_state.shutdown_receiver();
            sync_loop_for_account(loop_state, task_account_id.clone(), shutdown_rx).await;
            task_state.finish_sync_loop(&task_account_id);
        });
        state.register_sync_loop_handle(account_id, handle);
    }
}

async fn sync_loop_for_account(
    state: Arc<AppState>,
    account_id: AccountId,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut backoff_secs: u64 = 0;
    let mut skip_sleep = true;

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
                            describe_sync_cursor(post_sync_cursor.as_ref())
                        )),
                    )
                    .await;
                if count > 0 {
                    if let Err(error) = state
                        .semantic
                        .enqueue_ingest_messages(&outcome.upserted_message_ids)
                        .await
                    {
                        tracing::error!(account = %account_id, "Semantic indexing failed: {error}");
                    }
                    if let Err(error) = apply_rules_to_messages(
                        &state,
                        &account_id,
                        provider.as_ref(),
                        &outcome.upserted_message_ids,
                    )
                    .await
                    {
                        tracing::error!(account = %account_id, "Rule execution failed: {error}");
                    }
                }
                tracing::info!(account = %account_id, "Sync completed: {count} messages");
                let event = IpcMessage {
                    id: 0,
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
                        payload: IpcPayload::Event(DaemonEvent::LabelCountsUpdated { counts }),
                    };
                    let _ = state.event_tx.send(counts_event);
                }

                if let Ok(Some(cursor)) = state.store.get_sync_cursor(&account_id).await {
                    if matches!(cursor, SyncCursor::GmailBackfill { .. }) {
                        tracing::info!(account = %account_id, "Backfill in progress, re-syncing in 2s");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        skip_sleep = true;
                        continue;
                    }
                }
            }
            Err(e) => {
                let err_str = e.to_string();
                let failure_class = classify_sync_error(&err_str);
                let consecutive_failures = existing_status
                    .as_ref()
                    .map(|status| status.consecutive_failures.saturating_add(1))
                    .unwrap_or(1);
                let mut backoff_until = None;
                if err_str.contains("Rate limited") {
                    let secs = err_str
                        .split("retry after ")
                        .nth(1)
                        .and_then(|s| s.trim_end_matches('s').parse::<u64>().ok())
                        .unwrap_or(120);
                    backoff_secs = secs + 10;
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
                            describe_sync_cursor(post_error_cursor.as_ref())
                        )),
                    )
                    .await;
                tracing::error!(account = %account_id, "Sync error: {err_str}");
                let event = IpcMessage {
                    id: 0,
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

fn describe_sync_cursor(cursor: Option<&SyncCursor>) -> String {
    match cursor {
        Some(SyncCursor::Initial) | None => "initial".to_string(),
        Some(SyncCursor::Gmail { history_id }) => format!("gmail history_id={history_id}"),
        Some(SyncCursor::GmailBackfill {
            history_id,
            page_token,
        }) => format!(
            "gmail_backfill history_id={history_id} page_token={}",
            truncate_token(page_token)
        ),
        Some(SyncCursor::Imap {
            uid_validity,
            uid_next,
            mailboxes,
            ..
        }) => format!(
            "imap uid_validity={uid_validity} uid_next={uid_next} mailboxes={}",
            mailboxes.len()
        ),
    }
}

fn truncate_token(token: &str) -> String {
    let truncated: String = token.chars().take(24).collect();
    if token.chars().count() > 24 {
        format!("{truncated}...")
    } else {
        truncated
    }
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

    match action {
        RuleAction::AddLabel { label } => {
            provider
                .modify_labels(&provider_message_id, std::slice::from_ref(label), &[])
                .await
                .map_err(|e| e.to_string())?;
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
            provider
                .modify_labels(&provider_message_id, &[], std::slice::from_ref(label))
                .await
                .map_err(|e| e.to_string())?;
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
        RuleAction::Archive => {
            provider
                .modify_labels(&provider_message_id, &[], &["INBOX".to_string()])
                .await
                .map_err(|e| e.to_string())?;
        }
        RuleAction::Trash => {
            provider
                .trash(&provider_message_id)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .move_to_trash(message_id, mxr_core::EventSource::RuleEngine)
                .await
                .map_err(|e| e.to_string())?;
        }
        RuleAction::Star => {
            provider
                .set_starred(&provider_message_id, true)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .set_starred(message_id, true, mxr_core::EventSource::RuleEngine)
                .await
                .map_err(|e| e.to_string())?;
        }
        RuleAction::MarkRead => {
            provider
                .set_read(&provider_message_id, true)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .set_read(message_id, true, mxr_core::EventSource::RuleEngine)
                .await
                .map_err(|e| e.to_string())?;
        }
        RuleAction::MarkUnread => {
            provider
                .set_read(&provider_message_id, false)
                .await
                .map_err(|e| e.to_string())?;
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
            other => panic!("expected rule history, got {:?}", other),
        }
    }
}
