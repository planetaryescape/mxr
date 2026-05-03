#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::cli::{LabelsAction, OutputFormat};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::types::Label;
use mxr_protocol::*;

fn render_labels(labels: &[Label], format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => serde_json::to_string_pretty(labels)?,
        OutputFormat::Jsonl => jsonl(labels)?,
        OutputFormat::Csv => {
            let mut out = String::from("name,kind,unread_count,total_count\n");
            for label in labels {
                out.push_str(&format!(
                    "{},{},{},{}\n",
                    label.name, label.provider_id, label.unread_count, label.total_count
                ));
            }
            out
        }
        OutputFormat::Ids => labels
            .iter()
            .map(|label| label.name.clone())
            .collect::<Vec<_>>()
            .join("\n"),
        OutputFormat::Table => {
            if labels.is_empty() {
                "No labels".to_string()
            } else {
                let mut out = format!(
                    "{:<24} {:<10} {:>8} {:>8}\n",
                    "NAME", "KIND", "UNREAD", "TOTAL"
                );
                out.push_str(&format!("{}\n", "-".repeat(56)));
                for label in labels {
                    let kind = match label.kind {
                        mxr_core::types::LabelKind::System => "system",
                        mxr_core::types::LabelKind::Folder => "folder",
                        mxr_core::types::LabelKind::User => "user",
                    };
                    out.push_str(&format!(
                        "{:<24} {:<10} {:>8} {:>8}\n",
                        label.name, kind, label.unread_count, label.total_count
                    ));
                }
                out.trim_end().to_string()
            }
        }
    })
}

pub async fn run(action: Option<LabelsAction>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;

    match action {
        None => {
            let resp = client
                .request(Request::ListLabels { account_id: None })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Labels { labels },
                } => {
                    println!("{}", render_labels(&labels, resolve_format(format))?);
                }
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        Some(LabelsAction::Create {
            name,
            color,
            dry_run,
        }) => {
            if dry_run {
                println!(
                    "Would create label \"{name}\"{}",
                    color
                        .as_deref()
                        .map(|c| format!(" with color {c}"))
                        .unwrap_or_default()
                );
                return Ok(());
            }
            let resp = client
                .request(Request::CreateLabel {
                    name,
                    color,
                    account_id: None,
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Label { label },
                } => {
                    println!("{}", label.name);
                }
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        Some(LabelsAction::Delete { name, dry_run, yes }) => {
            if dry_run {
                println!("Would delete label \"{name}\"");
                return Ok(());
            }
            if !yes && !confirm_destructive(&format!(
                "Delete label \"{name}\"? Messages keep their other labels. [y/N] "
            ))? {
                println!("Aborted.");
                return Ok(());
            }
            let resp = client
                .request(Request::DeleteLabel {
                    name,
                    account_id: None,
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Ack,
                } => println!("Deleted"),
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        Some(LabelsAction::Rename {
            old,
            new,
            dry_run,
            yes,
        }) => {
            if dry_run {
                println!("Would rename label \"{old}\" -> \"{new}\"");
                return Ok(());
            }
            if !yes && !confirm_destructive(&format!(
                "Rename label \"{old}\" -> \"{new}\"? [y/N] "
            ))? {
                println!("Aborted.");
                return Ok(());
            }
            let resp = client
                .request(Request::RenameLabel {
                    old,
                    new,
                    account_id: None,
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Label { label },
                } => println!("{}", label.name),
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
    }

    Ok(())
}

fn confirm_destructive(prompt: &str) -> anyhow::Result<bool> {
    use std::io::{self, IsTerminal, Write};
    if !io::stdin().is_terminal() {
        // Refuse to silently proceed when there is no human to confirm.
        anyhow::bail!(
            "destructive action requires confirmation; pass --yes to proceed non-interactively"
        );
    }
    print!("{prompt}");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let trimmed = answer.trim().to_ascii_lowercase();
    Ok(trimmed == "y" || trimmed == "yes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::{AccountId, LabelId};

    fn sample_labels() -> Vec<Label> {
        vec![
            Label {
                id: LabelId::new(),
                account_id: AccountId::new(),
                name: "Inbox".to_string(),
                kind: mxr_core::types::LabelKind::System,
                color: None,
                provider_id: "INBOX".to_string(),
                unread_count: 3,
                total_count: 12,
            },
            Label {
                id: LabelId::new(),
                account_id: AccountId::new(),
                name: "Projects".to_string(),
                kind: mxr_core::types::LabelKind::User,
                color: Some("#ff6600".to_string()),
                provider_id: "Projects".to_string(),
                unread_count: 1,
                total_count: 4,
            },
        ]
    }

    #[test]
    fn render_labels_json_includes_name() {
        let rendered = render_labels(&sample_labels(), OutputFormat::Json).unwrap();
        assert!(rendered.contains("\"name\": \"Inbox\""));
    }

    #[test]
    fn render_labels_table_includes_counts() {
        let rendered = render_labels(&sample_labels(), OutputFormat::Table).unwrap();
        assert!(rendered.contains("Inbox"));
        assert!(rendered.contains("12"));
    }
}
