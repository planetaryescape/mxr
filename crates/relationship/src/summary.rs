use anyhow::Result;
use mxr_core::id::AccountId;
use mxr_llm::{
    wrap_untrusted_mail, BackfillHeldJob, ChatMessage, CompletionRequest, LlmError, LlmFeature,
    LlmRuntime, UNTRUSTED_MAIL_BEGIN, UNTRUSTED_MAIL_END, UNTRUSTED_MAIL_GUARD,
};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{ContactRelationshipSummaryRecord, RelationshipMessageSample, Store};
use serde::Deserialize;
use std::sync::Arc;

const MAX_EXCERPTS: usize = 10;
const MAX_PROMPT_CHARS: usize = 12_000;
const SUMMARY_TASK: &str = "Build an inspectable relationship profile from these email excerpts. Ground only in the excerpts. Do not infer facts, familiarity, meetings, or topics not present. Return strict JSON: {\"text\":\"<=200 token summary using tends-to phrasing\",\"known_topics\":[\"topic\"]}.";
const TRUNCATION_NOTE: &str = "\n[...truncated...]\n";

#[derive(Debug, Deserialize)]
struct SummaryResponse {
    text: String,
    #[serde(default)]
    known_topics: Vec<String>,
}

pub(crate) fn attempt_key(account_id: &AccountId, email: &str) -> String {
    format!("{}:{}", account_id.as_str(), email.to_ascii_lowercase())
}

/// The contact an `attempt_key` names, for re-running work an LLM outage
/// put off.
pub fn parse_attempt_key(key: &str) -> Option<(AccountId, String)> {
    let (account_id, email) = key.split_once(':')?;
    Some((account_id.parse().ok()?, email.to_string()))
}

/// What one summary refresh did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SummaryRun {
    /// The contact's mail went to the model.
    pub sent: bool,
    /// A new summary was stored.
    pub written: bool,
    /// The model call or storing its answer failed.
    pub failed: bool,
}

/// `force` is for explicit user rebuilds: it skips the backoff that
/// otherwise stops re-sending an input that recently failed, and the rule
/// that background summaries wait for mail with the contact that arrived
/// live rather than through the initial backfill.
pub async fn generate_relationship_summary(
    store: &Store,
    llm: &Arc<LlmRuntime>,
    account_id: &AccountId,
    email: &str,
    force: bool,
) -> Result<SummaryRun> {
    let not_sent = SummaryRun::default();
    let Some(style) = store.get_contact_style(account_id, email).await? else {
        return Ok(not_sent);
    };
    let feature_llm = llm.for_feature(LlmFeature::RelationshipSummary);
    let attempt_key = attempt_key(account_id, email);
    if !force && !feature_llm.background_attempt_due(&attempt_key, &style.source_hash) {
        return Ok(not_sent);
    }
    let samples = store.recent_contact_messages(account_id, email, 40).await?;
    if !summary_due(store, account_id, email, &style.source_hash, &samples).await? {
        return Ok(not_sent);
    }
    if !force && !store.contact_has_live_arrival(account_id, email).await? {
        llm.record_backfill_hold(BackfillHeldJob::RelationshipSummary, &attempt_key);
        return Ok(not_sent);
    }

    let reader_config = ReaderConfig::default();
    let overhead = UNTRUSTED_MAIL_GUARD.len()
        + SUMMARY_TASK.len()
        + UNTRUSTED_MAIL_BEGIN.len()
        + UNTRUSTED_MAIL_END.len()
        + TRUNCATION_NOTE.len()
        + "\n\n\n\n\n\n".len();
    let excerpt_budget = MAX_PROMPT_CHARS.saturating_sub(overhead);
    let mut excerpts = String::new();
    let mut source_ids = Vec::new();
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
        source_ids.push(sample.message_id.clone());
        let body = clean(Some(&sample.body), None, &reader_config).content;
        excerpts.push_str(&format!(
            "Message {} ({direction}, {}):\n{}\n\n",
            sample.message_id,
            sample.date,
            body.trim()
        ));
        if excerpts.len() > excerpt_budget {
            mxr_core::text::truncate_to_char_boundary(&mut excerpts, excerpt_budget);
            excerpts.push_str(TRUNCATION_NOTE);
            break;
        }
    }
    let prompt = format!(
        "{UNTRUSTED_MAIL_GUARD}\n\n{SUMMARY_TASK}\n\n{}",
        wrap_untrusted_mail(&excerpts)
    );

    let response = match feature_llm
        .complete_background(CompletionRequest {
            messages: vec![ChatMessage::user(prompt)],
            max_tokens: Some(300),
            temperature: Some(0.2),
        })
        .await
    {
        Ok(response) => response,
        Err(LlmError::Disabled) => return Ok(not_sent),
        Err(error @ LlmError::PrivacyBlocked(_)) => return Err(error.into()),
        Err(error @ LlmError::CircuitOpen { .. }) => {
            feature_llm.record_background_failure(&attempt_key, &style.source_hash, &error);
            return Ok(not_sent);
        }
        Err(error) => {
            feature_llm.record_background_failure(&attempt_key, &style.source_hash, &error);
            tracing::warn!(%account_id, %email, %error, "relationship summary call failed");
            return Ok(SummaryRun {
                sent: true,
                written: false,
                failed: true,
            });
        }
    };
    feature_llm.record_background_attempt(&attempt_key, &style.source_hash, true);
    let sent = SummaryRun {
        sent: true,
        ..SummaryRun::default()
    };
    let parsed = parse_summary_response(&response.content)?;
    if parsed.text.trim().is_empty() {
        return Ok(sent);
    }
    let stored = store
        .upsert_contact_relationship_summary_if_sources_exist(
            &ContactRelationshipSummaryRecord {
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
            },
            &source_ids,
        )
        .await;
    Ok(match stored {
        Ok(written) => SummaryRun { written, ..sent },
        Err(error) => {
            tracing::warn!(%account_id, %email, %error, "relationship summary was not stored");
            SummaryRun {
                failed: true,
                ..sent
            }
        }
    })
}

/// Whether a contact's summary is stale enough to regenerate: it has none,
/// or its style changed and at least three messages came after it.
async fn summary_due(
    store: &Store,
    account_id: &AccountId,
    email: &str,
    source_hash: &str,
    samples: &[RelationshipMessageSample],
) -> Result<bool> {
    let Some(existing) = store
        .get_contact_relationship_summary(account_id, email)
        .await?
    else {
        return Ok(true);
    };
    if existing.source_hash == source_hash {
        return Ok(false);
    }
    let unsummarized = samples
        .iter()
        .filter(|sample| !sample.is_list_sender && sample.date > existing.computed_at)
        .count();
    Ok(unsummarized >= 3)
}

/// Whether an explicit rebuild of this contact (a style rebuild, then a
/// forced summary) would send its mail to the model, on a working endpoint.
/// The dry run of a bulk rebuild counts these; it writes nothing.
pub async fn rebuild_would_send(
    store: &Store,
    account_id: &AccountId,
    email: &str,
) -> Result<bool> {
    let source_hash = match crate::service::style_source_hash(store, account_id, email).await? {
        Some(hash) => hash,
        None => match store.get_contact_style(account_id, email).await? {
            Some(style) => style.source_hash,
            None => return Ok(false),
        },
    };
    let samples = store.recent_contact_messages(account_id, email, 40).await?;
    summary_due(store, account_id, email, &source_hash, &samples).await
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
            generate_relationship_summary(&store, &llm, &account.id, "alice@example.com", false)
                .await
                .expect("generate");

        assert!(!generated.sent);
        let summary = store
            .get_contact_relationship_summary(&account.id, "alice@example.com")
            .await
            .expect("load summary")
            .expect("summary exists");
        assert_eq!(summary.source_hash, "style-v1");
    }

    #[tokio::test]
    async fn summary_prompt_guards_and_wraps_excerpts() {
        use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmProvider};
        use std::sync::Mutex;

        #[derive(Default)]
        struct Capture {
            msgs: Mutex<Vec<mxr_llm::ChatMessage>>,
        }
        #[async_trait::async_trait]
        impl LlmProvider for Capture {
            async fn complete(
                &self,
                req: CompletionRequest,
            ) -> std::result::Result<CompletionResponse, mxr_llm::LlmError> {
                *self.msgs.lock().expect("msgs lock") = req.messages.clone();
                Ok(CompletionResponse {
                    content: r#"{"text":"tends to be terse","known_topics":[]}"#.into(),
                    model: "stub".into(),
                    finish_reason: Some("stop".into()),
                })
            }
            fn capabilities(&self) -> LlmCapabilities {
                LlmCapabilities {
                    context_window: 8192,
                    supports_streaming: false,
                }
            }
            fn model_name(&self) -> &str {
                "stub"
            }
        }

        let store = Store::in_memory().await.expect("store");
        let account = test_account();
        store.insert_account(&account).await.expect("account");
        let now = Utc::now();
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
                computed_at: now,
                source_hash: "style-fresh".to_string(),
                drift_detected: false,
                drift_reason: None,
                drift_detected_at: None,
            })
            .await
            .expect("style");
        let backfilled = insert_message(&store, &account, "m1", now).await;
        record_arrival(&store, &account, &backfilled, false).await;

        let cap = std::sync::Arc::new(Capture::default());
        let llm = std::sync::Arc::new(LlmRuntime::new(
            cap.clone() as std::sync::Arc<dyn LlmProvider>
        ));
        let held =
            generate_relationship_summary(&store, &llm, &account.id, "alice@example.com", false)
                .await
                .expect("held");
        assert_eq!(held, SummaryRun::default(), "backfilled mail waits");
        assert!(cap.msgs.lock().expect("msgs lock").is_empty());
        assert!(llm
            .backfill_holds()
            .contains(&(BackfillHeldJob::RelationshipSummary, 1)));

        let live = insert_message(&store, &account, "m2", now).await;
        record_arrival(&store, &account, &live, true).await;
        let generated =
            generate_relationship_summary(&store, &llm, &account.id, "alice@example.com", false)
                .await
                .expect("generate");
        assert!(generated.written, "summary should have been generated");

        let msgs = cap.msgs.lock().expect("msgs lock");
        let user = &msgs[0].content;
        assert!(
            user.contains(mxr_llm::UNTRUSTED_MAIL_GUARD),
            "user-only prompt must lead with the injection guard"
        );
        let begin = user
            .rfind(mxr_llm::UNTRUSTED_MAIL_BEGIN)
            .expect("begin marker present");
        let end = user
            .rfind(mxr_llm::UNTRUSTED_MAIL_END)
            .expect("end marker present");
        let body = user
            .find("Can you send the update?")
            .expect("excerpt body present");
        assert!(
            begin < body && body < end,
            "mail excerpts must sit between the untrusted-content markers"
        );
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

    async fn record_arrival(store: &Store, account: &Account, id: &MessageId, live: bool) {
        store
            .record_message_arrival(
                id,
                &account.id,
                "alice@example.com",
                &mxr_core::ArrivalEvidence::default(),
                live,
            )
            .await
            .expect("arrival");
    }

    async fn insert_message(
        store: &Store,
        account: &Account,
        provider_id: &str,
        date: chrono::DateTime<Utc>,
    ) -> MessageId {
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
                message_id: message_id.clone(),
                text_plain: Some("Can you send the update?".to_string()),
                text_html: None,
                attachments: Vec::new(),
                fetched_at: date,
                metadata: MessageMetadata::default(),
            })
            .await
            .expect("body");
        message_id
    }
}
