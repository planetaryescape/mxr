use crate::cli::OutputFormat;
use crate::commands::selection::{resolve_message_ids, SelectionLimit};
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::MessageId;
use mxr_protocol::*;

pub async fn run(
    message_id: Option<String>,
    search: Option<String>,
    account: Option<String>,
    first: bool,
    limit: Option<u32>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let ids = resolve_message_ids(
        &mut client,
        message_id.into_iter().collect(),
        search,
        account_id.as_ref(),
        SelectionLimit::from_flags(first, limit),
    )
    .await?;
    if ids.is_empty() {
        anyhow::bail!("No messages matched");
    }

    let fmt = resolve_format(format);
    match fmt {
        OutputFormat::Json => {
            let mut payloads = Vec::with_capacity(ids.len());
            for id in &ids {
                let headers = fetch_headers(&mut client, id.clone()).await?;
                payloads.push(serde_json::json!({
                    "message_id": id.as_str(),
                    "headers": headers
                        .into_iter()
                        .map(|(k, v)| serde_json::json!({"name": k, "value": v}))
                        .collect::<Vec<_>>(),
                }));
            }
            if ids.len() == 1 {
                println!("{}", serde_json::to_string_pretty(&payloads[0])?);
            } else {
                println!("{}", serde_json::to_string_pretty(&payloads)?);
            }
        }
        OutputFormat::Jsonl => {
            for id in &ids {
                let headers = fetch_headers(&mut client, id.clone()).await?;
                let rows: Vec<_> = headers
                    .iter()
                    .map(|(name, value)| {
                        serde_json::json!({
                            "message_id": id.as_str(),
                            "name": name,
                            "value": value,
                        })
                    })
                    .collect();
                println!("{}", jsonl(&rows)?);
            }
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["message_id", "name", "value"])?;
            for id in &ids {
                let headers = fetch_headers(&mut client, id.clone()).await?;
                for (name, value) in &headers {
                    writer.write_record(&[id.as_str().clone(), name.clone(), value.clone()])?;
                }
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for id in &ids {
                println!("{id}");
            }
        }
        OutputFormat::Table => {
            for (index, id) in ids.iter().enumerate() {
                if ids.len() > 1 {
                    if index > 0 {
                        println!();
                    }
                    println!("--- {} ---", id.as_str());
                }
                let headers = fetch_headers(&mut client, id.clone()).await?;
                for (key, value) in &headers {
                    println!("{key}: {value}");
                }
            }
        }
    }
    Ok(())
}

async fn fetch_headers(
    client: &mut IpcClient,
    id: MessageId,
) -> anyhow::Result<Vec<(String, String)>> {
    let resp = client
        .request(Request::GetHeaders { message_id: id })
        .await?;
    expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Headers { headers },
        } => Some(headers),
        _ => None,
    })
}
