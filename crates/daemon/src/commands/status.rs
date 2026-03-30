#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::{
    AccountSyncStatus, DaemonHealthClass, Request, Response, ResponseData, IPC_PROTOCOL_VERSION,
};

struct StatusRender<'a> {
    uptime_secs: u64,
    accounts: &'a [String],
    total_messages: u32,
    daemon_pid: Option<u32>,
    sync_statuses: &'a [AccountSyncStatus],
    daemon_version: Option<&'a str>,
    daemon_build_id: Option<&'a str>,
    protocol_version: u32,
    repair_required: bool,
    restart_required: bool,
    health_class: DaemonHealthClass,
}

fn render_status(view: StatusRender<'_>, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => serde_json::to_string_pretty(&serde_json::json!({
            "uptime_secs": view.uptime_secs,
            "accounts": view.accounts,
            "total_messages": view.total_messages,
            "daemon_pid": view.daemon_pid,
            "sync_statuses": view.sync_statuses,
            "daemon_version": view.daemon_version,
            "daemon_build_id": view.daemon_build_id,
            "protocol_version": view.protocol_version,
            "repair_required": view.repair_required,
            "restart_required": view.restart_required,
            "health_class": view.health_class,
        }))?,
        _ => {
            let mut lines = vec![
                format!("Health: {}", view.health_class.as_str()),
                format!("Uptime: {}s", view.uptime_secs),
                format!(
                    "Daemon PID: {}",
                    view.daemon_pid
                        .map(|pid| pid.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ),
                format!("Accounts: {}", view.accounts.join(", ")),
                format!("Total messages: {}", view.total_messages),
                format!(
                    "Daemon version: {}",
                    view.daemon_version.unwrap_or("legacy/unknown")
                ),
                format!(
                    "Build: {}",
                    view.daemon_build_id.unwrap_or("legacy/unknown")
                ),
                "Sync:".to_string(),
            ];
            if view.sync_statuses.is_empty() {
                if view.protocol_version < IPC_PROTOCOL_VERSION {
                    lines.push("  unavailable from legacy daemon".to_string());
                } else {
                    lines.push("  no accounts".to_string());
                }
            } else {
                for sync in view.sync_statuses {
                    lines.push(format!(
                        "  {} healthy={} in_progress={} last_success={} last_error={}",
                        sync.account_name,
                        sync.healthy,
                        sync.sync_in_progress,
                        sync.last_success_at.as_deref().unwrap_or("never"),
                        sync.last_error.as_deref().unwrap_or("-"),
                    ));
                }
            }
            if view.restart_required {
                lines.push(format!(
                    "Note: running daemon does not match this binary (protocol {}, client {}). Use `mxr restart`.",
                    view.protocol_version, IPC_PROTOCOL_VERSION
                ));
            }
            if view.repair_required {
                lines.push(
                    "Note: search index needs repair or rebuild. Use `mxr doctor --reindex` or restart the daemon."
                        .to_string(),
                );
            }
            lines.join("\n")
        }
    })
}

pub async fn run(format: Option<OutputFormat>, watch: bool) -> anyhow::Result<()> {
    let fmt = resolve_format(format);

    loop {
        let mut client = IpcClient::connect().await?;
        let resp = client.request(Request::GetStatus).await?;

        match resp {
            Response::Ok {
                data:
                    ResponseData::Status {
                        uptime_secs,
                        accounts,
                        total_messages,
                        daemon_pid,
                        sync_statuses,
                        protocol_version,
                        daemon_version,
                        daemon_build_id,
                        repair_required,
                    },
            } => {
                let restart_required = crate::server::daemon_requires_restart(
                    protocol_version,
                    daemon_version.as_deref(),
                    daemon_build_id.as_deref(),
                );
                let health_class = crate::server::classify_health(
                    &sync_statuses,
                    repair_required,
                    restart_required,
                );
                println!(
                    "{}",
                    render_status(
                        StatusRender {
                            uptime_secs,
                            accounts: &accounts,
                            total_messages,
                            daemon_pid,
                            sync_statuses: &sync_statuses,
                            daemon_version: daemon_version.as_deref(),
                            daemon_build_id: daemon_build_id.as_deref(),
                            protocol_version,
                            repair_required,
                            restart_required,
                            health_class,
                        },
                        fmt.clone(),
                    )?
                );
            }
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        }

        if !watch {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if fmt != OutputFormat::Json {
            println!();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::AccountId;

    #[test]
    fn render_status_json_has_expected_fields() {
        let rendered = render_status(
            StatusRender {
                uptime_secs: 42,
                accounts: &["main".into()],
                total_messages: 10,
                daemon_pid: Some(999),
                sync_statuses: &[AccountSyncStatus {
                    account_id: AccountId::new(),
                    account_name: "main".into(),
                    last_attempt_at: None,
                    last_success_at: Some("2026-03-20T10:00:00+00:00".into()),
                    last_error: None,
                    failure_class: None,
                    consecutive_failures: 0,
                    backoff_until: None,
                    sync_in_progress: false,
                    current_cursor_summary: Some("initial".into()),
                    last_synced_count: 10,
                    healthy: true,
                }],
                daemon_version: Some("0.4.6"),
                daemon_build_id: Some("0.4.6:/tmp/mxr:1:1"),
                protocol_version: IPC_PROTOCOL_VERSION,
                repair_required: false,
                restart_required: false,
                health_class: DaemonHealthClass::Healthy,
            },
            OutputFormat::Json,
        )
        .unwrap();
        assert!(rendered.contains("\"uptime_secs\": 42"));
        assert!(rendered.contains("\"daemon_pid\": 999"));
        assert!(rendered.contains("\"total_messages\": 10"));
    }

    #[test]
    fn render_status_table_includes_health_class() {
        let rendered = render_status(
            StatusRender {
                uptime_secs: 1,
                accounts: &["main".into()],
                total_messages: 10,
                daemon_pid: None,
                sync_statuses: &[],
                daemon_version: Some("0.4.6"),
                daemon_build_id: Some("0.4.6:/tmp/mxr:1:1"),
                protocol_version: IPC_PROTOCOL_VERSION,
                repair_required: true,
                restart_required: false,
                health_class: DaemonHealthClass::RepairRequired,
            },
            OutputFormat::Table,
        )
        .unwrap();
        assert!(rendered.contains("Health: repair_required"));
        assert!(rendered.contains("search index needs repair"));
    }
}
