use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StylometryMetrics {
    pub formality_score: f64,
    pub avg_sentence_len: f64,
    pub sentence_count: u32,
    pub word_count: u32,
    pub emoji_rate_per_1k_chars: f64,
    pub exclamation_rate_per_msg: f64,
    pub questions_per_msg: f64,
    pub contraction_rate: f64,
    pub lowercase_opener_rate: f64,
    pub top_openers: Vec<String>,
    pub top_signoffs: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DirectionalMetrics {
    pub yours: StylometryMetrics,
    pub theirs: StylometryMetrics,
    pub yours_count: u32,
    pub theirs_count: u32,
    pub source_hash: String,
}

#[derive(Debug, Clone)]
pub struct WeightedText<'a> {
    pub id: String,
    pub text: &'a str,
    pub date: DateTime<Utc>,
}

pub fn compute_metrics(text: &str) -> StylometryMetrics {
    let words = words(text);
    let word_count = words.len() as u32;
    let sentence_count = text.matches(['.', '!', '?']).count().max(1) as u32;
    let avg_sentence_len = word_count as f64 / sentence_count as f64;
    let contractions = words.iter().filter(|word| word.contains('\'')).count() as f64;
    let contraction_rate = rate(contractions, word_count as f64);
    let exclamations = text.matches('!').count() as f64;
    let questions = text.matches('?').count() as f64;
    let emoji_rate_per_1k_chars = rate(
        non_ascii_symbol_count(text) as f64 * 1000.0,
        text.len().max(1) as f64,
    );
    let lowercase_opener_rate = first_alpha(text).map_or(0.0, |ch| ch.is_lowercase() as u8 as f64);
    let casual_score = (contraction_rate * 3.0)
        + rate(exclamations, sentence_count as f64)
        + lowercase_opener_rate
        + (emoji_rate_per_1k_chars / 20.0).min(1.0);
    let formal_length = (avg_sentence_len / 24.0).min(1.0);
    let formality_score = (formal_length + 1.0 - casual_score.min(1.0)) / 2.0;

    StylometryMetrics {
        formality_score: clamp01(formality_score),
        avg_sentence_len,
        sentence_count,
        word_count,
        emoji_rate_per_1k_chars,
        exclamation_rate_per_msg: exclamations,
        questions_per_msg: questions,
        contraction_rate,
        lowercase_opener_rate,
        top_openers: extract_openers(text),
        top_signoffs: extract_signoffs(text),
    }
}

pub fn aggregate_metrics(texts: &[WeightedText<'_>]) -> (StylometryMetrics, String) {
    if texts.is_empty() {
        return (StylometryMetrics::default(), String::new());
    }
    let now = Utc::now();
    let mut total_weight = 0.0;
    let mut formality = 0.0;
    let mut avg_sentence_len = 0.0;
    let mut word_count = 0.0;
    let mut sentence_count = 0.0;
    let mut emoji = 0.0;
    let mut exclamations = 0.0;
    let mut questions = 0.0;
    let mut contractions = 0.0;
    let mut lowercase_openers = 0.0;
    let mut hasher = Sha256::new();
    for text in texts {
        let metrics = compute_metrics(text.text);
        let age_days = (now - text.date).num_days().max(0) as f64;
        let weight = if age_days > 365.0 * 5.0 {
            0.0
        } else {
            0.5_f64.powf(age_days / 90.0)
        };
        if weight <= 0.0 {
            continue;
        }
        total_weight += weight;
        formality += metrics.formality_score * weight;
        avg_sentence_len += metrics.avg_sentence_len * weight;
        word_count += metrics.word_count as f64 * weight;
        sentence_count += metrics.sentence_count as f64 * weight;
        emoji += metrics.emoji_rate_per_1k_chars * weight;
        exclamations += metrics.exclamation_rate_per_msg * weight;
        questions += metrics.questions_per_msg * weight;
        contractions += metrics.contraction_rate * weight;
        lowercase_openers += metrics.lowercase_opener_rate * weight;
        hasher.update(text.id.as_bytes());
        hasher.update(text.text.as_bytes());
        hasher.update(format!("{weight:.6}").as_bytes());
    }
    if total_weight == 0.0 {
        return (StylometryMetrics::default(), String::new());
    }
    (
        StylometryMetrics {
            formality_score: formality / total_weight,
            avg_sentence_len: avg_sentence_len / total_weight,
            sentence_count: sentence_count.round() as u32,
            word_count: word_count.round() as u32,
            emoji_rate_per_1k_chars: emoji / total_weight,
            exclamation_rate_per_msg: exclamations / total_weight,
            questions_per_msg: questions / total_weight,
            contraction_rate: contractions / total_weight,
            lowercase_opener_rate: lowercase_openers / total_weight,
            top_openers: Vec::new(),
            top_signoffs: Vec::new(),
        },
        base16ct::lower::encode_string(&hasher.finalize()),
    )
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| word.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '\''))
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn rate(numerator: f64, denominator: f64) -> f64 {
    if denominator <= 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn non_ascii_symbol_count(text: &str) -> usize {
    text.chars()
        .filter(|ch| !ch.is_ascii() && !ch.is_alphabetic())
        .count()
}

fn first_alpha(text: &str) -> Option<char> {
    text.chars().find(|ch| ch.is_alphabetic())
}

fn extract_openers(text: &str) -> Vec<String> {
    let first = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let opener = first
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|ch: char| !ch.is_alphanumeric());
    let known = ["hi", "hey", "hello", "dear", "thanks", "good"];
    if known.contains(&opener.to_ascii_lowercase().as_str()) {
        vec![opener.to_string()]
    } else {
        Vec::new()
    }
}

fn extract_signoffs(text: &str) -> Vec<String> {
    let known = [
        "cheers",
        "best",
        "regards",
        "thanks",
        "thank you",
        "talk soon",
    ];
    text.lines()
        .rev()
        .map(str::trim)
        .find(|line| known.iter().any(|known| line.eq_ignore_ascii_case(known)))
        .map(|line| vec![line.to_string()])
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casual_text_has_lower_formality_than_formal_text() {
        let casual = compute_metrics("hey! can't wait to see this 😊 thanks!");
        let formal = compute_metrics(
            "Dear Alice, I reviewed the proposal and will provide written feedback tomorrow.",
        );
        assert!(casual.formality_score < formal.formality_score);
    }
}
