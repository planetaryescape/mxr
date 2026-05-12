use super::{relationship_profile, HandlerResult};
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::types::Address;
use mxr_humanizer::{score as humanizer_score, HumanizerOpts};
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature};
use mxr_protocol::{
    DraftLengthHintData, HumanizerReportSummaryData, ResponseData, VoiceMatchConfidenceData,
    VoiceMatchData, VoiceRegisterData,
};
use mxr_relationship::stylometry::StylometryMetrics;
use mxr_relationship::{compute_metrics, infer_register, score_voice_match, VoiceMatchConfidence};

const SYSTEM_PROMPT: &str = "You draft email for a busy professional. Produce only the email body, with no subject line and no signature. Be direct, specific, and plain-spoken. Never invent prior familiarity or facts that are not provided.";

pub(super) async fn draft_new(
    state: &AppState,
    account_id: &AccountId,
    to: Address,
    purpose: &str,
    register: Option<VoiceRegisterData>,
    length_hint: Option<DraftLengthHintData>,
) -> HandlerResult {
    let context =
        voice_context_for_recipient(state, account_id, &to.email, purpose, register).await?;
    let mut prompt = String::new();
    if !context.prompt.is_empty() {
        prompt.push_str("[VOICE CONTEXT]\n");
        prompt.push_str(&context.prompt);
        prompt.push_str("\n\n");
    }
    prompt.push_str("[TASK]\nWrite a new email");
    if let Some(name) = to.name.as_deref().filter(|name| !name.trim().is_empty()) {
        prompt.push_str(&format!(" to {name} <{}>", to.email));
    } else {
        prompt.push_str(&format!(" to {}", to.email));
    }
    if let Some(length) = length_hint {
        prompt.push_str(&format!(". Length: {}", length_label(length)));
    }
    prompt.push_str(". Purpose: ");
    prompt.push_str(purpose.trim());

    let response = match state
        .llm
        .for_feature(LlmFeature::DraftNew)
        .complete(CompletionRequest {
            messages: vec![
                ChatMessage::system(SYSTEM_PROMPT),
                ChatMessage::user(prompt),
            ],
            max_tokens: Some(match length_hint.unwrap_or(DraftLengthHintData::Medium) {
                DraftLengthHintData::Short => 220,
                DraftLengthHintData::Medium => 450,
                DraftLengthHintData::Long => 750,
            }),
            temperature: Some(0.4),
        })
        .await
    {
        Ok(response) => response,
        Err(LlmError::Disabled) => return Err("LLM is disabled. Enable it in [llm].".to_string()),
        Err(error) => return Err(format!("LLM error: {error}")),
    };
    finish_draft_suggestion(
        state,
        response.content.trim().to_string(),
        response.model,
        context.baseline,
    )
    .await
}

pub(crate) struct DraftVoiceContext {
    pub prompt: String,
    pub baseline: Option<(StylometryMetrics, u32)>,
}

pub(crate) async fn voice_context_for_recipient(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
    purpose: &str,
    register: Option<VoiceRegisterData>,
) -> Result<DraftVoiceContext, String> {
    if let Some(profile) =
        relationship_profile::load_relationship_profile(state, account_id, email).await?
    {
        let mut prompt = String::from("This is weak background guidance. The task overrides it. Anything not in known topics is unknown; do not invent familiarity.\n");
        if let Some(summary) = profile.summary {
            prompt.push_str(&format!("- Relationship: {}\n", summary.text.trim()));
            if !summary.known_topics.is_empty() {
                prompt.push_str(&format!(
                    "- Known topics: {}\n",
                    summary.known_topics.join(", ")
                ));
            }
        }
        let mut baseline = None;
        if let Some(style) = profile.style {
            if style.msg_count_used >= 5 && style.msg_count_used_theirs >= 1 {
                prompt.push_str(&format!(
                    "- Your style to them: formality {:.2}, avg sentence {:.1} words, based on {} messages\n",
                    style.formality_score, style.avg_sentence_len, style.msg_count_used
                ));
                prompt.push_str(&format!(
                    "- Their style to you: formality {:.2}, avg sentence {:.1} words\n",
                    style.formality_score_theirs, style.avg_sentence_len_theirs
                ));
                baseline = Some((
                    StylometryMetrics {
                        formality_score: style.formality_score,
                        avg_sentence_len: style.avg_sentence_len,
                        ..StylometryMetrics::default()
                    },
                    style.msg_count_used,
                ));
            }
        }
        if !profile.open_commitments.is_empty() {
            prompt.push_str("- Outstanding commitments:\n");
            for commitment in profile.open_commitments.iter().take(5) {
                prompt.push_str(&format!(
                    "  - {:?}: {}\n",
                    commitment.direction, commitment.what
                ));
            }
        }
        return Ok(DraftVoiceContext { prompt, baseline });
    }

    let desired_register =
        register_label(register.unwrap_or_else(|| match infer_register(purpose) {
            mxr_relationship::VoiceRegister::Casual => VoiceRegisterData::Casual,
            mxr_relationship::VoiceRegister::Formal => VoiceRegisterData::Formal,
            mxr_relationship::VoiceRegister::Neutral => VoiceRegisterData::Neutral,
        }));
    let mut prompt = format!(
        "The recipient has no prior relationship profile. Write in the user's typical {desired_register} register without inventing familiarity.\n"
    );
    let mut baseline = None;
    if let Some(profile) = state
        .store
        .get_user_voice_profile(account_id)
        .await
        .map_err(|error| error.to_string())?
    {
        if let Some(mode) = profile
            .register_modes
            .iter()
            .find(|mode| mode.name == desired_register)
        {
            prompt.push_str(&format!(
                "- User voice: formality {:.2}, avg sentence {:.1} words, exemplars: {}\n",
                mode.formality_score,
                mode.avg_sentence_len,
                mode.exemplar_message_ids.join(", ")
            ));
            baseline = Some((
                StylometryMetrics {
                    formality_score: mode.formality_score,
                    avg_sentence_len: mode.avg_sentence_len,
                    ..StylometryMetrics::default()
                },
                profile.msg_count_used,
            ));
        }
    }
    Ok(DraftVoiceContext { prompt, baseline })
}

pub(crate) async fn finish_draft_suggestion(
    state: &AppState,
    body: String,
    model: String,
    baseline: Option<(StylometryMetrics, u32)>,
) -> HandlerResult {
    let (body, humanizer, rewrite_iterations) = if state.config_snapshot().humanizer.apply_to_drafts
    {
        super::humanizer::rewrite_to_threshold(state, body, None).await?
    } else {
        let humanizer = humanizer_summary(humanizer_score(&body, &HumanizerOpts::default()));
        (body, humanizer, 0)
    };
    let voice_match = baseline.map(|(baseline, count)| {
        let report = score_voice_match(&compute_metrics(&body), &baseline, count);
        VoiceMatchData {
            score: report.score,
            confidence: match report.confidence {
                VoiceMatchConfidence::Low => VoiceMatchConfidenceData::Low,
                VoiceMatchConfidence::Medium => VoiceMatchConfidenceData::Medium,
                VoiceMatchConfidence::High => VoiceMatchConfidenceData::High,
            },
            notable_deltas: report.notable_deltas,
        }
    });
    Ok(ResponseData::DraftSuggestion {
        body,
        model,
        voice_match,
        humanizer: Some(humanizer),
        rewrite_iterations,
    })
}

fn humanizer_summary(report: mxr_humanizer::HumanizerReport) -> HumanizerReportSummaryData {
    super::humanizer::report_summary(report)
}

fn length_label(length: DraftLengthHintData) -> &'static str {
    match length {
        DraftLengthHintData::Short => "short",
        DraftLengthHintData::Medium => "medium",
        DraftLengthHintData::Long => "long",
    }
}

fn register_label(register: VoiceRegisterData) -> &'static str {
    match register {
        VoiceRegisterData::Casual => "casual",
        VoiceRegisterData::Neutral => "neutral",
        VoiceRegisterData::Formal => "formal",
    }
}
