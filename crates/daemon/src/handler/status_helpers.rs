use super::helpers::{dir_size, file_size, protocol_event_entry, recent_log_lines};
use crate::state::AppState;
use mxr_core::{Account, AccountId};
use mxr_protocol::{AccountSyncStatus, FeatureHealth, FeatureHealthReport};
use mxr_store::SyncRuntimeStatus;
use std::collections::HashMap;

pub(super) async fn collect_status_snapshot(
    state: &AppState,
) -> Result<(Vec<String>, u32, Vec<AccountSyncStatus>), String> {
    let started_at = std::time::Instant::now();
    let accounts = state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?;
    let message_counts = state
        .store
        .count_messages_grouped_by_account()
        .await
        .map_err(|e| e.to_string())?;
    let runtime_statuses = state
        .store
        .list_sync_runtime_statuses()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|runtime| (runtime.account_id.clone(), runtime))
        .collect::<HashMap<_, _>>();
    let cursors = state
        .store
        .list_sync_cursors()
        .await
        .map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    let mut total_messages = 0;
    let mut sync_statuses = Vec::new();

    for account in accounts {
        names.push(account.name.clone());
        total_messages += message_counts.get(&account.id).copied().unwrap_or(0);
        sync_statuses.push(account_sync_status(
            account.clone(),
            runtime_statuses.get(&account.id).cloned(),
            cursors.get(&account.id).cloned(),
        ));
    }

    if names.is_empty() {
        names.push("unknown".to_string());
    }

    tracing::trace!(
        account_count = sync_statuses.len(),
        total_messages,
        elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
        "status snapshot collected"
    );

    Ok((names, total_messages, sync_statuses))
}

pub(super) async fn build_account_sync_status(
    state: &AppState,
    account_id: &AccountId,
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

    Ok(account_sync_status(account, runtime, cursor))
}

/// Fallback cursor summary for status reports built from the store alone
/// (no live provider in scope). When a richer description exists from the
/// last sync cycle, it's cached on `SyncRuntimeStatus.current_cursor_summary`
/// and used in preference — see `loops.rs::describe_sync_cursor` for the
/// provider-aware version.
pub(super) fn describe_cursor_for_status(cursor: Option<&mxr_core::types::SyncCursor>) -> String {
    match cursor {
        None => "initial".to_string(),
        Some(c) if c.is_empty() => "initial".to_string(),
        Some(c) => format!("opaque len={}", c.as_bytes().len()),
    }
}

pub(super) async fn collect_doctor_report(
    state: &AppState,
) -> Result<mxr_protocol::DoctorReport, String> {
    let started_at = std::time::Instant::now();
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
    let semantic_snapshot = state.semantic.status_snapshot().await.ok();
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
    let recent_error_logs = recent_log_lines(state, 10, Some("error"))
        .await
        .unwrap_or_default();
    let recommended_next_steps = if matches!(health_class, mxr_protocol::DaemonHealthClass::Healthy)
    {
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
        && matches!(health_class, mxr_protocol::DaemonHealthClass::Healthy);

    // Structured findings: classify the existing signals into
    // categorised entries with shell-runnable remediation. Clients (TUI,
    // future agents) can reason about individual issues without parsing
    // free text.
    let findings = build_doctor_findings(
        &sync_statuses,
        &recent_error_logs,
        data_dir_exists,
        database_exists,
        index_exists,
        socket_exists,
        repair_required,
        restart_required,
        semantic_enabled,
        &data_stats,
    );

    let report = mxr_protocol::DoctorReport {
        healthy,
        health_class,
        lexical_index_freshness,
        last_successful_sync_at,
        lexical_last_rebuilt_at,
        semantic_enabled,
        semantic_active_profile,
        semantic_index_freshness,
        semantic_last_indexed_at,
        feature_health: Some(feature_health_report(state)),
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
        database_size_bytes: file_size(state, db_path.clone()).await,
        index_path: index_path.display().to_string(),
        index_size_bytes: dir_size(state, index_path.clone()).await,
        log_path: log_path.display().to_string(),
        log_size_bytes: file_size(state, log_path.clone()).await,
        sync_statuses,
        recent_sync_events,
        recent_error_logs,
        recommended_next_steps,
        findings,
    };

    tracing::trace!(
        elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
        "doctor report collected"
    );

    Ok(report)
}

pub(super) fn feature_health_report(state: &AppState) -> FeatureHealthReport {
    let config = state.config_snapshot();
    let llm_health = if config.llm.enabled {
        FeatureHealth::Healthy
    } else {
        FeatureHealth::Disabled
    };
    let relationship_summary_health = llm_feature_health(
        state,
        mxr_llm::LlmFeature::RelationshipSummary,
        if config.llm.enabled {
            FeatureHealth::Healthy
        } else {
            FeatureHealth::Degraded {
                reason: "LLM disabled; stylometry remains available but summaries are skipped"
                    .to_string(),
            }
        },
    );
    let commitments_health = llm_feature_health(
        state,
        mxr_llm::LlmFeature::Commitments,
        if config.llm.enabled {
            FeatureHealth::Healthy
        } else {
            FeatureHealth::Degraded {
                reason: "LLM disabled; stored commitments remain readable".to_string(),
            }
        },
    );
    let voice_match_health = llm_feature_health(
        state,
        mxr_llm::LlmFeature::VoiceMatch,
        FeatureHealth::Healthy,
    );

    FeatureHealthReport {
        semantic: if config.search.semantic.enabled {
            FeatureHealth::Healthy
        } else {
            FeatureHealth::Disabled
        },
        summarize: llm_health.clone(),
        relationship_profile: relationship_summary_health,
        commitments: commitments_health,
        draft_assist: llm_health,
        voice_match: voice_match_health,
        humanizer: if config.humanizer.enabled {
            if config.humanizer.auto_fix && !config.llm.enabled {
                FeatureHealth::Degraded {
                    reason: "LLM disabled; detection works but auto-fix is unavailable".to_string(),
                }
            } else {
                FeatureHealth::Healthy
            }
        } else {
            FeatureHealth::Disabled
        },
    }
}

fn llm_feature_health(
    state: &AppState,
    feature: mxr_llm::LlmFeature,
    default: FeatureHealth,
) -> FeatureHealth {
    state
        .llm
        .feature_block_reason(feature)
        .map(|reason| FeatureHealth::Degraded { reason })
        .unwrap_or(default)
}

/// Classify the doctor's raw signals into structured findings with
/// remediation. Pattern-matches recent error log lines for common
/// failure modes — OAuth refresh failed, rate-limited, network
/// unreachable — so the user gets a copy-pasteable next step instead
/// of a free-text dump.
pub(crate) fn build_doctor_findings(
    sync_statuses: &[mxr_protocol::AccountSyncStatus],
    recent_errors: &[String],
    data_dir_exists: bool,
    database_exists: bool,
    index_exists: bool,
    socket_exists: bool,
    repair_required: bool,
    restart_required: bool,
    semantic_enabled: bool,
    data_stats: &mxr_protocol::DoctorDataStats,
) -> Vec<mxr_protocol::DoctorFinding> {
    use mxr_protocol::{DoctorFinding, DoctorFindingCategory, DoctorFindingSeverity};
    let mut findings = Vec::new();

    if !data_dir_exists {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Storage,
            severity: DoctorFindingSeverity::Error,
            message: "Data directory missing".into(),
            remediation: vec!["mxr daemon --foreground".into()],
        });
    }
    if !database_exists {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Storage,
            severity: DoctorFindingSeverity::Error,
            message: "SQLite database missing".into(),
            remediation: vec!["mxr daemon --foreground".into(), "mxr doctor".into()],
        });
    }
    if !index_exists || repair_required {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::SearchIndex,
            severity: DoctorFindingSeverity::Warning,
            message: "Search index missing or needs rebuild".into(),
            remediation: vec!["mxr doctor --reindex".into()],
        });
    }
    if !socket_exists {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Daemon,
            severity: DoctorFindingSeverity::Error,
            message: "Daemon socket missing — daemon not running?".into(),
            remediation: vec!["mxr daemon --foreground".into(), "mxr status".into()],
        });
    }
    if restart_required {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Daemon,
            severity: DoctorFindingSeverity::Warning,
            message: "Daemon protocol changed — restart needed".into(),
            remediation: vec!["mxr daemon --restart".into()],
        });
    }

    if semantic_enabled {
        let missing_messages = data_stats.messages_missing_semantic_chunks;
        let missing_embeddings = data_stats.semantic_chunks_missing_embeddings;
        if missing_messages > 0 || missing_embeddings > 0 {
            findings.push(DoctorFinding {
                category: DoctorFindingCategory::Semantic,
                severity: DoctorFindingSeverity::Warning,
                message: format!(
                    "Semantic backfill pending: {missing_messages} message(s) need chunks, {missing_embeddings} chunk(s) need embeddings"
                ),
                remediation: vec!["mxr doctor --backfill-semantic".into()],
            });
        }
    }

    if data_stats.relationship_drifts > 0 {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Generic,
            severity: DoctorFindingSeverity::Info,
            message: format!(
                "Relationship voice drift detected for {} contact(s)",
                data_stats.relationship_drifts
            ),
            remediation: vec!["mxr profile <email> --rebuild".into()],
        });
    }

    for status in sync_statuses {
        if let Some(err) = status.last_error.as_deref() {
            findings.push(classify_sync_error(&status.account_name, err));
        }
    }

    for line in recent_errors {
        if let Some(finding) = classify_log_line(line) {
            findings.push(finding);
        }
    }

    findings
}

fn classify_sync_error(account: &str, err: &str) -> mxr_protocol::DoctorFinding {
    use mxr_protocol::{DoctorFinding, DoctorFindingCategory, DoctorFindingSeverity};
    let lower = err.to_lowercase();
    let (category, remediation) = if lower.contains("oauth")
        || lower.contains("invalid_grant")
        || lower.contains("token")
        || lower.contains("unauthorized")
    {
        (
            DoctorFindingCategory::OAuth,
            vec![format!(
                "Re-authenticate `{account}`: run `mxr accounts add` for the same provider/account key, or use the account OAuth flow in the web app."
            )],
        )
    } else if lower.contains("connection refused")
        || lower.contains("dns")
        || lower.contains("timed out")
        || lower.contains("timeout")
    {
        (
            DoctorFindingCategory::Network,
            vec![
                format!("mxr sync --account {account} --wait --wait-timeout-secs 120"),
                "mxr doctor".into(),
            ],
        )
    } else if lower.contains("rate") && lower.contains("limit") {
        (
            DoctorFindingCategory::Sync,
            vec![format!(
                "mxr sync --account {account} --wait --wait-timeout-secs 300"
            )],
        )
    } else if lower.contains("locked") || lower.contains("busy") {
        (
            DoctorFindingCategory::SqliteLock,
            vec!["# Close other mxr processes".into()],
        )
    } else {
        (DoctorFindingCategory::Sync, vec![])
    };
    DoctorFinding {
        category,
        severity: DoctorFindingSeverity::Error,
        message: format!("Sync error on {account}: {err}"),
        remediation,
    }
}

fn classify_log_line(line: &str) -> Option<mxr_protocol::DoctorFinding> {
    use mxr_protocol::{DoctorFinding, DoctorFindingCategory, DoctorFindingSeverity};
    let lower = line.to_lowercase();
    if lower.contains("invalid_grant") || lower.contains("token expired") || lower.contains("oauth")
    {
        return Some(DoctorFinding {
            category: DoctorFindingCategory::OAuth,
            severity: DoctorFindingSeverity::Warning,
            message: "OAuth token issue in recent logs".into(),
            remediation: vec![
                "Re-authenticate: `mxr accounts add` (same key) or the web app account OAuth flow."
                    .into(),
            ],
        });
    }
    if lower.contains("database is locked") {
        return Some(DoctorFinding {
            category: DoctorFindingCategory::SqliteLock,
            severity: DoctorFindingSeverity::Warning,
            message: "SQLite lock contention in recent logs".into(),
            remediation: vec!["# Close other mxr processes".into()],
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::{DoctorFindingCategory, DoctorFindingSeverity};

    #[test]
    fn classify_sync_error_maps_oauth_failures_to_reauth_remediation() {
        let finding = classify_sync_error("work", "invalid_grant: token expired");

        assert_eq!(finding.category, DoctorFindingCategory::OAuth);
        assert_eq!(finding.severity, DoctorFindingSeverity::Error);
        assert!(finding.message.contains("work"));
        assert!(
            finding
                .remediation
                .iter()
                .any(|step| step.contains("mxr accounts add")),
            "OAuth findings should tell the user how to re-authenticate"
        );
    }

    #[test]
    fn classify_sync_error_maps_network_failures_without_guessing_a_fix() {
        let finding = classify_sync_error("imap", "DNS lookup timed out");

        assert_eq!(finding.category, DoctorFindingCategory::Network);
        assert_eq!(finding.severity, DoctorFindingSeverity::Error);
        assert_eq!(
            finding.remediation,
            vec![
                "mxr sync --account imap --wait --wait-timeout-secs 120".to_string(),
                "mxr doctor".to_string()
            ]
        );
    }

    #[test]
    fn classify_sync_error_maps_rate_limits_to_retry_guidance() {
        let finding = classify_sync_error("gmail", "provider rate limit exceeded");

        assert_eq!(finding.category, DoctorFindingCategory::Sync);
        assert_eq!(
            finding.remediation,
            vec!["mxr sync --account gmail --wait --wait-timeout-secs 300".to_string()]
        );
    }

    #[test]
    fn classify_sync_error_maps_sqlite_contention() {
        let finding = classify_sync_error("local", "database is busy");

        assert_eq!(finding.category, DoctorFindingCategory::SqliteLock);
        assert_eq!(
            finding.remediation,
            vec!["# Close other mxr processes".to_string()]
        );
    }

    #[test]
    fn classify_sync_error_keeps_unknown_failures_as_sync() {
        let finding = classify_sync_error("imap", "unexpected provider payload");

        assert_eq!(finding.category, DoctorFindingCategory::Sync);
        assert!(finding.remediation.is_empty());
    }

    #[test]
    fn classify_log_line_detects_oauth_errors() {
        let finding = classify_log_line("WARN oauth token expired for account").unwrap();

        assert_eq!(finding.category, DoctorFindingCategory::OAuth);
        assert_eq!(finding.severity, DoctorFindingSeverity::Warning);
        assert!(finding
            .remediation
            .iter()
            .any(|step| step.contains("mxr accounts add")));
    }

    #[test]
    fn classify_log_line_detects_sqlite_lock_contention() {
        let finding = classify_log_line("ERROR database is locked").unwrap();

        assert_eq!(finding.category, DoctorFindingCategory::SqliteLock);
        assert_eq!(finding.severity, DoctorFindingSeverity::Warning);
    }

    #[test]
    fn classify_log_line_ignores_unclassified_lines() {
        assert!(classify_log_line("sync completed successfully").is_none());
    }
}

pub(crate) fn doctor_data_stats(
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
        messages_missing_semantic_chunks: counts.messages_missing_semantic_chunks,
        semantic_chunks_missing_embeddings: counts.semantic_chunks_missing_embeddings,
        relationship_drifts: counts.relationship_drifts,
    }
}

fn account_sync_status(
    account: Account,
    runtime: Option<SyncRuntimeStatus>,
    cursor: Option<mxr_core::types::SyncCursor>,
) -> AccountSyncStatus {
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
    let sync_in_progress = runtime.as_ref().is_some_and(|row| row.sync_in_progress);
    let consecutive_failures = runtime.as_ref().map_or(0, |row| row.consecutive_failures);
    let healthy = !sync_in_progress
        && last_error.is_none()
        && backoff_until.is_none()
        && last_success_at.is_some();

    AccountSyncStatus {
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
        last_synced_count: runtime.as_ref().map_or(0, |row| row.last_synced_count),
        healthy,
    }
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
            (false, None, mxr_protocol::IndexFreshness::Disabled, None)
        };
    };

    if !snapshot.enabled {
        return (false, None, mxr_protocol::IndexFreshness::Disabled, None);
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
        Some(mxr_core::types::SemanticProfileStatus::Error) => mxr_protocol::IndexFreshness::Error,
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
