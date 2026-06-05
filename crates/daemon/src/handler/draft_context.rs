//! Shared context assembly for AI draft generation, used by both
//! `draft_new` (compose) and `draft_assist` (thread reader).
//!
//! Responsibilities:
//! - Build the relationship/voice block for the recipient(s) with NO hard
//!   message-count threshold — whatever profile exists is used, and sparse
//!   data degrades gracefully via the voice-match confidence. The block is a
//!   first-class instruction to write in the user's voice to this person; the
//!   "never invent facts or familiarity" guardrail is preserved.
//! - Infer the tone (register) and length the user typically uses with this
//!   contact from the stored stylometry, so the UI doesn't need manual dials.
//! - Retrieve the user's prior sent mail as voice exemplars (grounding).
//! - Assemble the user message within a char budget (thread context is
//!   trimmed last; the task line is never trimmed).
//! - Ensure the contact's profile is fresh before drafting.
//! - Finish a draft into a `DraftSuggestion`, carrying the inferred fields.

use super::{relationship_profile, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Envelope, MessageDirection, SemanticChunkSourceKind};
use mxr_humanizer::{score as humanizer_score, writing_constraints, HumanizerOpts};
use mxr_protocol::{
    ContactStyleData, DraftLengthHintData, HumanizerReportSummaryData, ResponseData,
    VoiceMatchConfidenceData, VoiceMatchData, VoiceRegisterData,
};
use mxr_relationship::stylometry::StylometryMetrics;
use mxr_relationship::{compute_metrics, infer_register, score_voice_match, VoiceMatchConfidence};
use std::collections::BTreeSet;

pub(crate) const PROMPT_BUDGET_CHARS: usize = 24_000;
const GROUNDING_LIMIT: usize = 3;
const GROUNDING_SEARCH_LIMIT: usize = 8;
const GROUNDING_BUDGET_CHARS: usize = 4_000;
pub(crate) const RELATIONSHIP_BUDGET_CHARS: usize = 2_000;
/// Global ceiling on the assembled user message (chars), sized so the
/// message plus the fixed system prompt and writing constraints fit an 8k
/// context window with headroom for the model's response.
pub(crate) const ASSEMBLED_MESSAGE_BUDGET_CHARS: usize = 28_000;

/// The relationship/voice context plus the inferred tone/length for a draft.
pub(crate) struct DraftContext {
    /// The relationship/voice block (without the framing header — the
    /// assembler adds that).
    pub prompt: String,
    /// Baseline stylometry for post-hoc voice-match scoring.
    pub baseline: Option<(StylometryMetrics, u32)>,
    /// Effective register used for the draft (inferred, unless overridden).
    pub inferred_register: VoiceRegisterData,
    /// Effective length used for the draft (inferred, unless overridden).
    pub inferred_length: DraftLengthHintData,
    /// Human-readable note for the UI, e.g. "Matched to alice@x (casual, short)".
    pub context_note: Option<String>,
}

/// Assemble the relationship/voice block for the recipient(s) and infer the
/// register/length to use. `register_override`/`length_override` short-circuit
/// inference (but the returned `inferred_*` still reflect the effective values
/// so the UI chip is accurate).
pub(crate) async fn build_relationship_block(
    state: &AppState,
    account_id: &AccountId,
    for_emails: &[String],
    purpose: &str,
    register_override: Option<VoiceRegisterData>,
    length_override: Option<DraftLengthHintData>,
    budget_chars: usize,
) -> DraftContext {
    let multi = for_emails.len() > 1;
    let mut prompt = String::new();
    let mut baseline: Option<(StylometryMetrics, u32)> = None;
    let mut primary_style: Option<ContactStyleData> = None;
    let mut matched_email: Option<String> = None;

    for email in for_emails.iter().take(3) {
        let Ok(Some(profile)) =
            relationship_profile::load_relationship_profile(state, account_id, email).await
        else {
            continue;
        };
        let mut block = String::new();
        if multi {
            block.push_str(&format!("Contact: {email}\n"));
        }
        if let Some(summary) = &profile.summary {
            block.push_str(&format!("- Relationship: {}\n", summary.text.trim()));
            if !summary.known_topics.is_empty() {
                block.push_str(&format!(
                    "- Known topics: {}\n",
                    summary.known_topics.join(", ")
                ));
            }
        }
        if let Some(style) = &profile.style {
            block.push_str(&format!(
                "- Your style to them: formality {:.2}, avg sentence {:.1} words (from {} of your messages)\n",
                style.formality_score, style.avg_sentence_len, style.msg_count_used
            ));
            if style.msg_count_used_theirs > 0 {
                block.push_str(&format!(
                    "- Their style to you: formality {:.2}, avg sentence {:.1} words\n",
                    style.formality_score_theirs, style.avg_sentence_len_theirs
                ));
            }
            if primary_style.is_none() {
                primary_style = Some(style.clone());
                matched_email = Some(email.to_string());
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
            block.push_str("- Outstanding commitments:\n");
            for commitment in profile.open_commitments.iter().take(3) {
                block.push_str(&format!(
                    "  - {:?}: {}\n",
                    commitment.direction, commitment.what
                ));
            }
        }
        if !block.is_empty() {
            prompt.push_str(&block);
            prompt.push('\n');
        }
        if prompt.len() > budget_chars {
            prompt.truncate(budget_chars);
            prompt.push_str("\n[...relationship context truncated...]\n");
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
    // the chosen register, so voice-match scoring still has something to grade
    // against and the model sees the user's own register stats.
    if baseline.is_none() {
        if let Some(profile) = &user_voice {
            let label = register_label(inferred_register);
            if let Some(mode) = profile
                .register_modes
                .iter()
                .find(|mode| mode.name == label)
            {
                prompt.push_str(&format!(
                    "- My usual {label} voice: formality {:.2}, avg sentence {:.1} words\n",
                    mode.formality_score, mode.avg_sentence_len
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
    }

    let context_note = build_context_note(
        matched_email.as_deref(),
        user_voice.is_some(),
        inferred_register,
        inferred_length,
    );

    DraftContext {
        prompt,
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

/// Build a transcript (oldest→newest) from thread envelopes for the LLM.
pub(crate) async fn build_transcript(state: &AppState, envelopes: &[Envelope]) -> String {
    let mut transcript = String::new();
    for env in envelopes {
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
    transcript
}

/// Collect the non-owned contact emails involved in a thread (capped to a few).
pub(crate) async fn thread_contact_emails(state: &AppState, envelopes: &[Envelope]) -> Vec<String> {
    let Some(first) = envelopes.first() else {
        return Vec::new();
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
    emails.into_iter().collect()
}

fn maybe_insert_contact(emails: &mut BTreeSet<String>, owned: &BTreeSet<String>, email: &str) {
    let email = email.trim().to_ascii_lowercase();
    if email.is_empty() || owned.contains(&email) {
        return;
    }
    emails.insert(email);
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

/// Retrieve up to a few of the user's prior SENT messages, semantically
/// similar to the query, as voice exemplars. `exclude_thread_id` drops the
/// current thread; `exclude_after` drops anything newer than the latest thread
/// message (so the model never grounds on a future/in-progress reply).
pub(crate) async fn prior_sent_grounding(
    state: &AppState,
    exclude_thread_id: Option<&ThreadId>,
    query: &str,
    exclude_after: Option<DateTime<Utc>>,
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
            tracing::warn!(error = %error, "draft grounding semantic search unavailable");
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
            tracing::warn!(error = %error, "draft grounding direction lookup failed");
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
            tracing::warn!(error = %error, "draft grounding message lookup failed");
            return String::new();
        }
    };

    let mut grounding = String::new();
    let mut included = 0usize;
    for envelope in envelopes {
        if exclude_thread_id.is_some_and(|thread_id| &envelope.thread_id == thread_id) {
            continue;
        }
        if exclude_after.is_some_and(|date| envelope.date >= date) {
            continue;
        }
        let body = match state.store.get_body(&envelope.id).await {
            Ok(Some(body)) => body
                .text_plain
                .or(body.text_html)
                .unwrap_or_else(|| envelope.snippet.clone()),
            _ => envelope.snippet.clone(),
        };
        grounding.push_str(&format!("--- Sent: {} ---\n{}\n\n", envelope.subject, body));
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

/// Compose the user message in priority order within a char budget. The
/// transcript (when present) is protected to a floor and trimmed last;
/// grounding is trimmed first; the task line is never trimmed.
pub(crate) fn assemble_user_message_within_budget(
    relationship: &str,
    grounding: &str,
    transcript: &str,
    task_line: &str,
    budget_chars: usize,
) -> String {
    let constraints = writing_constraints();
    let task = task_line.trim();
    let fixed_len = "[WRITING CONSTRAINTS]\n".len()
        + constraints.len()
        + "\n\n".len()
        + "\n[TASK]\n".len()
        + task.len();
    let remaining = budget_chars.saturating_sub(fixed_len);

    let transcript_floor = if transcript.is_empty() {
        0
    } else {
        remaining * 6 / 10
    };
    let mut transcript_budget = transcript.len().min(remaining);
    if transcript_budget < transcript_floor.min(transcript.len()) {
        transcript_budget = transcript_floor.min(transcript.len());
    }
    let after_transcript = remaining.saturating_sub(transcript_budget);
    let grounding_budget = grounding.len().min(after_transcript);
    let after_grounding = after_transcript.saturating_sub(grounding_budget);
    let relationship_budget = relationship.len().min(after_grounding);

    let truncated_transcript = truncate_with_marker(transcript, transcript_budget, "thread");
    let truncated_grounding = truncate_with_marker(grounding, grounding_budget, "prior replies");
    let truncated_relationship =
        truncate_with_marker(relationship, relationship_budget, "relationship context");

    let mut message = String::with_capacity(budget_chars.min(8192));
    if !truncated_relationship.is_empty() {
        message.push_str("[VOICE CONTEXT]\n");
        message.push_str(
            "Write in my voice to this person — match the tone, formality, and length I use \
with them. Ground every fact only in the thread and known topics below; never invent facts, \
meetings, or familiarity that aren't shown here.\n\n",
        );
        message.push_str(&truncated_relationship);
        message.push_str("\n\n");
    }
    message.push_str("[WRITING CONSTRAINTS]\n");
    message.push_str(constraints);
    message.push_str("\n\n");
    if !truncated_grounding.is_empty() {
        message.push_str("[PRIOR SENT MESSAGES TO MATCH MY VOICE]\n");
        message.push_str(&truncated_grounding);
        message.push_str("\n\n");
    }
    if !truncated_transcript.is_empty() {
        message.push_str("[THREAD SO FAR]\n");
        message.push_str(&truncated_transcript);
        message.push('\n');
    }
    message.push_str("[TASK]\n");
    message.push_str(task);
    message
}

fn truncate_with_marker(text: &str, max_chars: usize, label: &str) -> String {
    if max_chars == 0 || text.is_empty() {
        return String::new();
    }
    if text.len() <= max_chars {
        return text.to_string();
    }
    let marker = format!("\n[...{label} truncated...]\n");
    if marker.len() >= max_chars {
        return marker;
    }
    let body_chars = max_chars - marker.len();
    let mut cut = body_chars.min(text.len());
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut out = String::with_capacity(max_chars);
    out.push_str(&text[..cut]);
    out.push_str(&marker);
    out
}

/// Token budget for the LLM response, derived from the (effective) length.
pub(crate) fn max_tokens_for_length(length: DraftLengthHintData) -> u32 {
    match length {
        DraftLengthHintData::Short => 260,
        DraftLengthHintData::Medium => 480,
        DraftLengthHintData::Long => 760,
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
/// the inferred tone/length and context note.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn finish_draft_suggestion(
    state: &AppState,
    body: String,
    model: String,
    baseline: Option<(StylometryMetrics, u32)>,
    voice_context: Option<&str>,
    inferred_register: VoiceRegisterData,
    inferred_length: DraftLengthHintData,
    context_note: Option<String>,
) -> HandlerResult {
    let (body, humanizer, rewrite_iterations) = if state.config_snapshot().humanizer.apply_to_drafts
    {
        super::humanizer::rewrite_to_threshold_with_context(state, body, None, voice_context)
            .await?
    } else {
        let humanizer = report_summary(humanizer_score(&body, &HumanizerOpts::default()));
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
        inferred_register: Some(inferred_register),
        inferred_length: Some(inferred_length),
        context_note,
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
            computed_at: chrono::DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
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

    #[test]
    fn assemble_fits_all_sections_when_within_budget() {
        let message = assemble_user_message_within_budget(
            "rel_short",
            "grounding_short",
            "thread_short",
            "Now draft my reply. Instruction: reply briefly",
            10_000,
        );
        assert!(message.contains("rel_short"));
        assert!(message.contains("grounding_short"));
        assert!(message.contains("thread_short"));
        assert!(message.contains("Now draft my reply. Instruction: reply briefly"));
        assert!(message.contains("Write in my voice to this person"));
    }

    #[test]
    fn assemble_truncates_grounding_before_thread() {
        let huge_grounding = "G".repeat(5_000);
        let small_transcript = "T".repeat(500);
        let message = assemble_user_message_within_budget(
            &"R".repeat(200),
            &huge_grounding,
            &small_transcript,
            "reply",
            3_000,
        );
        assert!(
            message.contains(&"T".repeat(500)),
            "thread survives in full"
        );
        assert!(message.contains("[...prior replies truncated...]"));
        assert!(!message.contains(&"G".repeat(5_000)));
    }

    #[test]
    fn assemble_never_truncates_task_line() {
        let task = "Now write a new email to alice@example.com. Purpose: decline politely";
        let message =
            assemble_user_message_within_budget("", &"G".repeat(100_000), "", task, 5_000);
        assert!(
            message.ends_with(task),
            "task line preserved intact: {message}"
        );
    }

    #[test]
    fn assemble_omits_thread_section_for_new_message() {
        let message = assemble_user_message_within_budget(
            "rel",
            "",
            "",
            "Write a new email to bob@example.com. Purpose: intro",
            10_000,
        );
        assert!(!message.contains("[THREAD SO FAR]"));
        assert!(message.contains("[TASK]"));
    }

    #[test]
    fn truncate_with_marker_respects_utf8_boundaries() {
        let text = "résumé résumé résumé résumé".repeat(10);
        let out = truncate_with_marker(&text, 60, "example");
        assert!(out.contains("[...example truncated...]"));
        let _ = std::str::from_utf8(out.as_bytes()).expect("valid UTF-8");
    }
}
