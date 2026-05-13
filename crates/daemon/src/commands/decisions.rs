use crate::cli::OutputFormat;
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(
    account: Option<String>,
    topic: Option<String>,
    since_days: Option<u32>,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await?;
    let resp = client
        .request(Request::ListDecisionLog {
            account_id,
            topic,
            since_days,
            limit,
        })
        .await?;
    print(resp, resolve_format(format))
}

fn print(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::DecisionLog { decisions },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&decisions)?),
            OutputFormat::Jsonl => {
                for d in decisions {
                    println!("{}", serde_json::to_string(&d)?);
                }
            }
            OutputFormat::Ids => {
                for d in decisions {
                    println!("{}", d.id);
                }
            }
            _ => {
                for d in decisions {
                    let topic = d.topic.as_deref().unwrap_or("(no topic)");
                    let when = d
                        .decided_at
                        .unwrap_or(d.extracted_at)
                        .format("%Y-%m-%d");
                    println!("[{when}] [{topic}] {}", d.decision);
                    if !d.evidence_msg_ids.is_empty() {
                        println!(
                            "    cite: {}",
                            d.evidence_msg_ids
                                .iter()
                                .map(|m| m.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
