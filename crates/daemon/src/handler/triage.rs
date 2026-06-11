use super::{runtime, summarize, HandlerResult};
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::types::{Address, Envelope, SearchMode, SortOrder};
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature};
use mxr_protocol::{ResponseData, SearchResultItem, TriageMessageData, TriageVerdictData};
use mxr_store::TriageCacheRecord;
use sha2::{Digest, Sha256};

pub(crate) const TRIAGE_PROMPT_VERSION: &str = "summarize-v3:triage-v1";

pub(crate) async fn triage_search(
    state: &AppState,
    query: &str,
    limit: u32,
    offset: u32,
    account_id: Option<&AccountId>,
    mode: SearchMode,
    sort: SortOrder,
) -> HandlerResult {
    let search =
        runtime::search(state, query, limit, offset, account_id, mode, sort, false).await?;
    let ResponseData::SearchResults {
        results,
        total,
        has_more,
        next_offset,
        ..
    } = search
    else {
        return Err("unexpected search response".into());
    };

    let mut slots: Vec<Option<TriageMessageData>> = vec![None; results.len()];
    let mut misses = Vec::new();

    for (index, result) in results.into_iter().enumerate() {
        let Some(envelope) = state.store.get_envelope(&result.message_id).await? else {
            continue;
        };
        let body_text = load_body_text(state, &envelope).await;
        let content_hash = triage_content_hash(&envelope, &body_text);
        if let Some(cached) = state
            .store
            .get_triage_cache(&envelope.id, TRIAGE_PROMPT_VERSION)
            .await?
            .filter(|record| record.content_hash == content_hash)
        {
            slots[index] = Some(cache_to_data(cached, result.score, true)?);
        } else {
            misses.push(TriageMiss {
                index,
                result,
                envelope,
                body_text,
                content_hash,
            });
        }
    }

    let llm_calls = misses.len() as u32;
    for miss in misses {
        let data = generate_triage(state, &miss).await?;
        slots[miss.index] = Some(data);
    }

    Ok(ResponseData::TriageResults {
        messages: slots.into_iter().flatten().collect(),
        total,
        has_more,
        next_offset,
        llm_calls,
        prompt_version: TRIAGE_PROMPT_VERSION.to_string(),
    })
}

struct TriageMiss {
    index: usize,
    result: SearchResultItem,
    envelope: Envelope,
    body_text: String,
    content_hash: String,
}

async fn generate_triage(state: &AppState, miss: &TriageMiss) -> Result<TriageMessageData, String> {
    let prompt = build_triage_prompt(state, &miss.envelope, &miss.body_text).await;
    let request = CompletionRequest {
        messages: vec![
            ChatMessage::system(summarize::SYSTEM_PROMPT),
            ChatMessage::user(prompt),
        ],
        max_tokens: Some(220),
        temperature: Some(0.0),
    };
    let response = match state
        .llm
        .for_feature(LlmFeature::Summarize)
        .complete(request)
        .await
    {
        Ok(response) => response,
        Err(LlmError::Disabled) => {
            return Err(
                "LLM is disabled. Enable it in [llm] in your config and configure a model \
                 (Ollama / LM Studio / OpenAI). See `mxr config`."
                    .to_string(),
            );
        }
        Err(error) => return Err(format!("LLM error: {error}")),
    };

    let parsed = parse_verdict_line(&response.content)?;
    let generated_at = chrono::Utc::now();
    let record = TriageCacheRecord {
        message_id: miss.envelope.id.clone(),
        account_id: miss.envelope.account_id.clone(),
        thread_id: miss.envelope.thread_id.clone(),
        prompt_version: TRIAGE_PROMPT_VERSION.to_string(),
        content_hash: miss.content_hash.clone(),
        verdict: parsed.verdict.token().to_string(),
        verdict_line: parsed.line.clone(),
        reason: parsed.reason.clone(),
        model: response.model.clone(),
        generated_at,
    };
    state
        .store
        .upsert_triage_cache(&record)
        .await
        .map_err(|error| error.to_string())?;
    Ok(cache_to_data(record, miss.result.score, false).expect("fresh verdict parses"))
}

async fn load_body_text(state: &AppState, envelope: &Envelope) -> String {
    state
        .store
        .get_body(&envelope.id)
        .await
        .ok()
        .flatten()
        .and_then(|body| body.text_plain.or(body.text_html))
        .unwrap_or_else(|| envelope.snippet.clone())
}

async fn build_triage_prompt(state: &AppState, envelope: &Envelope, body_text: &str) -> String {
    let owned_addresses = state
        .store
        .list_account_addresses(&envelope.account_id)
        .await
        .unwrap_or_default();
    let mut prompt = String::new();
    prompt.push_str("Account owner addresses:\n");
    if owned_addresses.is_empty() {
        prompt.push_str("- unknown\n");
    } else {
        for address in owned_addresses {
            let primary = if address.is_primary { " (primary)" } else { "" };
            prompt.push_str(&format!("- {}{}\n", address.email, primary));
        }
    }
    let date = envelope
        .date
        .with_timezone(&chrono::Local)
        .format("%a %b %e %Y %H:%M %Z");
    prompt.push_str(&format!(
        "\nClassify this single search result. Return the strict first-line triage verdict, then a blank line, then at most one short summary sentence.\n\n--- Message 1 of 1 ---\nDate: {}\nFrom: {}\nTo: {}\nCc: {}\nBcc: {}\nSubject: {}\nBody:\n{}\n",
        date,
        format_address(&envelope.from),
        format_addresses(&envelope.to),
        format_addresses(&envelope.cc),
        format_addresses(&envelope.bcc),
        envelope.subject,
        body_text.trim(),
    ));
    prompt
}

fn cache_to_data(
    record: TriageCacheRecord,
    score: f32,
    cached: bool,
) -> Result<TriageMessageData, String> {
    let verdict = verdict_from_token(&record.verdict)
        .ok_or_else(|| format!("cached triage verdict is invalid: {}", record.verdict))?;
    Ok(TriageMessageData {
        message_id: record.message_id,
        account_id: record.account_id,
        thread_id: record.thread_id,
        score,
        verdict,
        verdict_token: verdict.token().to_string(),
        verdict_line: record.verdict_line,
        reason: record.reason,
        model: record.model,
        cached,
        generated_at: record.generated_at,
    })
}

struct ParsedVerdict {
    verdict: TriageVerdictData,
    line: String,
    reason: String,
}

fn parse_verdict_line(text: &str) -> Result<ParsedVerdict, String> {
    let line = text
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .ok_or_else(|| "LLM triage response was empty".to_string())?;
    let (verdict, reason) = if let Some(reason) = line.strip_prefix("ACTION REQUIRED — ") {
        (TriageVerdictData::Action, reason)
    } else if let Some(reason) = line.strip_prefix("ACTION — ") {
        (TriageVerdictData::Action, reason)
    } else if let Some(reason) = line.strip_prefix("FYI — ") {
        (TriageVerdictData::Fyi, reason)
    } else if let Some(reason) = line.strip_prefix("ROUTINE — ") {
        (TriageVerdictData::Routine, reason)
    } else {
        return Err(format!(
            "LLM triage response did not start with ACTION/FYI/ROUTINE verdict: {line:?}"
        ));
    };
    Ok(ParsedVerdict {
        verdict,
        line: line.to_string(),
        reason: reason.trim().to_string(),
    })
}

fn verdict_from_token(token: &str) -> Option<TriageVerdictData> {
    match token.trim().to_ascii_uppercase().as_str() {
        "ACTION" | "ACTION_REQUIRED" | "ACTION REQUIRED" => Some(TriageVerdictData::Action),
        "FYI" => Some(TriageVerdictData::Fyi),
        "ROUTINE" => Some(TriageVerdictData::Routine),
        _ => None,
    }
}

fn triage_content_hash(envelope: &Envelope, body_text: &str) -> String {
    let mut hasher = Sha256::new();
    hash_str(&mut hasher, summarize::SUMMARY_PROMPT_VERSION);
    hash_str(&mut hasher, TRIAGE_PROMPT_VERSION);
    hash_str(&mut hasher, &envelope.id.as_str());
    hash_str(&mut hasher, &envelope.provider_id);
    hash_str(
        &mut hasher,
        envelope.message_id_header.as_deref().unwrap_or_default(),
    );
    hash_str(&mut hasher, &envelope.subject);
    hash_str(&mut hasher, &envelope.date.timestamp().to_string());
    hash_address(&mut hasher, &envelope.from);
    hash_addresses(&mut hasher, &envelope.to);
    hash_addresses(&mut hasher, &envelope.cc);
    hash_addresses(&mut hasher, &envelope.bcc);
    hash_str(&mut hasher, &envelope.snippet);
    hash_str(&mut hasher, body_text);
    base16ct::lower::encode_string(&hasher.finalize())
}

fn hash_addresses(hasher: &mut Sha256, addresses: &[Address]) {
    hash_str(hasher, &addresses.len().to_string());
    for address in addresses {
        hash_address(hasher, address);
    }
}

fn hash_address(hasher: &mut Sha256, address: &Address) {
    hash_str(hasher, address.name.as_deref().unwrap_or_default());
    hash_str(hasher, &address.email);
}

fn hash_str(hasher: &mut Sha256, value: &str) {
    hasher.update(value.len().to_string().as_bytes());
    hasher.update(b":");
    hasher.update(value.as_bytes());
    hasher.update(b"\n");
}

fn format_addresses(addresses: &[Address]) -> String {
    if addresses.is_empty() {
        return "(none)".into();
    }
    addresses
        .iter()
        .map(format_address)
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_address(address: &Address) -> String {
    match address
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        Some(name) => format!("{name} <{}>", address.email),
        None => address.email.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strict_verdict_lines() {
        let action = parse_verdict_line("ACTION REQUIRED — pay the invoice\n\nSummary").unwrap();
        assert_eq!(action.verdict, TriageVerdictData::Action);
        assert_eq!(action.reason, "pay the invoice");
        let fyi = parse_verdict_line("FYI — parcel delivered").unwrap();
        assert_eq!(fyi.verdict.token(), "FYI");
        let routine = parse_verdict_line("ROUTINE — newsletter digest").unwrap();
        assert_eq!(routine.verdict.token(), "ROUTINE");
    }
}
