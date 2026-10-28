//! `mxr draft eval --set quick` over the labelled compose corpus, with no
//! model in CI. The snapshots are the reviewable record: the prompt today's
//! code builds for every case, and the grade table for recorded model
//! outputs, good and bad. A prompt or check change shows up as a diff in them.
//! The live part runs the same cases through a scripted model to pin how many
//! requests a run makes and what it reports.

use super::*;
use crate::handler::compose_corpus::{assemble, grade};
use crate::handler::compose_eval::quick;
use crate::handler::draft_output::clean_draft;
use mxr_composer::check::{Check, Report};
use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
use mxr_protocol::ResponseData;
use mxr_provider_fake::fixtures::compose::{self, Case, DraftState};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

/// A fixed date, so the prompts do not move with the clock.
const TODAY: &str = "Wednesday 21 October 2026";

fn prompts_report() -> String {
    let mut out = String::new();
    let mut system_written = false;
    for case in compose::cases() {
        let assembled = assemble(case, TODAY);
        // The system message as the model receives it: the base task plus the
        // untrusted-mail guard, exactly as the draft path appends it.
        if !system_written {
            out.push_str("SYSTEM MESSAGE (the same for every case)\n");
            out.push_str(&mxr_llm::guarded_system_prompt(&assembled.prompt.system));
            out.push_str("\n\n");
            system_written = true;
        }
        let typed = match case.draft {
            DraftState::Empty => "nothing typed",
            DraftState::Typed(_) => "typed text sent: finish",
            DraftState::Selection { .. } => "typed text and selection sent: edit selection",
        };
        let near = case
            .twin_of
            .map_or(String::new(), |base| format!(" · near miss of {base}"));
        let instruction = if case.instruction.is_empty() {
            String::new()
        } else {
            format!(" · instruction \"{}\"", case.instruction)
        };
        writeln!(
            out,
            "== {}{near} · {typed}{instruction} · max_tokens {}",
            case.id, assembled.max_tokens
        )
        .unwrap_or_default();
        out.push_str(&assembled.prompt.user);
        out.push_str("\n\n");
    }
    out
}

/// One row per recorded output, with each check's verdict, then the failures
/// spelled out: what the draft had to do and what it did.
fn grade_report() -> String {
    let mut out = String::new();
    let mut rows: Vec<(&Case, &str, Report)> = Vec::new();
    for case in compose::cases() {
        for recording in case.recorded {
            let text = clean_draft(recording.text, Some(compose::ME_NAME));
            rows.push((case, recording.name, grade(case, recording.text, &text)));
        }
    }
    let verdict = |report: &Report, check: Check| match report.passed(check) {
        Some(true) => "pass".to_string(),
        Some(false) => "FAIL".to_string(),
        None => "-".to_string(),
    };
    writeln!(
        out,
        "{:<28} {:<30} {:<6} {:<8} {:<6} {:<9} {:<7} {:<10} style score: marks",
        "case", "output", "kept", "answers", "gaps", "invented", "length", "leftovers"
    )
    .unwrap_or_default();
    for (case, name, report) in &rows {
        writeln!(
            out,
            "{:<28} {:<30} {:<6} {:<8} {:<6} {:<9} {:<7} {:<10} {}",
            case.id,
            name,
            verdict(report, Check::Kept),
            verdict(report, Check::Answers),
            verdict(report, Check::Gaps),
            verdict(report, Check::Invented),
            verdict(report, Check::Length),
            verdict(report, Check::Leftovers),
            if report.style.patterns.is_empty() {
                report.style.score.to_string()
            } else {
                format!(
                    "{}: {}",
                    report.style.score,
                    report.style.patterns.join(", ")
                )
            },
        )
        .unwrap_or_default();
    }
    out.push('\n');
    for check in Check::ALL {
        let failed = rows
            .iter()
            .filter(|(_, _, report)| report.passed(check) == Some(false))
            .count();
        writeln!(
            out,
            "{:<10} {} of {} outputs fail",
            check.as_str(),
            failed,
            rows.len()
        )
        .unwrap_or_default();
    }
    out.push_str("\nFailures\n");
    for (case, name, report) in &rows {
        for grade in report.grades.iter().filter(|grade| !grade.pass) {
            writeln!(
                out,
                "{} · {name} · {}: {} -> {}",
                case.id,
                grade.check.as_str(),
                grade.requirement,
                grade.detail
            )
            .unwrap_or_default();
        }
    }
    out
}

/// The check a recorded defect is meant to trip.
fn check_for(defect: &str) -> Option<Check> {
    match defect {
        "bad: subject line" | "bad: recap of thread" | "bad: duplicate signature" => {
            Some(Check::Leftovers)
        }
        "bad: dropped typed sentence" | "bad: reversed typed sentence" => Some(Check::Kept),
        "bad: invented price" => Some(Check::Invented),
        "bad: preamble" => Some(Check::Leftovers),
        "bad: now for no" => Some(Check::Answers),
        _ => None,
    }
}

#[test]
fn the_prompts_for_the_quick_set_are_snapshotted() {
    insta::assert_snapshot!(prompts_report());
}

#[test]
fn the_recorded_outputs_are_graded_and_the_table_is_snapshotted() {
    insta::assert_snapshot!(grade_report());
}

#[test]
fn every_good_output_passes_and_every_defect_fails_the_check_it_names() {
    let mut defects = 0;
    for case in compose::cases() {
        for recording in case.recorded {
            let text = clean_draft(recording.text, Some(compose::ME_NAME));
            let report = grade(case, recording.text, &text);
            if recording.name == "good" {
                assert!(
                    report.all_pass(),
                    "{} good output: {:?}",
                    case.id,
                    report.grades
                );
                continue;
            }
            defects += 1;
            let expected = check_for(recording.name)
                .unwrap_or_else(|| panic!("no check for {}", recording.name));
            assert_eq!(
                report.passed(expected),
                Some(false),
                "{} {}: {:?}",
                case.id,
                recording.name,
                report.grades
            );
        }
    }
    assert!(defects >= 5, "only {defects} defects recorded");
}

#[test]
fn offline_grader_names_style_marks_and_still_rejects_invented_amounts() {
    let case = compose::cases()
        .next()
        .expect("the labelled corpus is populated");
    let marked = "Additionally, the review is ready.";
    let plain = "The review is ready.";
    let marked_report = grade(case, marked, marked);
    let plain_report = grade(case, plain, plain);
    assert!(marked_report.style.score < plain_report.style.score);
    assert!(marked_report
        .style
        .patterns
        .iter()
        .any(|name| name == "Overused AI words"));

    let invented = "The total is $777.";
    let invented_report = grade(case, invented, invented);
    assert_eq!(invented_report.passed(Check::Invented), Some(false));
}

/// Answers every draft the same way, and keeps what it was asked.
#[derive(Default)]
struct Scripted {
    prompts: Mutex<Vec<String>>,
}

#[async_trait]
impl LlmProvider for Scripted {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.prompts
            .lock()
            .unwrap()
            .push(req.messages[1].content.clone());
        Ok(CompletionResponse {
            content: "Hi,\n\nSounds good.\n\nAlex".into(),
            model: "scripted".into(),
            finish_reason: Some("stop".into()),
        })
    }

    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            context_window: 8192,
            supports_streaming: false,
        }
    }

    fn model_name(&self) -> &str {
        "scripted"
    }
}

#[tokio::test]
async fn the_quick_set_drafts_each_case_once_and_grades_every_check() {
    let state = AppState::in_memory().await.unwrap();
    let llm = Arc::new(Scripted::default());
    state.llm.replace(llm.clone());

    let ResponseData::DraftEvalQuick { cases, summary } = quick(&state).await.unwrap() else {
        panic!("expected a quick eval report");
    };
    assert_eq!(cases.len(), 40);
    assert_eq!(
        llm.prompts.lock().unwrap().len(),
        40,
        "one request per case"
    );
    assert!(cases.iter().all(|case| case.error.is_none()), "{cases:?}");
    assert!(cases.iter().all(|case| case.checks.len() == 6));
    assert_eq!(summary.cases, 40);
    assert_eq!(summary.errors, 0);
    assert_eq!(summary.model, "scripted");
    // The scripted reply keeps nothing, so every typed case fails `kept`; the
    // 18 cases with nothing typed pass it.
    let kept = summary
        .checks
        .iter()
        .find(|count| count.check == "kept")
        .expect("kept is counted");
    assert_eq!((kept.passed, kept.evaluated), (18, 40));
    for prompt in llm.prompts.lock().unwrap().iter() {
        assert!(prompt.contains("[TASK]") && prompt.contains("[ABOUT ME]"));
    }
}

#[tokio::test]
async fn quick_without_a_model_says_one_is_needed() {
    let state = AppState::in_memory().await.unwrap();
    let error = quick(&state).await.unwrap_err();
    assert!(error.to_string().contains("needs a model"), "{error}");
}

#[tokio::test]
async fn the_quick_set_reports_progress_once_per_case_in_order() {
    let state = AppState::in_memory().await.unwrap();
    state.llm.replace(Arc::new(Scripted::default()));
    let mut events = state.event_tx.subscribe();
    quick(&state).await.unwrap();

    let mut progress = Vec::new();
    while let Ok(message) = events.try_recv() {
        if let mxr_protocol::IpcPayload::Event(mxr_protocol::DaemonEvent::OperationProgress {
            operation,
            current,
            total,
            message,
            ..
        }) = message.payload
        {
            progress.push((operation, current, total, message));
        }
    }
    let ids: Vec<&str> = compose::cases().map(|case| case.id).collect();
    assert_eq!(progress.len(), ids.len(), "one event per case");
    for (index, (operation, current, total, id)) in progress.iter().enumerate() {
        assert_eq!(operation, "draft-eval-quick");
        assert_eq!(*current as usize, index + 1);
        assert_eq!(*total, Some(ids.len() as u32));
        assert_eq!(id, ids[index]);
    }
}
