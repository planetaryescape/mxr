use crate::cli::OutputFormat;
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;
use std::str::FromStr;

pub async fn run(
    message_id: Option<String>,
    query: Option<String>,
    include_self: bool,
    account: Option<String>,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await?;

    let resolved_query = match (message_id, query) {
        (Some(_), Some(_)) => {
            // clap's conflicts_with prevents this in practice; defensive.
            anyhow::bail!("--query and a positional message id are mutually exclusive");
        }
        (None, Some(q)) => q,
        (Some(id_str), None) => {
            let id = mxr_core::MessageId::from_str(&id_str)
                .map_err(|e| anyhow::anyhow!("invalid message id: {e}"))?;
            // Pull the body to use as the query corpus.
            let resp = client
                .request(Request::GetBody { message_id: id })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Body { body },
                } => {
                    let q = body
                        .text_plain
                        .clone()
                        .unwrap_or_else(|| body.text_html.clone().unwrap_or_default());
                    if q.trim().is_empty() {
                        anyhow::bail!("message body is empty; use --query instead");
                    }
                    q
                }
                Response::Error { message, .. } => anyhow::bail!(message),
                _ => anyhow::bail!("unexpected response loading message body"),
            }
        }
        (None, None) => {
            anyhow::bail!("either a positional message id or --query is required");
        }
    };

    let resp = client
        .request(Request::FindExpert {
            account_id,
            query: resolved_query,
            include_self,
            limit,
        })
        .await?;
    print(resp, resolve_format(format))
}

fn print(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::ExpertSuggestions { experts },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&experts)?),
            OutputFormat::Jsonl => {
                for e in experts {
                    println!("{}", serde_json::to_string(&e)?);
                }
            }
            OutputFormat::Ids => {
                for e in experts {
                    println!("{}", e.email);
                }
            }
            _ => {
                if experts.is_empty() {
                    println!("(no experts found in archive for this query)");
                }
                for e in experts {
                    println!(
                        "  {} ({} answer thread(s))",
                        e.display_name.as_deref().unwrap_or(e.email.as_str()),
                        e.answered_thread_count
                    );
                    println!("    {}", e.reason);
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
