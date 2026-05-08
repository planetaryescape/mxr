//! `mxr snippets` — manage compose snippets.

use crate::cli::{OutputFormat, SnippetsAction};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_protocol::*;

pub async fn run(
    action: Option<SnippetsAction>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let action = action.unwrap_or(SnippetsAction::List);
    let mut client = IpcClient::connect().await?;

    match action {
        SnippetsAction::List => {
            let resp = client.request(Request::ListSnippets).await?;
            let fmt = resolve_format(format);
            match resp {
                Response::Ok {
                    data: ResponseData::Snippets { snippets },
                } => match fmt {
                    OutputFormat::Json => {
                        println!("{}", serde_json::to_string_pretty(&snippets)?);
                    }
                    OutputFormat::Jsonl => {
                        println!("{}", jsonl(&snippets)?);
                    }
                    _ => {
                        if snippets.is_empty() {
                            println!("No snippets defined");
                        } else {
                            for s in &snippets {
                                let preview: String = s.body.chars().take(60).collect();
                                let suffix = if s.body.chars().count() > 60 {
                                    "…"
                                } else {
                                    ""
                                };
                                println!("  ;{}: {preview}{suffix}", s.name);
                            }
                        }
                    }
                },
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        SnippetsAction::Set { name, body, vars } => {
            let vars: Vec<String> = vars
                .map(|s| {
                    s.split(',')
                        .map(|v| v.trim().to_string())
                        .filter(|v| !v.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let resp = client
                .request(Request::SetSnippet {
                    name: name.clone(),
                    body,
                    vars,
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::SnippetData { snippet },
                } => {
                    println!("Saved ;{}", snippet.name);
                }
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        SnippetsAction::Remove { name } => {
            let resp = client
                .request(Request::DeleteSnippet { name: name.clone() })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Ack,
                } => {
                    println!("Deleted ;{name}");
                }
                Response::Error { message, .. } => anyhow::bail!("{message}"),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
    }

    Ok(())
}
