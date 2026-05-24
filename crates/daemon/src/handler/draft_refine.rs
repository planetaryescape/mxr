use super::draft_new::{finish_draft_suggestion, voice_context_for_recipient};
use super::HandlerResult;
use crate::state::AppState;
use mxr_humanizer::writing_constraints;
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature};
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
    let context = voice_context_for_recipient(
        state,
        &draft.account_id,
        &recipient.email,
        knobs.add_context.as_deref().unwrap_or("refine draft"),
        None,
    )
    .await?;
    let mut prompt = String::new();
    if !context.prompt.is_empty() {
        prompt.push_str("[VOICE CONTEXT]\n");
        prompt.push_str(&context.prompt);
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
                ChatMessage::system(SYSTEM_PROMPT),
                ChatMessage::user(prompt),
            ],
            max_tokens: Some(700),
            temperature: Some(0.35),
        })
        .await
    {
        Ok(response) => response,
        Err(LlmError::Disabled) => return Err(crate::handler::HandlerError::Message("LLM is disabled. Enable it in [llm].".to_string())),
        Err(error) => return Err(format!("LLM error: {error}").into()),
    };
    finish_draft_suggestion(
        state,
        response.content.trim().to_string(),
        response.model,
        context.baseline,
        Some(context.prompt.as_str()),
    )
    .await
}
