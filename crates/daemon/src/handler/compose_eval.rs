//! `mxr draft eval --set quick`: draft every case of the labelled compose
//! corpus with the configured draft model and grade each draft against what
//! its case required (blueprint 34). Cases run one after another, one
//! request each. Nothing is saved, sent or logged, and the humanizer rewrite
//! pass does not run, so the numbers are the draft pass alone.

use super::compose_corpus;
use super::diagnostics_impl::emit_operation_event;
use super::draft_compose::complete_with_retry;
use super::draft_output;
use super::draft_provenance::DraftPolicy;
use super::draft_voice;
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_composer::check::Check;
use mxr_llm::{LlmFeature, PinnedLlm};
use mxr_protocol::{
    DaemonEvent, DraftEvalCheckCountData, DraftEvalCheckData, DraftEvalQuickCaseData,
    DraftEvalQuickSummaryData, ResponseData,
};
use mxr_provider_fake::fixtures::compose;

const NO_MODEL: &str = "Draft eval --set quick needs a model: no model is set up for drafting. \
Configure one under [llm], then run it again. `--set mine` also needs a model.";

/// The operation name on each progress event, so a client can tell this run's
/// events from any other operation's.
pub(crate) const PROGRESS_OPERATION: &str = "draft-eval-quick";

pub(super) async fn quick(state: &AppState) -> HandlerResult {
    let policy = DraftPolicy::pin(state, LlmFeature::DraftAssist);
    if !policy.llm.is_configured() {
        return Err(HandlerError::Message(NO_MODEL.to_string()));
    }
    if let Some(reason) = policy.llm.blocked_reason() {
        return Err(HandlerError::Message(format!(
            "Draft eval --set quick is blocked by your privacy settings: {reason}"
        )));
    }
    let today = draft_voice::today();
    let mut model = String::new();
    let mut cases = Vec::new();
    let total = u32::try_from(compose::cases().count()).unwrap_or(u32::MAX);
    let operation_id = uuid::Uuid::now_v7().to_string();
    for (index, case) in compose::cases().enumerate() {
        cases.push(draft_case(&policy.llm, case, &today, &mut model).await);
        // One event per case, after its draft is graded, so a client can show
        // progress through a run that takes minutes.
        emit_operation_event(
            state,
            DaemonEvent::OperationProgress {
                operation_id: operation_id.clone(),
                operation: PROGRESS_OPERATION.to_string(),
                account_id: None,
                current: u32::try_from(index + 1).unwrap_or(u32::MAX),
                total: Some(total),
                message: case.id.to_string(),
            },
        );
    }
    let summary = summarise(&cases, model);
    Ok(ResponseData::DraftEvalQuick { cases, summary })
}

async fn draft_case(
    llm: &PinnedLlm,
    case: &compose::Case,
    today: &str,
    model: &mut String,
) -> DraftEvalQuickCaseData {
    let mut out = DraftEvalQuickCaseData {
        id: case.id.to_string(),
        description: case.description.to_string(),
        draft: String::new(),
        words: 0,
        checks: Vec::new(),
        style_score: 0,
        error: None,
    };
    let assembled = compose_corpus::assemble(case, today);
    match complete_with_retry(llm, &assembled.prompt, assembled.max_tokens).await {
        Ok((raw, used)) => {
            model.clone_from(&used);
            let text = draft_output::clean_draft(&raw, Some(compose::ME_NAME));
            let report = compose_corpus::grade(case, &raw, &text);
            out.words = u32::try_from(text.split_whitespace().count()).unwrap_or(u32::MAX);
            out.checks = report
                .grades
                .into_iter()
                .map(|grade| DraftEvalCheckData {
                    check: grade.check.as_str().to_string(),
                    pass: grade.pass,
                    requirement: grade.requirement,
                    detail: grade.detail,
                })
                .collect();
            out.style_score = report.style.score;
            out.draft = text;
        }
        Err(error) => out.error = Some(error.to_string()),
    }
    out
}

fn summarise(cases: &[DraftEvalQuickCaseData], model: String) -> DraftEvalQuickSummaryData {
    let drafted: Vec<&DraftEvalQuickCaseData> =
        cases.iter().filter(|case| case.error.is_none()).collect();
    let checks = Check::ALL
        .iter()
        .map(|check| {
            let name = check.as_str();
            let evaluated = drafted
                .iter()
                .filter(|case| case.checks.iter().any(|grade| grade.check == name))
                .count();
            let passed = drafted
                .iter()
                .filter(|case| {
                    case.checks
                        .iter()
                        .any(|grade| grade.check == name && grade.pass)
                })
                .count();
            DraftEvalCheckCountData {
                check: name.to_string(),
                passed: u32::try_from(passed).unwrap_or(u32::MAX),
                evaluated: u32::try_from(evaluated).unwrap_or(u32::MAX),
            }
        })
        .collect();
    let style_total: f64 = drafted.iter().map(|case| f64::from(case.style_score)).sum();
    let style_mean = if drafted.is_empty() {
        0.0
    } else {
        style_total / drafted.len() as f64
    };
    DraftEvalQuickSummaryData {
        cases: u32::try_from(cases.len()).unwrap_or(u32::MAX),
        errors: u32::try_from(cases.len() - drafted.len()).unwrap_or(u32::MAX),
        checks,
        style_mean,
        model,
    }
}
