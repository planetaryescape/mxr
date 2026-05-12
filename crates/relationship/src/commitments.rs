use anyhow::Result;
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId};
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature, LlmRuntime};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{CommitmentDirection, CommitmentStatus, ContactCommitmentRecord, Store};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::str::FromStr;
use std::sync::Arc;

const MAX_EXCERPTS: usize = 12;
const MAX_PROMPT_CHARS: usize = 14_000;

#[derive(Debug, Deserialize)]
struct CommitmentResponse {
    #[serde(default)]
    commitments: Vec<ExtractedCommitment>,
}

#[derive(Debug, Deserialize)]
struct ExtractedCommitment {
    who_owes: String,
    what: String,
    evidence_msg_id: String,
    direction: CommitmentDirection,
    #[serde(default)]
    by_when: Option<String>,
}

pub async fn extract_commitments(
    store: &Store,
    llm: &Arc<LlmRuntime>,
    account_id: &AccountId,
    email: &str,
) -> Result<usize> {
    let samples = store.recent_contact_messages(account_id, email, 40).await?;
    if samples.is_empty() {
        return Ok(0);
    }
    let reader_config = ReaderConfig::default();
    let mut prompt = String::from(
        "Extract only explicit open asks, promises, decisions, or follow-ups from these emails. Do not infer implied work. Return strict JSON: {\"commitments\":[{\"who_owes\":\"name/email\",\"what\":\"concrete obligation\",\"by_when\":\"RFC3339 or null\",\"evidence_msg_id\":\"message id from input\",\"direction\":\"yours|theirs\"}]}. Use direction=yours when the account owner owes the contact, theirs when the contact owes the account owner.\n\n",
    );
    for sample in samples
        .iter()
        .filter(|sample| !sample.is_list_sender)
        .take(MAX_EXCERPTS)
    {
        let body = clean(Some(&sample.body), None, &reader_config).content;
        prompt.push_str(&format!(
            "Message {} from {} at {} in thread {}:\n{}\n\n",
            sample.message_id,
            sample.from_email,
            sample.date,
            sample.thread_id,
            body.trim()
        ));
        if prompt.len() > MAX_PROMPT_CHARS {
            prompt.truncate(MAX_PROMPT_CHARS);
            prompt.push_str("\n[...truncated...]\n");
            break;
        }
    }

    let response = match llm
        .for_feature(LlmFeature::Commitments)
        .complete(CompletionRequest {
            messages: vec![ChatMessage::user(prompt)],
            max_tokens: Some(500),
            temperature: Some(0.1),
        })
        .await
    {
        Ok(response) => response,
        Err(LlmError::Disabled) => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    let parsed = parse_commitments_response(&response.content)?;
    let mut inserted = 0;
    for commitment in parsed.commitments {
        if commitment.what.trim().is_empty() || commitment.who_owes.trim().is_empty() {
            continue;
        }
        let Ok(evidence_msg_id) = MessageId::from_str(commitment.evidence_msg_id.trim()) else {
            continue;
        };
        let Some(sample) = samples
            .iter()
            .find(|sample| sample.message_id == evidence_msg_id)
        else {
            continue;
        };
        let record = ContactCommitmentRecord {
            id: commitment_id(
                account_id,
                email,
                &sample.thread_id,
                commitment.direction,
                &commitment.what,
                &evidence_msg_id,
            ),
            account_id: account_id.clone(),
            email: email.to_ascii_lowercase(),
            thread_id: sample.thread_id.clone(),
            direction: commitment.direction,
            status: CommitmentStatus::Open,
            who_owes: commitment.who_owes.trim().to_string(),
            what: normalize_what(&commitment.what),
            by_when: commitment.by_when.as_deref().and_then(parse_datetime),
            evidence_msg_id,
            extracted_at: Utc::now(),
            resolved_at: None,
        };
        store.upsert_contact_commitment(&record).await?;
        inserted += 1;
    }
    Ok(inserted)
}

fn parse_commitments_response(content: &str) -> Result<CommitmentResponse> {
    let trimmed = strip_json_fence(content.trim());
    if trimmed.starts_with('[') {
        let commitments = serde_json::from_str(trimmed)?;
        return Ok(CommitmentResponse { commitments });
    }
    Ok(serde_json::from_str(trimmed)?)
}

fn strip_json_fence(content: &str) -> &str {
    content
        .strip_prefix("```json")
        .or_else(|| content.strip_prefix("```"))
        .and_then(|content| content.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(content)
}

fn normalize_what(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    if value.trim().eq_ignore_ascii_case("null") || value.trim().is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn commitment_id(
    account_id: &AccountId,
    email: &str,
    thread_id: &mxr_core::id::ThreadId,
    direction: CommitmentDirection,
    what: &str,
    evidence_msg_id: &MessageId,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(account_id.as_str());
    hasher.update(email.to_ascii_lowercase());
    hasher.update(thread_id.as_str());
    hasher.update(direction.as_str());
    hasher.update(normalize_what(what).to_ascii_lowercase());
    hasher.update(evidence_msg_id.as_str());
    format!("commitment-{:x}", hasher.finalize())
}
