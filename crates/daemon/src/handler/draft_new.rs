use super::{relationship_profile, HandlerResult};
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::types::Address;
use mxr_humanizer::{score as humanizer_score, writing_constraints, HumanizerOpts};
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
    prompt.push_str("[WRITING CONSTRAINTS]\n");
    prompt.push_str(writing_constraints());
    prompt.push_str("\n\n");
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
        Err(LlmError::Disabled) => {
            return Err(crate::handler::HandlerError::Message(
                "LLM is disabled. Enable it in [llm].".to_string(),
            ))
        }
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
        if let Some(style) = profile.style {
            if style.msg_count_used >= 5 && style.msg_count_used_theirs >= 1 {
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
                prompt.push_str(&format!(
                    "- Your style to them: formality {:.2}, avg sentence {:.1} words, based on {} messages\n",
                    style.formality_score, style.avg_sentence_len, style.msg_count_used
                ));
                prompt.push_str(&format!(
                    "- Their style to you: formality {:.2}, avg sentence {:.1} words\n",
                    style.formality_score_theirs, style.avg_sentence_len_theirs
                ));
                let baseline = Some((
                    StylometryMetrics {
                        formality_score: style.formality_score,
                        avg_sentence_len: style.avg_sentence_len,
                        ..StylometryMetrics::default()
                    },
                    style.msg_count_used,
                ));
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
        }
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
    voice_context: Option<&str>,
) -> HandlerResult {
    let (body, humanizer, rewrite_iterations) = if state.config_snapshot().humanizer.apply_to_drafts
    {
        super::humanizer::rewrite_to_threshold_with_context(state, body, None, voice_context)
            .await?
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
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
                content: "Plain reply".to_string(),
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

    #[tokio::test]
    async fn draft_new_uses_user_voice_fallback_when_profile_below_threshold() {
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

        let response = draft_new(
            &state,
            &account_id,
            Address {
                name: None,
                email: "customer@example.com".to_string(),
            },
            "follow up on pricing",
            Some(VoiceRegisterData::Neutral),
            None,
        )
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
        assert!(prompt.contains("[WRITING CONSTRAINTS]"));
        assert!(prompt.contains("recipient has no prior relationship profile"));
        assert!(!prompt.contains("Customer prefers short pricing updates."));
        assert!(!prompt.contains("Known topics: pricing"));
    }
}
