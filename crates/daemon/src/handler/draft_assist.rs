//! Draft assist: generate a reply grounded on the thread context plus
//! a hand-tuned system prompt plus similar prior sent messages to
//! ground the generated voice.

use super::HandlerResult;
use crate::state::AppState;
use mxr_core::id::ThreadId;
use mxr_core::types::{MessageDirection, SemanticChunkSourceKind};
use mxr_llm::{ChatMessage, CompletionRequest, LlmError};
use mxr_protocol::ResponseData;

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

    let semantic_query = format!(
        "{}\n{}\n{}",
        envelopes[0].subject,
        instruction.trim(),
        transcript
    );
    let grounding = prior_sent_grounding(state, thread_id, &semantic_query, &envelopes).await;
    let user_message = if grounding.is_empty() {
        format!(
            "Thread so far:\n\n{transcript}\n\
             Now draft my reply. Instruction: {}",
            instruction.trim()
        )
    } else {
        format!(
            "Prior sent replies to match my voice:\n\n{grounding}\n\
             Thread so far:\n\n{transcript}\n\
             Now draft my reply. Instruction: {}",
            instruction.trim()
        )
    };

    let request = CompletionRequest {
        messages: vec![
            ChatMessage::system(SYSTEM_PROMPT),
            ChatMessage::user(user_message),
        ],
        max_tokens: Some(600),
        temperature: Some(0.4),
    };

    match state.llm.complete(request).await {
        Ok(response) => Ok(ResponseData::DraftSuggestion {
            body: response.content.trim().to_string(),
            model: response.model,
        }),
        Err(LlmError::Disabled) => Err(
            "LLM is disabled. Enable it in [llm] in your config and configure a model \
             (Ollama / LM Studio / OpenAI). See `mxr config`."
                .to_string(),
        ),
        Err(e) => Err(format!("LLM error: {e}")),
    }
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
            ResponseData::DraftSuggestion { ref body, ref model }
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
        assert!(prompt.contains("Thread so far:"));
        assert!(!prompt.contains("Prior sent replies to match my voice:"));
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
            ResponseData::DraftSuggestion { ref body, ref model }
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
