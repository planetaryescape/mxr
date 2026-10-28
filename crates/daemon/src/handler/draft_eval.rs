//! `mxr draft eval`: how close do AI drafts get to what the user actually
//! writes? Each of the user's recent replies is replayed: the conversation
//! and the voice material are rebuilt as they stood just before the reply
//! (`DraftRequest::before`), a draft is generated through the same path the
//! product uses, and the two are compared. Local only; nothing is saved,
//! sent or written to the activity log.
//!
//! The relationship summary and stylometry are the current ones, so they
//! may reflect the reply being replayed; the examples, habits and
//! conversation never do.

use super::draft_compose::{draft_in_thread, DraftRequest};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_composer::check::replay_invented_numbers;
use mxr_core::id::AccountId;
use mxr_protocol::{DraftEvalCaseData, DraftEvalSummaryData, ResponseData};
use mxr_relationship::{analyse_habits, clean_for_voice};
use mxr_store::MyReplySample;

pub(crate) const MAX_EVAL_CASES: u32 = 50;
const NO_ACCOUNT: &str =
    "No account is set up. Add one with `mxr accounts add`, or pass --account once one exists.";

pub(super) async fn draft_eval(
    state: &AppState,
    account_id: Option<&AccountId>,
    limit: u32,
    rewrite: bool,
) -> HandlerResult {
    let account_id = match account_id {
        Some(id) => id.clone(),
        None => state
            .default_account_id_opt()
            .ok_or_else(|| HandlerError::InvalidRequest(NO_ACCOUNT.to_string()))?,
    };
    let limit = limit.clamp(1, MAX_EVAL_CASES);
    // Ask for more than needed: very short or template replies are skipped.
    let samples = state
        .store
        .my_reply_samples(&account_id, None, None, None, limit * 3)
        .await
        .map_err(|error| error.to_string())?;
    let mut cases = Vec::new();
    let mut model = String::new();
    for sample in samples {
        if cases.len() as u32 >= limit {
            break;
        }
        let actual = clean_for_voice(&sample.reply_body);
        let words = actual.split_whitespace().count();
        if !(3..=350).contains(&words) {
            continue;
        }
        cases.push(replay(state, &account_id, &sample, actual, &mut model, rewrite).await);
    }
    let summary = summarise(&cases, model);
    Ok(ResponseData::DraftEval { cases, summary })
}

async fn replay(
    state: &AppState,
    account_id: &AccountId,
    sample: &MyReplySample,
    actual: String,
    model: &mut String,
    rewrite: bool,
) -> DraftEvalCaseData {
    let mut case = DraftEvalCaseData {
        reply_message_id: sample.reply_message_id.clone(),
        counterparty: sample.counterparty_email.clone(),
        actual_words: actual.split_whitespace().count() as u32,
        actual,
        draft: String::new(),
        draft_words: 0,
        greeting_match: false,
        sign_off_match: false,
        invented_numbers: Vec::new(),
        placeholders: 0,
        error: None,
    };
    let envelopes: Vec<_> = match state
        .store
        .get_thread_envelopes_for_model(&sample.thread_id)
        .await
    {
        Ok(envelopes) => envelopes
            .into_iter()
            .filter(|envelope| envelope.date < sample.replied_at)
            .collect(),
        Err(error) => {
            case.error = Some(error.to_string());
            return case;
        }
    };
    if envelopes.is_empty() {
        case.error = Some("the conversation before this reply isn't stored".to_string());
        return case;
    }
    let known: String = {
        let mut text = String::new();
        for envelope in &envelopes {
            if let Ok(Some(body)) = state.store.get_body(&envelope.id).await {
                text.push_str(&body.text_plain.or(body.text_html).unwrap_or_default());
                text.push('\n');
            }
            text.push_str(&envelope.subject);
            text.push('\n');
        }
        text
    };
    let request = DraftRequest {
        instruction: "",
        register: None,
        length_hint: None,
        before: Some(sample.replied_at),
        rewrite_policy: if rewrite {
            super::draft_compose::DraftRewritePolicy::Enabled
        } else {
            super::draft_compose::DraftRewritePolicy::Disabled
        },
    };
    let result = draft_in_thread(
        state,
        account_id,
        &request,
        envelopes,
        Some(&sample.parent_message_id),
        None,
    )
    .await;
    match result {
        Ok(ResponseData::DraftSuggestion {
            body, model: used, ..
        }) => {
            model.clone_from(&used);
            case.draft_words = body.split_whitespace().count() as u32;
            case.placeholders = body.matches("[[?:").count() as u32;
            case.invented_numbers = replay_invented_numbers(&body, &known);
            let names: Vec<&str> = Vec::new();
            let mine = analyse_habits(&[case.actual.as_str()], &names);
            let theirs = analyse_habits(&[body.as_str()], &names);
            case.greeting_match = same(
                mine.greeting.as_ref().map(|habit| habit.text.as_str()),
                theirs.greeting.as_ref().map(|habit| habit.text.as_str()),
            );
            case.sign_off_match = same(
                mine.sign_off.as_ref().map(|habit| habit.text.as_str()),
                theirs.sign_off.as_ref().map(|habit| habit.text.as_str()),
            );
            case.draft = body;
        }
        Ok(other) => case.error = Some(format!("unexpected response: {other:?}")),
        Err(error) => case.error = Some(format!("{error:?}")),
    }
    case
}

/// Greetings and sign-offs compared by their words, ignoring case and
/// punctuation ("Cheers," matches "cheers").
fn same(left: Option<&str>, right: Option<&str>) -> bool {
    let key = |text: &str| {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
    };
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => key(left) == key(right),
        _ => false,
    }
}

fn summarise(cases: &[DraftEvalCaseData], model: String) -> DraftEvalSummaryData {
    let done: Vec<&DraftEvalCaseData> = cases.iter().filter(|case| case.error.is_none()).collect();
    let n = done.len().max(1) as f64;
    let mut ratios: Vec<f64> = done
        .iter()
        .map(|case| f64::from(case.draft_words) / f64::from(case.actual_words.max(1)))
        .collect();
    ratios.sort_by(f64::total_cmp);
    DraftEvalSummaryData {
        cases: cases.len() as u32,
        failed: (cases.len() - done.len()) as u32,
        median_length_ratio: ratios.get(ratios.len() / 2).copied().unwrap_or(0.0),
        greeting_match_rate: done.iter().filter(|case| case.greeting_match).count() as f64 / n,
        sign_off_match_rate: done.iter().filter(|case| case.sign_off_match).count() as f64 / n,
        invented_number_rate: done
            .iter()
            .filter(|case| !case.invented_numbers.is_empty())
            .count() as f64
            / n,
        model,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::TestEnvelopeBuilder;
    use mxr_core::types::{MessageBody, MessageDirection, MessageMetadata};
    use mxr_llm::{CompletionRequest, CompletionResponse, LlmCapabilities, LlmError, LlmProvider};
    use mxr_store::{ContactRelationshipSummaryRecord, ContactStyleRecord, UserVoiceProfileRecord};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Recorder {
        prompts: Mutex<Vec<String>>,
        calls: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for Recorder {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            *self.calls.lock().unwrap() += 1;
            if req.messages[0].content.contains("ghostwriting") {
                self.prompts
                    .lock()
                    .unwrap()
                    .push(req.messages[1].content.clone());
            }
            Ok(CompletionResponse {
                content: "Moreover, delve into the fast-paced landscape. This robust framework serves as a testament. Additionally, it is crucial to highlight the pivotal role it plays.\ns".into(),
                model: "test-llm".into(),
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
            "test-llm"
        }
    }

    #[tokio::test]
    async fn replays_a_reply_without_showing_the_model_that_reply() {
        let state = AppState::in_memory().await.unwrap();
        let mut config = state.config_snapshot();
        config.humanizer.apply_to_drafts = true;
        config.humanizer.enabled = true;
        config.humanizer.auto_fix = true;
        config.humanizer.score_threshold = 100;
        state.set_config_for_test(config).await;
        let llm = Arc::new(Recorder::default());
        state.llm.replace(llm.clone());
        let account_id = state.default_account_id();
        let me = state
            .store
            .list_account_addresses(&account_id)
            .await
            .unwrap()
            .into_iter()
            .next()
            .map_or_else(|| "user@example.com".into(), |address| address.email);
        let now = chrono::Utc::now();
        let thread = mxr_core::ThreadId::new();

        let mut ask = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(thread.clone())
            .provider_id("ask")
            .sender_address("Alice", "alice@example.com")
            .date(now - chrono::Duration::hours(3))
            .build();
        ask.message_id_header = Some("<ask@x>".into());
        let mut reply = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(thread.clone())
            .provider_id("reply")
            .sender_address("Sam Rivers", &me)
            .recipient_address(Some("Alice"), "alice@example.com")
            .date(now - chrono::Duration::hours(2))
            .build();
        reply.in_reply_to = Some("<ask@x>".into());
        for (envelope, direction, text) in [
            (&ask, MessageDirection::Inbound, "Can you do Friday at 2pm?"),
            (
                &reply,
                MessageDirection::Outbound,
                "SECRET-ACTUAL-REPLY friday 2pm is great\ns",
            ),
        ] {
            state
                .store
                .upsert_envelope_with_direction(envelope, direction)
                .await
                .unwrap();
            state
                .store
                .insert_body(&MessageBody {
                    message_id: envelope.id.clone(),
                    text_plain: Some(text.into()),
                    text_html: None,
                    attachments: Vec::new(),
                    fetched_at: now,
                    metadata: MessageMetadata::default(),
                })
                .await
                .unwrap();
        }
        state.store.try_create_reply_pair(&reply).await.unwrap();
        let computed_after_reply = chrono::Utc::now();
        state
            .store
            .upsert_contact_relationship_summary(&ContactRelationshipSummaryRecord {
                account_id: account_id.clone(),
                email: "alice@example.com".into(),
                text: "PRIVATE-AFTER-CUTOFF-SUMMARY".into(),
                model: "test-model".into(),
                known_topics: Vec::new(),
                computed_at: computed_after_reply,
                source_hash: "after-reply".into(),
                last_error: None,
            })
            .await
            .unwrap();
        state
            .store
            .upsert_contact_style(&ContactStyleRecord {
                account_id: account_id.clone(),
                email: "alice@example.com".into(),
                formality_score: 0.95,
                formality_score_theirs: 0.95,
                avg_sentence_len: 40.0,
                avg_sentence_len_theirs: 40.0,
                msg_count_used: 30,
                msg_count_used_theirs: 20,
                metrics_json: "{}".into(),
                metrics_json_theirs: "{}".into(),
                computed_at: computed_after_reply,
                source_hash: "after-reply".into(),
                drift_detected: false,
                drift_reason: None,
                drift_detected_at: None,
            })
            .await
            .unwrap();
        state
            .store
            .upsert_user_voice_profile(&UserVoiceProfileRecord {
                account_id: account_id.clone(),
                formality_score: 0.95,
                avg_sentence_len: 40.0,
                msg_count_used: 100,
                metrics_json: "{}".into(),
                register_modes: Vec::new(),
                computed_at: computed_after_reply,
                source_hash: "after-reply".into(),
            })
            .await
            .unwrap();

        let response = draft_eval(&state, Some(&account_id), 5, false)
            .await
            .unwrap();
        let ResponseData::DraftEval { cases, summary } = response else {
            panic!("expected an eval report");
        };
        assert_eq!(cases.len(), 1, "{cases:?}");
        let case = &cases[0];
        assert!(case.error.is_none(), "{case:?}");
        assert!(case.actual.contains("SECRET-ACTUAL-REPLY"));
        assert_eq!(case.draft, "Moreover, delve into the fast-paced landscape. This robust framework serves as a testament. Additionally, it is crucial to highlight the pivotal role it plays.\ns");
        assert!(case.sign_off_match, "both sign off 's'");
        assert!(
            case.invented_numbers.is_empty(),
            "{:?}",
            case.invented_numbers
        );
        assert_eq!(summary.cases, 1);

        {
            let prompts = llm.prompts.lock().unwrap();
            assert_eq!(prompts.len(), 1);
            assert!(prompts[0].contains("Can you do Friday at 2pm?"));
            assert!(
                !prompts[0].contains("SECRET-ACTUAL-REPLY"),
                "the replayed reply leaked into its own prompt"
            );
            assert!(!prompts[0].contains("PRIVATE-AFTER-CUTOFF-SUMMARY"));
            assert!(!prompts[0].contains("My usual register is formal."));
            assert!(!prompts[0].contains("My usual email length is long."));
        }
        assert_eq!(
            *llm.calls.lock().unwrap(),
            1,
            "replay does not rewrite by default"
        );

        assert!(super::super::humanizer::rewrite_due(
            &state,
            "Moreover, delve into the fast-paced landscape. This robust framework serves as a testament. Additionally, it is crucial to highlight the pivotal role it plays.\ns"
        ));
        draft_eval(&state, Some(&account_id), 5, true)
            .await
            .unwrap();
        assert!(
            *llm.calls.lock().unwrap() > 2,
            "--rewrite enables the configured rewrite pass"
        );
    }

    #[tokio::test]
    async fn replaying_my_replies_with_no_account_is_a_clear_error() {
        let state = AppState::in_memory_without_accounts().await.unwrap();
        let error = draft_eval(&state, None, 5, false).await.unwrap_err();
        assert!(
            error.to_string().contains("No account is set up"),
            "{error}"
        );
    }

    #[test]
    fn greeting_comparison_ignores_case_and_punctuation() {
        assert!(same(Some("Hey {name},"), Some("hey {name}")));
        assert!(same(None, None));
        assert!(!same(Some("Hi {name},"), None));
    }
}
