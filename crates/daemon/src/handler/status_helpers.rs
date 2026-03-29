use mxr_protocol::AccountSyncStatus;
use crate::state::AppState;
use std::sync::Arc;

pub(super) async fn collect_status_snapshot(
    state: &Arc<AppState>,
) -> Result<(Vec<String>, u32, Vec<AccountSyncStatus>), String> {
    let accounts = state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    let mut total_messages = 0;
    let mut sync_statuses = Vec::new();

    for account in accounts {
        names.push(account.name.clone());
        total_messages += state
            .store
            .count_messages_by_account(&account.id)
            .await
            .map_err(|e| e.to_string())?;
        sync_statuses.push(build_account_sync_status(state, &account.id).await?);
    }

    if names.is_empty() {
        names.push("unknown".to_string());
    }

    Ok((names, total_messages, sync_statuses))
}

pub(super) async fn build_account_sync_status(
    state: &Arc<AppState>,
    account_id: &mxr_core::AccountId,
) -> Result<AccountSyncStatus, String> {
    let account = state
        .store
        .get_account(account_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Account not found: {account_id}"))?;
    let runtime = state
        .store
        .get_sync_runtime_status(account_id)
        .await
        .map_err(|e| e.to_string())?;
    let cursor = state
        .store
        .get_sync_cursor(account_id)
        .await
        .map_err(|e| e.to_string())?;

    let last_attempt_at = runtime
        .as_ref()
        .and_then(|row| row.last_attempt_at)
        .map(|dt| dt.to_rfc3339());
    let last_success_at = runtime
        .as_ref()
        .and_then(|row| row.last_success_at)
        .map(|dt| dt.to_rfc3339());
    let last_error = runtime.as_ref().and_then(|row| row.last_error.clone());
    let backoff_until = runtime
        .as_ref()
        .and_then(|row| row.backoff_until)
        .map(|dt| dt.to_rfc3339());
    let sync_in_progress = runtime
        .as_ref()
        .is_some_and(|row| row.sync_in_progress);
    let consecutive_failures = runtime
        .as_ref()
        .map_or(0, |row| row.consecutive_failures);
    let healthy = !sync_in_progress
        && last_error.is_none()
        && backoff_until.is_none()
        && last_success_at.is_some();

    Ok(AccountSyncStatus {
        account_id: account.id,
        account_name: account.name,
        last_attempt_at,
        last_success_at,
        last_error,
        failure_class: runtime.as_ref().and_then(|row| row.failure_class.clone()),
        consecutive_failures,
        backoff_until,
        sync_in_progress,
        current_cursor_summary: Some(
            runtime
                .as_ref()
                .and_then(|row| row.current_cursor_summary.clone())
                .unwrap_or_else(|| describe_cursor_for_status(cursor.as_ref())),
        ),
        last_synced_count: runtime
            .as_ref()
            .map_or(0, |row| row.last_synced_count),
        healthy,
    })
}

pub(super) fn describe_cursor_for_status(
    cursor: Option<&mxr_core::types::SyncCursor>,
) -> String {
    match cursor {
        Some(mxr_core::types::SyncCursor::Initial) | None => "initial".to_string(),
        Some(mxr_core::types::SyncCursor::Gmail { history_id }) => {
            format!("gmail history_id={history_id}")
        }
        Some(mxr_core::types::SyncCursor::GmailBackfill {
            history_id,
            page_token,
        }) => {
            let short: String = page_token.chars().take(24).collect();
            if page_token.chars().count() > 24 {
                format!("gmail_backfill history_id={history_id} page_token={short}...")
            } else {
                format!("gmail_backfill history_id={history_id} page_token={short}")
            }
        }
        Some(mxr_core::types::SyncCursor::Imap {
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

pub(super) async fn collect_doctor_report(
    state: &Arc<AppState>,
) -> Result<mxr_protocol::DoctorReport, String> {
    use super::{protocol_event_entry, recent_log_lines};

    let data_dir = mxr_config::data_dir();
    let db_path = data_dir.join("mxr.db");
    let index_path = data_dir.join("search_index");
    let log_path = data_dir.join("logs").join("mxr.log");
    let socket_path = crate::state::AppState::socket_path();

    let data_dir_exists = data_dir.exists();
    let database_exists = db_path.exists();
    let index_exists = index_path.exists();
    let socket_exists = socket_path.exists();
    let (_, total_messages, sync_statuses) = collect_status_snapshot(state).await?;
    let data_stats = doctor_data_stats(
        state
            .store
            .collect_record_counts()
            .await
            .map_err(|e| e.to_string())?,
    );
    let repair_required = crate::server::search_requires_repair(state, total_messages).await;
    let restart_required = false;
    let lexical_index_freshness =
        lexical_index_freshness(index_exists, repair_required, restart_required);
    let last_successful_sync_at = latest_successful_sync_at(&sync_statuses);
    let lexical_last_rebuilt_at = state
        .store
        .latest_event_timestamp("search", Some("Lexical index rebuilt"))
        .await
        .map_err(|e| e.to_string())?
        .map(|value| value.to_rfc3339());
    let semantic_config = state.config_snapshot().search.semantic.clone();
    let semantic_snapshot = state.semantic.lock().await.status_snapshot().await.ok();
    let (
        semantic_enabled,
        semantic_active_profile,
        semantic_index_freshness,
        semantic_last_indexed_at,
    ) = semantic_freshness_from_snapshot(
        semantic_snapshot.as_ref(),
        semantic_config.enabled,
        semantic_config.active_profile.as_str(),
    );
    let health_class =
        crate::server::classify_health(&sync_statuses, repair_required, restart_required);
    let recent_sync_events = state
        .store
        .list_events(10, None, Some("sync"))
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(protocol_event_entry)
        .collect();
    let recent_error_logs = recent_log_lines(10, Some("error")).unwrap_or_default();
    let recommended_next_steps = if matches!(
        health_class,
        mxr_protocol::DaemonHealthClass::Healthy
    ) {
        vec!["mxr status".to_string()]
    } else {
        vec![
            "mxr status".to_string(),
            "mxr sync --status".to_string(),
            "mxr logs --level error".to_string(),
            "mxr daemon --foreground".to_string(),
        ]
    };
    let healthy = data_dir_exists
        && database_exists
        && index_exists
        && socket_exists
        && matches!(
            health_class,
            mxr_protocol::DaemonHealthClass::Healthy
        );

    Ok(mxr_protocol::DoctorReport {
        healthy,
        health_class,
        lexical_index_freshness,
        last_successful_sync_at,
        lexical_last_rebuilt_at,
        semantic_enabled,
        semantic_active_profile,
        semantic_index_freshness,
        semantic_last_indexed_at,
        data_stats,
        data_dir_exists,
        database_exists,
        index_exists,
        socket_exists,
        socket_reachable: true,
        stale_socket: false,
        daemon_running: true,
        daemon_pid: Some(std::process::id()),
        daemon_protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
        daemon_version: Some(crate::server::current_daemon_version().to_string()),
        daemon_build_id: Some(crate::server::current_build_id()),
        index_lock_held: false,
        index_lock_error: None,
        restart_required,
        repair_required,
        database_path: db_path.display().to_string(),
        database_size_bytes: file_size(&db_path),
        index_path: index_path.display().to_string(),
        index_size_bytes: dir_size(&index_path),
        log_path: log_path.display().to_string(),
        log_size_bytes: file_size(&log_path),
        sync_statuses,
        recent_sync_events,
        recent_error_logs,
        recommended_next_steps,
    })
}

pub(super) fn doctor_data_stats(
    counts: mxr_store::StoreRecordCounts,
) -> mxr_protocol::DoctorDataStats {
    mxr_protocol::DoctorDataStats {
        accounts: counts.accounts,
        labels: counts.labels,
        messages: counts.messages,
        unread_messages: counts.unread_messages,
        starred_messages: counts.starred_messages,
        messages_with_attachments: counts.messages_with_attachments,
        message_labels: counts.message_labels,
        bodies: counts.bodies,
        attachments: counts.attachments,
        drafts: counts.drafts,
        snoozed: counts.snoozed,
        saved_searches: counts.saved_searches,
        rules: counts.rules,
        rule_logs: counts.rule_logs,
        sync_log: counts.sync_log,
        sync_runtime_statuses: counts.sync_runtime_statuses,
        event_log: counts.event_log,
        semantic_profiles: counts.semantic_profiles,
        semantic_chunks: counts.semantic_chunks,
        semantic_embeddings: counts.semantic_embeddings,
    }
}

pub(super) fn dir_size(path: &std::path::Path) -> u64 {
    if !path.exists() {
        return 0;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                dir_size(&path)
            } else {
                entry.metadata().map(|meta| meta.len()).unwrap_or(0)
            }
        })
        .sum()
}

pub(super) fn file_size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

pub(crate) fn latest_successful_sync_at(
    sync_statuses: &[mxr_protocol::AccountSyncStatus],
) -> Option<String> {
    sync_statuses
        .iter()
        .filter_map(|status| status.last_success_at.as_deref())
        .filter_map(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .max()
        .map(|value| value.to_rfc3339())
}

pub(super) fn lexical_index_freshness(
    index_exists: bool,
    repair_required: bool,
    restart_required: bool,
) -> mxr_protocol::IndexFreshness {
    if repair_required || !index_exists {
        mxr_protocol::IndexFreshness::RepairRequired
    } else if restart_required {
        mxr_protocol::IndexFreshness::Stale
    } else {
        mxr_protocol::IndexFreshness::Current
    }
}

pub(super) fn semantic_freshness_from_snapshot(
    snapshot: Option<&mxr_core::types::SemanticStatusSnapshot>,
    enabled_fallback: bool,
    active_profile_fallback: &str,
) -> (
    bool,
    Option<String>,
    mxr_protocol::IndexFreshness,
    Option<String>,
) {
    let Some(snapshot) = snapshot else {
        return if enabled_fallback {
            (
                true,
                Some(active_profile_fallback.to_string()),
                mxr_protocol::IndexFreshness::Unknown,
                None,
            )
        } else {
            (
                false,
                None,
                mxr_protocol::IndexFreshness::Disabled,
                None,
            )
        };
    };

    if !snapshot.enabled {
        return (
            false,
            None,
            mxr_protocol::IndexFreshness::Disabled,
            None,
        );
    }

    let active_profile = snapshot.active_profile.as_str().to_string();
    let active_record = snapshot
        .profiles
        .iter()
        .find(|profile| profile.profile == snapshot.active_profile);
    let freshness = match active_record.map(|profile| profile.status) {
        Some(mxr_core::types::SemanticProfileStatus::Ready) => {
            mxr_protocol::IndexFreshness::Current
        }
        Some(mxr_core::types::SemanticProfileStatus::Indexing)
        | Some(mxr_core::types::SemanticProfileStatus::Pending) => {
            mxr_protocol::IndexFreshness::Indexing
        }
        Some(mxr_core::types::SemanticProfileStatus::Error) => {
            mxr_protocol::IndexFreshness::Error
        }
        None => mxr_protocol::IndexFreshness::Stale,
    };

    (
        true,
        Some(active_profile),
        freshness,
        active_record
            .and_then(|profile| profile.last_indexed_at)
            .map(|value| value.to_rfc3339()),
    )
}
