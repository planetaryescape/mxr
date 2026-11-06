use crate::{cli::OutputFormat, ipc_client::IpcClient};
use mxr_protocol::{Request, Response};

pub async fn inline_image_bytes(
    message_id: String,
    source: String,
    offset: u64,
    length: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let format = format.unwrap_or(OutputFormat::Json);
    if !matches!(format, OutputFormat::Json | OutputFormat::Jsonl) {
        anyhow::bail!("Inline image bytes support --format json or jsonl");
    }
    let mut client = IpcClient::connect().await?;
    match client
        .request(Request::ReadInlineImage {
            message_id: message_id.parse()?,
            source,
            offset,
            length,
        })
        .await?
    {
        Response::Ok { data } => crate::output::print_json(&data, format)?,
        Response::Error { message, kind, .. } => {
            return Err(super::super::daemon_response_error(message, kind))
        }
    }
    Ok(())
}
