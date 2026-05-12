use crate::stylometry::StylometryMetrics;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceRegister {
    Casual,
    Neutral,
    Formal,
}

impl VoiceRegister {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Casual => "casual",
            Self::Neutral => "neutral",
            Self::Formal => "formal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisterMode {
    pub register: VoiceRegister,
    pub metrics: StylometryMetrics,
    pub exemplar_message_ids: Vec<String>,
}

pub fn infer_register(purpose: &str) -> VoiceRegister {
    let lower = purpose.to_ascii_lowercase();
    if lower.contains("formal") || lower.contains("proposal") || lower.contains("executive") {
        VoiceRegister::Formal
    } else if lower.contains("quick") || lower.contains("thanks") || lower.contains("fyi") {
        VoiceRegister::Casual
    } else {
        VoiceRegister::Neutral
    }
}

pub fn build_register_modes(samples: &[(String, StylometryMetrics)]) -> Vec<RegisterMode> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.1.formality_score.total_cmp(&b.1.formality_score));
    let chunk_size = sorted.len().div_ceil(3).max(1);
    [
        VoiceRegister::Casual,
        VoiceRegister::Neutral,
        VoiceRegister::Formal,
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(index, register)| {
        let start = index * chunk_size;
        let end = ((index + 1) * chunk_size).min(sorted.len());
        let chunk = sorted.get(start..end)?;
        if chunk.is_empty() {
            return None;
        }
        Some(RegisterMode {
            register,
            metrics: centroid(chunk.iter().map(|(_, metrics)| metrics)),
            exemplar_message_ids: chunk.iter().take(3).map(|(id, _)| id.clone()).collect(),
        })
    })
    .collect()
}

fn centroid<'a>(metrics: impl Iterator<Item = &'a StylometryMetrics>) -> StylometryMetrics {
    let mut count = 0.0;
    let mut result = StylometryMetrics::default();
    for metrics in metrics {
        count += 1.0;
        result.formality_score += metrics.formality_score;
        result.avg_sentence_len += metrics.avg_sentence_len;
        result.word_count += metrics.word_count;
        result.sentence_count += metrics.sentence_count;
    }
    if count > 0.0 {
        result.formality_score /= count;
        result.avg_sentence_len /= count;
        result.word_count = (result.word_count as f64 / count).round() as u32;
        result.sentence_count = (result.sentence_count as f64 / count).round() as u32;
    }
    result
}
