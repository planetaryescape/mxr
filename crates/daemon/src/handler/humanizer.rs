use super::HandlerResult;
use crate::state::AppState;
use mxr_humanizer::{score, HumanizerOpts};
use mxr_llm::{ChatMessage, CompletionRequest, LlmError, LlmFeature};
use mxr_protocol::{HumanizerHitData, HumanizerReportSummaryData, ResponseData};
use mxr_reader::{clean, ReaderConfig};

pub(super) async fn score_text(text: &str) -> HandlerResult {
    Ok(ResponseData::HumanizerReport {
        report: report_summary(score(text, &HumanizerOpts::default())),
    })
}

pub(super) async fn rewrite_text(
    state: &AppState,
    text: &str,
    max_iterations: Option<u8>,
) -> HandlerResult {
    let (text, report, iterations) = rewrite_to_threshold(state, text.to_string(), max_iterations)
        .await
        .map_err(|error| error.to_string())?;
    Ok(ResponseData::HumanizedText {
        text,
        report,
        iterations,
    })
}

pub(crate) async fn rewrite_to_threshold(
    state: &AppState,
    text: String,
    max_iterations: Option<u8>,
) -> Result<(String, HumanizerReportSummaryData, u8), String> {
    let config = state.config_snapshot().humanizer;
    let opts = HumanizerOpts {
        score_threshold: config.score_threshold,
    };
    let cleaned = clean(Some(&text), None, &ReaderConfig::default()).content;
    let initial = score(&cleaned, &opts);
    if !config.enabled || initial.score >= config.score_threshold {
        return Ok((text, report_summary(initial), 0));
    }
    if !config.auto_fix {
        return Ok((text, report_summary(initial), 0));
    }

    let max_iterations = max_iterations
        .unwrap_or(config.max_rewrite_iterations)
        .min(config.max_rewrite_iterations)
        .max(1);
    let original_text = text.clone();
    let original_score = initial.score;
    let mut current_text = text;
    let mut current_report = initial;
    let mut iterations = 0;

    for _ in 0..max_iterations {
        let prompt = rewrite_prompt(&current_text, &current_report);
        let response = match state
            .llm
            .for_feature(LlmFeature::HumanizeRewrite)
            .complete(CompletionRequest {
                messages: vec![ChatMessage::user(prompt)],
                max_tokens: Some(600),
                temperature: Some(0.3),
            })
            .await
        {
            Ok(response) => response,
            Err(LlmError::Disabled) => break,
            Err(error) => return Err(format!("LLM error: {error}")),
        };
        let candidate = response.content.trim().to_string();
        if candidate.is_empty() {
            break;
        }
        let candidate_cleaned = clean(Some(&candidate), None, &ReaderConfig::default()).content;
        let candidate_report = score(&candidate_cleaned, &opts);
        if candidate_report.score <= current_report.score {
            break;
        }
        current_text = candidate;
        current_report = candidate_report;
        iterations += 1;
        if current_report.score >= config.score_threshold {
            break;
        }
    }

    if iterations > 0 && current_report.score.saturating_sub(original_score) >= 10 {
        Ok((current_text, report_summary(current_report), iterations))
    } else {
        Ok((original_text, report_summary(score(&cleaned, &opts)), 0))
    }
}

fn rewrite_prompt(text: &str, report: &mxr_humanizer::HumanizerReport) -> String {
    let mut prompt = String::from(
        "Rewrite the draft below to remove the flagged AI-writing patterns while preserving meaning, tone, and the recipient-specific voice match. Return only the rewritten draft.\n\n[FLAGGED PATTERNS]\n",
    );
    for hit in report.hits.iter().take(12) {
        prompt.push_str(&format!(
            "- {:?}: {:?}{}\n",
            hit.category,
            hit.matched,
            hit.suggestion
                .as_deref()
                .map(|suggestion| format!(" ({suggestion})"))
                .unwrap_or_default()
        ));
    }
    prompt.push_str("\n[DRAFT]\n");
    prompt.push_str(text);
    prompt
}

pub(crate) fn report_summary(report: mxr_humanizer::HumanizerReport) -> HumanizerReportSummaryData {
    HumanizerReportSummaryData {
        score: report.score,
        hits: report
            .hits
            .into_iter()
            .take(8)
            .map(|hit| HumanizerHitData {
                category: format!("{:?}", hit.category).to_ascii_lowercase(),
                matched: hit.matched,
                suggestion: hit.suggestion,
            })
            .collect(),
    }
}
