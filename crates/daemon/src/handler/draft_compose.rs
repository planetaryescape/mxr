//! Unified AI draft generation: one entry for new messages, replies from
//! compose (`source_message_id`), replies from the reader (`thread_id`) and
//! forwards (a `to` outside the conversation).
//!
//! The draft is written *as the user*: who they are, real emails they sent
//! (to this person first) and the habits those show, the conversation as
//! ME / THEM turns with the message to answer marked, and the tone and
//! length they asked for (`draft_voice` gathers, `draft_prompt` builds,
//! `draft_output` cleans). Never auto-sends.

use super::draft_prompt::{self, DraftMode, Me, PromptInput};
use super::draft_provenance::DraftPolicy;
use super::draft_voice::{self, Counterparty, VoiceMaterial};
use super::{draft_context, draft_output, relationship_profile, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use draft_context::DraftContext;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, Envelope};
use mxr_llm::{
    guarded_system_prompt, ChatMessage, CompletionRequest, LlmError, LlmFeature, PinnedLlm,
};
use mxr_protocol::{DraftLengthHintData, VoiceRegisterData};

/// Longest instruction plus recipient label accepted. The instruction is
/// never cut (it must reach the model whole), so it is bounded up front.
pub(crate) const MAX_INSTRUCTION_BYTES: usize = 4_000;

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
    let recipient_bytes = to.as_ref().map_or(0, |address| {
        address.email.len() + address.name.as_deref().map_or(0, str::len)
    });
    let request_bytes = recipient_bytes + instruction.len();
    if request_bytes > MAX_INSTRUCTION_BYTES {
        return Err(crate::handler::HandlerError::InvalidRequest(format!(
            "draft is too long: the instruction and recipient are {request_bytes} bytes, over the \
             {MAX_INSTRUCTION_BYTES} byte limit; shorten the instruction"
        )));
    }
    let request = DraftRequest {
        instruction,
        register,
        length_hint,
        before: None,
    };

    // Reply to an explicit thread (reader / quick-reply).
    if let Some(thread_id) = thread_id.as_ref() {
        let envelopes = state.store.get_thread_envelopes(thread_id).await?;
        if envelopes.is_empty() {
            return Err(format!("Thread {thread_id} has no messages to reply to").into());
        }
        let account = thread_account(account_id, &envelopes)?;
        let forward_to = to.filter(|address| !is_participant(&envelopes, &address.email));
        return draft_in_thread(state, &account, &request, envelopes, None, forward_to).await;
    }

    // From compose: resolve the source message to its thread. If it isn't
    // synced locally, fall through to new-message mode.
    if let Some(message_id) = source_message_id.as_ref() {
        let envelopes = draft_context::resolve_thread_envelopes(state, message_id).await;
        if !envelopes.is_empty() {
            let account = thread_account(account_id, &envelopes)?;
            // A recipient outside the conversation means a forward.
            let forward_to = to.filter(|address| !is_participant(&envelopes, &address.email));
            return draft_in_thread(
                state,
                &account,
                &request,
                envelopes,
                Some(message_id),
                forward_to,
            )
            .await;
        }
    }

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
    draft_new(state, account, to, &request).await
}

/// What the user asked for. `before` replays history (offline evaluation):
/// nothing on or after it is used.
pub(crate) struct DraftRequest<'a> {
    pub instruction: &'a str,
    pub register: Option<VoiceRegisterData>,
    pub length_hint: Option<DraftLengthHintData>,
    pub before: Option<DateTime<Utc>>,
}

/// The account a reply drafts as: the conversation's own. A request naming
/// a different account is refused, or the draft would take its voice and
/// its cited sources from one account for another account's conversation.
fn thread_account(
    requested: Option<&AccountId>,
    envelopes: &[Envelope],
) -> Result<AccountId, crate::handler::HandlerError> {
    let owner = &envelopes[0].account_id;
    match requested {
        Some(requested) if requested != owner => {
            Err(crate::handler::HandlerError::InvalidRequest(format!(
                "that conversation belongs to account {owner}, not {requested}; \
                 draft from its own account"
            )))
        }
        _ => Ok(owner.clone()),
    }
}

fn is_participant(envelopes: &[Envelope], email: &str) -> bool {
    envelopes.iter().any(|envelope| {
        std::iter::once(&envelope.from)
            .chain(envelope.to.iter())
            .chain(envelope.cc.iter())
            .any(|address| address.email.eq_ignore_ascii_case(email))
    })
}

/// A reply (or forward) in an existing conversation.
pub(crate) async fn draft_in_thread(
    state: &AppState,
    account_id: &AccountId,
    request: &DraftRequest<'_>,
    envelopes: Vec<Envelope>,
    source: Option<&MessageId>,
    forward_to: Option<Address>,
) -> HandlerResult {
    let me = draft_voice::me(state, account_id).await;
    let owned = draft_voice::owned(&me);
    let target = draft_voice::pick_target(&envelopes, &owned, source);
    let counterparty = match &forward_to {
        Some(address) => Some(Counterparty {
            email: address.email.to_ascii_lowercase(),
            name: address.name.clone(),
        }),
        None => target.and_then(|target| draft_voice::counterparty_of(target, &owned)),
    };
    let mode = match &forward_to {
        Some(_) => DraftMode::Forward {
            to: counterparty
                .as_ref()
                .map(Counterparty::label)
                .unwrap_or_default(),
        },
        None => DraftMode::Reply,
    };
    let thread_id = envelopes[0].thread_id.clone();
    let target_id = target.map(|envelope| envelope.id.clone());
    let turns = draft_voice::conversation(state, &envelopes, &owned, target_id.as_ref()).await;
    let feature = LlmFeature::DraftAssist;
    draft_with(
        state,
        account_id,
        request,
        feature,
        &me,
        mode,
        counterparty,
        turns,
        Some(&thread_id),
    )
    .await
}

async fn draft_new(
    state: &AppState,
    account_id: &AccountId,
    to: Address,
    request: &DraftRequest<'_>,
) -> HandlerResult {
    let me = draft_voice::me(state, account_id).await;
    let counterparty = Counterparty {
        email: to.email.to_ascii_lowercase(),
        name: to.name.clone(),
    };
    let mode = DraftMode::New {
        to: counterparty.label(),
    };
    draft_with(
        state,
        account_id,
        request,
        LlmFeature::DraftNew,
        &me,
        mode,
        Some(counterparty),
        Vec::new(),
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn draft_with(
    state: &AppState,
    account_id: &AccountId,
    request: &DraftRequest<'_>,
    feature: LlmFeature,
    me: &Me,
    mode: DraftMode,
    counterparty: Option<Counterparty>,
    turns: Vec<draft_prompt::Turn>,
    thread_id: Option<&ThreadId>,
) -> HandlerResult {
    let contact_emails: Vec<String> = counterparty
        .iter()
        .map(|person| person.email.clone())
        .collect();
    if request.before.is_none() {
        if let Some(email) = contact_emails.first() {
            draft_context::ensure_contact_fresh(state, account_id, email).await;
        }
    }
    // Tone/length inference, the voice-match baseline and the UI note.
    let context = draft_context::build_relationship_block(
        state,
        account_id,
        &contact_emails,
        request.instruction,
        request.register,
        request.length_hint,
    )
    .await;
    // The user's other mail only reaches a cloud model with their opt-in.
    let policy = DraftPolicy::pin(state, feature);
    let share = policy.share_history;
    let background = match contact_emails.first().filter(|_| share) {
        Some(email) => relationship_profile::load_relationship_profile(state, account_id, email)
            .await
            .ok()
            .flatten()
            .and_then(|profile| profile.summary.map(|summary| summary.text)),
        None => None,
    };
    let material = if share {
        draft_voice::voice_material(
            state,
            account_id,
            counterparty.as_ref(),
            thread_id,
            request.before,
        )
        .await
    } else {
        VoiceMaterial::none()
    };
    let target_words = target_words(request.length_hint, &material);
    let max_tokens = max_tokens_for_words(target_words);
    let budget_chars = prompt_budget_chars(state.llm.capabilities().context_window, max_tokens);
    let today = draft_voice::today();
    let prompt = draft_prompt::build(&PromptInput {
        mode,
        me,
        today: &today,
        habits: &material.habits,
        examples: &material.examples,
        background: background.as_deref(),
        turns: &turns,
        tone: request.register,
        target_words,
        instruction: request.instruction,
        budget_chars,
    });
    tracing::debug!(
        examples = prompt.examples_used,
        turns = prompt.turns_kept.len(),
        voice_samples = material.samples,
        target_words,
        "assembled draft prompt"
    );
    // Only the examples that fit the prompt shaped the draft; the rewrite
    // pass sees no others, so the sources stay the whole list.
    let examples_used = &material.examples[..prompt.examples_used];
    let voice_context = voice_context_for_rewrite(&material.habits, examples_used);
    let (body, model) = complete_with_retry(&policy.llm, &prompt, max_tokens).await?;
    let body_text = draft_output::clean_draft(&body, me.name.as_deref());
    let note = if share {
        context_note(&context, examples_used.len(), counterparty.as_ref())
    } else {
        Some(CLOUD_NOTE.to_string())
    };
    let history_used =
        !examples_used.is_empty() || !material.habits.is_empty() || prompt.background_used;
    let provenance = policy.provenance(
        &model,
        history_used,
        examples_used
            .iter()
            .map(|example| example.source.clone())
            .collect(),
        prompt
            .turns_kept
            .iter()
            .map(|index| turns[*index].source.clone())
            .collect(),
    );
    draft_context::finish_draft_suggestion(
        state,
        account_id,
        body_text,
        provenance,
        context.baseline,
        &voice_context,
        context.inferred_register,
        context.inferred_length,
        note,
    )
    .await
}

pub(crate) const CLOUD_NOTE: &str = "Your LLM is a cloud endpoint, so this draft saw only the \
conversation, not your other emails. Set llm.allow_cloud_relationship_data = true to draft in your voice.";

/// Words to aim for: the user's choice, else their median with this
/// person (or in general), else a short email.
pub(crate) fn target_words(
    length_hint: Option<DraftLengthHintData>,
    material: &VoiceMaterial,
) -> u32 {
    match length_hint {
        Some(DraftLengthHintData::Short) => 40,
        Some(DraftLengthHintData::Medium) => 90,
        Some(DraftLengthHintData::Long) => 180,
        None => material.median_words.unwrap_or(70).clamp(10, 250),
    }
}

/// Room for the draft plus a reasoning model's thinking.
fn max_tokens_for_words(words: u32) -> u32 {
    (words * 3 + 400).clamp(600, 2_000)
}

/// The prompt must leave room for the answer inside the model's context
/// window (about three characters per token for English mail).
fn prompt_budget_chars(context_window: u32, max_tokens: u32) -> usize {
    let tokens = context_window
        .saturating_sub(max_tokens)
        .saturating_sub(200) as usize;
    (tokens * 3).clamp(6_000, draft_context::ASSEMBLED_MESSAGE_BUDGET_CHARS)
}

/// The humanizer's rewrite pass must keep the voice, so it sees the same
/// habits and examples.
fn voice_context_for_rewrite(habits: &[String], examples: &[draft_prompt::VoiceExample]) -> String {
    let mut out = habits.join("\n");
    for example in examples.iter().take(3) {
        out.push_str("\n\nExample of my writing:\n");
        out.push_str(&example.my_email);
    }
    out
}

fn context_note(
    context: &DraftContext,
    examples: usize,
    counterparty: Option<&Counterparty>,
) -> Option<String> {
    match (counterparty, examples) {
        (Some(person), n) if n > 0 => Some(format!(
            "Written in your voice from {n} of your emails (to {} first)",
            person.email
        )),
        _ => context.context_note.clone(),
    }
}

/// Complete, and when the model stops for length, try once more with more
/// room: a cut-off email is worse than a slow one.
async fn complete_with_retry(
    llm: &PinnedLlm,
    prompt: &draft_prompt::Prompt,
    max_tokens: u32,
) -> Result<(String, String), crate::handler::HandlerError> {
    let mut budget = max_tokens;
    for attempt in 0..2 {
        let response = match llm
            .complete(CompletionRequest {
                messages: vec![
                    // Mail-derived blocks in the user message are wrapped in
                    // untrusted-content markers; the guard tells the model
                    // they are data. Output is a suggestion, never auto-sent.
                    ChatMessage::system(guarded_system_prompt(&prompt.system)),
                    ChatMessage::user(prompt.user.clone()),
                ],
                max_tokens: Some(budget),
                temperature: Some(0.5),
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
        let cut_off = response.finish_reason.as_deref() == Some("length");
        if !cut_off {
            return Ok((response.content, response.model));
        }
        if attempt == 0 {
            budget = (budget * 2).min(4_000);
            continue;
        }
        return Err(crate::handler::HandlerError::Message(format!(
            "The model stopped before finishing the draft ({budget} tokens). Try a shorter length, \
             or a model with less reasoning overhead."
        )));
    }
    unreachable!("the loop returns on its second attempt")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::test_fixtures::TestEnvelopeBuilder;
    use mxr_core::types::{MessageBody, MessageDirection, MessageMetadata};
    use mxr_llm::{CompletionResponse, LlmCapabilities, LlmProvider};
    use mxr_protocol::ResponseData;
    use mxr_store::{ContactRelationshipSummaryRecord, ContactStyleRecord};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct CapturingLlm {
        last_request: Mutex<Option<CompletionRequest>>,
        /// `None` is an on-machine endpoint.
        base_url: Option<&'static str>,
    }

    impl CapturingLlm {
        fn at(base_url: &'static str) -> Self {
            Self {
                base_url: Some(base_url),
                ..Self::default()
            }
        }
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

        fn base_url(&self) -> Option<&str> {
            self.base_url
        }
    }

    fn provenance(response: &ResponseData) -> &mxr_protocol::DraftProvenanceData {
        match response {
            ResponseData::DraftSuggestion {
                provenance: Some(provenance),
                ..
            } => provenance,
            other => panic!("expected a DraftSuggestion with provenance, got {other:?}"),
        }
    }

    fn ids(sources: &[mxr_protocol::DraftSourceData]) -> Vec<mxr_core::MessageId> {
        sources
            .iter()
            .map(|source| source.message_id.clone())
            .collect()
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

    /// A provider for one feature: answers each call with the next
    /// `(reply, answered_as)` (the last repeats), while its configured name
    /// is "configured-name", and counts its calls.
    struct ScriptedLlm {
        replies: Vec<(&'static str, &'static str)>,
        base_url: Option<&'static str>,
        calls: Mutex<u32>,
    }

    impl ScriptedLlm {
        fn new(
            reply: &'static str,
            answered_as: &'static str,
            base_url: Option<&'static str>,
        ) -> Self {
            Self::sequence(vec![(reply, answered_as)], base_url)
        }

        fn sequence(
            replies: Vec<(&'static str, &'static str)>,
            base_url: Option<&'static str>,
        ) -> Self {
            Self {
                replies,
                base_url,
                calls: Mutex::new(0),
            }
        }

        fn calls(&self) -> u32 {
            *self.calls.lock().expect("calls lock")
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for ScriptedLlm {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            let call = {
                let mut calls = self.calls.lock().expect("calls lock");
                *calls += 1;
                *calls as usize - 1
            };
            let (reply, answered_as) = self.replies[call.min(self.replies.len() - 1)];
            Ok(CompletionResponse {
                content: reply.to_string(),
                model: answered_as.to_string(),
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
            "configured-name"
        }

        fn base_url(&self) -> Option<&str> {
            self.base_url
        }
    }

    /// Scores under the humanizer threshold, so a rewrite pass is due.
    const MACHINE_MADE: &str = "Additionally, this serves as a testament, not just showcasing our \
        commitment to innovation, but fostering a vibrant tapestry of ideas.";

    /// Draft a reply from the user's history with a local drafting model and
    /// `rewriter` serving only the humanizer's rewrite pass.
    async fn draft_with_rewriter(rewriter: Arc<ScriptedLlm>) -> ResponseData {
        draft_from_history(rewriter).await.2
    }

    /// As [`draft_with_rewriter`], keeping the state and account.
    async fn draft_from_history(
        rewriter: Arc<ScriptedLlm>,
    ) -> (AppState, mxr_core::id::AccountId, ResponseData) {
        let state = AppState::in_memory().await.unwrap();
        let drafter = Arc::new(ScriptedLlm::new(MACHINE_MADE, "drafter-7b", None));
        state.llm.replace(drafter);
        let mut providers: std::collections::HashMap<LlmFeature, Arc<dyn LlmProvider>> =
            std::collections::HashMap::new();
        providers.insert(LlmFeature::HumanizeRewrite, rewriter);
        state
            .llm
            .replace_feature_providers(providers, std::collections::HashMap::new());
        let account_id = state.default_account_id();
        seed_private_reply(&state, &account_id).await;
        let (thread_id, _, _) = seed_inbound_thread(&state, &account_id).await;
        let response = draft_compose(
            &state,
            Some(&account_id),
            None,
            "",
            None,
            Some(thread_id),
            None,
            None,
        )
        .await
        .unwrap();
        (state, account_id, response)
    }

    // Credit: the kept rewrite is credited to the model that wrote it, and
    // a later pass whose answer was dropped is named on its own.
    #[tokio::test]
    async fn a_kept_rewrite_is_credited_to_its_writer_and_a_later_dropped_pass_named() {
        let rewriter = Arc::new(ScriptedLlm::sequence(
            vec![
                // Better than the draft (64 to 87), still under the bar.
                (
                    "Additionally, this serves as a testament to our work.",
                    "first-pass",
                ),
                // No better: dropped.
                (MACHINE_MADE, "second-pass"),
            ],
            None,
        ));
        let state = AppState::in_memory().await.unwrap();
        let mut config = state.config_snapshot();
        config.humanizer.score_threshold = 99;
        config.humanizer.max_rewrite_iterations = 2;
        state.set_config_for_test(config).await;
        state
            .llm
            .replace(Arc::new(ScriptedLlm::new(MACHINE_MADE, "drafter-7b", None)));
        let mut providers: std::collections::HashMap<LlmFeature, Arc<dyn LlmProvider>> =
            std::collections::HashMap::new();
        providers.insert(LlmFeature::HumanizeRewrite, rewriter.clone());
        state
            .llm
            .replace_feature_providers(providers, std::collections::HashMap::new());
        let account_id = state.default_account_id();
        let (thread_id, _, _) = seed_inbound_thread(&state, &account_id).await;
        let response = draft_compose(
            &state,
            Some(&account_id),
            None,
            "",
            None,
            Some(thread_id),
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(rewriter.calls(), 2);
        let rewrite = provenance(&response).rewrite.clone().expect("disclosed");
        assert_eq!(
            rewrite.outcome,
            mxr_protocol::DraftRewriteOutcomeData::Applied
        );
        assert_eq!(rewrite.model, "first-pass");
        assert_eq!(rewrite.rejected_by.as_deref(), Some("second-pass"));
        assert!(provenance(&response).summary_line().ends_with(
            "rewritten by local model first-pass, a later pass by second-pass not used"
        ));
    }

    // Privacy: text an earlier draft wrote from history can't be refined by
    // a cloud model without opt-in; a body the user wrote can.
    #[tokio::test]
    async fn a_cloud_refine_of_history_text_is_refused_and_of_my_own_text_allowed() {
        let local_rewriter = Arc::new(ScriptedLlm::new(MACHINE_MADE, "rewriter", None));
        let (state, account_id, response) = draft_from_history(local_rewriter).await;
        assert!(provenance(&response).history_used);
        let suggestion = match response {
            ResponseData::DraftSuggestion { body, .. } => body,
            other => panic!("{other:?}"),
        };
        let cloud = Arc::new(ScriptedLlm::new(
            "Refined.",
            "cloud-refiner",
            Some("https://api.example-cloud.com/v1"),
        ));
        let mut providers: std::collections::HashMap<LlmFeature, Arc<dyn LlmProvider>> =
            std::collections::HashMap::new();
        providers.insert(LlmFeature::DraftRefine, cloud.clone());
        state
            .llm
            .replace_feature_providers(providers, std::collections::HashMap::new());
        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id: account_id.clone(),
            from: None,
            reply_headers: None,
            intent: mxr_core::types::DraftIntent::Reply,
            to: vec![Address {
                name: None,
                email: "customer@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "re".into(),
            content: mxr_core::types::DraftContent::markdown("placeholder"),
            inline_assets: Vec::new(),
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        state.store.insert_draft(&draft).await.unwrap();
        let knobs = mxr_protocol::DraftRefineKnobsData::default();

        let edited = format!("Hi,\n\n{suggestion}\n\nThanks");
        let error = crate::handler::draft_refine::draft_refine(
            &state,
            &draft.id,
            knobs.clone(),
            Some(&edited),
        )
        .await
        .expect_err("history text never goes to the cloud refiner");
        assert!(
            matches!(&error, crate::handler::HandlerError::InvalidRequest(text)
                if text == crate::handler::draft_refine::HISTORY_BODY_REFUSAL),
            "{error:?}"
        );
        assert_eq!(cloud.calls(), 0);

        // Nor through the standalone humanizer's cloud rewrite.
        let mut providers: std::collections::HashMap<LlmFeature, Arc<dyn LlmProvider>> =
            std::collections::HashMap::new();
        providers.insert(LlmFeature::DraftRefine, cloud.clone());
        providers.insert(LlmFeature::HumanizeRewrite, cloud.clone());
        state
            .llm
            .replace_feature_providers(providers, std::collections::HashMap::new());
        let error = crate::handler::humanizer::rewrite_text(&state, &edited, None)
            .await
            .expect_err("history text never goes to the cloud rewriter");
        assert!(
            matches!(&error, crate::handler::HandlerError::InvalidRequest(text)
                if text == crate::handler::draft_refine::HISTORY_BODY_REFUSAL),
            "{error:?}"
        );
        assert_eq!(cloud.calls(), 0);

        let mine = "Can we move our call to Thursday afternoon instead?";
        let response =
            crate::handler::draft_refine::draft_refine(&state, &draft.id, knobs, Some(mine))
                .await
                .expect("the user's own words may go");
        assert_eq!(cloud.calls(), 1);
        let disclosed = provenance(&response);
        assert_eq!(disclosed.locality, mxr_protocol::AiLocalityData::Cloud);
        assert!(!disclosed.history_used);

        // A local humanize of that history text is history text too: its
        // output is remembered for the same account.
        let rewritten = "Friday works for me and I will send the notes by noon.";
        let mut providers: std::collections::HashMap<LlmFeature, Arc<dyn LlmProvider>> =
            std::collections::HashMap::new();
        providers.insert(
            LlmFeature::HumanizeRewrite,
            Arc::new(ScriptedLlm::new(rewritten, "local-rewriter", None)),
        );
        state
            .llm
            .replace_feature_providers(providers, std::collections::HashMap::new());
        crate::handler::humanizer::rewrite_text(&state, &edited, None)
            .await
            .expect("a local model may rewrite it");
        assert_eq!(
            crate::history_text::source_accounts(&state.store, None, rewritten)
                .await
                .unwrap(),
            vec![account_id]
        );
    }

    // Privacy: a draft written from the user's history never goes to a cloud
    // rewrite model they haven't opted in to; the draft says it was skipped.
    #[tokio::test]
    async fn a_history_draft_skips_a_cloud_rewrite_and_says_so() {
        let rewriter = Arc::new(ScriptedLlm::new(
            "Friday works.",
            "cloud-rewriter",
            Some("https://api.example-cloud.com/v1"),
        ));
        let response = draft_with_rewriter(rewriter.clone()).await;
        assert_eq!(rewriter.calls(), 0, "the cloud rewrite never ran");
        let disclosed = provenance(&response);
        assert!(disclosed.history_used);
        let rewrite = disclosed.rewrite.as_ref().expect("the skip is disclosed");
        assert_eq!(
            rewrite.outcome,
            mxr_protocol::DraftRewriteOutcomeData::Skipped
        );
        assert_eq!(rewrite.locality, mxr_protocol::AiLocalityData::Cloud);
        assert!(disclosed
            .summary_line()
            .ends_with("skipped to keep your history local"));
    }

    // Truthful: the rewrite names the model that answered, and a rewrite
    // whose text was dropped is still disclosed, as not used.
    #[tokio::test]
    async fn a_rewrite_names_who_answered_and_whether_its_text_was_used() {
        let used = Arc::new(ScriptedLlm::new(
            "Friday works for me.",
            "rewriter-actual",
            None,
        ));
        let response = draft_with_rewriter(used.clone()).await;
        assert!(used.calls() > 0);
        let rewrite = provenance(&response).rewrite.clone().expect("disclosed");
        assert_eq!(
            rewrite.outcome,
            mxr_protocol::DraftRewriteOutcomeData::Applied
        );
        assert_eq!(rewrite.model, "rewriter-actual");
        assert!(rewrite.history_used, "a local rewrite keeps the voice");
        match &response {
            ResponseData::DraftSuggestion { body, .. } => assert_eq!(body, "Friday works for me."),
            other => panic!("{other:?}"),
        }

        // No better than the draft: the call happened, its text didn't land.
        let dropped = Arc::new(ScriptedLlm::new(MACHINE_MADE, "rewriter-actual", None));
        let response = draft_with_rewriter(dropped.clone()).await;
        assert!(dropped.calls() > 0);
        let rewrite = provenance(&response).rewrite.clone().expect("disclosed");
        assert_eq!(
            rewrite.outcome,
            mxr_protocol::DraftRewriteOutcomeData::Rejected
        );
        assert_eq!(rewrite.model, "rewriter-actual");
        assert!(provenance(&response)
            .summary_line()
            .ends_with("rewrite attempted by local model rewriter-actual, not used"));
    }

    // Account scope: a conversation from another account is refused, by
    // thread or by source message, before any model call, so one account's
    // voice and sources never dress another account's reply.
    #[tokio::test]
    async fn a_conversation_from_another_account_is_refused() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let mine = state.default_account_id();
        let other = mxr_core::AccountId::new();
        state
            .store
            .insert_account(&crate::test_fixtures::test_account_with_id(other.clone()))
            .await
            .unwrap();
        let (thread_id, message_id, _) = seed_inbound_thread(&state, &other).await;

        for (source, thread) in [(None, Some(thread_id.clone())), (Some(message_id), None)] {
            let error = draft_compose(&state, Some(&mine), None, "", source, thread, None, None)
                .await
                .expect_err("another account's conversation is refused");
            assert!(
                matches!(error, crate::handler::HandlerError::InvalidRequest(ref text)
                    if text.contains("belongs to account")),
                "{error:?}"
            );
        }
        assert!(llm.last_request.lock().unwrap().is_none(), "no model call");

        // Its own account drafts it.
        draft_compose(
            &state,
            Some(&other),
            None,
            "",
            None,
            Some(thread_id),
            None,
            None,
        )
        .await
        .expect("the owning account drafts");
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

    // An oversized instruction is rejected up front: it is never cut, so it
    // can't be allowed to crowd out the conversation. The rejection carries
    // an explicit InvalidRequest wire kind.
    #[tokio::test]
    async fn oversized_instruction_is_rejected_as_invalid_request() {
        let state = AppState::in_memory().await.unwrap();
        let account_id = state.default_account_id();
        let huge = "x".repeat(MAX_INSTRUCTION_BYTES + 1);
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
        // Assert the WIRE kind, not just the message text.
        match err.into_response() {
            mxr_protocol::Response::Error { kind, message, .. } => {
                assert_eq!(kind, mxr_protocol::IpcErrorKind::InvalidRequest);
                assert!(message.contains("too long"), "message: {message}");
            }
            other => panic!("expected an error response, got {other:?}"),
        }
    }

    // The recipient's bytes count too: a long display name plus an
    // instruction that alone would fit is still rejected.
    #[tokio::test]
    async fn long_recipient_plus_maximal_instruction_is_rejected() {
        let state = AppState::in_memory().await.unwrap();
        let account_id = state.default_account_id();
        // Fills the limit alongside a bare "a@b.com"; the name pushes it over.
        let instruction = "x".repeat(MAX_INSTRUCTION_BYTES - "a@b.com".len());
        let to = Address {
            name: Some("N".repeat(300)),
            email: "a@b.com".to_string(),
        };
        let err = draft_compose(
            &state,
            Some(&account_id),
            Some(to),
            &instruction,
            None,
            None,
            None,
            None,
        )
        .await
        .expect_err("long recipient must push the task line over the limit");
        match err.into_response() {
            mxr_protocol::Response::Error { kind, .. } => {
                assert_eq!(kind, mxr_protocol::IpcErrorKind::InvalidRequest);
            }
            other => panic!("expected an error response, got {other:?}"),
        }
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

    // Voice: the prompt carries a real reply the user sent this person,
    // with what they were answering; unrelated inbound mail stays out, and
    // the conversation is labelled with the message to answer marked.
    #[tokio::test]
    async fn reply_uses_my_real_replies_to_this_person_as_examples() {
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CapturingLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        let now = chrono::Utc::now();
        let me = state
            .store
            .list_account_addresses(&account_id)
            .await
            .unwrap()
            .into_iter()
            .next()
            .map_or_else(|| "user@example.com".into(), |address| address.email);

        async fn seed(
            state: &AppState,
            envelope: &Envelope,
            direction: MessageDirection,
            text: &str,
        ) {
            state
                .store
                .upsert_envelope_with_direction(envelope, direction)
                .await
                .unwrap();
            state
                .store
                .insert_body(&body(envelope.id.clone(), text))
                .await
                .unwrap();
        }

        // An earlier exchange: the customer asked, I answered.
        let mut asked = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(mxr_core::ThreadId::new())
            .provider_id("earlier-ask")
            .sender_address("Customer", "customer@example.com")
            .date(now - chrono::Duration::days(9))
            .build();
        asked.message_id_header = Some("<ask@example.com>".into());
        seed(
            &state,
            &asked,
            MessageDirection::Inbound,
            "Could you send the invoice again?",
        )
        .await;
        let mut answered = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(asked.thread_id.clone())
            .provider_id("earlier-answer")
            .sender_address("Sam Rivers", &me)
            .recipient_address(Some("Customer"), "customer@example.com")
            .date(now - chrono::Duration::days(8))
            .build();
        answered.in_reply_to = Some("<ask@example.com>".into());
        seed(
            &state,
            &answered,
            MessageDirection::Outbound,
            "yep, resent just now. shout if it's missing\ns",
        )
        .await;
        state
            .store
            .try_create_reply_pair(&answered, MessageDirection::Outbound)
            .await
            .unwrap();
        // Someone else's mail must not shape my voice.
        let unrelated = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(mxr_core::ThreadId::new())
            .provider_id("unrelated")
            .sender_address("Vendor", "vendor@example.com")
            .date(now - chrono::Duration::days(6))
            .build();
        seed(
            &state,
            &unrelated,
            MessageDirection::Inbound,
            "External pricing notes should not shape my voice.",
        )
        .await;

        let (thread_id, current_id, inbound_text) = seed_inbound_thread(&state, &account_id).await;
        let response = draft_compose(
            &state,
            Some(&account_id),
            None,
            "say Friday works",
            None,
            Some(thread_id.clone()),
            None,
            None,
        )
        .await
        .unwrap();

        // The disclosure cites exactly the reply the prompt carried, and the
        // conversation it read; the unrelated mail is in neither.
        let provenance = provenance(&response);
        assert_eq!(provenance.model, "test-llm");
        assert_eq!(provenance.locality, mxr_protocol::AiLocalityData::Local);
        assert!(provenance.history_used);
        assert_eq!(ids(&provenance.voice_examples), vec![answered.id.clone()]);
        let example = &provenance.voice_examples[0];
        assert!(example.from_me);
        assert_eq!(example.person, "customer@example.com");
        assert_eq!(example.thread_id, answered.thread_id);
        // The store keeps whole seconds.
        assert_eq!(example.date.timestamp(), answered.date.timestamp());
        assert_eq!(ids(&provenance.conversation), vec![current_id]);
        assert_eq!(provenance.conversation[0].thread_id, thread_id);
        assert!(!provenance.conversation[0].from_me);
        assert!(provenance.rewrite.is_none());
        assert_eq!(
            provenance.summary_line(),
            "Local model test-llm · used 1 of your emails to Customer · history used"
        );

        let request = llm.last_request.lock().unwrap().clone().unwrap();
        let system = &request.messages[0].content;
        let user = &request.messages[1].content;
        assert!(
            system.contains("ghostwriting a reply to the message marked TARGET as Sam Rivers"),
            "{system}"
        );
        assert!(
            user.contains("They wrote:\nCould you send the invoice again?\nI replied:\nyep, resent just now. shout if it's missing\ns"),
            "{user}"
        );
        assert!(
            user.contains(&format!("<<< TARGET ---\n{inbound_text}")),
            "{user}"
        );
        assert!(
            user.contains("What I want to say: say Friday works"),
            "{user}"
        );
        assert!(!user.contains("External pricing notes"), "{user}");
    }

    // Privacy: a cloud model without the user's opt-in sees the conversation
    // being drafted, never their other emails or the relationship summary.
    #[tokio::test]
    async fn a_cloud_model_without_opt_in_never_sees_my_other_mail() {
        let state = AppState::in_memory().await.unwrap();
        let mut config = state.config_snapshot();
        config.llm.enabled = true;
        config.llm.base_url = "https://api.example-cloud.com/v1".into();
        config.llm.allow_cloud_relationship_data = false;
        state.set_config_for_test(config).await;
        let llm = Arc::new(CapturingLlm::at("https://api.example-cloud.com/v1"));
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        seed_contact(&state, &account_id, "customer@example.com", 0.2, 5).await;

        seed_private_reply(&state, &account_id).await;

        let (thread_id, current_id, inbound_text) = seed_inbound_thread(&state, &account_id).await;
        let response = draft_compose(
            &state,
            Some(&account_id),
            None,
            "",
            None,
            Some(thread_id),
            None,
            None,
        )
        .await
        .unwrap();
        let prompt = captured_prompt(&llm);
        assert!(
            prompt.contains(inbound_text),
            "the conversation itself still goes"
        );
        assert!(!prompt.contains("PRIVATE-OLD-REPLY"), "{prompt}");
        assert!(
            !prompt.contains("Customer prefers short pricing updates."),
            "{prompt}"
        );
        // It says so: a cloud model, no history, no voice sources; only the
        // conversation it did read.
        let disclosed = provenance(&response);
        assert_eq!(disclosed.locality, mxr_protocol::AiLocalityData::Cloud);
        assert!(!disclosed.history_used);
        assert!(disclosed.voice_examples.is_empty());
        assert_eq!(ids(&disclosed.conversation), vec![current_id]);
        assert_eq!(
            disclosed.summary_line(),
            "Cloud model test-llm · history not used"
        );
        let (_, note) = draft_suggestion(response);
        assert_eq!(note.as_deref(), Some(CLOUD_NOTE));
    }

    // Pinning: the config still says the model is on this machine, but the
    // provider serving the request is a cloud one (a reload landed). What
    // the prompt carries and what the draft discloses follow the provider
    // that answered, not the config.
    #[tokio::test]
    async fn the_disclosure_follows_the_provider_that_answered_not_the_config() {
        let state = AppState::in_memory().await.unwrap();
        let mut config = state.config_snapshot();
        config.llm.enabled = true;
        config.llm.base_url = "http://localhost:11434/v1".into();
        config.llm.allow_cloud_relationship_data = false;
        state.set_config_for_test(config).await;
        let llm = Arc::new(CapturingLlm::at("https://api.example-cloud.com/v1"));
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        seed_private_reply(&state, &account_id).await;
        let (thread_id, _, _) = seed_inbound_thread(&state, &account_id).await;

        let response = draft_compose(
            &state,
            Some(&account_id),
            None,
            "",
            None,
            Some(thread_id),
            None,
            None,
        )
        .await
        .unwrap();

        assert!(!captured_prompt(&llm).contains("PRIVATE-OLD-REPLY"));
        let disclosed = provenance(&response);
        assert_eq!(disclosed.locality, mxr_protocol::AiLocalityData::Cloud);
        assert!(!disclosed.history_used);
        assert!(disclosed.voice_examples.is_empty());
    }

    /// An earlier exchange with the customer whose reply must stay private
    /// from a cloud model without opt-in.
    async fn seed_private_reply(state: &AppState, account_id: &mxr_core::id::AccountId) {
        let mut asked = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(mxr_core::ThreadId::new())
            .provider_id("old-ask")
            .sender_address("Customer", "customer@example.com")
            .date(chrono::Utc::now() - chrono::Duration::days(9))
            .build();
        asked.message_id_header = Some("<old-ask@x>".into());
        state
            .store
            .upsert_envelope_with_direction(&asked, MessageDirection::Inbound)
            .await
            .unwrap();
        state
            .store
            .insert_body(&body(asked.id.clone(), "Old question?"))
            .await
            .unwrap();
        let mut answered = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(asked.thread_id.clone())
            .provider_id("old-answer")
            .sender_address("Sam Rivers", "user@example.com")
            .recipient_address(Some("Customer"), "customer@example.com")
            .date(chrono::Utc::now() - chrono::Duration::days(8))
            .build();
        answered.in_reply_to = Some("<old-ask@x>".into());
        state
            .store
            .upsert_envelope_with_direction(&answered, MessageDirection::Outbound)
            .await
            .unwrap();
        state
            .store
            .insert_body(&body(
                answered.id.clone(),
                "PRIVATE-OLD-REPLY yes of course",
            ))
            .await
            .unwrap();
        state
            .store
            .try_create_reply_pair(&answered, MessageDirection::Outbound)
            .await
            .unwrap();
    }

    // Output: chatter the model adds around the email never reaches compose.
    #[tokio::test]
    async fn draft_output_is_cleaned_of_model_chatter() {
        struct ChattyLlm;
        #[async_trait::async_trait]
        impl LlmProvider for ChattyLlm {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                Ok(CompletionResponse {
                    content: "Here's a draft reply:\n\nSubject: Re: Pricing\n\nFriday works.\n\nLet me know if you'd like changes!".into(),
                    model: "test-llm".into(),
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
                "test-llm"
            }
        }
        let state = AppState::in_memory().await.unwrap();
        let mut config = state.config_snapshot();
        config.humanizer.apply_to_drafts = false;
        state.set_config_for_test(config).await;
        // After the config: applying it rebuilds the LLM runtime.
        state.llm.replace(Arc::new(ChattyLlm));
        let account_id = state.default_account_id();
        let (thread_id, _, _) = seed_inbound_thread(&state, &account_id).await;
        let response = draft_compose(
            &state,
            Some(&account_id),
            None,
            "",
            None,
            Some(thread_id),
            None,
            None,
        )
        .await
        .unwrap();
        match response {
            ResponseData::DraftSuggestion { body, .. } => assert_eq!(body, "Friday works."),
            other => panic!("expected a draft, got {other:?}"),
        }
    }

    // A draft cut off by the token limit is retried with more room, then
    // reported rather than handed over half-written.
    #[tokio::test]
    async fn a_draft_cut_off_twice_is_an_error_not_half_an_email() {
        #[derive(Default)]
        struct CutOffLlm {
            budgets: Mutex<Vec<Option<u32>>>,
        }
        #[async_trait::async_trait]
        impl LlmProvider for CutOffLlm {
            async fn complete(
                &self,
                req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                // The contact-profile refresh asks for a summary too; count drafts.
                if req.messages[0].content.contains("ghostwriting") {
                    self.budgets.lock().unwrap().push(req.max_tokens);
                }
                Ok(CompletionResponse {
                    content: "Friday works, and the".into(),
                    model: "test-llm".into(),
                    finish_reason: Some("length".into()),
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
        let state = AppState::in_memory().await.unwrap();
        let llm = Arc::new(CutOffLlm::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        let (thread_id, _, _) = seed_inbound_thread(&state, &account_id).await;
        let error = draft_compose(
            &state,
            Some(&account_id),
            None,
            "",
            None,
            Some(thread_id),
            None,
            None,
        )
        .await
        .expect_err("a twice cut-off draft is an error");
        assert!(format!("{error:?}").contains("stopped before finishing"));
        let budgets = llm.budgets.lock().unwrap().clone();
        assert_eq!(budgets.len(), 2);
        assert!(
            budgets[1] > budgets[0],
            "the retry gets more room: {budgets:?}"
        );
    }
}
