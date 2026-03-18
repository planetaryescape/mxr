use crate::ipc_client::IpcClient;
use mxr_protocol::*;

pub async fn run() -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::ListLabels { account_id: None })
        .await?;

    match resp {
        Response::Ok {
            data: ResponseData::Labels { labels },
        } => {
            if labels.is_empty() {
                println!("No labels");
            } else {
                for l in &labels {
                    println!("  {} ({}/{} unread)", l.name, l.unread_count, l.total_count);
                }
            }
        }
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
