use super::HandlerResult;
use mxr_humanizer::{score, HumanizerOpts};
use mxr_protocol::{HumanizerHitData, HumanizerReportSummaryData, ResponseData};

pub(super) async fn score_text(text: &str) -> HandlerResult {
    Ok(ResponseData::HumanizerReport {
        report: report_summary(score(text, &HumanizerOpts::default())),
    })
}

pub(super) async fn rewrite_text(text: &str, _max_iterations: Option<u8>) -> HandlerResult {
    Ok(ResponseData::HumanizedText {
        text: text.to_string(),
        report: report_summary(score(text, &HumanizerOpts::default())),
        iterations: 0,
    })
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
