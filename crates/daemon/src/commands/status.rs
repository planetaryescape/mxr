use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::{AccountSyncStatus, Request, Response, ResponseData};

fn render_status(
    uptime_secs: u64,
    accounts: &[String],
    total_messages: u32,
    daemon_pid: Option<u32>,
    sync_statuses: &[AccountSyncStatus],
    format: OutputFormat,
) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => serde_json::to_string_pretty(&serde_json::json!({
            "uptime_secs": uptime_secs,
            "accounts": accounts,
            "total_messages": total_messages,
            "daemon_pid": daemon_pid,
            "sync_statuses": sync_statuses,
        }))?,
        _ => {
            let mut lines = vec![
                format!("Uptime: {uptime_secs}s"),
                format!(
                    "Daemon PID: {}",
                    daemon_pid
                        .map(|pid| pid.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ),
                format!("Accounts: {}", accounts.join(", ")),
                format!("Total messages: {total_messages}"),
                "Sync:".to_string(),
            ];
            if sync_statuses.is_empty() {
                lines.push("  no accounts".to_string());
            } else {
                for sync in sync_statuses {
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
                    },
            } => {
                println!(
                    "{}",
                    render_status(
                        uptime_secs,
                        &accounts,
                        total_messages,
                        daemon_pid,
                        &sync_statuses,
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
            42,
            &["main".into()],
            10,
            Some(999),
            &[AccountSyncStatus {
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
            OutputFormat::Json,
        )
        .unwrap();
        assert!(rendered.contains("\"uptime_secs\": 42"));
        assert!(rendered.contains("\"daemon_pid\": 999"));
        assert!(rendered.contains("\"total_messages\": 10"));
    }
}
