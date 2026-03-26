use crate::cli::SearchModeArg;
use crate::commands::expect_response;
use crate::ipc_client::IpcClient;
use crate::mxr_protocol::*;

pub async fn run(query: String, mode: Option<SearchModeArg>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::Count {
            query,
            mode: mode.map(Into::into),
        })
        .await?;

    let count = expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Count { count },
        } => Some(count),
        _ => None,
    })?;
    println!("{count}");
    Ok(())
}
