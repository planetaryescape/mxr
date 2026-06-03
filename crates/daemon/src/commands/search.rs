use crate::cli::{OutputFormat, SearchModeArg, SearchSortArg};
use crate::commands::resolve_optional_account;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::MessageId;
use mxr_core::types::{Envelope, MessageFlags, SortOrder};
use mxr_protocol::{Request, Response, ResponseData, SearchExplain, SearchResultItem};
use std::collections::HashMap;

pub async fn run(
    query: Option<String>,
    account: Option<String>,
    format: Option<OutputFormat>,
    limit: Option<u32>,
    offset: u32,
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
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let resp = client
        .request(Request::Search {
            query,
            limit,
            offset,
            account_id,
            mode: mode.map(Into::into),
            sort: Some(sort.map_or(SortOrder::DateDesc, Into::into)),
            explain,
        })
        .await?;

    let fmt = resolve_format(format);
    let (results, total, has_more, next_offset, explain_payload) =
        crate::commands::expect_response(resp, |r| match r {
            Response::Ok {
                data:
                    ResponseData::SearchResults {
                        results,
                        total,
                        has_more,
                        next_offset,
                        explain: explain_payload,
                    },
            } => Some((results, total, has_more, next_offset, explain_payload)),
            _ => None,
        })?;

    let envelopes = fetch_search_envelopes(&mut client, &results).await?;

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

    match fmt {
        OutputFormat::Json => {
            let payload = serde_json::json!({
                "results": json_items,
                "paging": {
                    "limit": limit,
                    "offset": offset,
                    "total": total,
                    "has_more": has_more,
                    "next_offset": next_offset,
                },
                "explain": if explain { explain_payload } else { None },
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        OutputFormat::Jsonl => {
            println!("{}", jsonl(&json_items)?);
            eprintln!(
                "{}",
                serde_json::to_string(&serde_json::json!({
                    "paging": {
                        "limit": limit,
                        "offset": offset,
                        "total": total,
                        "has_more": has_more,
                        "next_offset": next_offset,
                    },
                    "explain": explain_payload,
                }))?
            );
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "message_id",
                "from",
                "subject",
                "date",
                "read",
                "starred",
                "score",
            ])?;
            for (env, score) in &envelopes {
                writer.write_record(vec![
                    env.id.as_str(),
                    format!(
                        "{} <{}>",
                        env.from.name.as_deref().unwrap_or(""),
                        env.from.email
                    ),
                    env.subject.clone(),
                    env.date.to_rfc3339(),
                    env.flags.contains(MessageFlags::READ).to_string(),
                    env.flags.contains(MessageFlags::STARRED).to_string(),
                    score.to_string(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
            eprintln!(
                "{}",
                serde_json::to_string(&serde_json::json!({
                    "paging": {
                        "limit": limit,
                        "offset": offset,
                        "total": total,
                        "has_more": has_more,
                        "next_offset": next_offset,
                    },
                    "explain": explain_payload,
                }))?
            );
        }
        OutputFormat::Ids => {
            for (env, _) in &envelopes {
                println!("{}", env.id.as_str());
            }
            render_paging_hint(has_more, next_offset, total, envelopes.len(), offset);
            render_explain(explain_payload.as_ref());
        }
        OutputFormat::Table => {
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
                    println!("{unread} {from_trunc:<20} {subject_trunc:<45} {date}");
                }
                print!("\n{} results", envelopes.len());
                if total > envelopes.len() as u32 || offset > 0 {
                    print!(" (total {total}, offset {offset})");
                }
                println!();
                if has_more {
                    if let Some(next_offset) = next_offset {
                        println!("More results available: rerun with --offset {next_offset}");
                    }
                }
            }
            render_explain(explain_payload.as_ref());
        }
    }
    Ok(())
}

async fn fetch_search_envelopes(
    client: &mut IpcClient,
    results: &[SearchResultItem],
) -> anyhow::Result<Vec<(Envelope, f32)>> {
    if results.is_empty() {
        return Ok(Vec::new());
    }

    let message_ids = results
        .iter()
        .map(|result| result.message_id.clone())
        .collect::<Vec<_>>();
    let resp = client
        .request(Request::ListEnvelopesByIds {
            message_ids: message_ids.clone(),
        })
        .await?;
    let envelopes = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        } => Some(envelopes),
        _ => None,
    })?;
    let mut by_id = envelopes
        .into_iter()
        .map(|envelope| (envelope.id.clone(), envelope))
        .collect::<HashMap<MessageId, Envelope>>();

    Ok(results
        .iter()
        .filter_map(|result| {
            by_id
                .remove(&result.message_id)
                .map(|envelope| (envelope, result.score))
        })
        .collect())
}

fn render_paging_hint(
    has_more: bool,
    next_offset: Option<u32>,
    total: u32,
    returned: usize,
    offset: u32,
) {
    if total > returned as u32 || offset > 0 || has_more {
        eprintln!("# search page: returned={returned} total={total} offset={offset}");
    }
    if has_more {
        if let Some(next_offset) = next_offset {
            eprintln!("# more results: rerun with --offset {next_offset}");
        }
    }
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
