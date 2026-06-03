use crate::cli::{OutputFormat, SearchModeArg, TriageSortArg, TriageVerdictArg};
use crate::commands::resolve_optional_account;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::types::{Envelope, SortOrder};
use mxr_protocol::{Request, Response, ResponseData, TriageMessageData, TriageVerdictData};
use std::collections::HashMap;

pub async fn run(
    query: String,
    account: Option<String>,
    format: Option<OutputFormat>,
    limit: Option<u32>,
    mode: Option<SearchModeArg>,
    sort: Option<TriageSortArg>,
    verdict_filter: Option<TriageVerdictArg>,
) -> anyhow::Result<()> {
    let limit = limit.unwrap_or(50);
    if query.trim().is_empty() {
        anyhow::bail!("Triage query required");
    }
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let resp = request_triage(&mut client, query, account_id, limit, mode).await?;
    render_triage(resp, &mut client, format, sort, verdict_filter).await
}

pub(crate) async fn request_triage(
    client: &mut IpcClient,
    query: String,
    account_id: Option<mxr_core::AccountId>,
    limit: u32,
    mode: Option<SearchModeArg>,
) -> anyhow::Result<TriagePayload> {
    let resp = client
        .request(Request::TriageSearch {
            query,
            limit,
            offset: 0,
            account_id,
            mode: mode.map(Into::into),
            sort: Some(SortOrder::DateDesc),
        })
        .await?;
    crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data:
                ResponseData::TriageResults {
                    messages,
                    total,
                    has_more,
                    next_offset,
                    llm_calls,
                    prompt_version,
                },
        } => Some(TriagePayload {
            messages,
            total,
            has_more,
            next_offset,
            llm_calls,
            prompt_version,
        }),
        _ => None,
    })
}

#[derive(Debug)]
pub(crate) struct TriagePayload {
    pub(crate) messages: Vec<TriageMessageData>,
    pub(crate) total: u32,
    pub(crate) has_more: bool,
    pub(crate) next_offset: Option<u32>,
    pub(crate) llm_calls: u32,
    pub(crate) prompt_version: String,
}

pub(crate) async fn render_triage(
    mut payload: TriagePayload,
    client: &mut IpcClient,
    format: Option<OutputFormat>,
    sort: Option<TriageSortArg>,
    verdict_filter: Option<TriageVerdictArg>,
) -> anyhow::Result<()> {
    eprintln!(
        "triage: {} LLM call{} ({} cached, limit {})",
        payload.llm_calls,
        if payload.llm_calls == 1 { "" } else { "s" },
        payload.messages.iter().filter(|m| m.cached).count(),
        payload.messages.len(),
    );

    if let Some(filter) = verdict_filter {
        let verdict = filter.to_data();
        payload
            .messages
            .retain(|message| message.verdict == verdict);
    }
    if matches!(sort, Some(TriageSortArg::Verdict)) {
        payload.messages.sort_by_key(|message| message.verdict);
    }

    let envelopes = fetch_envelopes(client, &payload.messages).await?;
    let fmt = resolve_format(format);
    let rows = payload
        .messages
        .iter()
        .map(|message| row_json(message, envelopes.get(&message.message_id.to_string())))
        .collect::<Vec<_>>();

    match fmt {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "messages": rows,
                    "total": payload.total,
                    "has_more": payload.has_more,
                    "next_offset": payload.next_offset,
                    "llm_calls": payload.llm_calls,
                    "prompt_version": payload.prompt_version,
                }))?
            );
        }
        OutputFormat::Jsonl => println!("{}", jsonl(&rows)?),
        OutputFormat::Ids => {
            for message in &payload.messages {
                println!("{}", message.message_id.as_str());
            }
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "message_id",
                "verdict",
                "reason",
                "from",
                "subject",
                "date",
                "cached",
            ])?;
            for message in &payload.messages {
                let env = envelopes.get(&message.message_id.to_string());
                writer.write_record(vec![
                    message.message_id.to_string(),
                    message.verdict_token.clone(),
                    message.reason.clone(),
                    env.map(format_from).unwrap_or_default(),
                    env.map(|e| e.subject.clone()).unwrap_or_default(),
                    env.map(|e| e.date.to_rfc3339()).unwrap_or_default(),
                    message.cached.to_string(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Table => render_table(&payload.messages, &envelopes),
    }
    Ok(())
}

async fn fetch_envelopes(
    client: &mut IpcClient,
    messages: &[TriageMessageData],
) -> anyhow::Result<HashMap<String, Envelope>> {
    let message_ids = messages
        .iter()
        .map(|message| message.message_id.clone())
        .collect::<Vec<_>>();
    if message_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let resp = client
        .request(Request::ListEnvelopesByIds { message_ids })
        .await?;
    let envelopes = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        } => Some(envelopes),
        _ => None,
    })?;
    Ok(envelopes
        .into_iter()
        .map(|envelope| (envelope.id.to_string(), envelope))
        .collect())
}

fn row_json(message: &TriageMessageData, env: Option<&Envelope>) -> serde_json::Value {
    serde_json::json!({
        "message_id": message.message_id.as_str(),
        "thread_id": message.thread_id.as_str(),
        "verdict": message.verdict_token,
        "verdict_line": message.verdict_line,
        "reason": message.reason,
        "from": env.map(format_from).unwrap_or_default(),
        "subject": env.map(|e| e.subject.clone()).unwrap_or_default(),
        "date": env.map(|e| e.date.to_rfc3339()).unwrap_or_default(),
        "score": message.score,
        "cached": message.cached,
        "model": message.model,
    })
}

fn render_table(messages: &[TriageMessageData], envelopes: &HashMap<String, Envelope>) {
    if messages.is_empty() {
        println!("No triage results found.");
        return;
    }
    println!(
        "{:<8} {:<30} {:<20} {:<42} {:<10}",
        "VERDICT", "REASON", "FROM", "SUBJECT", "DATE"
    );
    println!("{}", "-".repeat(115));
    for message in messages {
        let env = envelopes.get(&message.message_id.to_string());
        let from = env.map(format_from).unwrap_or_default();
        let subject = env.map_or("", |e| e.subject.as_str());
        let date = env
            .map(|e| e.date.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        println!(
            "{:<8} {:<30} {:<20} {:<42} {:<10}",
            message.verdict_token,
            truncate(&message.reason, 30),
            truncate(&from, 20),
            truncate(subject, 42),
            date,
        );
    }
    println!("\n{} results", messages.len());
}

fn format_from(env: &Envelope) -> String {
    match env
        .from
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        Some(name) => format!("{name} <{}>", env.from.email),
        None => env.from.email.clone(),
    }
}

fn truncate(value: &str, max: usize) -> String {
    let mut out = value.chars().take(max).collect::<String>();
    if value.chars().count() > max && max > 1 {
        out.pop();
        out.push('…');
    }
    out
}

impl TriageVerdictArg {
    fn to_data(self) -> TriageVerdictData {
        match self {
            Self::Action => TriageVerdictData::Action,
            Self::Fyi => TriageVerdictData::Fyi,
            Self::Routine => TriageVerdictData::Routine,
        }
    }
}
