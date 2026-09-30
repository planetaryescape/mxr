//! Relationship context for AI drafts: infer the tone (register) and
//! length the user typically uses with a contact from stored stylometry,
//! keep the contact's profile fresh, and finish a model's draft into a
//! `DraftSuggestion` (humanizer pass, voice-match score, inferred fields).
//! The prompt itself is built by `draft_prompt` from `draft_voice`.

use super::draft_provenance::DraftPolicy;
use super::{relationship_profile, HandlerResult};
use crate::state::AppState;
use mxr_core::id::{AccountId, MessageId};
use mxr_core::types::Envelope;
use mxr_humanizer::{score as humanizer_score, HumanizerOpts};
use mxr_llm::LlmFeature;
use mxr_protocol::{
    ContactStyleData, DraftLengthHintData, DraftProvenanceData, DraftRewriteOutcomeData,
    HumanizerReportSummaryData, ResponseData, VoiceMatchConfidenceData, VoiceMatchData,
    VoiceRegisterData,
};
use mxr_relationship::stylometry::StylometryMetrics;
use mxr_relationship::{compute_metrics, infer_register, score_voice_match, VoiceMatchConfidence};

/// Global ceiling on the assembled user message (chars), sized so the
/// message plus the fixed system prompt and writing constraints fit an 8k
/// context window with headroom for the model's response.
pub(crate) const ASSEMBLED_MESSAGE_BUDGET_CHARS: usize = 28_000;

/// The inferred tone/length for a draft, the voice-match baseline and the
/// note shown in the UI.
pub(crate) struct DraftContext {
    /// Baseline stylometry for post-hoc voice-match scoring.
    pub baseline: Option<(StylometryMetrics, u32)>,
    /// Effective register used for the draft (inferred, unless overridden).
    pub inferred_register: VoiceRegisterData,
    /// Effective length used for the draft (inferred, unless overridden).
    pub inferred_length: DraftLengthHintData,
    /// Human-readable note for the UI, e.g. "Matched to alice@x (casual, short)".
    pub context_note: Option<String>,
}

/// Infer the register/length to use from the stored style of the first
/// recipient that has one. `register_override`/`length_override`
/// short-circuit inference (the returned `inferred_*` still reflect the
/// effective values so the UI chip is accurate).
pub(crate) async fn build_relationship_block(
    state: &AppState,
    account_id: &AccountId,
    for_emails: &[String],
    purpose: &str,
    register_override: Option<VoiceRegisterData>,
    length_override: Option<DraftLengthHintData>,
) -> DraftContext {
    let mut baseline: Option<(StylometryMetrics, u32)> = None;
    let mut primary_style: Option<ContactStyleData> = None;
    let mut matched_email: Option<String> = None;

    for email in for_emails.iter().take(3) {
        let Ok(Some(profile)) =
            relationship_profile::load_relationship_profile(state, account_id, email).await
        else {
            continue;
        };
        if let Some(style) = &profile.style {
            primary_style = Some(style.clone());
            matched_email = Some(email.clone());
            baseline = Some((
                StylometryMetrics {
                    formality_score: style.formality_score,
                    avg_sentence_len: style.avg_sentence_len,
                    ..StylometryMetrics::default()
                },
                style.msg_count_used,
            ));
            break;
        }
    }

    let user_voice = state
        .store
        .get_user_voice_profile(account_id)
        .await
        .ok()
        .flatten();
    let uv_formality = user_voice.as_ref().map(|p| p.formality_score);
    let uv_avg = user_voice.as_ref().map(|p| p.avg_sentence_len);

    let inferred_register = register_override.unwrap_or_else(|| {
        register_from_style(primary_style.as_ref(), uv_formality, Some(purpose))
    });
    let inferred_length =
        length_override.unwrap_or_else(|| length_from_style(primary_style.as_ref(), uv_avg));

    // No contact-specific baseline → fall back to the user's voice profile for
    // the chosen register, so voice-match scoring still has something to grade.
    if baseline.is_none() {
        if let Some(profile) = &user_voice {
            let label = register_label(inferred_register);
            if let Some(mode) = profile
                .register_modes
                .iter()
                .find(|mode| mode.name == label)
            {
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
    }

    let context_note = build_context_note(
        matched_email.as_deref(),
        user_voice.is_some(),
        inferred_register,
        inferred_length,
    );

    DraftContext {
        baseline,
        inferred_register,
        inferred_length,
        context_note,
    }
}

/// Infer the register from how the contact and the user write to each other,
/// weighting the contact's own formality more heavily (it signals how they
/// expect to be addressed). Falls back to the user's global voice, then to the
/// purpose text, then Neutral.
pub(crate) fn register_from_style(
    style: Option<&ContactStyleData>,
    user_voice_formality: Option<f64>,
    purpose_fallback: Option<&str>,
) -> VoiceRegisterData {
    let formality = match style {
        Some(s) if s.msg_count_used_theirs > 0 => {
            Some(0.6 * s.formality_score_theirs + 0.4 * s.formality_score)
        }
        Some(s) => Some(s.formality_score),
        None => user_voice_formality,
    };
    if let Some(f) = formality {
        return if f < 0.35 {
            VoiceRegisterData::Casual
        } else if f < 0.65 {
            VoiceRegisterData::Neutral
        } else {
            VoiceRegisterData::Formal
        };
    }
    match purpose_fallback.map(infer_register) {
        Some(mxr_relationship::VoiceRegister::Casual) => VoiceRegisterData::Casual,
        Some(mxr_relationship::VoiceRegister::Formal) => VoiceRegisterData::Formal,
        _ => VoiceRegisterData::Neutral,
    }
}

/// Infer the length hint from the typical sentence length in the relationship
/// (a proxy for how terse the user is with this person), falling back to the
/// user's global voice, then Medium.
pub(crate) fn length_from_style(
    style: Option<&ContactStyleData>,
    user_voice_avg_sentence_len: Option<f64>,
) -> DraftLengthHintData {
    let len = match style {
        Some(s) if s.msg_count_used_theirs > 0 => {
            Some(0.5 * s.avg_sentence_len_theirs + 0.5 * s.avg_sentence_len)
        }
        Some(s) => Some(s.avg_sentence_len),
        None => user_voice_avg_sentence_len,
    };
    match len {
        Some(l) if l <= 12.0 => DraftLengthHintData::Short,
        Some(l) if l <= 20.0 => DraftLengthHintData::Medium,
        Some(_) => DraftLengthHintData::Long,
        None => DraftLengthHintData::Medium,
    }
}

fn build_context_note(
    matched_email: Option<&str>,
    has_user_voice: bool,
    register: VoiceRegisterData,
    length: DraftLengthHintData,
) -> Option<String> {
    let tone = format!("{}, {}", register_label(register), length_label(length));
    match matched_email {
        Some(email) => Some(format!("Matched to {email} ({tone})")),
        None if has_user_voice => Some(format!("Using your usual voice ({tone})")),
        None => None,
    }
}

/// Resolve the message being replied to/forwarded into its thread envelopes.
/// Returns an empty Vec if the message isn't found locally, so callers fall
/// back to new-message mode rather than failing.
pub(crate) async fn resolve_thread_envelopes(
    state: &AppState,
    message_id: &MessageId,
) -> Vec<Envelope> {
    let Ok(Some(envelope)) = state.store.get_envelope(message_id).await else {
        return Vec::new();
    };
    state
        .store
        .get_thread_envelopes(&envelope.thread_id)
        .await
        .unwrap_or_default()
}

/// If the contact's profile is missing or older than their newest message,
/// rebuild it (style + summary + commitments) before drafting. Best-effort:
/// drafting never blocks on or fails from a refresh error.
pub(crate) async fn ensure_contact_fresh(state: &AppState, account_id: &AccountId, email: &str) {
    let style = state
        .store
        .get_contact_style(account_id, email)
        .await
        .ok()
        .flatten();
    let summary = state
        .store
        .get_contact_relationship_summary(account_id, email)
        .await
        .ok()
        .flatten();
    let newest = state
        .store
        .recent_contact_messages(account_id, email, 1)
        .await
        .ok()
        .and_then(|samples| samples.first().map(|sample| sample.date));
    let stale = match (&style, newest) {
        (None, Some(_)) => true,
        (Some(style), Some(newest)) => summary.is_none() || style.computed_at < newest,
        _ => false,
    };
    if !stale {
        return;
    }
    if let Err(error) = state
        .relationship
        .rebuild_contact(account_id.clone(), email.to_string())
        .await
    {
        tracing::warn!(%email, %error, "on-draft contact profile refresh failed");
    }
}

pub(crate) fn register_label(register: VoiceRegisterData) -> &'static str {
    match register {
        VoiceRegisterData::Casual => "casual",
        VoiceRegisterData::Neutral => "neutral",
        VoiceRegisterData::Formal => "formal",
    }
}

pub(crate) fn length_label(length: DraftLengthHintData) -> &'static str {
    match length {
        DraftLengthHintData::Short => "short",
        DraftLengthHintData::Medium => "medium",
        DraftLengthHintData::Long => "long",
    }
}

/// Run the humanizer pass, score voice match, and package the response with
/// the inferred tone/length, context note and provenance. `voice_context`
/// (habits and past emails) reaches the rewrite model only when its own
/// pinned endpoint may see the user's history; and a draft written from
/// that history never goes to a rewrite model that may not see it, since
/// the draft itself carries the voice and facts drawn from it.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn finish_draft_suggestion(
    state: &AppState,
    account_id: &AccountId,
    body: String,
    mut provenance: DraftProvenanceData,
    baseline: Option<(StylometryMetrics, u32)>,
    voice_context: &str,
    inferred_register: VoiceRegisterData,
    inferred_length: DraftLengthHintData,
    context_note: Option<String>,
) -> HandlerResult {
    let (body, humanizer, rewrite_iterations) = if state.config_snapshot().humanizer.apply_to_drafts
    {
        let rewrite = DraftPolicy::pin(state, LlmFeature::HumanizeRewrite);
        if provenance.history_used && !rewrite.share_history {
            if super::humanizer::rewrite_due(state, &body) {
                provenance.rewrite = Some(rewrite.rewrite_provenance(
                    rewrite.llm.model_name(),
                    false,
                    DraftRewriteOutcomeData::Skipped,
                ));
            }
            let humanizer = report_summary(humanizer_score(&body, &HumanizerOpts::default()));
            (body, humanizer, 0)
        } else {
            let voice_context = Some(voice_context)
                .filter(|context| rewrite.share_history && !context.trim().is_empty());
            let rewritten = super::humanizer::rewrite_to_threshold_with_context(
                state,
                &rewrite.llm,
                body,
                None,
                voice_context,
            )
            .await?;
            provenance.rewrite = rewritten.provenance(&rewrite, voice_context.is_some());
            (rewritten.text, rewritten.report, rewritten.iterations)
        }
    } else {
        let humanizer = report_summary(humanizer_score(&body, &HumanizerOpts::default()));
        (body, humanizer, 0)
    };
    // Remembered so a later refine or humanize of this text can't hand it
    // to a cloud model without opt-in (see `history_text`).
    if provenance.history_used
        || provenance
            .rewrite
            .as_ref()
            .is_some_and(|rewrite| rewrite.history_used)
    {
        crate::history_text::record(&state.store, account_id, &body).await;
    }
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
        model: provenance.model.clone(),
        voice_match,
        humanizer: Some(humanizer),
        rewrite_iterations,
        inferred_register: Some(inferred_register),
        inferred_length: Some(inferred_length),
        context_note,
        provenance: Some(provenance),
    })
}

fn report_summary(report: mxr_humanizer::HumanizerReport) -> HumanizerReportSummaryData {
    super::humanizer::report_summary(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(
        formality: f64,
        theirs: f64,
        avg: f64,
        avg_theirs: f64,
        theirs_count: u32,
    ) -> ContactStyleData {
        ContactStyleData {
            formality_score: formality,
            formality_score_theirs: theirs,
            avg_sentence_len: avg,
            avg_sentence_len_theirs: avg_theirs,
            msg_count_used: 6,
            msg_count_used_theirs: theirs_count,
            computed_at: chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap(),
            source_hash: "h".to_string(),
        }
    }

    #[test]
    fn register_from_style_maps_formality_bands() {
        assert_eq!(
            register_from_style(Some(&style(0.1, 0.1, 8.0, 8.0, 2)), None, None),
            VoiceRegisterData::Casual
        );
        assert_eq!(
            register_from_style(Some(&style(0.5, 0.5, 14.0, 14.0, 2)), None, None),
            VoiceRegisterData::Neutral
        );
        assert_eq!(
            register_from_style(Some(&style(0.9, 0.9, 22.0, 22.0, 2)), None, None),
            VoiceRegisterData::Formal
        );
    }

    #[test]
    fn register_from_style_weights_theirs() {
        // Their formality is weighted more (0.6) than the user's (0.4): with
        // theirs=0.9, yours=0.35 the blend is 0.6*0.9 + 0.4*0.35 = 0.68 →
        // Formal, where a plain average (0.625) would land in Neutral. This
        // isolates the heavier weighting on the contact's own formality.
        assert_eq!(
            register_from_style(Some(&style(0.35, 0.9, 10.0, 22.0, 4)), None, None),
            VoiceRegisterData::Formal
        );
    }

    #[test]
    fn register_falls_back_to_user_voice_then_purpose() {
        assert_eq!(
            register_from_style(None, Some(0.9), None),
            VoiceRegisterData::Formal
        );
        assert_eq!(register_from_style(None, None, Some("yo")), {
            match infer_register("yo") {
                mxr_relationship::VoiceRegister::Casual => VoiceRegisterData::Casual,
                mxr_relationship::VoiceRegister::Formal => VoiceRegisterData::Formal,
                mxr_relationship::VoiceRegister::Neutral => VoiceRegisterData::Neutral,
            }
        });
    }

    #[test]
    fn length_from_style_maps_sentence_len() {
        assert_eq!(
            length_from_style(Some(&style(0.5, 0.5, 8.0, 8.0, 2)), None),
            DraftLengthHintData::Short
        );
        assert_eq!(
            length_from_style(Some(&style(0.5, 0.5, 16.0, 16.0, 2)), None),
            DraftLengthHintData::Medium
        );
        assert_eq!(
            length_from_style(Some(&style(0.5, 0.5, 26.0, 26.0, 2)), None),
            DraftLengthHintData::Long
        );
        assert_eq!(length_from_style(None, None), DraftLengthHintData::Medium);
    }
}
