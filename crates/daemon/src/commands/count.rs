use crate::cli::{OutputFormat, SearchModeArg};
use crate::commands::expect_response;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(
    query: String,
    mode: Option<SearchModeArg>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::Count {
            query: query.clone(),
            mode: mode.map(Into::into),
        })
        .await?;

    let count = expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Count { count },
        } => Some(count),
        _ => None,
    })?;
    match resolve_format(format) {
        OutputFormat::Json | OutputFormat::Jsonl => {
            println!(
                "{}",
                serde_json::to_string(&serde_json::json!({"query": query, "count": count}))?
            );
        }
        _ => println!("{count}"),
    }
    Ok(())
}
