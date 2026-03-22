use crate::cli::{OutputFormat, SearchModeArg, SearchSortArg};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::types::{Envelope, MessageFlags, SortOrder};
use mxr_protocol::{Request, Response, ResponseData, SearchExplain};

pub async fn run(
    query: Option<String>,
    format: Option<OutputFormat>,
    limit: Option<u32>,
    mode: Option<SearchModeArg>,
    sort: Option<SearchSortArg>,
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
            offset: 0,
            mode: mode.map(Into::into),
            sort: Some(sort.map(Into::into).unwrap_or(SortOrder::DateDesc)),
            explain,
        })
        .await?;

    let fmt = resolve_format(format);
    match resp {
        Response::Ok {
            data:
                ResponseData::SearchResults {
                    results,
                    has_more: _,
                    explain: explain_payload,
                },
        } => {
            // Fetch envelopes for each result
            let mut envelopes: Vec<(Envelope, f32)> = Vec::new();
            if !results.is_empty() {
                for r in &results {
                    let env_resp = client
                        .request(mxr_protocol::Request::GetEnvelope {
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
                    if explain {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "results": json_items,
                                "explain": explain_payload,
                            }))?
                        );
                    } else {
                        println!("{}", serde_json::to_string_pretty(&json_items)?);
                    }
                }
                OutputFormat::Ids => {
                    for (env, _) in &envelopes {
                        println!("{}", env.id.as_str());
                    }
                    render_explain(explain_payload.as_ref());
                }
                _ => {
                    if envelopes.is_empty() {
                        println!("No results found.");
                    } else {
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
                    render_explain(explain_payload.as_ref());
                }
            }
        }
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn render_explain(explain: Option<&SearchExplain>) {
    let Some(explain) = explain else {
        return;
    };

    println!("\nExplain");
    println!(
        "requested={} executed={} lexical_candidates={} dense_candidates={} final_results={}",
        explain.requested_mode.as_str(),
        explain.executed_mode.as_str(),
        explain.lexical_candidates,
        explain.dense_candidates,
        explain.final_results,
    );
    if let Some(query) = &explain.semantic_query {
        println!("semantic_query={query}");
    }
    if let Some(window) = explain.dense_window {
        println!(
            "windows: lexical={} dense={}",
            explain.lexical_window, window
        );
    } else {
        println!("windows: lexical={}", explain.lexical_window);
    }
    if let Some(rrf_k) = explain.rrf_k {
        println!("rrf_k={rrf_k}");
    }
    for note in &explain.notes {
        println!("note: {note}");
    }
    for result in explain.results.iter().take(5) {
        println!(
            "#{} {} final={:.4} lexical_rank={:?} dense_rank={:?}",
            result.rank,
            result.message_id.as_str(),
            result.final_score,
            result.lexical_rank,
            result.dense_rank,
        );
    }
}
