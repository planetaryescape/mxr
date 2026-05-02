use crate::cli::OutputFormat;
use crate::commands::expect_response;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::MessageId;
use mxr_protocol::*;

pub async fn run(message_id: String, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mid = MessageId::from_uuid(uuid::Uuid::parse_str(&message_id)?);
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::GetHeaders { message_id: mid })
        .await?;

    let headers = expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Headers { headers },
        } => Some(headers),
        _ => None,
    })?;
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&headers)?),
        OutputFormat::Jsonl => {
            let rows = headers
                .iter()
                .map(|(name, value)| {
                    serde_json::json!({
                        "name": name,
                        "value": value,
                    })
                })
                .collect::<Vec<_>>();
            println!("{}", jsonl(&rows)?);
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["name", "value"])?;
            for (name, value) in &headers {
                writer.write_record([name, value])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for (key, _) in &headers {
                println!("{key}");
            }
        }
        OutputFormat::Table => {
            for (key, value) in &headers {
                println!("{key}: {value}");
            }
        }
    }
    Ok(())
}
