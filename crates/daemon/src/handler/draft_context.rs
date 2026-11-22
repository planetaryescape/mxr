use super::draft_compose::DraftRewritePolicy;
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

pub(crate) const ASSEMBLED_MESSAGE_BUDGET_CHARS: usize = 28_000;

pub(crate) struct DraftContext {
    pub baseline: Option<(StylometryMetrics, u32)>,
    pub background: Option<String>,
    pub history_used: bool,
    pub style_used: bool,
    pub inferred_register: VoiceRegisterData,
    pub inferred_length: DraftLengthHintData,
    pub context_note: Option<String>,
}

pub(crate) async fn build_relationship_block(
    state: &AppState,
    account_id: &AccountId,
    for_emails: &[String],
    purpose: &str,
    register_override: Option<VoiceRegisterData>,
    length_override: Option<DraftLengthHintData>,
    allow_history: bool,
) -> DraftContext {
    let mut baseline: Option<(StylometryMetrics, u32)> = None;
    let mut primary_style: Option<ContactStyleData> = None;
    let mut matched_email: Option<String> = None;
    let mut background = None;

    for email in for_emails.iter().take(if allow_history { 3 } else { 0 }) {
        let Ok(Some(profile)) =
            relationship_profile::load_relationship_profile(state, account_id, email).await
        else {
            continue;
        };
        if background.is_none() {
            background = profile.summary.as_ref().map(|summary| summary.text.clone());
        }
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

    let user_voice = if allow_history {
        state
            .store
            .get_user_voice_profile(account_id)
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let uv_formality = user_voice.as_ref().map(|p| p.formality_score);
    let uv_avg = user_voice.as_ref().map(|p| p.avg_sentence_len);

    let inferred_register = register_override.unwrap_or_else(|| {
        register_from_style(primary_style.as_ref(), uv_formality, Some(purpose))
    });
    let inferred_length =
        length_override.unwrap_or_else(|| length_from_style(primary_style.as_ref(), uv_avg));

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
        history_used: matched_email.is_some() || user_voice.is_some() || background.is_some(),
        style_used: primary_style.is_some() || user_voice.is_some(),
        background,
        inferred_register,
        inferred_length,
        context_note,
    }
}

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

pub(crate) async fn resolve_thread_envelopes(
    state: &AppState,
    message_id: &MessageId,
) -> Vec<Envelope> {
    let Ok(Some(envelope)) = state.store.get_envelope(message_id).await else {
        return Vec::new();
    };
    let mut envelopes = state
        .store
        .get_thread_envelopes_for_model(&envelope.thread_id)
        .await
        .unwrap_or_default();
    if !envelopes
        .iter()
        .any(|candidate| candidate.id == envelope.id)
    {
        envelopes.push(envelope);
        envelopes.sort_by_key(|candidate| candidate.date);
    }
    envelopes
}

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
    rewrite_policy: DraftRewritePolicy,
) -> HandlerResult {
    let (body, humanizer, rewrite_iterations) = if match rewrite_policy {
        DraftRewritePolicy::Configured => state.config_snapshot().humanizer.apply_to_drafts,
        DraftRewritePolicy::Disabled => false,
        DraftRewritePolicy::Enabled => true,
    } {
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
