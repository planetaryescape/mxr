#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

use crate::cli::OutputFormat;
use crate::commands::diagnostics::{client_local_metadata, selected_target_metadata};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::types::SemanticRuntimeMetrics;
use mxr_protocol::{
    AccountSyncStatus, DaemonHealthClass, FeatureHealthReport, Request, Response, ResponseData,
    IPC_PROTOCOL_VERSION,
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
    semantic_runtime: Option<&'a SemanticRuntimeMetrics>,
    feature_health: Option<&'a FeatureHealthReport>,
    restart_required: bool,
    health_class: DaemonHealthClass,
    degraded: bool,
    selected_target: serde_json::Value,
    client_local: serde_json::Value,
    remote_target: bool,
}

fn render_status(view: StatusRender<'_>, format: OutputFormat) -> anyhow::Result<String> {
    let mut data = serde_json::json!({
        "uptime_secs": view.uptime_secs,
        "accounts": view.accounts,
        "total_messages": view.total_messages,
        "daemon_pid": view.daemon_pid,
        "sync_statuses": view.sync_statuses,
        "daemon_version": view.daemon_version,
        "daemon_build_id": view.daemon_build_id,
        "protocol_version": view.protocol_version,
        "repair_required": view.repair_required,
        "semantic_runtime": view.semantic_runtime,
        "feature_health": view.feature_health,
        "restart_required": view.restart_required,
        "health_class": view.health_class,
        "degraded": view.degraded,
    });
    if view.remote_target {
        data["daemon_target"] = view.selected_target.clone();
        data["client_local"] = view.client_local.clone();
    } else {
        data["runtime_instance"] = serde_json::json!(mxr_config::app_instance_name());
        data["config_path"] = serde_json::json!(mxr_config::config_file_path());
        data["data_dir"] = serde_json::json!(mxr_config::data_dir());
        data["socket_path"] = view.client_local["socket_path"].clone();
    }
    Ok(match format {
        OutputFormat::Json => serde_json::to_string_pretty(&data)?,
        OutputFormat::Jsonl => serde_json::to_string(&data)?,
        _ => {
            let mut lines = vec![
                format!(
                    "{}: {}",
                    if view.remote_target {
                        "Selected daemon"
                    } else {
                        "Socket"
                    },
                    if view.remote_target {
                        view.selected_target["display"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string()
                    } else {
                        view.client_local["socket_path"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string()
                    }
                ),
                format!(
                    "{}: {}",
                    if view.remote_target {
                        "Client runtime"
                    } else {
                        "Runtime"
                    },
                    mxr_config::app_instance_name()
                ),
                format!(
                    "{}: {}",
                    if view.remote_target {
                        "Client config"
                    } else {
                        "Config"
                    },
                    mxr_config::config_file_path().display()
                ),
                format!(
                    "{}: {}",
                    if view.remote_target {
                        "Client data"
                    } else {
                        "Data"
                    },
                    mxr_config::data_dir().display()
                ),
                format!("Health: {}", view.health_class.as_str()),
                format!("Uptime: {}s", view.uptime_secs),
                format!(
                    "Daemon PID: {}",
                    view.daemon_pid
                        .map_or_else(|| "unknown".to_string(), |pid| pid.to_string())
                ),
                // A degraded snapshot has no reading behind these, so print
                // what is true — that the daemon did not say — rather than the
                // zero and empty string it sent as filler.
                format!(
                    "Accounts: {}",
                    if view.degraded {
                        "unknown".to_string()
                    } else {
                        view.accounts.join(", ")
                    }
                ),
                format!(
                    "Total messages: {}",
                    if view.degraded {
                        "unknown".to_string()
                    } else {
                        view.total_messages.to_string()
                    }
                ),
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
            if view.degraded {
                lines.push("  unknown".to_string());
            } else if view.sync_statuses.is_empty() {
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
            if view.degraded {
                lines.push(
                    "Note: the daemon could not read the database within its status budget. Rerun once the current sync or reindex settles."
                        .to_string(),
                );
            }
            if view.restart_required {
                if view.remote_target {
                    lines.push(format!(
                        "Note: selected daemon protocol {} differs from this client protocol {}; check that this client supports the selected daemon.",
                        view.protocol_version, IPC_PROTOCOL_VERSION
                    ));
                } else {
                    lines.push(format!(
                        "Note: running daemon does not match this binary (protocol {}, client {}). Use `mxr restart`.",
                        view.protocol_version, IPC_PROTOCOL_VERSION
                    ));
                }
            }
            if view.repair_required {
                if view.remote_target {
                    lines.push(
                        "Note: the selected daemon's search index needs repair. Repair it on the daemon host, then restart that daemon."
                            .to_string(),
                    );
                } else {
                    lines.push(
                        "Note: search index needs repair or rebuild. Use `mxr doctor --reindex` or restart the daemon."
                            .to_string(),
                    );
                }
            }
            lines.join("\n")
        }
    })
}

pub async fn run(format: Option<OutputFormat>, watch: bool) -> anyhow::Result<()> {
    let fmt = resolve_format(format);
    let (selected_target, remote_target) = selected_target_metadata()?;

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
                        semantic_runtime,
                        feature_health,
                        degraded,
                    },
            } => {
                let restart_required = if remote_target {
                    protocol_version != IPC_PROTOCOL_VERSION
                } else {
                    crate::server::daemon_requires_restart(
                        protocol_version,
                        daemon_version.as_deref(),
                        daemon_build_id.as_deref(),
                    )
                };
                let health_class = crate::server::classify_health(
                    &sync_statuses,
                    repair_required,
                    restart_required,
                    degraded,
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
                            semantic_runtime: semantic_runtime.as_ref(),
                            feature_health: feature_health.as_ref(),
                            restart_required,
                            health_class,
                            degraded,
                            selected_target: selected_target.clone(),
                            client_local: client_local_metadata()?,
                            remote_target,
                        },
                        fmt.clone(),
                    )?
                );
            }
            Response::Error { message, .. } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        }

        if !watch {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if !matches!(fmt, OutputFormat::Json | OutputFormat::Jsonl) {
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
                    progress: None,
                }],
                daemon_version: Some("0.4.6"),
                daemon_build_id: Some("0.4.6:/tmp/mxr:1:1"),
                protocol_version: IPC_PROTOCOL_VERSION,
                repair_required: false,
                semantic_runtime: Some(&SemanticRuntimeMetrics {
                    queue_depth: 3,
                    in_flight: 1,
                    last_queue_wait_ms: Some(12),
                    last_extract_ms: Some(34),
                    last_embedding_prep_ms: Some(56),
                    last_ingest_ms: Some(78),
                }),
                feature_health: None,
                restart_required: false,
                health_class: DaemonHealthClass::Healthy,
                degraded: false,
                selected_target: serde_json::json!({"display":"unix:///tmp/mxr.sock"}),
                client_local: serde_json::json!({"socket_path":"/tmp/mxr.sock"}),
                remote_target: false,
            },
            OutputFormat::Json,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["uptime_secs"], 42);
        assert_eq!(value["daemon_pid"], 999);
        assert_eq!(value["total_messages"], 10);
        assert_eq!(value["runtime_instance"], mxr_config::app_instance_name());
        assert_eq!(
            value["config_path"],
            mxr_config::config_file_path().display().to_string()
        );
        assert_eq!(
            value["data_dir"],
            mxr_config::data_dir().display().to_string()
        );
        assert_eq!(value["socket_path"], "/tmp/mxr.sock");
        assert!(value.get("daemon_target").is_none());
        assert!(value.get("client_local").is_none());
        assert!(value.get("semantic_runtime").is_some());
    }

    #[test]
    fn a_degraded_status_table_prints_unknown_rather_than_zero() {
        let rendered = render_status(
            StatusRender {
                uptime_secs: 1,
                accounts: &[],
                total_messages: 0,
                daemon_pid: Some(999),
                sync_statuses: &[],
                daemon_version: Some("0.4.6"),
                daemon_build_id: Some("0.4.6:/tmp/mxr:1:1"),
                protocol_version: IPC_PROTOCOL_VERSION,
                repair_required: false,
                semantic_runtime: None,
                feature_health: None,
                restart_required: false,
                health_class: DaemonHealthClass::Degraded,
                degraded: true,
                selected_target: serde_json::json!({"display":"unix:///tmp/mxr.sock"}),
                client_local: serde_json::json!({"socket_path":"/tmp/mxr.sock"}),
                remote_target: false,
            },
            OutputFormat::Table,
        )
        .unwrap();

        // The zero and the empty list are filler the daemon sent because it
        // ran out of time to read, not a reading. Printing them and then
        // adding a note that contradicts them is worse than saying nothing.
        assert!(rendered.contains("Health: degraded"), "got {rendered}");
        assert!(rendered.contains("Accounts: unknown"), "got {rendered}");
        assert!(
            rendered.contains("Total messages: unknown"),
            "got {rendered}"
        );
        assert!(!rendered.contains("no accounts"), "got {rendered}");
        assert!(
            rendered.contains("could not read the database"),
            "got {rendered}"
        );
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
                semantic_runtime: None,
                feature_health: None,
                restart_required: false,
                health_class: DaemonHealthClass::RepairRequired,
                degraded: false,
                selected_target: serde_json::json!({"display":"unix:///tmp/mxr.sock"}),
                client_local: serde_json::json!({"socket_path":"/tmp/mxr.sock"}),
                remote_target: false,
            },
            OutputFormat::Table,
        )
        .unwrap();
        assert!(rendered.contains("Health: repair_required"));
        assert!(rendered.contains("search index needs repair"));
    }

    #[test]
    fn remote_repair_advice_points_to_selected_daemon_host() {
        let rendered = render_status(
            StatusRender {
                uptime_secs: 1,
                accounts: &[],
                total_messages: 0,
                daemon_pid: None,
                sync_statuses: &[],
                daemon_version: None,
                daemon_build_id: None,
                protocol_version: IPC_PROTOCOL_VERSION,
                repair_required: true,
                semantic_runtime: None,
                feature_health: None,
                restart_required: false,
                health_class: DaemonHealthClass::RepairRequired,
                degraded: false,
                selected_target: serde_json::json!({"display":"cmd://<command hidden>"}),
                client_local: serde_json::json!({"socket_path":"/client/mxr.sock"}),
                remote_target: true,
            },
            OutputFormat::Table,
        )
        .unwrap();
        assert!(rendered.contains("Repair it on the daemon host"));
        assert!(!rendered.contains("doctor --reindex"));
    }
}
