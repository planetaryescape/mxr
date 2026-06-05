//! Shared rendering for `DraftSuggestion` responses so `mxr draft` and
//! `mxr draft-assist` surface identical fields — body, model, voice match,
//! humanizer, and the inferred tone/length + "Matched to …" context note.

use crate::cli::{DraftLengthArg, VoiceRegisterArg};
use mxr_protocol::{
    DraftLengthHintData, HumanizerReportSummaryData, ResponseData, VoiceMatchData,
    VoiceRegisterData,
};
use serde_json::{json, Value};

pub(crate) struct DraftSuggestionView {
    pub body: String,
    pub model: String,
    pub voice_match: Option<VoiceMatchData>,
    pub humanizer: Option<HumanizerReportSummaryData>,
    pub rewrite_iterations: u8,
    pub inferred_register: Option<VoiceRegisterData>,
    pub inferred_length: Option<DraftLengthHintData>,
    pub context_note: Option<String>,
}

impl DraftSuggestionView {
    pub(crate) fn from_response(data: ResponseData) -> Option<Self> {
        match data {
            ResponseData::DraftSuggestion {
                body,
                model,
                voice_match,
                humanizer,
                rewrite_iterations,
                inferred_register,
                inferred_length,
                context_note,
            } => Some(Self {
                body,
                model,
                voice_match,
                humanizer,
                rewrite_iterations,
                inferred_register,
                inferred_length,
                context_note,
            }),
            _ => None,
        }
    }
}

/// The canonical JSON object for a draft suggestion (shared by both commands).
pub(crate) fn draft_suggestion_json(view: &DraftSuggestionView) -> Value {
    json!({
        "body": view.body,
        "model": view.model,
        "voice_match": view.voice_match,
        "humanizer": view.humanizer,
        "rewrite_iterations": view.rewrite_iterations,
        "inferred_register": view.inferred_register,
        "inferred_length": view.inferred_length,
        "context_note": view.context_note,
    })
}

/// Human-readable side notes (stderr) for table output — includes the
/// inferred-tone "Matched to …" note so the CLI is at parity with the GUI.
pub(crate) fn eprint_draft_notes(view: &DraftSuggestionView) {
    eprintln!("\n[via {} — review before sending]", view.model);
    if let Some(note) = &view.context_note {
        eprintln!("{note}");
    }
    if let Some(voice_match) = &view.voice_match {
        eprintln!(
            "voice_match={:.2} {:?}",
            voice_match.score, voice_match.confidence
        );
    }
    if let Some(humanizer) = &view.humanizer {
        eprintln!("humanizer={}/100", humanizer.score);
    }
    if view.rewrite_iterations > 0 {
        eprintln!("rewritten {}x", view.rewrite_iterations);
    }
}

pub(crate) fn register_data(value: VoiceRegisterArg) -> VoiceRegisterData {
    match value {
        VoiceRegisterArg::Casual => VoiceRegisterData::Casual,
        VoiceRegisterArg::Neutral => VoiceRegisterData::Neutral,
        VoiceRegisterArg::Formal => VoiceRegisterData::Formal,
    }
}

pub(crate) fn length_data(value: DraftLengthArg) -> DraftLengthHintData {
    match value {
        DraftLengthArg::Short => DraftLengthHintData::Short,
        DraftLengthArg::Medium => DraftLengthHintData::Medium,
        DraftLengthArg::Long => DraftLengthHintData::Long,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Behavior 9: the shared JSON renderer surfaces the inferred tone/length
    // + context note — the contract both `mxr draft` and `draft-assist` rely
    // on for parity.
    #[test]
    fn draft_suggestion_json_surfaces_inferred_tone() {
        let view = DraftSuggestionView {
            body: "hi".to_string(),
            model: "m".to_string(),
            voice_match: None,
            humanizer: None,
            rewrite_iterations: 0,
            inferred_register: Some(VoiceRegisterData::Formal),
            inferred_length: Some(DraftLengthHintData::Short),
            context_note: Some("Matched to a@b (formal, short)".to_string()),
        };
        let value = draft_suggestion_json(&view);
        assert_eq!(value["inferred_register"], json!("formal"));
        assert_eq!(value["inferred_length"], json!("short"));
        assert_eq!(
            value["context_note"],
            json!("Matched to a@b (formal, short)")
        );
    }
}
