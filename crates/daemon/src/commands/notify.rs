use crate::cli::OutputFormat;
use crate::commands::daemon_response_error;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::types::Label;
use mxr_protocol::{Request, Response, ResponseData};

pub fn inbox_unread_count(labels: &[Label]) -> u32 {
    labels
        .iter()
        .find(|label| label.name == "INBOX")
        .map_or(0, |label| label.unread_count)
}

pub fn render_notify(unread: u32, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => serde_json::to_string_pretty(&serde_json::json!({
            "unread": unread,
        }))?,
        OutputFormat::Jsonl => serde_json::to_string(&serde_json::json!({
            "unread": unread,
        }))?,
        _ => unread.to_string(),
    })
}

pub async fn run(format: Option<OutputFormat>, watch: bool) -> anyhow::Result<()> {
    let fmt = resolve_format(format);

    loop {
        let mut client = IpcClient::connect().await?;
        let resp = client
            .request(Request::ListLabels {
                account_id: None,
                all_accounts: false,
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Labels { labels },
            } => {
                println!(
                    "{}",
                    render_notify(inbox_unread_count(&labels), fmt.clone())?
                );
            }
            Response::Error { message, kind, .. } => {
                return Err(daemon_response_error(message, kind))
            }
            _ => anyhow::bail!("Unexpected response"),
        }

        if !watch {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    Ok(())
}
