use crate::ipc_client::IpcClient;
use mxr_protocol::*;

pub async fn run(query: String) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::Count { query }).await?;

    match resp {
        Response::Ok {
            data: ResponseData::Count { count },
        } => {
            println!("{}", count);
        }
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
