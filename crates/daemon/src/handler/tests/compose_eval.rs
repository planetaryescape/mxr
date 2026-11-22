use super::*;
use crate::handler::compose_corpus::grade;
use crate::handler::compose_eval::quick;
use crate::handler::draft_output::clean_draft;
use mxr_composer::check::Check;
use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
use mxr_protocol::ResponseData;
use mxr_provider_fake::fixtures::compose;
use std::sync::{Arc, Mutex};

fn check_for(defect: &str) -> Option<Check> {
    match defect {
        "bad: subject line" | "bad: recap of thread" | "bad: duplicate signature" => {
            Some(Check::Leftovers)
        }
        "bad: dropped typed sentence" | "bad: reversed typed sentence" => Some(Check::Kept),
        "bad: invented price" => Some(Check::Invented),
        "bad: preamble" => Some(Check::Leftovers),
        "bad: now for no" | "bad: borrowed negative for time" => Some(Check::Answers),
        _ => None,
    }
}

#[test]
fn every_good_output_passes_and_every_defect_fails_the_check_it_names() {
    let mut defects = 0;
    for case in compose::cases() {
        for recording in case.recorded {
            let text = clean_draft(recording.text, Some(compose::ME_NAME));
            let report = grade(case, recording.text, &text);
            if recording.name.starts_with("good") {
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
