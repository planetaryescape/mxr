use crate::cli::OutputFormat;
use crate::handler::{dir_size_sync, doctor_data_stats, file_size_sync, recent_log_lines_sync};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::{
    AccountSyncStatus, DaemonHealthClass, DoctorDataStats, DoctorReport, EventLogEntry, Request,
    Response, ResponseData,
};
use mxr_search::SearchIndex;
use mxr_store::Store;
use tokio::net::UnixStream;

pub struct DoctorRunOptions {
    pub reindex: bool,
    pub reindex_semantic: bool,
    pub check: bool,
    pub semantic_status: bool,
    pub verbose: bool,
    pub index_stats: bool,
    pub store_stats: bool,
    pub rebuild_analytics: bool,
    pub refresh_contacts: bool,
    pub format: Option<OutputFormat>,
}

pub async fn run(options: DoctorRunOptions) -> anyhow::Result<()> {
    let fmt = resolve_format(options.format);

    if options.rebuild_analytics {
        let mut client = IpcClient::connect().await?;
        match client.request(Request::RebuildAnalytics).await? {
            Response::Ok {
                data: ResponseData::AnalyticsRebuildSummary {
                    directions_reclassified,
                    list_ids_backfilled,
                    reply_pairs_resolved,
                    business_hours_backfilled,
                    contacts_rows,
                },
            } => match fmt {
                OutputFormat::Json | OutputFormat::Jsonl => {
                    let value = serde_json::json!({
                        "directions_reclassified": directions_reclassified,
                        "list_ids_backfilled": list_ids_backfilled,
                        "reply_pairs_resolved": reply_pairs_resolved,
                        "business_hours_backfilled": business_hours_backfilled,
                        "contacts_rows": contacts_rows,
                    });
                    println!("{}", serde_json::to_string_pretty(&value)?);
                }
                _ => {
                    println!("Analytics rebuild complete:");
                    println!("  directions reclassified  : {directions_reclassified}");
                    println!("  list_ids backfilled      : {list_ids_backfilled}");
                    println!("  reply pairs resolved     : {reply_pairs_resolved}");
                    println!("  business-hours backfilled: {business_hours_backfilled}");
                    println!("  contacts rows refreshed  : {contacts_rows}");
                }
            },
            Response::Error { message } => anyhow::bail!("{message}"),
            other => anyhow::bail!("Unexpected response: {other:?}"),
        }
        return Ok(());
    }

    if options.refresh_contacts {
        let mut client = IpcClient::connect().await?;
        match client.request(Request::RefreshContacts).await? {
            Response::Ok {
                data: ResponseData::RefreshedContacts { rows },
            } => {
                println!("Refreshed {rows} contact rows.");
            }
            Response::Error { message } => anyhow::bail!("{message}"),
            other => anyhow::bail!("Unexpected response: {other:?}"),
        }
        return Ok(());
    }

    if options.semantic_status || options.reindex_semantic {
        let mut client = IpcClient::connect().await?;
        let request = if options.reindex_semantic {
            Request::ReindexSemantic
        } else {
            Request::GetSemanticStatus
        };
        let response = client.request(request).await?;
        match response {
            Response::Ok {
                data: ResponseData::SemanticStatus { snapshot },
            } => match fmt {
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&snapshot)?),
                OutputFormat::Jsonl => println!("{}", serde_json::to_string(&snapshot)?),
                _ => {
                    println!(
                        "enabled={} active_profile={}",
                        snapshot.enabled,
                        snapshot.active_profile.as_str()
                    );
                    for profile in snapshot.profiles {
                        println!(
                            "{} status={:?} dims={} indexed_at={}",
                            profile.profile.as_str(),
                            profile.status,
                            profile.dimensions,
                            profile
                                .last_indexed_at
                                .map(|v| v.to_rfc3339())
                                .unwrap_or_else(|| "-".into())
                        );
                    }
                }
            },
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        }
        if options.semantic_status && !options.reindex {
            return Ok(());
        }
    }

    let report = collect_report().await?;
    let data_dir = mxr_config::data_dir();
    let db_path = data_dir.join("mxr.db");
    let index_path = data_dir.join("search_index");

    if options.check {
        print_report(&report, fmt, options.verbose)?;
        if report.healthy {
            return Ok(());
        }
        anyhow::bail!("mxr health check failed");
    }

    if options.index_stats {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "path": report.index_path,
                "exists": report.index_exists,
                "size_bytes": report.index_size_bytes,
                "index_lock_held": report.index_lock_held,
                "index_lock_error": report.index_lock_error,
            }))?
        );
        return Ok(());
    }

    if options.store_stats {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "database_path": report.database_path,
                "database_size_bytes": report.database_size_bytes,
                "log_path": report.log_path,
                "log_size_bytes": report.log_size_bytes,
                "accounts": report.sync_statuses.len(),
            }))?
        );
        return Ok(());
    }

    print_report(&report, fmt, options.verbose)?;

    if options.reindex {
        println!("\nReindex requested - this requires daemon restart to take effect.");
        if index_path.exists() {
            std::fs::remove_dir_all(&index_path)?;
            println!("Removed search index directory. Restart daemon to rebuild.");
        }
    }

    if !db_path.exists() {
        println!("\nNext: configure an account, then run `mxr daemon --foreground`.");
    }

    Ok(())
}

async fn collect_report() -> anyhow::Result<DoctorReport> {
    let data_dir = mxr_config::data_dir();
    let db_path = data_dir.join("mxr.db");
    let index_path = data_dir.join("search_index");
    let log_path = data_dir.join("logs").join("mxr.log");
    let socket_path = crate::state::AppState::socket_path();

    let data_dir_exists = data_dir.exists();
    let database_exists = db_path.exists();
    let index_exists = index_path.exists();
    let socket_exists = socket_path.exists();
    let socket_reachable = UnixStream::connect(&socket_path).await.is_ok();
    let stale_socket = socket_exists && !socket_reachable;

    let mut daemon_pid = None;
    let mut daemon_running = socket_reachable;
    let mut sync_statuses = Vec::new();
    let mut daemon_protocol_version = 0;
    let mut daemon_version = None;
    let mut daemon_build_id = None;
    let mut repair_required = false;
    let mut restart_required = false;
    let mut semantic_enabled = false;
    let mut semantic_active_profile = None;
    let mut semantic_index_freshness = mxr_protocol::IndexFreshness::Disabled;
    let mut semantic_last_indexed_at = None;

    if socket_reachable {
        if let Ok(mut client) = IpcClient::connect().await {
            match client.request(Request::GetStatus).await {
                Ok(Response::Ok {
                    data:
                        ResponseData::Status {
                            daemon_pid: pid,
                            sync_statuses: statuses,
                            protocol_version,
                            daemon_version: version,
                            daemon_build_id: build_id,
                            repair_required: repair,
                            ..
                        },
                }) => {
                    daemon_pid = pid;
                    sync_statuses = statuses;
                    daemon_protocol_version = protocol_version;
                    daemon_version = version;
                    daemon_build_id = build_id;
                    repair_required = repair;
                    restart_required = crate::server::daemon_requires_restart(
                        daemon_protocol_version,
                        daemon_version.as_deref(),
                        daemon_build_id.as_deref(),
                    );
                }
                _ => {
                    restart_required = true;
                }
            }
            if !restart_required {
                if let Ok(Response::Ok {
                    data: ResponseData::SemanticStatus { snapshot },
                }) = client.request(Request::GetSemanticStatus).await
                {
                    (
                        semantic_enabled,
                        semantic_active_profile,
                        semantic_index_freshness,
                        semantic_last_indexed_at,
                    ) = semantic_freshness_from_snapshot(
                        Some(&snapshot),
                        snapshot.enabled,
                        snapshot.active_profile.as_str(),
                    );
                }
            }
        } else {
            restart_required = true;
        }
    }

    let mut recent_sync_events = Vec::new();
    let mut data_stats = DoctorDataStats::default();
    if database_exists {
        let store = Store::new(&db_path).await?;
        if sync_statuses.is_empty() {
            sync_statuses = collect_sync_statuses_from_store(&store).await?;
        }
        data_stats = doctor_data_stats(store.collect_record_counts().await?);
        if semantic_active_profile.is_none() {
            let config = mxr_config::load_config().unwrap_or_default();
            let active_profile = config.search.semantic.active_profile;
            let active_record = store.get_semantic_profile(active_profile).await?;
            (
                semantic_enabled,
                semantic_active_profile,
                semantic_index_freshness,
                semantic_last_indexed_at,
            ) = semantic_freshness_from_store(
                config.search.semantic.enabled,
                active_profile,
                active_record.as_ref(),
            );
        }
        recent_sync_events = store
            .list_events(10, None, Some("sync"))
            .await?
            .into_iter()
            .map(protocol_event_entry)
            .collect();
    } else {
        daemon_running = false;
    }

    let (index_lock_held, index_lock_error) = probe_index_lock(&index_path, socket_reachable);
    let recent_error_logs = recent_log_lines_sync(10, Some("error")).unwrap_or_default();
    let lexical_index_freshness =
        lexical_index_freshness(index_exists, repair_required, restart_required);
    let last_successful_sync_at = latest_successful_sync_at(&sync_statuses);
    let lexical_last_rebuilt_at = if database_exists {
        let store = Store::new(&db_path).await?;
        store
            .latest_event_timestamp("search", Some("Lexical index rebuilt"))
            .await?
            .map(|value| value.to_rfc3339())
    } else {
        None
    };
    let health_class = if restart_required {
        DaemonHealthClass::RestartRequired
    } else if repair_required
        || stale_socket
        || !data_dir_exists
        || !database_exists
        || !index_exists
        || index_lock_held
    {
        DaemonHealthClass::RepairRequired
    } else if !socket_reachable || sync_statuses.iter().any(|status| !status.healthy) {
        DaemonHealthClass::Degraded
    } else {
        DaemonHealthClass::Healthy
    };
    let recommended_next_steps = recommended_next_steps(
        socket_reachable,
        stale_socket,
        &sync_statuses,
        restart_required,
        repair_required,
    );
    let healthy = data_dir_exists
        && database_exists
        && index_exists
        && socket_reachable
        && !index_lock_held
        && matches!(health_class, DaemonHealthClass::Healthy);

    Ok(DoctorReport {
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
        socket_reachable,
        stale_socket,
        daemon_running,
        daemon_pid,
        daemon_protocol_version,
        daemon_version,
        daemon_build_id,
        index_lock_held,
        index_lock_error,
        restart_required,
        repair_required,
        database_path: db_path.display().to_string(),
        database_size_bytes: file_size_sync(&db_path),
        index_path: index_path.display().to_string(),
        index_size_bytes: dir_size_sync(&index_path),
        log_path: log_path.display().to_string(),
        log_size_bytes: file_size_sync(&log_path),
        sync_statuses,
        recent_sync_events,
        recent_error_logs,
        recommended_next_steps,
    })
}

async fn collect_sync_statuses_from_store(store: &Store) -> anyhow::Result<Vec<AccountSyncStatus>> {
    let accounts = store.list_accounts().await?;
    let mut statuses = Vec::new();

    for account in accounts {
        let runtime = store.get_sync_runtime_status(&account.id).await?;
        let cursor = store.get_sync_cursor(&account.id).await?;
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
            .map(|row| row.sync_in_progress)
            .unwrap_or(false);

        statuses.push(AccountSyncStatus {
            account_id: account.id,
            account_name: account.name,
            last_attempt_at: runtime
                .as_ref()
                .and_then(|row| row.last_attempt_at)
                .map(|dt| dt.to_rfc3339()),
            last_success_at: last_success_at.clone(),
            last_error,
            failure_class: runtime.as_ref().and_then(|row| row.failure_class.clone()),
            consecutive_failures: runtime
                .as_ref()
                .map(|row| row.consecutive_failures)
                .unwrap_or(0),
            backoff_until: backoff_until.clone(),
            sync_in_progress,
            current_cursor_summary: Some(
                runtime
                    .as_ref()
                    .and_then(|row| row.current_cursor_summary.clone())
                    .unwrap_or_else(|| describe_cursor(cursor.as_ref())),
            ),
            last_synced_count: runtime
                .as_ref()
                .map(|row| row.last_synced_count)
                .unwrap_or(0),
            healthy: !sync_in_progress
                && backoff_until.is_none()
                && last_success_at.is_some()
                && runtime
                    .as_ref()
                    .and_then(|row| row.last_error.as_ref())
                    .is_none(),
        });
    }

    Ok(statuses)
}

fn print_report(report: &DoctorReport, format: OutputFormat, verbose: bool) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
        }
        OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string(report)?);
        }
        _ => {
            println!("Health:       {}", report.health_class.as_str());
            println!("Healthy:      {}", report.healthy);
            println!(
                "Daemon:       {}{}",
                if report.daemon_running {
                    "running"
                } else {
                    "down"
                },
                daemon_pid_suffix(report.daemon_pid)
            );
            println!(
                "Socket:       {} (exists: {}, stale: {})",
                if report.socket_reachable {
                    "reachable"
                } else {
                    "unreachable"
                },
                report.socket_exists,
                report.stale_socket
            );
            println!(
                "Search index: {} (exists: {}, lock_held: {})",
                report.index_path, report.index_exists, report.index_lock_held
            );
            println!(
                "Database:     {} (exists: {})",
                report.database_path, report.database_exists
            );
            println!(
                "Logs:         {} ({} bytes)",
                report.log_path, report.log_size_bytes
            );
            println!(
                "Daemon info:  version={} protocol={} build={}",
                report.daemon_version.as_deref().unwrap_or("unknown"),
                report.daemon_protocol_version,
                report.daemon_build_id.as_deref().unwrap_or("unknown")
            );
            println!(
                "Lifecycle:    restart_required={} repair_required={}",
                report.restart_required, report.repair_required
            );
            println!(
                "Freshness:    last_sync={} lexical={} rebuilt_at={} semantic={} profile={} indexed_at={}",
                report.last_successful_sync_at.as_deref().unwrap_or("never"),
                report.lexical_index_freshness.as_str(),
                report.lexical_last_rebuilt_at.as_deref().unwrap_or("-"),
                report.semantic_index_freshness.as_str(),
                report.semantic_active_profile.as_deref().unwrap_or("-"),
                report.semantic_last_indexed_at.as_deref().unwrap_or("-"),
            );
            println!(
                "Data:         accounts={} labels={} messages={} unread={} starred={} attachments={}",
                report.data_stats.accounts,
                report.data_stats.labels,
                report.data_stats.messages,
                report.data_stats.unread_messages,
                report.data_stats.starred_messages,
                report.data_stats.attachments,
            );
            println!(
                "Records:      bodies={} message_labels={} drafts={} snoozed={} saved={} rules={}",
                report.data_stats.bodies,
                report.data_stats.message_labels,
                report.data_stats.drafts,
                report.data_stats.snoozed,
                report.data_stats.saved_searches,
                report.data_stats.rules,
            );
            println!(
                "Telemetry:    event_log={} sync_log={} runtime_status={} rule_logs={}",
                report.data_stats.event_log,
                report.data_stats.sync_log,
                report.data_stats.sync_runtime_statuses,
                report.data_stats.rule_logs,
            );
            if report.data_stats.semantic_profiles > 0
                || report.data_stats.semantic_chunks > 0
                || report.data_stats.semantic_embeddings > 0
            {
                println!(
                    "Semantic:     profiles={} chunks={} embeddings={}",
                    report.data_stats.semantic_profiles,
                    report.data_stats.semantic_chunks,
                    report.data_stats.semantic_embeddings,
                );
            }

            if let Some(error) = &report.index_lock_error {
                println!("Index lock:   {error}");
            }

            println!("\nSync health:");
            if report.sync_statuses.is_empty() {
                println!("  no accounts");
            } else {
                for sync in &report.sync_statuses {
                    println!(
                        "  {}: healthy={} in_progress={} last_success={} last_error={}",
                        sync.account_name,
                        sync.healthy,
                        sync.sync_in_progress,
                        sync.last_success_at.as_deref().unwrap_or("never"),
                        sync.last_error.as_deref().unwrap_or("-"),
                    );
                    if verbose {
                        println!(
                            "    failures={} backoff_until={} cursor={} last_synced_count={}",
                            sync.consecutive_failures,
                            sync.backoff_until.as_deref().unwrap_or("-"),
                            sync.current_cursor_summary.as_deref().unwrap_or("-"),
                            sync.last_synced_count,
                        );
                    }
                }
            }

            if verbose {
                println!("\nRecent sync events:");
                if report.recent_sync_events.is_empty() {
                    println!("  none");
                } else {
                    for event in &report.recent_sync_events {
                        println!("  {} [{}] {}", event.timestamp, event.level, event.summary);
                    }
                }

                println!("\nRecent error logs:");
                if report.recent_error_logs.is_empty() {
                    println!("  none");
                } else {
                    for line in &report.recent_error_logs {
                        println!("  {line}");
                    }
                }
            }

            println!("\nNext:");
            for step in &report.recommended_next_steps {
                println!("  {step}");
            }
        }
    }

    Ok(())
}

fn daemon_pid_suffix(pid: Option<u32>) -> String {
    pid.map(|pid| format!(" (pid {pid})")).unwrap_or_default()
}

fn probe_index_lock(index_path: &std::path::Path, daemon_running: bool) -> (bool, Option<String>) {
    if daemon_running || !index_path.exists() {
        return (false, None);
    }

    match SearchIndex::open(index_path) {
        Ok(_) => (false, None),
        Err(error) => {
            let message = error.to_string();
            let locked = message.contains("LockBusy") || message.contains("Lockfile");
            (locked, Some(message))
        }
    }
}

fn recommended_next_steps(
    socket_reachable: bool,
    stale_socket: bool,
    sync_statuses: &[AccountSyncStatus],
    restart_required: bool,
    repair_required: bool,
) -> Vec<String> {
    if stale_socket {
        return vec![
            format!(
                "rm {}",
                shell_escape_path(&crate::state::AppState::socket_path())
            ),
            "mxr daemon --foreground".to_string(),
            "mxr status".to_string(),
        ];
    }

    if !socket_reachable {
        return vec![
            "mxr daemon --foreground".to_string(),
            "mxr logs --level error".to_string(),
            "mxr doctor --verbose".to_string(),
        ];
    }

    if restart_required {
        return vec!["mxr restart".to_string(), "mxr status".to_string()];
    }

    if repair_required {
        return vec![
            "mxr doctor --reindex".to_string(),
            "mxr restart".to_string(),
            "mxr status".to_string(),
        ];
    }

    if sync_statuses.iter().any(|status| !status.healthy) {
        return vec![
            "mxr sync --status".to_string(),
            "mxr logs --level error".to_string(),
            "mxr status".to_string(),
        ];
    }

    vec!["mxr status".to_string()]
}

fn shell_escape_path(path: &std::path::Path) -> String {
    path.display().to_string().replace(' ', "\\ ")
}

fn describe_cursor(cursor: Option<&mxr_core::types::SyncCursor>) -> String {
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

fn protocol_event_entry(entry: mxr_store::EventLogEntry) -> EventLogEntry {
    EventLogEntry {
        timestamp: entry.timestamp,
        level: entry.level,
        category: entry.category,
        account_id: entry.account_id,
        message_id: entry.message_id,
        rule_id: entry.rule_id,
        summary: entry.summary,
        details: entry.details,
    }
}

use crate::handler::latest_successful_sync_at;

fn lexical_index_freshness(
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

fn semantic_freshness_from_snapshot(
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

fn semantic_freshness_from_store(
    enabled: bool,
    active_profile: mxr_core::SemanticProfile,
    active_record: Option<&mxr_core::types::SemanticProfileRecord>,
) -> (
    bool,
    Option<String>,
    mxr_protocol::IndexFreshness,
    Option<String>,
) {
    if !enabled {
        return (false, None, mxr_protocol::IndexFreshness::Disabled, None);
    }

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
        Some(active_profile.as_str().to_string()),
        freshness,
        active_record
            .and_then(|profile| profile.last_indexed_at)
            .map(|value| value.to_rfc3339()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommends_foreground_when_socket_unreachable() {
        let steps = recommended_next_steps(false, false, &[], false, false);
        assert!(steps
            .iter()
            .any(|step| step.contains("daemon --foreground")));
    }

    #[test]
    fn recommends_restart_when_daemon_mismatch_detected() {
        let steps = recommended_next_steps(true, false, &[], true, false);
        assert_eq!(
            steps,
            vec!["mxr restart".to_string(), "mxr status".to_string()]
        );
    }
}
