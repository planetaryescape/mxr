//! Draft assist: generate a reply grounded on the thread context plus
//! a hand-tuned system prompt plus similar prior sent messages to
//! ground the generated voice.

use super::{relationship_profile, HandlerResult};
use crate::state::AppState;
use mxr_core::id::ThreadId;
use mxr_core::types::{Envelope, MessageDirection, SemanticChunkSourceKind};
use mxr_humanizer::{score as humanizer_score, writing_constraints, HumanizerOpts};
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature};
use mxr_protocol::{
    HumanizerReportSummaryData, ResponseData, VoiceMatchConfidenceData, VoiceMatchData,
};
use mxr_relationship::stylometry::StylometryMetrics;
use mxr_relationship::{compute_metrics, score_voice_match, VoiceMatchConfidence};
use std::collections::BTreeSet;

const SYSTEM_PROMPT: &str = "You draft email replies for a busy professional. Given the thread \
context and the user's intent, produce just the reply body — no \
greeting line if the thread is mid-conversation, no signature, no \
subject line. Be direct, concise, plain prose. Match the formality \
and length of the thread you're replying to. If the user's instruction \
is short, lean toward shorter replies. Never add commentary about \
what you're doing — just write the reply.";

const PROMPT_BUDGET_CHARS: usize = 24_000;
const GROUNDING_LIMIT: usize = 3;
const GROUNDING_SEARCH_LIMIT: usize = 8;
const GROUNDING_BUDGET_CHARS: usize = 4_000;
const RELATIONSHIP_BUDGET_CHARS: usize = 2_000;

pub(super) async fn draft_assist(
    state: &AppState,
    thread_id: &ThreadId,
    instruction: &str,
) -> HandlerResult {
    let envelopes = state
        .store
        .get_thread_envelopes(thread_id)
        .await
        .map_err(|e| e.to_string())?;
    if envelopes.is_empty() {
        return Err(format!("Thread {} has no messages to reply to", thread_id));
    }

    // Build a transcript for the model. Most recent last so the LLM's
    // attention is closest to the message it's actually replying to.
    let mut transcript = String::new();
    for env in &envelopes {
        let from = env.from.name.as_deref().unwrap_or(env.from.email.as_str());
        let body = match state.store.get_body(&env.id).await {
            Ok(Some(body)) => body
                .text_plain
                .or(body.text_html)
                .unwrap_or_else(|| env.snippet.clone()),
            _ => env.snippet.clone(),
        };
        transcript.push_str(&format!("--- {from} ---\n{body}\n\n"));
        if transcript.len() > PROMPT_BUDGET_CHARS {
            transcript.truncate(PROMPT_BUDGET_CHARS);
            transcript.push_str("\n[...thread truncated...]\n");
            break;
        }
    }

    let relationship_context = relationship_context_for_thread(state, &envelopes).await;
    let semantic_query = format!(
        "{}\n{}\n{}",
        envelopes[0].subject,
        instruction.trim(),
        transcript
    );
    let grounding = prior_sent_grounding(state, thread_id, &semantic_query, &envelopes).await;
    let user_message =
        build_user_message(&relationship_context, &grounding, &transcript, instruction);

    let request = CompletionRequest {
        messages: vec![
            ChatMessage::system(SYSTEM_PROMPT),
            ChatMessage::user(user_message),
        ],
        max_tokens: Some(600),
        temperature: Some(0.4),
    };

    match state
        .llm
        .for_feature(LlmFeature::DraftAssist)
        .complete(request)
        .await
    {
        Ok(response) => {
            let body = response.content.trim().to_string();
            let (body, humanizer, rewrite_iterations) =
                if state.config_snapshot().humanizer.apply_to_drafts {
                    super::humanizer::rewrite_to_threshold_with_context(
                        state,
                        body,
                        None,
                        Some(relationship_context.prompt.as_str()),
                    )
                    .await?
                } else {
                    let humanizer =
                        humanizer_summary(humanizer_score(&body, &HumanizerOpts::default()));
                    (body, humanizer, 0)
                };
            let voice_match = voice_match_for_body(&body, &relationship_context);
            Ok(ResponseData::DraftSuggestion {
                body,
                model: response.model,
                voice_match,
                humanizer: Some(humanizer),
                rewrite_iterations,
            })
        }
        Err(LlmError::Disabled) => Err(
            "LLM is disabled. Enable it in [llm] in your config and configure a model \
             (Ollama / LM Studio / OpenAI). See `mxr config`."
                .to_string(),
        ),
        Err(e) => Err(format!("LLM error: {e}")),
    }
}

fn build_user_message(
    relationship_context: &RelationshipPromptContext,
    grounding: &str,
    transcript: &str,
    instruction: &str,
) -> String {
    let mut message = String::new();
    if !relationship_context.prompt.is_empty() {
        message.push_str("[VOICE CONTEXT]\n");
        message.push_str("This is weak background guidance. The current thread and my instruction override it. Anything not listed as a known topic is unknown; do not invent familiarity.\n\n");
        message.push_str(&relationship_context.prompt);
        message.push_str("\n\n");
    }
    message.push_str("[WRITING CONSTRAINTS]\n");
    message.push_str(writing_constraints());
    message.push_str("\n\n");
    if !grounding.is_empty() {
        message.push_str("[PRIOR SENT REPLIES TO MATCH MY VOICE]\n");
        message.push_str(grounding);
        message.push_str("\n\n");
    }
    message.push_str("[THREAD SO FAR]\n");
    message.push_str(transcript);
    message.push_str("\n[TASK]\nNow draft my reply. Instruction: ");
    message.push_str(instruction.trim());
    message
}

#[derive(Default)]
struct RelationshipPromptContext {
    prompt: String,
    baseline: Option<(StylometryMetrics, u32)>,
}

async fn relationship_context_for_thread(
    state: &AppState,
    envelopes: &[Envelope],
) -> RelationshipPromptContext {
    let Some(first) = envelopes.first() else {
        return RelationshipPromptContext::default();
    };
    let owned = state
        .store
        .list_account_addresses(&first.account_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|address| address.email.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut emails = BTreeSet::new();
    for envelope in envelopes {
        maybe_insert_contact(&mut emails, &owned, &envelope.from.email);
        for address in envelope
            .to
            .iter()
            .chain(envelope.cc.iter())
            .chain(envelope.bcc.iter())
        {
            maybe_insert_contact(&mut emails, &owned, &address.email);
        }
    }
    let mut prompt = String::new();
    let mut baseline = None;
    for email in emails.into_iter().take(3) {
        let Ok(Some(profile)) =
            relationship_profile::load_relationship_profile(state, &first.account_id, &email).await
        else {
            continue;
        };
        let Some(style) = profile.style else {
            continue;
        };
        if style.msg_count_used < 5 || style.msg_count_used_theirs < 1 {
            continue;
        }
        prompt.push_str(&format!("Recipient/contact: {email}\n"));
        if let Some(summary) = profile.summary {
            prompt.push_str(&format!("- Relationship: {}\n", summary.text.trim()));
            if !summary.known_topics.is_empty() {
                prompt.push_str(&format!(
                    "- Known topics: {}\n",
                    summary.known_topics.join(", ")
                ));
            }
        }
        prompt.push_str(&format!(
            "- Your style to them: formality {:.2}, avg sentence {:.1} words, based on {} messages\n",
            style.formality_score, style.avg_sentence_len, style.msg_count_used
        ));
        prompt.push_str(&format!(
            "- Their style to you: formality {:.2}, avg sentence {:.1} words, based on {} messages\n",
            style.formality_score_theirs, style.avg_sentence_len_theirs, style.msg_count_used_theirs
        ));
        if baseline.is_none() {
            baseline = Some((
                StylometryMetrics {
                    formality_score: style.formality_score,
                    avg_sentence_len: style.avg_sentence_len,
                    ..StylometryMetrics::default()
                },
                style.msg_count_used,
            ));
        }
        if !profile.open_commitments.is_empty() {
            prompt.push_str("- Outstanding commitments:\n");
            for commitment in profile.open_commitments.iter().take(3) {
                prompt.push_str(&format!(
                    "  - {:?}: {}\n",
                    commitment.direction, commitment.what
                ));
            }
        }
        prompt.push('\n');
        if prompt.len() > RELATIONSHIP_BUDGET_CHARS {
            prompt.truncate(RELATIONSHIP_BUDGET_CHARS);
            prompt.push_str("\n[...relationship context truncated...]\n");
            break;
        }
    }
    RelationshipPromptContext { prompt, baseline }
}

fn maybe_insert_contact(emails: &mut BTreeSet<String>, owned: &BTreeSet<String>, email: &str) {
    let email = email.trim().to_ascii_lowercase();
    if email.is_empty() || owned.contains(&email) {
        return;
    }
    emails.insert(email);
}

fn humanizer_summary(report: mxr_humanizer::HumanizerReport) -> HumanizerReportSummaryData {
    super::humanizer::report_summary(report)
}

fn voice_match_for_body(
    body: &str,
    relationship_context: &RelationshipPromptContext,
) -> Option<VoiceMatchData> {
    let (baseline, count) = relationship_context.baseline.as_ref()?;
    let draft_metrics = compute_metrics(body);
    let report = score_voice_match(&draft_metrics, baseline, *count);
    Some(VoiceMatchData {
        score: report.score,
        confidence: match report.confidence {
            VoiceMatchConfidence::Low => VoiceMatchConfidenceData::Low,
            VoiceMatchConfidence::Medium => VoiceMatchConfidenceData::Medium,
            VoiceMatchConfidence::High => VoiceMatchConfidenceData::High,
        },
        notable_deltas: report.notable_deltas,
    })
}

async fn prior_sent_grounding(
    state: &AppState,
    current_thread_id: &ThreadId,
    query: &str,
    current_thread_envelopes: &[mxr_core::types::Envelope],
) -> String {
    let hits = match state
        .semantic
        .search(
            query,
            GROUNDING_SEARCH_LIMIT,
            &[
                SemanticChunkSourceKind::Header,
                SemanticChunkSourceKind::Body,
            ],
        )
        .await
    {
        Ok(hits) => hits,
        Err(error) => {
            tracing::warn!(error = %error, "draft assist semantic grounding unavailable");
            return String::new();
        }
    };
    if hits.is_empty() {
        return String::new();
    }

    let hit_ids = hits
        .into_iter()
        .map(|hit| hit.message_id)
        .collect::<Vec<_>>();
    let directions = match state.store.list_message_directions_by_ids(&hit_ids).await {
        Ok(directions) => directions,
        Err(error) => {
            tracing::warn!(error = %error, "draft assist failed to load grounding directions");
            return String::new();
        }
    };
    let outbound_ids = hit_ids
        .into_iter()
        .filter(|message_id| directions.get(message_id) == Some(&MessageDirection::Outbound))
        .collect::<Vec<_>>();
    if outbound_ids.is_empty() {
        return String::new();
    }

    let envelopes = match state.store.list_envelopes_by_ids(&outbound_ids).await {
        Ok(envelopes) => envelopes,
        Err(error) => {
            tracing::warn!(error = %error, "draft assist failed to load grounding messages");
            return String::new();
        }
    };
    let latest_thread_date = current_thread_envelopes
        .iter()
        .map(|envelope| envelope.date)
        .max();

    let mut grounding = String::new();
    let mut included = 0usize;
    for envelope in envelopes {
        if &envelope.thread_id == current_thread_id {
            continue;
        }
        if latest_thread_date.is_some_and(|date| envelope.date >= date) {
            continue;
        }

        let body = match state.store.get_body(&envelope.id).await {
            Ok(Some(body)) => body
                .text_plain
                .or(body.text_html)
                .unwrap_or_else(|| envelope.snippet.clone()),
            _ => envelope.snippet.clone(),
        };
        grounding.push_str(&format!(
            "--- Sent reply: {} ---\n{}\n\n",
            envelope.subject, body
        ));
        included += 1;
        if included >= GROUNDING_LIMIT || grounding.len() > GROUNDING_BUDGET_CHARS {
            break;
        }
    }

    if grounding.len() > GROUNDING_BUDGET_CHARS {
        grounding.truncate(GROUNDING_BUDGET_CHARS);
        grounding.push_str("\n[...prior replies truncated...]\n");
    }
    grounding
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::test_fixtures::TestEnvelopeBuilder;
    #[cfg(feature = "local")]
    use mxr_core::types::Address;
    use mxr_core::types::{MessageBody, MessageDirection, MessageMetadata};
    use mxr_llm::{CompletionResponse, LlmCapabilities, LlmProvider};
    use mxr_store::{ContactRelationshipSummaryRecord, ContactStyleRecord};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct CapturingLlm {
        last_request: Mutex<Option<CompletionRequest>>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for CapturingLlm {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            *self.last_request.lock().expect("request lock") = Some(req);
            Ok(CompletionResponse {
                content: "Grounded reply".to_string(),
                model: "test-llm".to_string(),
                finish_reason: Some("stop".to_string()),
            })
        }

        fn capabilities(&self) -> LlmCapabilities {
            LlmCapabilities {
                context_window: 8192,
                supports_streaming: false,
            }
        }

        fn model_name(&self) -> &str {
            "test-llm"
        }
    }

    fn body(message_id: mxr_core::MessageId, text: &str) -> MessageBody {
        MessageBody {
            message_id,
            text_plain: Some(text.to_string()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    #[cfg(feature = "local")]
    fn semantic_test_embedder(
        _profile: mxr_core::types::SemanticProfile,
        texts: &[String],
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|text| {
                let pricing = text.contains("pricing") as u8 as f32;
                let rollout = text.contains("rollout") as u8 as f32;
                vec![pricing, rollout, 1.0]
            })
            .collect())
    }

    #[tokio::test]
    async fn draft_assist_works_without_semantic_grounding() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());

        let account_id = state.default_account_id();
        let thread_id = mxr_core::ThreadId::new();
        let current = TestEnvelopeBuilder::new()
            .account_id(account_id)
            .thread_id(thread_id.clone())
            .provider_id("current-inbound")
            .subject("Pricing rollout")
            .from_address("Customer", "customer@example.com")
            .snippet("Can you clarify pricing rollout timing?")
            .build();
        state
            .store
            .upsert_envelope_with_direction(&current, MessageDirection::Inbound)
            .await
            .unwrap();
        state
            .store
            .insert_body(&body(
                current.id.clone(),
                "Can you clarify pricing rollout timing before Friday?",
            ))
            .await
            .unwrap();

        let response = draft_assist(&state, &thread_id, "reply briefly")
            .await
            .unwrap();
        assert!(matches!(
            response,
            ResponseData::DraftSuggestion { ref body, ref model, .. }
                if body == "Grounded reply" && model == "test-llm"
        ));

        let request = llm
            .last_request
            .lock()
            .expect("request lock")
            .clone()
            .expect("captured request");
        let prompt = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(prompt.contains("[THREAD SO FAR]"));
        assert!(prompt.contains("[WRITING CONSTRAINTS]"));
        assert!(!prompt.contains("[PRIOR SENT REPLIES TO MATCH MY VOICE]"));
    }

    #[tokio::test]
    async fn draft_assist_injects_relationship_context_as_weak_guidance() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());

        let account_id = state.default_account_id();
        let computed_at = chrono::Utc::now();
        state
            .store
            .upsert_contact_relationship_summary(&ContactRelationshipSummaryRecord {
                account_id: account_id.clone(),
                email: "customer@example.com".to_string(),
                text: "Customer prefers short pricing updates.".to_string(),
                model: "test-model".to_string(),
                known_topics: vec!["pricing".to_string(), "rollout".to_string()],
                computed_at,
                source_hash: "relationship-v1".to_string(),
                last_error: None,
            })
            .await
            .unwrap();
        state
            .store
            .upsert_contact_style(&ContactStyleRecord {
                account_id: account_id.clone(),
                email: "customer@example.com".to_string(),
                formality_score: 0.2,
                formality_score_theirs: 0.4,
                avg_sentence_len: 8.0,
                avg_sentence_len_theirs: 10.0,
                msg_count_used: 5,
                msg_count_used_theirs: 3,
                metrics_json: "{}".to_string(),
                metrics_json_theirs: "{}".to_string(),
                computed_at,
                source_hash: "style-v1".to_string(),
                drift_detected: false,
                drift_reason: None,
                drift_detected_at: None,
            })
            .await
            .unwrap();

        let thread_id = mxr_core::ThreadId::new();
        let current = TestEnvelopeBuilder::new()
            .account_id(account_id)
            .thread_id(thread_id.clone())
            .provider_id("current-inbound")
            .subject("Pricing rollout")
            .from_address("Customer", "customer@example.com")
            .snippet("Can you clarify pricing rollout timing?")
            .build();
        state
            .store
            .upsert_envelope_with_direction(&current, MessageDirection::Inbound)
            .await
            .unwrap();
        state
            .store
            .insert_body(&body(current.id.clone(), "Can you clarify pricing timing?"))
            .await
            .unwrap();

        let response = draft_assist(&state, &thread_id, "reply briefly")
            .await
            .unwrap();
        assert!(matches!(
            response,
            ResponseData::DraftSuggestion {
                voice_match: Some(_),
                humanizer: Some(_),
                ..
            }
        ));

        let request = llm
            .last_request
            .lock()
            .expect("request lock")
            .clone()
            .expect("captured request");
        let prompt = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(prompt.contains("[VOICE CONTEXT]"));
        assert!(prompt.contains("This is weak background guidance"));
        assert!(prompt.contains("do not invent familiarity"));
        assert!(prompt.contains("Relationship: Customer prefers short pricing updates."));
        assert!(prompt.contains("Known topics: pricing, rollout"));
        assert!(prompt.contains("Your style to them: formality 0.20"));
    }

    #[tokio::test]
    async fn draft_assist_omits_below_threshold_relationship_context() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());

        let account_id = state.default_account_id();
        let computed_at = chrono::Utc::now();
        state
            .store
            .upsert_contact_relationship_summary(&ContactRelationshipSummaryRecord {
                account_id: account_id.clone(),
                email: "customer@example.com".to_string(),
                text: "Customer prefers short pricing updates.".to_string(),
                model: "test-model".to_string(),
                known_topics: vec!["pricing".to_string(), "rollout".to_string()],
                computed_at,
                source_hash: "relationship-v1".to_string(),
                last_error: None,
            })
            .await
            .unwrap();
        state
            .store
            .upsert_contact_style(&ContactStyleRecord {
                account_id: account_id.clone(),
                email: "customer@example.com".to_string(),
                formality_score: 0.2,
                formality_score_theirs: 0.4,
                avg_sentence_len: 8.0,
                avg_sentence_len_theirs: 10.0,
                msg_count_used: 4,
                msg_count_used_theirs: 3,
                metrics_json: "{}".to_string(),
                metrics_json_theirs: "{}".to_string(),
                computed_at,
                source_hash: "style-v1".to_string(),
                drift_detected: false,
                drift_reason: None,
                drift_detected_at: None,
            })
            .await
            .unwrap();

        let thread_id = mxr_core::ThreadId::new();
        let current = TestEnvelopeBuilder::new()
            .account_id(account_id)
            .thread_id(thread_id.clone())
            .provider_id("current-inbound")
            .subject("Pricing rollout")
            .from_address("Customer", "customer@example.com")
            .snippet("Can you clarify pricing rollout timing?")
            .build();
        state
            .store
            .upsert_envelope_with_direction(&current, MessageDirection::Inbound)
            .await
            .unwrap();
        state
            .store
            .insert_body(&body(current.id.clone(), "Can you clarify pricing timing?"))
            .await
            .unwrap();

        let response = draft_assist(&state, &thread_id, "reply briefly")
            .await
            .unwrap();
        assert!(matches!(
            response,
            ResponseData::DraftSuggestion {
                voice_match: None,
                ..
            }
        ));

        let request = llm
            .last_request
            .lock()
            .expect("request lock")
            .clone()
            .expect("captured request");
        let prompt = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!prompt.contains("[VOICE CONTEXT]"));
        assert!(!prompt.contains("Customer prefers short pricing updates."));
        assert!(!prompt.contains("Known topics: pricing, rollout"));
    }

    #[cfg(feature = "local")]
    #[tokio::test]
    async fn draft_assist_includes_relevant_prior_sent_mail_as_grounding() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());

        let account_id = state.default_account_id();
        let now = chrono::Utc::now();
        let reply_thread_id = mxr_core::ThreadId::new();
        let current = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(reply_thread_id.clone())
            .provider_id("current-inbound")
            .subject("Pricing rollout")
            .from_address("Customer", "customer@example.com")
            .to(vec![Address {
                name: Some("Me".to_string()),
                email: "user@example.com".to_string(),
            }])
            .date(now)
            .snippet("Can you clarify pricing rollout timing?")
            .build();
        let prior_sent = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(mxr_core::ThreadId::new())
            .provider_id("prior-sent")
            .subject("Pricing rollout")
            .from_address("Me", "user@example.com")
            .to_address(Some("Customer"), "customer@example.com")
            .date(now - chrono::Duration::days(7))
            .snippet("I can hold the rollout note until numbers are firm.")
            .build();
        let prior_inbound = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(mxr_core::ThreadId::new())
            .provider_id("prior-inbound")
            .subject("Pricing rollout")
            .from_address("Vendor", "vendor@example.com")
            .date(now - chrono::Duration::days(6))
            .snippet("External pricing notes should not shape my voice.")
            .build();

        state
            .store
            .upsert_envelope_with_direction(&current, MessageDirection::Inbound)
            .await
            .unwrap();
        state
            .store
            .upsert_envelope_with_direction(&prior_sent, MessageDirection::Outbound)
            .await
            .unwrap();
        state
            .store
            .upsert_envelope_with_direction(&prior_inbound, MessageDirection::Inbound)
            .await
            .unwrap();
        state
            .store
            .insert_body(&body(
                current.id.clone(),
                "Can you clarify pricing rollout timing before Friday?",
            ))
            .await
            .unwrap();
        state
            .store
            .insert_body(&body(
                prior_sent.id.clone(),
                "I can hold the rollout note until the pricing numbers are firm.",
            ))
            .await
            .unwrap();
        state
            .store
            .insert_body(&body(
                prior_inbound.id.clone(),
                "External pricing notes should not shape my voice.",
            ))
            .await
            .unwrap();

        state
            .semantic
            .set_test_embedder(semantic_test_embedder)
            .await
            .unwrap();
        let mut config = state.config_snapshot();
        config.search.semantic.enabled = true;
        state.set_config_for_test(config).await;
        state.llm.replace(llm.clone());
        state
            .semantic
            .ingest_messages(&[prior_sent.id.clone(), prior_inbound.id.clone()])
            .await
            .unwrap();

        let response = draft_assist(&state, &reply_thread_id, "reply about pricing rollout")
            .await
            .unwrap();
        assert!(matches!(
            response,
            ResponseData::DraftSuggestion { ref body, ref model, .. }
                if body == "Grounded reply" && model == "test-llm"
        ));

        let request = llm
            .last_request
            .lock()
            .expect("request lock")
            .clone()
            .expect("captured request");
        let prompt = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(prompt.contains("I can hold the rollout note until the pricing numbers are firm"));
        assert!(!prompt.contains("External pricing notes should not shape my voice"));
    }
}
