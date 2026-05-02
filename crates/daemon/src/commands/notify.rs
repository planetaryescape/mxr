#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::types::Label;
use mxr_protocol::{Request, Response, ResponseData};

pub fn inbox_unread_count(labels: &[Label]) -> u32 {
    labels
        .iter()
        .find(|label| label.name == "INBOX")
        .map(|label| label.unread_count)
        .unwrap_or(0)
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
            .request(Request::ListLabels { account_id: None })
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
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        }

        if !watch {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::types::{Label, LabelKind};
    use mxr_core::{AccountId, LabelId};

    #[test]
    fn inbox_unread_uses_inbox_label() {
        let labels = vec![
            Label {
                id: LabelId::new(),
                account_id: AccountId::new(),
                name: "INBOX".into(),
                kind: LabelKind::System,
                color: None,
                provider_id: "INBOX".into(),
                unread_count: 7,
                total_count: 10,
            },
            Label {
                id: LabelId::new(),
                account_id: AccountId::new(),
                name: "STARRED".into(),
                kind: LabelKind::System,
                color: None,
                provider_id: "STARRED".into(),
                unread_count: 99,
                total_count: 99,
            },
        ];

        assert_eq!(inbox_unread_count(&labels), 7);
    }

    #[test]
    fn render_notify_json_contains_unread_field() {
        let rendered = render_notify(3, OutputFormat::Json).unwrap();
        assert!(rendered.contains("\"unread\": 3"));
    }
}
