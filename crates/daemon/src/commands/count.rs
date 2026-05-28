use crate::cli::{OutputFormat, SearchModeArg};
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(
    query: String,
    account: Option<String>,
    mode: Option<SearchModeArg>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let resp = client
        .request(Request::Count {
            query: query.clone(),
            account_id,
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
