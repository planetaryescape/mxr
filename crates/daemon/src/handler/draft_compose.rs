//! Unified AI draft generation — one entry for new messages, replies from
//! compose (`source_message_id`), and replies from the reader (`thread_id`).
//! Thin orchestrator over `draft_context`: resolve the conversation (if any),
//! assemble relationship/voice context + grounding, infer tone/length (unless
//! overridden), and finish into a `DraftSuggestion`. Never auto-sends.

use super::{draft_context, HandlerResult};
use crate::state::AppState;
use draft_context::DraftContext;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, Envelope};
use mxr_llm::{guarded_system_prompt, ChatMessage, CompletionRequest, LlmError, LlmFeature};
use mxr_protocol::{DraftLengthHintData, VoiceRegisterData};

const SYSTEM_PROMPT_NEW: &str = "You write email for a busy professional in their own voice. \
Produce only the email body — no subject line, no signature. Be direct, specific, and \
plain-spoken, and match the tone and length the user uses with this person. Never invent prior \
familiarity or facts that are not provided.";

const SYSTEM_PROMPT_REPLY: &str = "You draft email replies for a busy professional in their own \
voice. Given the thread context and the user's intent, produce just the reply body — no greeting \
line if the thread is mid-conversation, no signature, no subject line. Match the formality and \
length the user uses with this person. Never add commentary about what you're doing, and never \
invent facts or familiarity that aren't in the thread.";

#[allow(clippy::too_many_arguments)]
pub(super) async fn draft_compose(
    state: &AppState,
    account_id: Option<&AccountId>,
    to: Option<Address>,
    instruction: &str,
    source_message_id: Option<MessageId>,
    thread_id: Option<ThreadId>,
    register: Option<VoiceRegisterData>,
    length_hint: Option<DraftLengthHintData>,
) -> HandlerResult {
    // The task line is never truncated, so an unbounded instruction would break
    // the assembled-prompt ceiling. Reject it here at the validation layer.
    let max_instruction = draft_context::max_instruction_chars();
    if instruction.len() > max_instruction {
        return Err(crate::handler::HandlerError::Message(format!(
            "draft instruction is too long: {} bytes exceeds the {max_instruction} byte limit \
             (assembled-prompt ceiling minus fixed scaffolding)",
            instruction.len()
        )));
    }

    // Reply to an explicit thread (reader / quick-reply).
    if let Some(thread_id) = thread_id.as_ref() {
        let envelopes = state.store.get_thread_envelopes(thread_id).await?;
        if envelopes.is_empty() {
            return Err(format!("Thread {thread_id} has no messages to reply to").into());
        }
        let account = account_id
            .cloned()
            .unwrap_or_else(|| envelopes[0].account_id.clone());
        return draft_reply(
            state,
            &account,
            instruction,
            register,
            length_hint,
            envelopes,
        )
        .await;
    }

    // Reply from compose: resolve the source message to its thread. If it
    // isn't synced locally, fall through to new-message mode.
    if let Some(message_id) = source_message_id.as_ref() {
        let envelopes = draft_context::resolve_thread_envelopes(state, message_id).await;
        if !envelopes.is_empty() {
            let account = account_id
                .cloned()
                .unwrap_or_else(|| envelopes[0].account_id.clone());
            return draft_reply(
                state,
                &account,
                instruction,
                register,
                length_hint,
                envelopes,
            )
            .await;
        }
    }

    // New message.
    let to = to.ok_or_else(|| {
        crate::handler::HandlerError::Message(
            "Draft needs a recipient (to) or a thread to reply to.".to_string(),
        )
    })?;
    let account = account_id.ok_or_else(|| {
        crate::handler::HandlerError::Message(
            "Draft needs an account for a new message.".to_string(),
        )
    })?;
    draft_brand_new(state, account, to, instruction, register, length_hint).await
}

async fn draft_reply(
    state: &AppState,
    account_id: &AccountId,
    instruction: &str,
    register: Option<VoiceRegisterData>,
    length_hint: Option<DraftLengthHintData>,
    envelopes: Vec<Envelope>,
) -> HandlerResult {
    let contacts = draft_context::thread_contact_emails(state, &envelopes).await;
    for email in contacts.iter().take(2) {
        draft_context::ensure_contact_fresh(state, account_id, email).await;
    }
    let context = draft_context::build_relationship_block(
        state,
        account_id,
        &contacts,
        instruction,
        register,
        length_hint,
        draft_context::RELATIONSHIP_BUDGET_CHARS,
    )
    .await;
    let transcript = draft_context::build_transcript(state, &envelopes).await;
    let latest = envelopes.iter().map(|envelope| envelope.date).max();
    let semantic_query = format!(
        "{}\n{}\n{}",
        envelopes[0].subject,
        instruction.trim(),
        transcript
    );
    let grounding = draft_context::prior_sent_grounding(
        state,
        Some(&envelopes[0].thread_id),
        &semantic_query,
        latest,
    )
    .await;
    let task_line = format!(
        "Now draft my reply. Length: {}. Instruction: {}",
        draft_context::length_label(context.inferred_length),
        instruction.trim()
    );
    let user_message = draft_context::assemble_user_message_within_budget(
        &context.prompt,
        &grounding,
        &transcript,
        &task_line,
        draft_context::ASSEMBLED_MESSAGE_BUDGET_CHARS,
    );
    complete_and_finish(
        state,
        LlmFeature::DraftAssist,
        SYSTEM_PROMPT_REPLY,
        user_message,
        context,
    )
    .await
}

async fn draft_brand_new(
    state: &AppState,
    account_id: &AccountId,
    to: Address,
    instruction: &str,
    register: Option<VoiceRegisterData>,
    length_hint: Option<DraftLengthHintData>,
) -> HandlerResult {
    draft_context::ensure_contact_fresh(state, account_id, &to.email).await;
    let context = draft_context::build_relationship_block(
        state,
        account_id,
        std::slice::from_ref(&to.email),
        instruction,
        register,
        length_hint,
        draft_context::RELATIONSHIP_BUDGET_CHARS,
    )
    .await;
    let semantic_query = format!("{}\n{}", instruction.trim(), to.email);
    let grounding = draft_context::prior_sent_grounding(state, None, &semantic_query, None).await;
    let task_line = format!(
        "Write a new email to {}. Length: {}. Purpose: {}",
        recipient_label(&to),
        draft_context::length_label(context.inferred_length),
        instruction.trim()
    );
    let user_message = draft_context::assemble_user_message_within_budget(
        &context.prompt,
        &grounding,
        "",
        &task_line,
        draft_context::ASSEMBLED_MESSAGE_BUDGET_CHARS,
    );
    complete_and_finish(
        state,
        LlmFeature::DraftNew,
        SYSTEM_PROMPT_NEW,
        user_message,
        context,
    )
    .await
}

async fn complete_and_finish(
    state: &AppState,
    feature: LlmFeature,
    system_prompt: &str,
    user_message: String,
    context: DraftContext,
) -> HandlerResult {
    let response = match state
        .llm
        .for_feature(feature)
        .complete(CompletionRequest {
            messages: vec![
                // The thread transcript inside `user_message` is wrapped in
                // untrusted-content delimiters by `assemble_user_message_*`;
                // the guard here tells the model they mark data, not
                // instructions. Output is a DraftSuggestion — written to a
                // draft/stdout, never auto-sent (see ai-email.md cut list).
                ChatMessage::system(guarded_system_prompt(system_prompt)),
                ChatMessage::user(user_message),
            ],
            max_tokens: Some(draft_context::max_tokens_for_length(
                context.inferred_length,
            )),
            temperature: Some(0.4),
        })
        .await
    {
        Ok(response) => response,
        Err(LlmError::Disabled) => {
            return Err(crate::handler::HandlerError::Message(
                "LLM is disabled. Enable it in [llm].".to_string(),
            ))
        }
        Err(error) => return Err(format!("LLM error: {error}").into()),
    };
    draft_context::finish_draft_suggestion(
        state,
        response.content.trim().to_string(),
        response.model,
        context.baseline,
        Some(context.prompt.as_str()),
        context.inferred_register,
        context.inferred_length,
        context.context_note,
    )
    .await
}

fn recipient_label(to: &Address) -> String {
    match to.name.as_deref().filter(|name| !name.trim().is_empty()) {
        Some(name) => format!("{name} <{}>", to.email),
        None => to.email.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::test_fixtures::TestEnvelopeBuilder;
    #[cfg(feature = "local")]
    use mxr_core::types::Address as CoreAddress;
    use mxr_core::types::{MessageBody, MessageDirection, MessageMetadata};
    use mxr_llm::{CompletionResponse, LlmCapabilities, LlmProvider};
    use mxr_protocol::ResponseData;
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
                content: "Drafted reply".to_string(),
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

    fn captured_prompt(llm: &CapturingLlm) -> String {
        llm.last_request
            .lock()
            .expect("request lock")
            .clone()
            .expect("captured request")
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn seed_contact(
        state: &AppState,
        account_id: &mxr_core::id::AccountId,
        email: &str,
        formality: f64,
        msg_count_used: u32,
    ) {
        let computed_at = chrono::Utc::now();
        state
            .store
            .upsert_contact_relationship_summary(&ContactRelationshipSummaryRecord {
                account_id: account_id.clone(),
                email: email.to_string(),
                text: "Customer prefers short pricing updates.".to_string(),
                model: "test-model".to_string(),
                known_topics: vec!["pricing".to_string()],
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
                email: email.to_string(),
                formality_score: formality,
                formality_score_theirs: formality,
                avg_sentence_len: 8.0,
                avg_sentence_len_theirs: 9.0,
                msg_count_used,
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
    }

    /// Seed an inbound thread; returns (thread_id, inbound body text).
    async fn seed_inbound_thread(
        state: &AppState,
        account_id: &mxr_core::id::AccountId,
    ) -> (mxr_core::ThreadId, mxr_core::MessageId, &'static str) {
        let thread_id = mxr_core::ThreadId::new();
        let inbound_text = "Can you clarify the pricing rollout timing before Friday?";
        let current = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(thread_id.clone())
            .provider_id("current-inbound")
            .subject("Pricing rollout")
            .sender_address("Customer", "customer@example.com")
            .snippet("Can you clarify pricing rollout timing?")
            .build();
        state
            .store
            .upsert_envelope_with_direction(&current, MessageDirection::Inbound)
            .await
            .unwrap();
        state
            .store
            .insert_body(&body(current.id.clone(), inbound_text))
            .await
            .unwrap();
        (thread_id, current.id, inbound_text)
    }

    fn draft_suggestion(response: ResponseData) -> (Option<VoiceRegisterData>, Option<String>) {
        match response {
            ResponseData::DraftSuggestion {
                inferred_register,
                context_note,
                ..
            } => (inferred_register, context_note),
            other => panic!("expected DraftSuggestion, got {other:?}"),
        }
    }

    // Behavior 1: thread_id mode drafts against the conversation.
    #[tokio::test]
    async fn reply_via_thread_id_uses_conversation() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        let (thread_id, _, inbound_text) = seed_inbound_thread(&state, &account_id).await;

        let response = draft_compose(
            &state,
            Some(&account_id),
            None,
            "reply briefly",
            None,
            Some(thread_id),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(matches!(response, ResponseData::DraftSuggestion { .. }));
        // The actual thread content reached the model (survives any draft_context rewrite).
        assert!(captured_prompt(&llm).contains(inbound_text));
    }

    // Injection hardening: reply prompt guards the system message and the
    // thread being replied to lands inside the untrusted-content markers.
    #[tokio::test]
    async fn reply_prompt_guards_system_and_wraps_thread() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        let (thread_id, _, inbound_text) = seed_inbound_thread(&state, &account_id).await;

        draft_compose(
            &state,
            Some(&account_id),
            None,
            "reply briefly",
            None,
            Some(thread_id),
            None,
            None,
        )
        .await
        .unwrap();

        let req = llm
            .last_request
            .lock()
            .expect("request lock")
            .clone()
            .expect("captured request");
        assert!(
            req.messages[0]
                .content
                .contains(mxr_llm::UNTRUSTED_MAIL_GUARD),
            "system prompt must carry the shared injection guard"
        );
        let user = &req.messages[1].content;
        let begin = user
            .find(mxr_llm::UNTRUSTED_MAIL_BEGIN)
            .expect("begin marker present");
        let end = user
            .find(mxr_llm::UNTRUSTED_MAIL_END)
            .expect("end marker present");
        let body = user.find(inbound_text).expect("inbound thread present");
        assert!(
            begin < body && body < end,
            "thread transcript must sit between the untrusted-content markers"
        );
    }

    // Behavior 2: source_message_id resolves to its thread.
    #[tokio::test]
    async fn reply_via_source_message_uses_conversation() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        let (_, source_id, inbound_text) = seed_inbound_thread(&state, &account_id).await;

        let response = draft_compose(
            &state,
            Some(&account_id),
            Some(Address {
                name: Some("Customer".to_string()),
                email: "customer@example.com".to_string(),
            }),
            "reply briefly",
            Some(source_id),
            None,
            None,
            None,
        )
        .await
        .unwrap();
        assert!(matches!(response, ResponseData::DraftSuggestion { .. }));
        assert!(captured_prompt(&llm).contains(inbound_text));
    }

    // Behavior 3: account_id omitted in thread mode → derived from the thread.
    #[tokio::test]
    async fn reply_derives_account_from_thread_when_omitted() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        let (thread_id, _, inbound_text) = seed_inbound_thread(&state, &account_id).await;

        let response = draft_compose(
            &state,
            None, // no account — must be derived from the thread
            None,
            "reply briefly",
            None,
            Some(thread_id),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(matches!(response, ResponseData::DraftSuggestion { .. }));
        assert!(captured_prompt(&llm).contains(inbound_text));
    }

    // Behavior 4: new message includes the relationship summary even when the
    // contact is below the old 5-message threshold.
    #[tokio::test]
    async fn new_message_includes_relationship_summary_below_old_threshold() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        seed_contact(&state, &account_id, "customer@example.com", 0.2, 2).await; // 2 < old gate of 5

        let response = draft_compose(
            &state,
            Some(&account_id),
            Some(Address {
                name: None,
                email: "customer@example.com".to_string(),
            }),
            "follow up on pricing",
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        let (register, note) = draft_suggestion(response);
        assert!(register.is_some());
        assert!(note.is_some());
        // The seeded relationship summary reached the model.
        assert!(captured_prompt(&llm).contains("Customer prefers short pricing updates."));
    }

    // Behavior 5: tone inferred from the contact's formality (casual band).
    #[tokio::test]
    async fn casual_contact_infers_casual_register() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        seed_contact(&state, &account_id, "buddy@example.com", 0.15, 6).await;

        let response = draft_compose(
            &state,
            Some(&account_id),
            Some(Address {
                name: None,
                email: "buddy@example.com".to_string(),
            }),
            "grab coffee next week",
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        let (register, note) = draft_suggestion(response);
        assert_eq!(register, Some(VoiceRegisterData::Casual));
        assert!(note.unwrap().contains("casual"));
    }

    // An oversized instruction is rejected at the validation layer so the
    // never-truncated task line can't break the assembled-prompt ceiling.
    #[tokio::test]
    async fn oversized_instruction_is_rejected() {
        let state = AppState::in_memory().await.unwrap();
        let account_id = state.default_account_id();
        let limit = draft_context::max_instruction_chars();
        let huge = "x".repeat(limit + 1);
        let err = draft_compose(
            &state,
            Some(&account_id),
            Some(Address {
                name: None,
                email: "a@b.com".to_string(),
            }),
            &huge,
            None,
            None,
            None,
            None,
        )
        .await
        .expect_err("oversized instruction must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("too long") && msg.contains(&limit.to_string()),
            "error must state the limit: {msg}"
        );
    }

    // Behavior 6: a manual register overrides the inferred tone.
    #[tokio::test]
    async fn manual_register_overrides_inference() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        seed_contact(&state, &account_id, "buddy@example.com", 0.15, 6).await; // would infer casual

        let response = draft_compose(
            &state,
            Some(&account_id),
            Some(Address {
                name: None,
                email: "buddy@example.com".to_string(),
            }),
            "grab coffee",
            None,
            None,
            Some(VoiceRegisterData::Formal),
            None,
        )
        .await
        .unwrap();
        let (register, _) = draft_suggestion(response);
        assert_eq!(register, Some(VoiceRegisterData::Formal));
    }

    // Behavior 7: context_note names the recipient when a profile exists.
    #[tokio::test]
    async fn context_note_names_recipient() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        seed_contact(&state, &account_id, "customer@example.com", 0.2, 6).await;

        let response = draft_compose(
            &state,
            Some(&account_id),
            Some(Address {
                name: None,
                email: "customer@example.com".to_string(),
            }),
            "follow up",
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        let (_, note) = draft_suggestion(response);
        assert!(note.unwrap().contains("customer@example.com"));
    }

    // Behavior 8: a new message with neither account nor thread is rejected.
    #[tokio::test]
    async fn new_message_without_account_or_thread_errors() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());

        let result = draft_compose(
            &state,
            None,
            Some(Address {
                name: None,
                email: "stranger@example.com".to_string(),
            }),
            "say hi",
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(result.is_err());
    }

    // Reply pulls in the recipient's relationship summary as context.
    #[tokio::test]
    async fn reply_injects_relationship_context() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        seed_contact(&state, &account_id, "customer@example.com", 0.2, 5).await;
        let (thread_id, _, _) = seed_inbound_thread(&state, &account_id).await;

        let response = draft_compose(
            &state,
            Some(&account_id),
            None,
            "reply briefly",
            None,
            Some(thread_id),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(matches!(
            response,
            ResponseData::DraftSuggestion {
                voice_match: Some(_),
                ..
            }
        ));
        assert!(captured_prompt(&llm).contains("Customer prefers short pricing updates."));
    }

    // Grounding: a relevant prior sent message is included; unrelated inbound is not.
    #[cfg(feature = "local")]
    #[tokio::test]
    async fn reply_includes_relevant_prior_sent_mail_as_grounding() {
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
            .sender_address("Customer", "customer@example.com")
            .to(vec![CoreAddress {
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
            .sender_address("Me", "user@example.com")
            .recipient_address(Some("Customer"), "customer@example.com")
            .date(now - chrono::Duration::days(7))
            .snippet("I can hold the rollout note until numbers are firm.")
            .build();
        let prior_inbound = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(mxr_core::ThreadId::new())
            .provider_id("prior-inbound")
            .subject("Pricing rollout")
            .sender_address("Vendor", "vendor@example.com")
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

        let response = draft_compose(
            &state,
            Some(&account_id),
            None,
            "reply about pricing rollout",
            None,
            Some(reply_thread_id),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(matches!(response, ResponseData::DraftSuggestion { .. }));
        let prompt = captured_prompt(&llm);
        assert!(prompt.contains("I can hold the rollout note until the pricing numbers are firm"));
        assert!(!prompt.contains("External pricing notes should not shape my voice"));
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
}
