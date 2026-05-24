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
    let samples = store.recent_contact_messages(account_id, email, 40).await?;
    if let Some(existing) = store
        .get_contact_relationship_summary(account_id, email)
        .await?
    {
        if existing.source_hash == style.source_hash {
            return Ok(false);
        }
        let unsummarized = samples
            .iter()
            .filter(|sample| !sample.is_list_sender && sample.date > existing.computed_at)
            .count();
        if unsummarized < 3 {
            return Ok(false);
        }
    }

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
        .complete_background(CompletionRequest {
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
        .map_or(content, str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::{
        Account, Address, BackendRef, MessageBody, MessageDirection, MessageFlags, MessageMetadata,
        ProviderKind, UnsubscribeMethod,
    };
    use mxr_llm::{LlmRuntime, NoopProvider};
    use mxr_store::{ContactStyleRecord, Store};

    #[tokio::test]
    async fn summary_regeneration_waits_for_three_unsummarized_messages() {
        let store = Store::in_memory().await.expect("store");
        let account = test_account();
        store.insert_account(&account).await.expect("account");
        let computed_at = Utc::now();
        store
            .upsert_contact_style(&ContactStyleRecord {
                account_id: account.id.clone(),
                email: "alice@example.com".to_string(),
                formality_score: 0.4,
                formality_score_theirs: 0.5,
                avg_sentence_len: 8.0,
                avg_sentence_len_theirs: 10.0,
                msg_count_used: 5,
                msg_count_used_theirs: 2,
                metrics_json: "{}".to_string(),
                metrics_json_theirs: "{}".to_string(),
                computed_at,
                source_hash: "style-v2".to_string(),
                drift_detected: false,
                drift_reason: None,
                drift_detected_at: None,
            })
            .await
            .expect("style");
        store
            .upsert_contact_relationship_summary(&ContactRelationshipSummaryRecord {
                account_id: account.id.clone(),
                email: "alice@example.com".to_string(),
                text: "Existing summary".to_string(),
                model: "test".to_string(),
                known_topics: vec!["pricing".to_string()],
                computed_at,
                source_hash: "style-v1".to_string(),
                last_error: None,
            })
            .await
            .expect("summary");
        for index in 0..2 {
            insert_message(
                &store,
                &account,
                &format!("new-{index}"),
                computed_at + chrono::Duration::minutes(index + 1),
            )
            .await;
        }
        let llm = std::sync::Arc::new(LlmRuntime::new(std::sync::Arc::new(NoopProvider)));

        let generated =
            generate_relationship_summary(&store, &llm, &account.id, "alice@example.com")
                .await
                .expect("generate");

        assert!(!generated);
        let summary = store
            .get_contact_relationship_summary(&account.id, "alice@example.com")
            .await
            .expect("load summary")
            .expect("summary exists");
        assert_eq!(summary.source_hash, "style-v1");
    }

    fn test_account() -> Account {
        Account {
            id: AccountId::new(),
            name: "Test".to_string(),
            email: "me@example.com".to_string(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".to_string(),
            }),
            send_backend: None,
            enabled: true,
        }
    }

    async fn insert_message(
        store: &Store,
        account: &Account,
        provider_id: &str,
        date: chrono::DateTime<Utc>,
    ) {
        let message_id = MessageId::new();
        let envelope = mxr_core::types::Envelope {
            id: message_id.clone(),
            account_id: account.id.clone(),
            provider_id: provider_id.to_string(),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Alice".to_string()),
                email: "alice@example.com".to_string(),
            },
            to: vec![Address {
                name: None,
                email: account.email.clone(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "Follow-up".to_string(),
            date,
            flags: MessageFlags::empty(),
            snippet: "Can you send the update?".to_string(),
            has_attachments: false,
            size_bytes: 10,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: Vec::new(),
            keywords: std::collections::BTreeSet::new(),
        };
        store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .expect("envelope");
        store
            .insert_body(&MessageBody {
                message_id,
                text_plain: Some("Can you send the update?".to_string()),
                text_html: None,
                attachments: Vec::new(),
                fetched_at: date,
                metadata: MessageMetadata::default(),
            })
            .await
            .expect("body");
    }
}
