use crate::stylometry::StylometryMetrics;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMatchConfidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceMatchReport {
    pub score: f64,
    pub confidence: VoiceMatchConfidence,
    pub notable_deltas: Vec<String>,
}

pub fn score_voice_match(
    draft: &StylometryMetrics,
    baseline: &StylometryMetrics,
    baseline_count: u32,
) -> VoiceMatchReport {
    if baseline_count < 3 {
        return VoiceMatchReport {
            score: 0.0,
            confidence: VoiceMatchConfidence::Low,
            notable_deltas: vec!["not enough prior sent mail for this contact".to_string()],
        };
    }
    let distance = ((draft.formality_score - baseline.formality_score).abs()
        + ((draft.avg_sentence_len - baseline.avg_sentence_len).abs() / 30.0).min(1.0))
        / 2.0;
    let score = (1.0 - distance).clamp(0.0, 1.0);
    let mut notable_deltas = Vec::new();
    if (draft.formality_score - baseline.formality_score).abs() > 0.25 {
        notable_deltas.push(format!(
            "formality differs by {:.2}",
            (draft.formality_score - baseline.formality_score).abs()
        ));
    }
    if (draft.avg_sentence_len - baseline.avg_sentence_len).abs() > 8.0 {
        notable_deltas.push(format!(
            "sentence length differs by {:.1} words",
            (draft.avg_sentence_len - baseline.avg_sentence_len).abs()
        ));
    }
    VoiceMatchReport {
        score,
        confidence: if baseline_count >= 10 {
            VoiceMatchConfidence::High
        } else {
            VoiceMatchConfidence::Medium
        },
        notable_deltas,
    }
}
