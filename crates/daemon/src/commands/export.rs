#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::ipc_client::IpcClient;
use mxr_core::{ExportFormat, ThreadId};
use mxr_protocol::*;
use std::path::PathBuf;

fn parse_export_format(value: &str) -> anyhow::Result<ExportFormat> {
    match value.to_ascii_lowercase().as_str() {
        "markdown" | "md" => Ok(ExportFormat::Markdown),
        "json" => Ok(ExportFormat::Json),
        "mbox" => Ok(ExportFormat::Mbox),
        "llm" | "llm-context" => Ok(ExportFormat::LlmContext),
        other => anyhow::bail!("Unsupported export format: {other}"),
    }
}

fn parse_thread_id(value: &str) -> anyhow::Result<ThreadId> {
    let uuid = uuid::Uuid::parse_str(value)
        .map_err(|e| anyhow::anyhow!("Invalid thread ID '{}': {}", value, e))?;
    Ok(ThreadId::from_uuid(uuid))
}

fn emit(content: String, output: Option<PathBuf>) -> anyhow::Result<()> {
    if let Some(path) = output {
        std::fs::write(path, content)?;
    } else {
        println!("{content}");
    }
    Ok(())
}

pub async fn run(
    thread_id: Option<String>,
    search: Option<String>,
    format: String,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    let format = parse_export_format(&format)?;
    let mut client = IpcClient::connect().await?;

    let response = match (thread_id, search) {
        (Some(thread_id), None) => {
            client
                .request(Request::ExportThread {
                    thread_id: parse_thread_id(&thread_id)?,
                    format,
                })
                .await?
        }
        (None, Some(query)) => {
            client
                .request(Request::ExportSearch { query, format })
                .await?
        }
        (Some(_), Some(_)) => anyhow::bail!("Choose either THREAD_ID or --search, not both"),
        (None, None) => anyhow::bail!("Provide THREAD_ID or --search"),
    };

    let content = crate::commands::expect_response(response, |r| match r {
        Response::Ok {
            data: ResponseData::ExportResult { content },
        } => Some(content),
        _ => None,
    })?;
    emit(content, output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_llm_export_alias() {
        assert_eq!(
            parse_export_format("llm").unwrap(),
            ExportFormat::LlmContext
        );
    }

    #[test]
    fn rejects_unknown_export_format() {
        assert!(parse_export_format("yaml").is_err());
    }
}
