use super::{draft_context, HandlerResult};
use crate::state::AppState;
use mxr_humanizer::writing_constraints;
use mxr_llm::{
    guarded_system_prompt, wrap_untrusted_mail, ChatMessage, CompletionRequest, LlmError,
    LlmFeature,
};
use mxr_protocol::DraftRefineKnobsData;

const SYSTEM_PROMPT: &str = "You refine an email draft. Preserve meaning, facts, and recipient-specific voice. Return only the revised draft body.";

pub(super) async fn draft_refine(
    state: &AppState,
    draft_id: &mxr_core::id::DraftId,
    knobs: DraftRefineKnobsData,
) -> HandlerResult {
    let draft = state
        .store
        .get_draft(draft_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Draft {draft_id} not found"))?;
    let recipient = draft
        .to
        .first()
        .ok_or_else(|| "Draft has no recipient to refine against".to_string())?;
    let context = draft_context::build_relationship_block(
        state,
        &draft.account_id,
        std::slice::from_ref(&recipient.email),
        knobs.add_context.as_deref().unwrap_or("refine draft"),
        None,
        None,
        draft_context::RELATIONSHIP_BUDGET_CHARS,
    )
    .await;
    let mut prompt = String::new();
    if !context.prompt.is_empty() {
        // The voice context is derived from stored mail (relationship
        // summary/stylometry); delimit it as untrusted content. The draft
        // itself is the user's own text to refine and stays unwrapped.
        prompt.push_str("[VOICE CONTEXT]\n");
        prompt.push_str(&wrap_untrusted_mail(&context.prompt));
        prompt.push_str("\n\n");
    }
    prompt.push_str("[WRITING CONSTRAINTS]\n");
    prompt.push_str(writing_constraints());
    prompt.push_str("\n\n");
    prompt.push_str("[REFINEMENT]\n");
    let mut any = false;
    if knobs.shorter {
        prompt.push_str("- Make it shorter.\n");
        any = true;
    }
    if knobs.warmer {
        prompt.push_str("- Make it warmer without adding fake familiarity.\n");
        any = true;
    }
    if knobs.more_formal {
        prompt.push_str("- Make it more formal.\n");
        any = true;
    }
    if knobs.less_emoji {
        prompt.push_str("- Use fewer emoji.\n");
        any = true;
    }
    if let Some(add_context) = knobs
        .add_context
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        prompt.push_str("- Add this context: ");
        prompt.push_str(add_context.trim());
        prompt.push('\n');
        any = true;
    }
    if !any {
        prompt.push_str("- Improve clarity while preserving style.\n");
    }
    prompt.push_str("\n[DRAFT]\n");
    prompt.push_str(&draft.body_markdown);

    let response = match state
        .llm
        .for_feature(LlmFeature::DraftRefine)
        .complete(CompletionRequest {
            messages: vec![
                ChatMessage::system(guarded_system_prompt(SYSTEM_PROMPT)),
                ChatMessage::user(prompt),
            ],
            max_tokens: Some(700),
            temperature: Some(0.35),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use mxr_core::types::{Address, Draft, DraftIntent};
    use mxr_llm::{CompletionResponse, LlmCapabilities, LlmProvider};
    use mxr_store::ContactRelationshipSummaryRecord;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct CaptureLlm {
        // Every completion the runtime makes (refine, and any humanizer
        // rewrite pass) so the assertion is robust to config.
        calls: Mutex<Vec<Vec<ChatMessage>>>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for CaptureLlm {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            self.calls.lock().unwrap().push(req.messages.clone());
            Ok(CompletionResponse {
                content: "Refined draft.".into(),
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

    #[tokio::test]
    async fn refine_prompt_guards_system_and_wraps_voice_context() {
        let state = AppState::in_memory().await.unwrap();
        let account_id = state.default_account_id();
        let cap = Arc::new(CaptureLlm::default());
        state.llm.replace(cap.clone());

        // Seed a relationship summary so the voice-context block is
        // populated with mail-derived text.
        state
            .store
            .upsert_contact_relationship_summary(&ContactRelationshipSummaryRecord {
                account_id: account_id.clone(),
                email: "customer@example.com".to_string(),
                text: "VOICE-CONTEXT-MARKER prefers terse pricing updates.".to_string(),
                model: "test".to_string(),
                known_topics: vec!["pricing".to_string()],
                computed_at: chrono::Utc::now(),
                source_hash: "rel-v1".to_string(),
                last_error: None,
            })
            .await
            .unwrap();

        let draft = Draft {
            id: mxr_core::DraftId::new(),
            account_id: account_id.clone(),
            reply_headers: None,
            intent: DraftIntent::Reply,
            to: vec![Address {
                name: None,
                email: "customer@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "re".into(),
            body_markdown: "draft body".into(),
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        state.store.insert_draft(&draft).await.unwrap();

        draft_refine(&state, &draft.id, DraftRefineKnobsData::default())
            .await
            .unwrap();

        // Find the refine call (its system message carries the guard).
        let calls = cap.calls.lock().unwrap();
        let refine = calls
            .iter()
            .find(|m| m[0].content.contains(mxr_llm::UNTRUSTED_MAIL_GUARD))
            .expect("a call whose system prompt carries the guard");
        let user = &refine[1].content;
        let begin = user
            .find(mxr_llm::UNTRUSTED_MAIL_BEGIN)
            .expect("begin marker present");
        let end = user
            .find(mxr_llm::UNTRUSTED_MAIL_END)
            .expect("end marker present");
        let marker = user
            .find("VOICE-CONTEXT-MARKER")
            .expect("voice context present");
        assert!(
            begin < marker && marker < end,
            "voice context must sit between the untrusted-content markers"
        );
        // The draft being refined is the user's own transform target and
        // must stay OUTSIDE the markers (it appears in the [DRAFT] section
        // after the wrapped voice context).
        let draft_pos = user.find("draft body").expect("draft body present");
        assert!(
            draft_pos > end,
            "the draft to refine must stay outside the untrusted-content markers"
        );
    }
}
