use crate::ipc_client::IpcClient;
use mxr_protocol::{Request, Response, ResponseData, IPC_PROTOCOL_VERSION};

fn render_sync_status(sync_statuses: &[mxr_protocol::AccountSyncStatus], protocol_version: u32) {
    if sync_statuses.is_empty() {
        if protocol_version < IPC_PROTOCOL_VERSION {
            println!("Sync status unavailable from legacy daemon");
        } else {
            println!("No sync-capable accounts");
        }
        return;
    }

    for sync in sync_statuses {
        println!("Account: {}", sync.account_name);
        println!(
            "  Healthy: {}  In progress: {}  Failures: {}",
            sync.healthy, sync.sync_in_progress, sync.consecutive_failures
        );
        println!(
            "  Last success: {}",
            sync.last_success_at.as_deref().unwrap_or("never")
        );
        println!(
            "  Last attempt: {}",
            sync.last_attempt_at.as_deref().unwrap_or("never")
        );
        println!(
            "  Last error: {}",
            sync.last_error.as_deref().unwrap_or("-")
        );
        println!(
            "  Backoff until: {}",
            sync.backoff_until.as_deref().unwrap_or("-")
        );
        println!(
            "  Cursor: {}",
            sync.current_cursor_summary.as_deref().unwrap_or("-")
        );
        println!("  Last synced count: {}", sync.last_synced_count);
    }
}

pub async fn run(_account: Option<String>, status: bool, _history: bool) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;

    if status {
        let resp = client.request(Request::GetStatus).await?;
        match resp {
            Response::Ok {
                data:
                    ResponseData::Status {
                        sync_statuses,
                        protocol_version,
                        ..
                    },
            } => {
                render_sync_status(&sync_statuses, protocol_version);
                if protocol_version < IPC_PROTOCOL_VERSION {
                    println!(
                        "\nNote: daemon protocol {} is older than client protocol {}. Restart the daemon after upgrading.",
                        protocol_version, IPC_PROTOCOL_VERSION
                    );
                }
            }
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        }
    } else {
        let resp = client
            .request(Request::SyncNow { account_id: None })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => {
                println!("Sync triggered");
            }
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    Ok(())
}
