use super::HandlerResult;
use crate::state::AppState;
use mxr_humanizer::{score, HumanizerOpts};
use mxr_llm::{
    wrap_untrusted_mail, ChatMessage, CompletionRequest, LlmError, LlmFeature, UNTRUSTED_MAIL_GUARD,
};
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
        .map_err(|error| error.clone())?;
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
    rewrite_to_threshold_with_context(state, text, max_iterations, None).await
}

pub(crate) async fn rewrite_to_threshold_with_context(
    state: &AppState,
    text: String,
    max_iterations: Option<u8>,
    voice_context: Option<&str>,
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
        let prompt = rewrite_prompt(&current_text, &current_report, voice_context);
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

fn rewrite_prompt(
    text: &str,
    report: &mxr_humanizer::HumanizerReport,
    voice_context: Option<&str>,
) -> String {
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
    if let Some(voice_context) = voice_context.filter(|value| !value.trim().is_empty()) {
        // Voice context is mail-derived (relationship summary/stylometry):
        // delimit it as untrusted content and lead the prompt with the
        // guard. The draft is the user's own text being rewritten, so it
        // stays outside the markers. (The rewrite is accepted only if it
        // improves the humanizer score, and its output is never auto-sent
        // — that score gate and the no-auto-send invariant are the
        // boundary; this preamble is defense-in-depth.)
        prompt.insert_str(0, &format!("{UNTRUSTED_MAIL_GUARD}\n\n"));
        prompt.push_str("\n[ORIGINAL VOICE CONTEXT]\n");
        prompt.push_str(&wrap_untrusted_mail(voice_context.trim()));
        prompt.push('\n');
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_prompt_wraps_mail_derived_voice_context_and_guards() {
        let report = score("Some draft text.", &HumanizerOpts::default());
        let prompt = rewrite_prompt("draft body here", &report, Some("VOICE-CTX-MARKER"));
        assert!(
            prompt.contains(UNTRUSTED_MAIL_GUARD),
            "prompt must carry the injection guard when voice context is present"
        );
        // The guard text quotes the marker strings, so use rfind to locate
        // the real wrapper (the last occurrence of each marker).
        let begin = prompt
            .rfind(mxr_llm::UNTRUSTED_MAIL_BEGIN)
            .expect("begin marker present");
        let end = prompt
            .rfind(mxr_llm::UNTRUSTED_MAIL_END)
            .expect("end marker present");
        let ctx = prompt
            .find("VOICE-CTX-MARKER")
            .expect("voice context present");
        assert!(
            begin < ctx && ctx < end,
            "voice context must sit between the untrusted-content markers"
        );
        // The draft being rewritten stays outside the markers.
        assert!(
            prompt.find("draft body here").expect("draft present") > end,
            "the draft to rewrite must not be wrapped as untrusted content"
        );
    }

    #[test]
    fn rewrite_prompt_without_voice_context_is_unwrapped() {
        // The standalone humanize path has no mail-derived input, so it
        // gets no guard/markers.
        let report = score("Some draft text.", &HumanizerOpts::default());
        let prompt = rewrite_prompt("draft body", &report, None);
        assert!(!prompt.contains(mxr_llm::UNTRUSTED_MAIL_BEGIN));
        assert!(!prompt.contains(UNTRUSTED_MAIL_GUARD));
    }
}
