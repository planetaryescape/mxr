use crate::cli::{OutputFormat, SearchGroupByArg, SearchModeArg, SearchSortArg};
use crate::commands::resolve_optional_account;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::types::{Envelope, MessageFlags, SortOrder};
use mxr_protocol::{
    Request, Response, ResponseData, SearchAggregationGroupBy, SearchAggregationRow, SearchExplain,
};

pub async fn run(
    query: Option<String>,
    account: Option<String>,
    format: Option<OutputFormat>,
    limit: Option<u32>,
    mode: Option<SearchModeArg>,
    sort: Option<SearchSortArg>,
    group_by: Option<SearchGroupByArg>,
    explain: bool,
) -> anyhow::Result<()> {
    let query = query.unwrap_or_default();
    if query.is_empty() {
        anyhow::bail!("Search query required");
    }
    let limit = limit.unwrap_or(50);
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    if let Some(group_by) = group_by {
        let resp = client
            .request(Request::SearchAggregation {
                query: query.clone(),
                account_id,
                mode: mode.map(Into::into),
                group_by: group_by.into(),
                limit: Some(limit),
            })
            .await?;
        let (group_by, total, groups) = crate::commands::expect_response(resp, |r| match r {
            Response::Ok {
                data:
                    ResponseData::SearchAggregation {
                        group_by,
                        total,
                        groups,
                        ..
                    },
            } => Some((group_by, total, groups)),
            _ => None,
        })?;
        render_aggregation(resolve_format(format), &query, group_by, total, &groups)?;
        return Ok(());
    }
    let resp = client
        .request(Request::Search {
            query,
            limit,
            offset: 0,
            account_id,
            mode: mode.map(Into::into),
            sort: Some(sort.map_or(SortOrder::DateDesc, Into::into)),
            explain,
        })
        .await?;

    let fmt = resolve_format(format);
    let (results, explain_payload) = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data:
                ResponseData::SearchResults {
                    results,
                    has_more: _,
                    explain: explain_payload,
                    ..
                },
        } => Some((results, explain_payload)),
        _ => None,
    })?;

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
        OutputFormat::Jsonl => {
            println!("{}", jsonl(&json_items)?);
            if let Some(explain_payload) = explain_payload.as_ref() {
                eprintln!("{}", serde_json::to_string(explain_payload)?);
            }
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
            if let Some(explain_payload) = explain_payload.as_ref() {
                eprintln!("{}", serde_json::to_string(explain_payload)?);
            }
        }
        OutputFormat::Ids => {
            for (env, _) in &envelopes {
                println!("{}", env.id.as_str());
            }
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
                println!("\n{} results", envelopes.len());
            }
            render_explain(explain_payload.as_ref());
        }
    }
    Ok(())
}

pub(crate) fn render_aggregation(
    fmt: OutputFormat,
    query: &str,
    group_by: SearchAggregationGroupBy,
    total: u32,
    groups: &[SearchAggregationRow],
) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "query": query,
                    "group_by": group_by.as_str(),
                    "total": total,
                    "groups": groups,
                }))?
            );
        }
        OutputFormat::Jsonl => {
            for group in groups {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "query": query,
                        "group_by": group_by.as_str(),
                        "key": group.key,
                        "label": group.label,
                        "count": group.count,
                        "unread": group.unread,
                        "oldest": group.oldest,
                        "newest": group.newest,
                    }))?
                );
            }
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "group_by", "key", "label", "count", "unread", "oldest", "newest",
            ])?;
            for group in groups {
                writer.write_record(vec![
                    group_by.as_str().to_string(),
                    group.key.clone(),
                    group.label.clone(),
                    group.count.to_string(),
                    group.unread.to_string(),
                    group
                        .oldest
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    group
                        .newest
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for group in groups {
                println!("{}", group.key);
            }
        }
        OutputFormat::Table => {
            if groups.is_empty() {
                println!("No groups found for {query:?}.");
            } else {
                println!(
                    "{:<32} {:>8} {:>8} {:<10} {:<10}",
                    group_by.as_str().to_ascii_uppercase(),
                    "COUNT",
                    "UNREAD",
                    "OLDEST",
                    "NEWEST"
                );
                println!("{}", "-".repeat(76));
                for group in groups {
                    let label: String = group.label.chars().take(32).collect();
                    println!(
                        "{label:<32} {count:>8} {unread:>8} {oldest:<10} {newest:<10}",
                        count = group.count,
                        unread = group.unread,
                        oldest = format_day(group.oldest),
                        newest = format_day(group.newest),
                    );
                }
                println!("\n{} messages across {} groups", total, groups.len());
            }
        }
    }
    Ok(())
}

fn format_day(ts: Option<i64>) -> String {
    ts.and_then(|value| chrono::DateTime::from_timestamp(value, 0))
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "-".into())
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
