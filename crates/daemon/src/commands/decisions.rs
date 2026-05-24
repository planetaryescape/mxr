use crate::cli::{DecisionsAction, OutputFormat};
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(
    action: Option<DecisionsAction>,
    account: Option<String>,
    topic: Option<String>,
    since_days: Option<u32>,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    match action {
        Some(DecisionsAction::Rebuild {
            account: rebuild_account,
            since_days,
            format,
        }) => {
            let account_id = resolve_account(&mut client, rebuild_account.as_deref()).await?;
            let resp = client
                .request(Request::RebuildDecisionLog {
                    account_id,
                    since_days,
                })
                .await?;
            print_rebuild(resp, resolve_format(format))
        }
        Some(DecisionsAction::Show { id, format }) => {
            let resp = client.request(Request::GetDecision { id }).await?;
            print_detail(resp, resolve_format(format))
        }
        None => {
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
    }
}

fn print_rebuild(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data:
                ResponseData::DecisionLogRebuildSummary {
                    extracted,
                    skipped,
                    errors,
                },
        } => match fmt {
            OutputFormat::Json | OutputFormat::Jsonl => {
                let payload = serde_json::json!({
                    "extracted": extracted,
                    "skipped": skipped,
                    "errors": errors,
                });
                println!("{}", serde_json::to_string(&payload)?);
            }
            _ => {
                println!(
                    "decisions rebuild: extracted={extracted} skipped={skipped} errors={errors}"
                );
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

/// Print a single decision row (or "not found" + nonzero exit).
fn print_detail(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::DecisionDetail { decision },
        } => match decision {
            None => anyhow::bail!("decision not found"),
            Some(d) => match fmt {
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&d)?),
                OutputFormat::Jsonl => println!("{}", serde_json::to_string(&d)?),
                OutputFormat::Ids => println!("{}", d.id),
                _ => {
                    let topic = d.topic.as_deref().unwrap_or("(no topic)");
                    let when = d.decided_at.unwrap_or(d.extracted_at).format("%Y-%m-%d");
                    println!("{} [{when}] [{topic}]", d.id);
                    println!("{}", d.decision);
                    if let Some(r) = d.rationale.as_deref() {
                        println!("Rationale: {r}");
                    }
                    if !d.evidence_msg_ids.is_empty() {
                        println!(
                            "Citations: {}",
                            d.evidence_msg_ids
                                .iter()
                                .map(std::string::ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                }
            },
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
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
                    let when = d.decided_at.unwrap_or(d.extracted_at).format("%Y-%m-%d");
                    println!("[{when}] [{topic}] {}", d.decision);
                    if !d.evidence_msg_ids.is_empty() {
                        println!(
                            "    cite: {}",
                            d.evidence_msg_ids
                                .iter()
                                .map(std::string::ToString::to_string)
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
