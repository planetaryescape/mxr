use crate::ipc_client::IpcClient;
use mxr_protocol::{Request, Response, ResponseData};

fn render_sync_status(sync_statuses: &[mxr_protocol::AccountSyncStatus]) {
    if sync_statuses.is_empty() {
        println!("No sync-capable accounts");
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
                data: ResponseData::Status { sync_statuses, .. },
            } => render_sync_status(&sync_statuses),
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
