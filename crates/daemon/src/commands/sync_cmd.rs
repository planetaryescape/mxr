use crate::ipc_client::IpcClient;
use mxr_protocol::*;

pub async fn run(_account: Option<String>, status: bool, _history: bool) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;

    if status {
        let resp = client.request(Request::GetStatus).await?;
        match resp {
            Response::Ok {
                data:
                    ResponseData::Status {
                        uptime_secs,
                        accounts,
                        total_messages,
                    },
            } => {
                println!("Uptime: {}s", uptime_secs);
                println!("Accounts: {}", accounts.join(", "));
                println!("Total messages: {}", total_messages);
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
