use super::helpers::{
    dir_size, file_size, log_total_bytes, protocol_event_entry, recent_log_lines,
};
use crate::state::AppState;
use mxr_core::{Account, AccountId};
use mxr_protocol::{AccountSyncStatus, FeatureHealth, FeatureHealthReport};
use mxr_store::SyncRuntimeStatus;
use std::collections::HashMap;

/// The DB-backed part of a status reply: account names, the store-wide message
/// count, and one sync status per account.
pub(super) type StatusSnapshotFields = (Vec<String>, u32, Vec<AccountSyncStatus>);

pub(super) async fn collect_status_snapshot(
    state: &AppState,
) -> Result<StatusSnapshotFields, String> {
    let started_at = std::time::Instant::now();
    let accounts = state
        .store
        .list_cached_mail_accounts()
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
        let progress = state.sync_progress(&account.id);
        sync_statuses.push(account_sync_status(
            account.clone(),
            runtime_statuses.get(&account.id).cloned(),
            cursors.get(&account.id).cloned(),
            progress,
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

    Ok(account_sync_status(
        account,
        runtime,
        cursor,
        state.sync_progress(account_id),
    ))
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
    // Single-source resolution (honors MXR_DAEMON_ADDR) so the doctor report's
    // socket reachability agrees with where the daemon binds.
    let socket_path = crate::server::resolve_daemon_socket().map_err(|error| error.to_string())?;

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
    // The doctor report waits for its snapshot rather than budgeting it, so
    // there is no degraded reading to account for here.
    let health_class =
        crate::server::classify_health(&sync_statuses, repair_required, restart_required, false);
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
    let sync_history = crate::sync_history::collect(&state.store)
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "doctor could not summarise sync history");
            Vec::new()
        });
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
    // One reading of the search worker for the whole report (see
    // `feature_health_report`).
    let search_worker_failed = search_worker_failed(state);
    // Structured findings: classify the existing signals into
    // categorised entries with shell-runnable remediation. Clients (TUI,
    // future agents) can reason about individual issues without parsing
    // free text.
    let findings = build_doctor_findings(DoctorFindingInputs {
        sync_statuses: &sync_statuses,
        recent_errors: &recent_error_logs,
        data_dir_exists,
        database_exists,
        index_exists,
        socket_exists,
        repair_required,
        restart_required,
        semantic_enabled,
        search_worker_stopped: search_worker_failed,
        address_cache_stale: state.account_addresses.is_stale(),
        data_stats: &data_stats,
    });

    #[cfg(feature = "langfuse")]
    let findings: Vec<_> = findings
        .into_iter()
        .chain(crate::llm_tracing::doctor_finding())
        .collect();

    let healthy = data_dir_exists
        && database_exists
        && index_exists
        && socket_exists
        && matches!(health_class, mxr_protocol::DaemonHealthClass::Healthy)
        && no_error_findings(&findings);

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
        feature_health: Some(feature_health_report(state, search_worker_failed)),
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
        history_retention_days: crate::loops::retention_days_on_disk(
            &mxr_config::config_file_path(),
        ),
        address_cache_stale: state.account_addresses.is_stale(),
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
        sync_history,
        log_total_bytes: log_total_bytes(state, data_dir.join("logs")).await,
    };

    tracing::trace!(
        elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
        "doctor report collected"
    );

    Ok(report)
}

/// `search_worker_failed` is passed in rather than read here so that a report
/// which also raises a search finding cannot disagree with itself: the worker
/// can die between two reads, and a report saying "search healthy" beside
/// "search index worker stopped" is worse than either answer alone.
pub(super) fn feature_health_report(
    state: &AppState,
    search_worker_failed: bool,
) -> FeatureHealthReport {
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
    let summarize_health =
        llm_feature_health(state, mxr_llm::LlmFeature::Summarize, llm_health.clone());
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

    let draft_assist_health =
        llm_feature_health(state, mxr_llm::LlmFeature::DraftAssist, llm_health.clone());

    FeatureHealthReport {
        search: search_feature_health(search_worker_failed),
        semantic: if config.search.semantic.enabled {
            FeatureHealth::Healthy
        } else {
            FeatureHealth::Disabled
        },
        summarize: summarize_health,
        relationship_profile: relationship_summary_health,
        commitments: commitments_health,
        draft_assist: draft_assist_health,
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

/// Whether the search index worker has stopped in a way anyone should hear
/// about.
///
/// It also stops on the way out of a clean shutdown, and reporting that as a
/// failure would have every shutting-down daemon claim search is broken in its
/// last status reply.
pub(super) fn search_worker_failed(state: &AppState) -> bool {
    state.search.worker_stopped() && !state.shutdown_requested()
}

fn search_feature_health(worker_failed: bool) -> FeatureHealth {
    if worker_failed {
        FeatureHealth::Degraded {
            reason: "search index worker stopped; restart the daemon (`mxr daemon --restart`)"
                .to_string(),
        }
    } else {
        FeatureHealth::Healthy
    }
}

fn llm_feature_health(
    state: &AppState,
    feature: mxr_llm::LlmFeature,
    default: FeatureHealth,
) -> FeatureHealth {
    if let Some(reason) = state.llm.feature_block_reason(feature) {
        return FeatureHealth::Degraded { reason };
    }
    feature_health_for_observation(state.llm.observation(Some(feature)), default)
}

fn feature_health_for_observation(
    observation: Option<mxr_llm::CompletionObservation>,
    default: FeatureHealth,
) -> FeatureHealth {
    match observation {
        Some(mxr_llm::CompletionObservation::Succeeded) => FeatureHealth::Healthy,
        Some(mxr_llm::CompletionObservation::Failed(kind)) => FeatureHealth::Degraded {
            reason: format!(
                "last observed model request failed: {}",
                failure_label(kind)
            ),
        },
        None => default,
    }
}

fn failure_label(kind: mxr_llm::LlmFailureKind) -> &'static str {
    match kind {
        mxr_llm::LlmFailureKind::Unreachable => "endpoint unreachable",
        mxr_llm::LlmFailureKind::RateLimited => "rate limited",
        mxr_llm::LlmFailureKind::Timeout => "request timed out",
        mxr_llm::LlmFailureKind::Unauthorized => "authentication failed",
        mxr_llm::LlmFailureKind::Empty => "empty response",
        mxr_llm::LlmFailureKind::Other => "provider error",
    }
}

/// Classify the doctor's raw signals into structured findings with
/// remediation. Pattern-matches recent error log lines for common
/// failure modes — OAuth refresh failed, rate-limited, network
/// unreachable — so the user gets a copy-pasteable next step instead
/// of a free-text dump.
pub(crate) struct DoctorFindingInputs<'a> {
    pub(crate) sync_statuses: &'a [mxr_protocol::AccountSyncStatus],
    pub(crate) recent_errors: &'a [String],
    pub(crate) data_dir_exists: bool,
    pub(crate) database_exists: bool,
    pub(crate) index_exists: bool,
    pub(crate) socket_exists: bool,
    pub(crate) repair_required: bool,
    pub(crate) restart_required: bool,
    pub(crate) semantic_enabled: bool,
    pub(crate) search_worker_stopped: bool,
    /// The daemon's address registry failed its last rebuild, so sync reads
    /// direction as Unknown until the next sync pass retries it.
    pub(crate) address_cache_stale: bool,
    pub(crate) data_stats: &'a mxr_protocol::DoctorDataStats,
}

/// A report is only healthy when nothing in it is an error.
///
/// `mxr doctor --check` exits non-zero on `healthy == false`, and that exit
/// code is what agents and CI read — a finding severe enough to print as an
/// error has to reach them, not just the pretty printer. Tying the two
/// together means a new Error-severity finding can never again be invisible to
/// a scripted caller.
pub(crate) fn no_error_findings(findings: &[mxr_protocol::DoctorFinding]) -> bool {
    findings
        .iter()
        .all(|finding| finding.severity != mxr_protocol::DoctorFindingSeverity::Error)
}

pub(crate) fn build_doctor_findings(
    inputs: DoctorFindingInputs<'_>,
) -> Vec<mxr_protocol::DoctorFinding> {
    use mxr_protocol::{DoctorFinding, DoctorFindingCategory, DoctorFindingSeverity};
    let mut findings = Vec::new();

    if !inputs.data_dir_exists {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Storage,
            severity: DoctorFindingSeverity::Error,
            message: "Data directory missing".into(),
            remediation: vec!["mxr daemon --foreground".into()],
        });
    }
    if !inputs.database_exists {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Storage,
            severity: DoctorFindingSeverity::Error,
            message: "SQLite database missing".into(),
            remediation: vec!["mxr daemon --foreground".into(), "mxr doctor".into()],
        });
    }
    if inputs.search_worker_stopped {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::SearchIndex,
            severity: DoctorFindingSeverity::Error,
            message: "Search index worker stopped — search is unavailable".into(),
            remediation: vec!["mxr daemon --restart".into()],
        });
    }
    if !inputs.index_exists || inputs.repair_required {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::SearchIndex,
            severity: DoctorFindingSeverity::Warning,
            message: "Search index missing or needs rebuild".into(),
            remediation: vec!["mxr doctor --reindex".into()],
        });
    }
    if !inputs.socket_exists {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Daemon,
            severity: DoctorFindingSeverity::Error,
            message: "Daemon socket missing — daemon not running?".into(),
            remediation: vec!["mxr daemon --foreground".into(), "mxr status".into()],
        });
    }
    if inputs.address_cache_stale {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Sync,
            severity: DoctorFindingSeverity::Warning,
            message: "Account address cache is stale — new sync messages get Unknown direction until it rebuilds".into(),
            remediation: vec!["mxr sync".into()],
        });
    }
    if inputs.restart_required {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Daemon,
            severity: DoctorFindingSeverity::Warning,
            message: "Daemon protocol changed — restart needed".into(),
            remediation: vec!["mxr daemon --restart".into()],
        });
    }

    if inputs.semantic_enabled {
        let missing_messages = inputs.data_stats.messages_missing_semantic_chunks;
        let missing_embeddings = inputs.data_stats.semantic_chunks_missing_embeddings;
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

    if inputs.data_stats.relationship_drifts > 0 {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Generic,
            severity: DoctorFindingSeverity::Info,
            message: format!(
                "Relationship voice drift detected for {} contact(s)",
                inputs.data_stats.relationship_drifts
            ),
            remediation: vec!["mxr profile <email> --rebuild".into()],
        });
    }

    for status in inputs.sync_statuses {
        if let Some(err) = status.last_error.as_deref() {
            findings.push(classify_sync_error(&status.account_name, err));
        }
    }

    for line in inputs.recent_errors {
        // The sync loop logs each failure it also records in sync status, so a
        // line that carries a status error is the same failure, already reported
        // above with its account and a more precise severity.
        let mirrors_sync_status = inputs.sync_statuses.iter().any(|status| {
            status
                .last_error
                .as_deref()
                .is_some_and(|err| !err.is_empty() && line.contains(err))
        });
        if mirrors_sync_status {
            continue;
        }
        if let Some(finding) = classify_log_line(line) {
            // Repeated retries log the same failure repeatedly.
            if !findings.contains(&finding) {
                findings.push(finding);
            }
        }
    }

    findings
}

fn classify_sync_error(account: &str, err: &str) -> mxr_protocol::DoctorFinding {
    use mxr_protocol::{DoctorFinding, DoctorFindingCategory, DoctorFindingSeverity};
    let lower = err.to_lowercase();
    if lower.contains("no credential found") || lower.contains("no usable credential found") {
        return DoctorFinding {
            category: DoctorFindingCategory::Sync,
            severity: DoctorFindingSeverity::Error,
            message: format!("Missing mail credential for {account}"),
            remediation: vec![
                "Add the account password to the resolved config directory's secrets.toml (mode 0600), or enable the OS keychain; headless disk-only setups can set MXR_KEYCHAIN=off".into(),
                format!(
                    "mxr sync --account {} --wait --wait-timeout-secs 120",
                    shell_words::quote(account)
                ),
            ],
        };
    }
    // The provider's Auth/OAuth wrappers do not mean the grant was rejected:
    // token-endpoint transport failures are retryable network failures too,
    // and a refreshed token that could not be saved is a local storage problem.
    let (category, remediation) = if lower.contains("local token cache write failed") {
        (
            DoctorFindingCategory::Storage,
            vec![
                "The sign-in is intact; mxr could not save the refreshed token. Check that the OS keychain is unlocked and the mxr token directory is writable.".into(),
                format!(
                    "mxr sync --account {} --wait --wait-timeout-secs 120",
                    shell_words::quote(account)
                ),
            ],
        )
    } else if crate::loops::classify_sync_error(err) == "network" {
        (
            DoctorFindingCategory::Network,
            vec![
                format!(
                    "mxr sync --account {} --wait --wait-timeout-secs 120",
                    shell_words::quote(account)
                ),
                "mxr doctor".into(),
            ],
        )
    } else if lower.contains("oauth")
        || lower.contains("invalid_grant")
        || lower.contains("token")
        || lower.contains("unauthorized")
        // Sync health already says "sign in again" for every auth-class failure
        // (a Gmail 401, an IMAP login the server rejected); doctor must agree.
        || crate::loops::classify_sync_error(err) == "auth"
    {
        // Re-adding the account cannot fix a rejected OAuth client or scope, so
        // name the real cause first and keep re-authorization as the last step.
        let mut steps = Vec::new();
        if lower.contains("invalid_client") || lower.contains("unauthorized_client") {
            steps.push("The provider rejected the OAuth client, not the sign-in: check the client ID and secret in the account config. For Gmail, a custom client must be a \"Desktop app\" client; for Outlook, check the app registration.".to_string());
        } else if lower.contains("invalid_scope") {
            steps.push("The provider rejected the requested scopes: check that the OAuth app may request them. For Gmail, enable the Gmail API and list the gmail.readonly, gmail.modify and gmail.labels scopes on the consent screen; for Outlook, check the app registration's API permissions.".to_string());
        }
        steps.push(format!(
            "Re-authenticate `{account}`: run `mxr accounts add` for the same provider/account key, or use the account OAuth flow in the web app."
        ));
        (DoctorFindingCategory::OAuth, steps)
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
    if crate::loops::classify_sync_error(line) == "network"
        && (lower.contains("oauth") || lower.contains("token refresh"))
    {
        return Some(DoctorFinding {
            category: DoctorFindingCategory::Network,
            severity: DoctorFindingSeverity::Warning,
            message: "Connection issue in recent logs".into(),
            remediation: vec!["mxr sync".into(), "mxr doctor".into()],
        });
    }
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
    progress: Option<mxr_protocol::SyncProgressData>,
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
    // A sync in flight is not a health problem: a multi-page backfill keeps
    // `sync_in_progress` true from its first page to its last, and an
    // account that is progressing through it is healthy. Health is about
    // errors and backoff.
    let healthy = last_error.is_none() && backoff_until.is_none() && last_success_at.is_some();

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
        progress,
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
        Some(
            mxr_core::types::SemanticProfileStatus::Indexing
            | mxr_core::types::SemanticProfileStatus::Pending,
        ) => mxr_protocol::IndexFreshness::Indexing,
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use mxr_protocol::{DoctorFindingCategory, DoctorFindingSeverity};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct RouteProvider(bool);

    #[async_trait]
    impl mxr_llm::LlmProvider for RouteProvider {
        async fn complete(
            &self,
            _request: mxr_llm::CompletionRequest,
        ) -> Result<mxr_llm::CompletionResponse, mxr_llm::LlmError> {
            if !self.0 {
                return Err(mxr_llm::LlmError::Unreachable("fixture".into()));
            }
            Ok(mxr_llm::CompletionResponse {
                content: "ok".into(),
                model: "test".into(),
                finish_reason: None,
            })
        }

        fn capabilities(&self) -> mxr_llm::LlmCapabilities {
            mxr_llm::LlmCapabilities {
                context_window: 1,
                supports_streaming: false,
            }
        }

        fn model_name(&self) -> &str {
            "test"
        }
    }

    #[tokio::test]
    async fn draft_assist_health_stays_degraded_when_another_route_succeeds() {
        let runtime = Arc::new(mxr_llm::LlmRuntime::new(Arc::new(RouteProvider(true))));
        let mut providers = HashMap::new();
        providers.insert(
            mxr_llm::LlmFeature::DraftAssist,
            Arc::new(RouteProvider(false)) as Arc<dyn mxr_llm::LlmProvider>,
        );
        runtime.replace_feature_providers(providers, HashMap::new());
        let request = || mxr_llm::CompletionRequest {
            messages: vec![mxr_llm::ChatMessage::user("fixture")],
            max_tokens: None,
            temperature: None,
        };

        assert!(runtime
            .for_feature(mxr_llm::LlmFeature::DraftAssist)
            .complete(request())
            .await
            .is_err());
        runtime.complete(request()).await.unwrap();

        let default = FeatureHealth::Healthy;
        assert_eq!(
            feature_health_for_observation(
                runtime.observation(Some(mxr_llm::LlmFeature::DraftAssist)),
                default.clone(),
            ),
            FeatureHealth::Degraded {
                reason: "last observed model request failed: endpoint unreachable".into(),
            }
        );
        assert_eq!(
            feature_health_for_observation(runtime.observation(None), default),
            FeatureHealth::Healthy
        );
    }

    /// Provider failures as the sync loop records them (`MxrError` display of
    /// each provider's error), across Gmail, IMAP and SMTP. Sync health and
    /// doctor must agree on every row, and only a rejected grant or rejected
    /// credentials may prescribe signing in again.
    #[test]
    fn provider_failures_classify_the_same_in_sync_health_and_doctor() {
        use mxr_protocol::SyncErrorKindData as Kind;
        const TOKEN_UNAVAILABLE: &str =
            "Provider error: Auth error: Token refresh temporarily unavailable";
        let rows: &[(&str, &str, Kind, DoctorFindingCategory, bool)] = &[
            // Gmail: a genuinely rejected grant, and an expired API session.
            ("gmail invalid_grant", "Provider error: Auth error: OAuth2 error: invalid_grant", Kind::Auth, DoctorFindingCategory::OAuth, true),
            ("gmail invalid_client", "Provider error: Auth error: OAuth2 error: invalid_client", Kind::Auth, DoctorFindingCategory::OAuth, true),
            ("gmail invalid_request", &format!("{TOKEN_UNAVAILABLE}: token endpoint returned invalid_request (HTTP 400 Bad Request)"), Kind::Offline, DoctorFindingCategory::Network, false),
            ("gmail unknown code", &format!("{TOKEN_UNAVAILABLE}: token endpoint returned upstream_failure (HTTP 400 Bad Request)"), Kind::Offline, DoctorFindingCategory::Network, false),
            ("gmail 401", "Provider error: Authentication expired — re-auth required", Kind::Auth, DoctorFindingCategory::OAuth, true),
            // Gmail: everything that is not a rejected grant.
            ("gmail token connection", &format!("{TOKEN_UNAVAILABLE}: token endpoint connection failed"), Kind::Offline, DoctorFindingCategory::Network, false),
            ("gmail token tls", &format!("{TOKEN_UNAVAILABLE}: token endpoint TLS handshake failed"), Kind::Offline, DoctorFindingCategory::Network, false),
            ("gmail token timeout", &format!("{TOKEN_UNAVAILABLE}: token endpoint request timed out"), Kind::Offline, DoctorFindingCategory::Network, false),
            ("gmail token 503", &format!("{TOKEN_UNAVAILABLE}: HTTP 503 Service Unavailable"), Kind::Offline, DoctorFindingCategory::Network, false),
            ("gmail token unreadable 4xx", &format!("{TOKEN_UNAVAILABLE}: token endpoint returned an unreadable HTTP 400 response"), Kind::Offline, DoctorFindingCategory::Network, false),
            ("gmail token 4xx without code", &format!("{TOKEN_UNAVAILABLE}: token endpoint returned HTTP 403 without an OAuth error code"), Kind::Offline, DoctorFindingCategory::Network, false),
            ("gmail token 429", "Rate limited — retry after 120s", Kind::RateLimited, DoctorFindingCategory::Sync, false),
            ("gmail 403 usage limit with reason", "Rate limited by Gmail messages.get (rateLimitExceeded: Rate Limit Exceeded; 412 requests in the last hour) — retry after 1234s", Kind::RateLimited, DoctorFindingCategory::Sync, false),
            ("gmail cache write", "Provider error: Local token cache write failed: Permission denied (os error 13)", Kind::Store, DoctorFindingCategory::Storage, false),
            // IMAP: rejected credentials, transport failure inside the login
            // wrapper, and an XOAUTH2 token fetch wrapped as an auth error.
            ("imap rejected credentials", "Provider error: Authentication failed: No Response: [AUTHENTICATIONFAILED] Invalid credentials (Failure)", Kind::Auth, DoctorFindingCategory::OAuth, true),
            ("imap login connection reset", "Provider error: Authentication failed: Connection reset by peer (os error 54)", Kind::Offline, DoctorFindingCategory::Network, false),
            ("imap login tls", "Provider error: Authentication failed: TLS handshake failed", Kind::Offline, DoctorFindingCategory::Network, false),
            ("imap connect", "Provider error: Connection error: tcp connect error: Connection refused (os error 61)", Kind::Offline, DoctorFindingCategory::Network, false),
            ("imap timeout", "Provider error: Timeout: LOGIN timed out after 30s", Kind::Offline, DoctorFindingCategory::Network, false),
            ("imap xoauth2 transport", &format!("Provider error: Authentication failed: token fetch failed: {TOKEN_UNAVAILABLE}: token endpoint connection failed"), Kind::Offline, DoctorFindingCategory::Network, false),
            ("imap xoauth2 invalid_grant", "Provider error: Authentication failed: token fetch failed: Auth error: OAuth2 error: invalid_grant", Kind::Auth, DoctorFindingCategory::OAuth, true),
            // SMTP.
            ("smtp rejected credentials", "Provider error: SMTP send failed: permanent error (535): 5.7.8 Authentication credentials invalid", Kind::Auth, DoctorFindingCategory::OAuth, true),
            ("smtp connection", "Provider error: SMTP send failed: connection error: connection refused", Kind::Offline, DoctorFindingCategory::Network, false),
        ];
        for (name, error, kind, category, reauthorizes) in rows {
            let class = crate::loops::classify_sync_error(error);
            assert_eq!(
                Kind::from_failure_class(Some(class)),
                *kind,
                "sync health: {name}"
            );
            let finding = classify_sync_error("work", error);
            assert_eq!(finding.category, *category, "doctor category: {name}");
            assert_eq!(
                finding
                    .remediation
                    .iter()
                    .any(|step| step.contains("accounts add")),
                *reauthorizes,
                "doctor reauthorization advice: {name}: {:?}",
                finding.remediation
            );
            assert!(
                !finding.remediation.is_empty(),
                "doctor names a next step: {name}"
            );
        }
    }

    #[test]
    fn a_rejected_client_or_scope_names_the_real_fix_before_reauthorizing() {
        for (code, expected) in [
            ("invalid_client", "OAuth client"),
            ("unauthorized_client", "OAuth client"),
            ("invalid_scope", "scopes"),
        ] {
            let finding = classify_sync_error(
                "work",
                &format!("Provider error: Auth error: OAuth2 error: {code}"),
            );
            assert!(
                finding.remediation[0].contains(expected),
                "{code}: {:?}",
                finding.remediation
            );
            assert!(finding.remediation.last().unwrap().contains("accounts add"));
        }
        let grant = classify_sync_error(
            "work",
            "Provider error: Auth error: OAuth2 error: invalid_grant",
        );
        assert_eq!(grant.remediation.len(), 1);
    }

    #[test]
    fn a_local_token_cache_failure_keeps_the_grant_and_names_storage() {
        let finding = classify_sync_error(
            "work",
            "Provider error: Local token cache write failed: Permission denied (os error 13)",
        );

        assert_eq!(finding.category, DoctorFindingCategory::Storage);
        let advice = finding.remediation.join("\n");
        assert!(advice.contains("keychain"), "{advice}");
        assert!(!advice.contains("accounts add"), "{advice}");
    }

    #[test]
    fn a_stale_address_cache_is_a_sync_warning_with_a_sync_remedy() {
        let findings = build_doctor_findings(DoctorFindingInputs {
            sync_statuses: &[],
            recent_errors: &[],
            data_dir_exists: true,
            database_exists: true,
            index_exists: true,
            socket_exists: true,
            repair_required: false,
            restart_required: false,
            semantic_enabled: false,
            search_worker_stopped: false,
            address_cache_stale: true,
            data_stats: &mxr_protocol::DoctorDataStats::default(),
        });

        let stale: Vec<_> = findings
            .iter()
            .filter(|finding| finding.message.contains("address cache"))
            .collect();
        assert_eq!(stale.len(), 1, "{findings:?}");
        assert_eq!(stale[0].category, DoctorFindingCategory::Sync);
        assert_eq!(stale[0].severity, DoctorFindingSeverity::Warning);
        assert_eq!(stale[0].remediation, vec!["mxr sync".to_string()]);
    }

    #[test]
    fn a_failure_is_reported_once_when_the_log_mirrors_sync_status() {
        let error = "Provider error: Auth error: Token refresh temporarily unavailable: token endpoint connection failed";
        let status = mxr_protocol::AccountSyncStatus {
            account_id: mxr_core::id::AccountId::new(),
            account_name: "work".into(),
            last_attempt_at: None,
            last_success_at: None,
            last_error: Some(error.to_string()),
            failure_class: Some("network".into()),
            consecutive_failures: 1,
            backoff_until: None,
            sync_in_progress: false,
            current_cursor_summary: None,
            last_synced_count: 0,
            healthy: false,
            progress: None,
        };
        let logged = format!("2026-10-21T10:00:00Z ERROR mxr: Sync error: {error}");
        let unrelated = "ERROR oauth token expired for another account";

        let findings = build_doctor_findings(DoctorFindingInputs {
            sync_statuses: &[status],
            recent_errors: &[
                logged.clone(),
                logged,
                unrelated.to_string(),
                unrelated.to_string(),
            ],
            data_dir_exists: true,
            database_exists: true,
            index_exists: true,
            socket_exists: true,
            repair_required: false,
            restart_required: false,
            semantic_enabled: false,
            search_worker_stopped: false,
            address_cache_stale: false,
            data_stats: &mxr_protocol::DoctorDataStats::default(),
        });

        let network: Vec<_> = findings
            .iter()
            .filter(|finding| finding.category == DoctorFindingCategory::Network)
            .collect();
        assert_eq!(network.len(), 1, "{findings:?}");
        assert_eq!(network[0].severity, DoctorFindingSeverity::Error);
        // A different failure in the log still surfaces, once.
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.message == "OAuth token issue in recent logs")
                .count(),
            1,
            "{findings:?}"
        );
    }

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
    fn oauth_transport_failures_offer_retry_not_reauthorization() {
        let error = "Provider error: Auth error: OAuth2 error: token refresh timed out after 60s";
        let finding = classify_sync_error("Work Account", error);
        assert_eq!(finding.category, DoctorFindingCategory::Network);
        assert!(finding
            .remediation
            .iter()
            .any(|step| step.starts_with("mxr sync")));
        assert_eq!(
            shell_words::split(&finding.remediation[0]).unwrap()[3],
            "Work Account"
        );

        let logged = classify_log_line(error).unwrap();
        assert_eq!(logged.category, DoctorFindingCategory::Network);
        assert!(!logged
            .remediation
            .iter()
            .any(|step| step.contains("accounts add")));
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
    fn classify_sync_error_gives_headless_credential_recovery_without_echoing_details() {
        let finding = classify_sync_error(
            "imap",
            "no credential found on disk for mxr/private-account (keychain disabled via MXR_KEYCHAIN=off)",
        );

        assert_eq!(finding.category, DoctorFindingCategory::Sync);
        assert_eq!(finding.severity, DoctorFindingSeverity::Error);
        assert_eq!(finding.message, "Missing mail credential for imap");
        assert!(finding.message.contains("imap"));
        assert!(finding
            .remediation
            .iter()
            .any(|step| step.contains("secrets.toml")));
        assert!(finding
            .remediation
            .iter()
            .any(|step| step.contains("MXR_KEYCHAIN=off")));
        assert!(!finding.message.contains("mxr/private-account"));
    }

    #[test]
    fn classify_sync_error_quotes_spaced_account_in_retry_command() {
        let finding = classify_sync_error("Missing Credential", "no credential found on disk");
        let command = finding
            .remediation
            .iter()
            .find(|step| step.starts_with("mxr sync --account "))
            .expect("missing-credential remediation should include a sync command");

        assert_eq!(
            shell_words::split(command).expect("remediation should be valid shell syntax"),
            [
                "mxr",
                "sync",
                "--account",
                "Missing Credential",
                "--wait",
                "--wait-timeout-secs",
                "120"
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
        assert!(classify_log_line("unrelated subsystem connection failed").is_none());
    }
}
