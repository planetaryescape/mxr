use anyhow::Result;
use mxr_core::id::AccountId;
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature, LlmRuntime};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{ContactRelationshipSummaryRecord, Store};
use serde::Deserialize;
use std::sync::Arc;

const MAX_EXCERPTS: usize = 10;
const MAX_PROMPT_CHARS: usize = 12_000;

#[derive(Debug, Deserialize)]
struct SummaryResponse {
    text: String,
    #[serde(default)]
    known_topics: Vec<String>,
}

pub async fn generate_relationship_summary(
    store: &Store,
    llm: &Arc<LlmRuntime>,
    account_id: &AccountId,
    email: &str,
) -> Result<bool> {
    let Some(style) = store.get_contact_style(account_id, email).await? else {
        return Ok(false);
    };
    if let Some(existing) = store
        .get_contact_relationship_summary(account_id, email)
        .await?
    {
        if existing.source_hash == style.source_hash {
            return Ok(false);
        }
    }

    let samples = store.recent_contact_messages(account_id, email, 40).await?;
    let reader_config = ReaderConfig::default();
    let mut prompt = String::from(
        "Build an inspectable relationship profile from these email excerpts. Ground only in the excerpts. Do not infer facts, familiarity, meetings, or topics not present. Return strict JSON: {\"text\":\"<=200 token summary using tends-to phrasing\",\"known_topics\":[\"topic\"]}.\n\n",
    );
    for sample in samples
        .iter()
        .filter(|sample| !sample.is_list_sender)
        .take(MAX_EXCERPTS)
    {
        let direction = if sample.from_email.eq_ignore_ascii_case(email) {
            "them_to_user"
        } else {
            "user_to_them"
        };
        let body = clean(Some(&sample.body), None, &reader_config).content;
        prompt.push_str(&format!(
            "Message {} ({direction}, {}):\n{}\n\n",
            sample.message_id,
            sample.date,
            body.trim()
        ));
        if prompt.len() > MAX_PROMPT_CHARS {
            prompt.truncate(MAX_PROMPT_CHARS);
            prompt.push_str("\n[...truncated...]\n");
            break;
        }
    }

    let response = match llm
        .for_feature(LlmFeature::RelationshipSummary)
        .complete(CompletionRequest {
            messages: vec![ChatMessage::user(prompt)],
            max_tokens: Some(300),
            temperature: Some(0.2),
        })
        .await
    {
        Ok(response) => response,
        Err(LlmError::Disabled) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let parsed = parse_summary_response(&response.content)?;
    if parsed.text.trim().is_empty() {
        return Ok(false);
    }
    store
        .upsert_contact_relationship_summary(&ContactRelationshipSummaryRecord {
            account_id: account_id.clone(),
            email: email.to_ascii_lowercase(),
            text: parsed.text.trim().to_string(),
            model: response.model,
            known_topics: parsed
                .known_topics
                .into_iter()
                .map(|topic| topic.trim().to_string())
                .filter(|topic| !topic.is_empty())
                .take(12)
                .collect(),
            computed_at: chrono::Utc::now(),
            source_hash: style.source_hash,
            last_error: None,
        })
        .await?;
    Ok(true)
}

fn parse_summary_response(content: &str) -> Result<SummaryResponse> {
    let trimmed = strip_json_fence(content.trim());
    serde_json::from_str(trimmed).or_else(|_| {
        Ok(SummaryResponse {
            text: content.trim().to_string(),
            known_topics: Vec::new(),
        })
    })
}

fn strip_json_fence(content: &str) -> &str {
    content
        .strip_prefix("```json")
        .or_else(|| content.strip_prefix("```"))
        .and_then(|content| content.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(content)
}
