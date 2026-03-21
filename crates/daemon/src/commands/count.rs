use crate::cli::SearchModeArg;
use crate::ipc_client::IpcClient;
use mxr_protocol::*;

pub async fn run(query: String, mode: Option<SearchModeArg>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::Count {
            query,
            mode: mode.map(Into::into),
        })
        .await?;

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
