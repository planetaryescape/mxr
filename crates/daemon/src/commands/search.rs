use crate::cli::{OutputFormat, SearchModeArg};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::types::{Envelope, MessageFlags};
use mxr_protocol::*;

pub async fn run(
    query: Option<String>,
    format: Option<OutputFormat>,
    limit: Option<u32>,
    mode: Option<SearchModeArg>,
    explain: bool,
) -> anyhow::Result<()> {
    let query = query.unwrap_or_default();
    if query.is_empty() {
        anyhow::bail!("Search query required");
    }
    let limit = limit.unwrap_or(50);
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::Search {
            query,
            limit,
            mode: mode.map(Into::into),
            explain,
        })
        .await?;

    let fmt = resolve_format(format);
    match resp {
        Response::Ok {
            data: ResponseData::SearchResults { results },
        } => {
            if results.is_empty() {
                println!("No results found.");
                return Ok(());
            }

            // Fetch envelopes for each result
            let mut envelopes: Vec<(Envelope, f32)> = Vec::new();
            for r in &results {
                let env_resp = client
                    .request(Request::GetEnvelope {
                        message_id: r.message_id.clone(),
                    })
                    .await?;
                if let Response::Ok {
                    data: ResponseData::Envelope { envelope },
                } = env_resp
                {
                    envelopes.push((envelope, r.score));
                }
            }

            match fmt {
                OutputFormat::Json => {
                    let json_items: Vec<serde_json::Value> = envelopes
                        .iter()
                        .map(|(env, score)| {
                            serde_json::json!({
                                "message_id": env.id.as_str(),
                                "from": format!("{} <{}>",
                                    env.from.name.as_deref().unwrap_or(""),
                                    env.from.email),
                                "subject": env.subject,
                                "date": env.date.to_rfc3339(),
                                "read": env.flags.contains(MessageFlags::READ),
                                "starred": env.flags.contains(MessageFlags::STARRED),
                                "score": score,
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&json_items)?);
                }
                OutputFormat::Ids => {
                    for (env, _) in &envelopes {
                        println!("{}", env.id.as_str());
                    }
                }
                _ => {
                    println!("{:<1} {:<20} {:<45} {:<12}", "", "FROM", "SUBJECT", "DATE");
                    println!("{}", "-".repeat(80));
                    for (env, _score) in &envelopes {
                        let unread = if !env.flags.contains(MessageFlags::READ) {
                            "●"
                        } else {
                            " "
                        };
                        let from = env.from.name.as_deref().unwrap_or(&env.from.email);
                        let from_trunc: String = from.chars().take(20).collect();
                        let subject_trunc: String = env.subject.chars().take(45).collect();
                        let date = env.date.format("%Y-%m-%d").to_string();
                        println!(
                            "{} {:<20} {:<45} {}",
                            unread, from_trunc, subject_trunc, date
                        );
                    }
                    println!("\n{} results", envelopes.len());
                }
            }
        }
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
