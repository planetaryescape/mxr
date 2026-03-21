use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::{
    AccountSyncStatus, DoctorReport, EventLogEntry, Request, Response, ResponseData,
};
use mxr_search::SearchIndex;
use mxr_store::Store;
use std::io::BufRead;
use tokio::net::UnixStream;

pub async fn run(
    reindex: bool,
    reindex_semantic: bool,
    check: bool,
    semantic_status: bool,
    verbose: bool,
    index_stats: bool,
    store_stats: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let fmt = resolve_format(format);

    if semantic_status || reindex_semantic {
        let mut client = IpcClient::connect().await?;
        let request = if reindex_semantic {
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
        if semantic_status && !reindex {
            return Ok(());
        }
    }

    let report = collect_report().await?;
    let data_dir = mxr_config::data_dir();
    let db_path = data_dir.join("mxr.db");
    let index_path = data_dir.join("search_index");

    if check {
        print_report(&report, fmt, verbose)?;
        if report.healthy {
            return Ok(());
        }
        anyhow::bail!("mxr health check failed");
    }

    if index_stats {
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

    if store_stats {
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

    print_report(&report, fmt, verbose)?;

    if reindex {
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

    if socket_reachable {
        if let Ok(mut client) = IpcClient::connect().await {
            if let Ok(Response::Ok {
                data:
                    ResponseData::Status {
                        daemon_pid: pid,
                        sync_statuses: statuses,
                        ..
                    },
            }) = client.request(Request::GetStatus).await
            {
                daemon_pid = pid;
                sync_statuses = statuses;
            }
        }
    }

    let mut recent_sync_events = Vec::new();
    if database_exists {
        let store = Store::new(&db_path).await?;
        if sync_statuses.is_empty() {
            sync_statuses = collect_sync_statuses_from_store(&store).await?;
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
    let recent_error_logs = recent_log_lines(10, Some("error")).unwrap_or_default();
    let recommended_next_steps =
        recommended_next_steps(socket_reachable, stale_socket, &sync_statuses);
    let healthy = data_dir_exists
        && database_exists
        && index_exists
        && socket_reachable
        && !index_lock_held
        && sync_statuses.iter().all(|status| status.healthy);

    Ok(DoctorReport {
        healthy,
        data_dir_exists,
        database_exists,
        index_exists,
        socket_exists,
        socket_reachable,
        stale_socket,
        daemon_running,
        daemon_pid,
        index_lock_held,
        index_lock_error,
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
        _ => {
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

fn recent_log_lines(limit: usize, level: Option<&str>) -> Result<Vec<String>, std::io::Error> {
    let log_path = mxr_config::data_dir().join("logs").join("mxr.log");
    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let file = std::fs::File::open(log_path)?;
    let mut lines = std::io::BufReader::new(file)
        .lines()
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(level) = level {
        let level = level.to_ascii_lowercase();
        lines.retain(|line| line.to_ascii_lowercase().contains(&level));
    }
    let start = lines.len().saturating_sub(limit.max(1));
    Ok(lines.split_off(start))
}

fn file_size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn dir_size(path: &std::path::Path) -> u64 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommends_foreground_when_socket_unreachable() {
        let steps = recommended_next_steps(false, false, &[]);
        assert!(steps
            .iter()
            .any(|step| step.contains("daemon --foreground")));
    }
}
