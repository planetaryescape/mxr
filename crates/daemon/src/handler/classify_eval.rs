//! `mxr classify eval`: today's classifier measured against labelled
//! scenarios (blueprint 32, build order 1). Stored mail whose provider id
//! and sender a scenario names is read back the way the product reads it,
//! and each of four answers is compared with the label:
//!
//! * who: the mail kind the shared `describe` path gives (what `GetMessageKind`
//!   and the arrivals line read), over the same sender signals placement uses,
//!   so mail in Spam, which `GetMessageKind` refuses, still has a kind;
//! * home: the arrivals line's destination under today's rules
//!   (`arrivals::effective_rows`), not the mode stored when the mail
//!   arrived, so a rule change shows without re-syncing. Mail with no
//!   arrivals row (history older than 30 days when the account first
//!   synced is not an arrival) is placed live by the same rules, as
//!   `current_bucket` does;
//! * to-do: a to-do row exists with this email as its source;
//! * record: a record row includes this email.
//!
//! Read-only and local. It changes no classification and writes nothing,
//! including no activity row.

use super::desk::{self_matcher, Senders};
use super::modes::MEMBERSHIP_MAX_THREADS;
use super::{arrivals, mail_kind, HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_protocol::{
    ClassifyEvalMismatchData, ClassifyEvalQuestionData, ClassifyEvalSetData,
    ClassifyEvalSummaryData, ResponseData, SenderKindData,
};
use mxr_provider_fake::fixtures::scenarios::{self, Labelled, LABEL_SCHEMA_VERSION};
use mxr_store::EvalCandidate;
use std::collections::{BTreeMap, HashMap};

/// What `home` reads, for the output.
const HOME_PROJECTION: &str = "arrivals line: where each email goes under today's rules, not the mode stored on arrival; mail with no arrivals row is placed live by the same rules";
const NO_LABELLED_MAIL: &str = "No labelled mail here; run mxr demo";
/// An email to place live: its id, thread and whether it is in Spam.
type Wanted = (MessageId, ThreadId, bool);

const QUESTIONS: [&str; 4] = ["who", "home", "todo", "record"];

/// What today's code answers for one email.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Actual {
    who: &'static str,
    who_reason: String,
    /// `None`: the email has no arrivals row, so no home to compare.
    home: Option<Home>,
    todo: bool,
    record: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Home {
    mode: String,
    reason: String,
    /// Worked out live: the email has no arrivals row.
    live: bool,
}

#[derive(Debug, Default)]
struct Tally {
    evaluated: [u32; 4],
    correct: [u32; 4],
    placed_live: u32,
    not_placed: u32,
}

const fn who_name(kind: SenderKindData) -> &'static str {
    match kind {
        SenderKindData::People => "person",
        SenderKindData::Reading => "list",
        SenderKindData::PaperTrail => "machine",
        SenderKindData::ScreenedOut => "screened_out",
    }
}

const fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

/// An arrivals row that was never placed reads `sorting`; both it and a
/// missing row mean there is no home yet.
fn placed(mode: &str) -> Option<&str> {
    (mode != "sorting").then_some(mode)
}

/// Compare one email's answers with its label. `home` is skipped, and
/// counted as not placed, when there is none.
fn grade(
    label: &Labelled,
    actual: &Actual,
    tally: &mut Tally,
    mismatches: &mut Vec<ClassifyEvalMismatchData>,
) {
    let mut check = |question: usize, expected: &str, got: &str, reason: &str| {
        tally.evaluated[question] += 1;
        if expected == got {
            tally.correct[question] += 1;
        } else {
            mismatches.push(ClassifyEvalMismatchData {
                scenario: label.scenario.to_string(),
                provider_id: label.provider_id.clone(),
                question: QUESTIONS[question].to_string(),
                expected: expected.to_string(),
                actual: got.to_string(),
                reason: reason.to_string(),
                twin_of: label.twin_of.map(str::to_string),
                label_review: label
                    .review
                    .filter(|review| review.questions.contains(&QUESTIONS[question]))
                    .map(|review| review.why.to_string()),
            });
        }
    };
    check(0, label.expect.who.as_str(), actual.who, &actual.who_reason);
    match &actual.home {
        Some(home) => {
            tally.placed_live += u32::from(home.live);
            check(1, label.expect.home.as_str(), &home.mode, &home.reason);
        }
        None => tally.not_placed += 1,
    }
    check(2, yes_no(label.expect.todo), yes_no(actual.todo), "");
    check(3, yes_no(label.expect.record), yes_no(actual.record), "");
}

/// Stable order whatever order the store returned rows in: scenario, then
/// provider id in numeric order, then question.
fn sort_mismatches(mismatches: &mut [ClassifyEvalMismatchData]) {
    mismatches.sort_by(|a, b| {
        let question = |m: &ClassifyEvalMismatchData| {
            QUESTIONS
                .iter()
                .position(|q| *q == m.question)
                .unwrap_or(QUESTIONS.len())
        };
        (
            &a.scenario,
            a.provider_id.len(),
            &a.provider_id,
            question(a),
        )
            .cmp(&(
                &b.scenario,
                b.provider_id.len(),
                &b.provider_id,
                question(b),
            ))
    });
}

fn summarise(
    labelled_in_set: u32,
    present: u32,
    unlabelled: u32,
    tally: &Tally,
) -> ClassifyEvalSummaryData {
    ClassifyEvalSummaryData {
        set: ClassifyEvalSetData::Quick,
        label_schema_version: LABEL_SCHEMA_VERSION,
        labelled_in_set,
        present,
        placed_live: tally.placed_live,
        not_placed: tally.not_placed,
        unlabelled,
        questions: QUESTIONS
            .iter()
            .enumerate()
            .map(|(index, question)| {
                let (evaluated, correct) = (tally.evaluated[index], tally.correct[index]);
                ClassifyEvalQuestionData {
                    question: (*question).to_string(),
                    evaluated,
                    correct,
                    accuracy_percent: if evaluated == 0 {
                        0.0
                    } else {
                        (f64::from(correct) / f64::from(evaluated) * 1000.0).round() / 10.0
                    },
                }
            })
            .collect(),
        home_projection: HOME_PROJECTION.to_string(),
        message: (present == 0).then(|| NO_LABELLED_MAIL.to_string()),
    }
}

pub(super) async fn classify_eval(
    state: &AppState,
    account_id: Option<&AccountId>,
    set: ClassifyEvalSetData,
) -> HandlerResult {
    let ClassifyEvalSetData::Quick = set;
    let accounts: Vec<AccountId> = account_id.cloned().into_iter().collect();
    let labels: Vec<&Labelled> = scenarios::quick_set().collect();
    let provider_ids: Vec<&str> = labels.iter().map(|l| l.provider_id.as_str()).collect();

    // Both demo accounts number their mail from 1 and reuse some of the same
    // people, and a smaller demo numbers its fillers differently, so only
    // the scenario account's email from the scenario's sender is the email
    // the label means.
    let scenario_account = scenarios::account_id();
    let present: Vec<_> = state
        .store
        .classify_eval_candidates(&accounts, &provider_ids)
        .await?
        .into_iter()
        .filter(|candidate| candidate.account_id == scenario_account)
        .filter_map(|candidate| {
            let label = scenarios::expectation_for(&candidate.provider_id)?;
            label
                .from_email
                .eq_ignore_ascii_case(&candidate.from_email)
                .then_some((candidate, label))
        })
        .collect();
    let total = state.store.classify_eval_message_count(&accounts).await?;
    let labelled_in_set = u32::try_from(labels.len()).unwrap_or(u32::MAX);
    if present.is_empty() {
        return Ok(ResponseData::ClassifyEval {
            summary: summarise(labelled_in_set, 0, total, &Tally::default()),
            mismatches: Vec::new(),
        });
    }

    let kinds = mail_kinds(state, &present).await?;
    // Mail deleted between the lookup and the read has nothing to grade.
    let present: Vec<_> = present
        .into_iter()
        .filter(|(candidate, _)| kinds.contains_key(&candidate.message_id))
        .collect();
    let present_count = u32::try_from(present.len()).unwrap_or(u32::MAX);
    let unlabelled = total.saturating_sub(present_count);
    let ids: Vec<MessageId> = present
        .iter()
        .map(|(candidate, _)| candidate.message_id.clone())
        .collect();
    let aspects = state.store.classify_eval_aspects(&ids).await?;
    let homes = current_homes(state, &present).await?;
    let mut tally = Tally::default();
    let mut mismatches = Vec::new();
    for (candidate, label) in &present {
        let Some((kind, reason)) = kinds.get(&candidate.message_id) else {
            continue;
        };
        let actual = Actual {
            who: who_name(*kind),
            who_reason: reason.clone(),
            home: homes.get(&candidate.message_id).cloned(),
            todo: aspects.todo.contains(&candidate.message_id),
            record: aspects.record.contains(&candidate.message_id),
        };
        grade(label, &actual, &mut tally, &mut mismatches);
    }
    sort_mismatches(&mut mismatches);
    Ok(ResponseData::ClassifyEval {
        summary: summarise(labelled_in_set, present_count, unlabelled, &tally),
        mismatches,
    })
}

/// Each email's mail kind and the reason for it, over the sender signals
/// placement reads: the thread, the sender's history, your choices.
async fn mail_kinds(
    state: &AppState,
    present: &[(EvalCandidate, &Labelled)],
) -> Result<HashMap<MessageId, (SenderKindData, String)>, HandlerError> {
    let mut by_account: BTreeMap<String, (AccountId, Vec<&EvalCandidate>)> = BTreeMap::new();
    for (candidate, _) in present {
        by_account
            .entry(candidate.account_id.as_str().clone())
            .or_insert_with(|| (candidate.account_id.clone(), Vec::new()))
            .1
            .push(candidate);
    }
    let mut kinds = HashMap::new();
    for (account_id, candidates) in by_account.into_values() {
        let mut threads: Vec<ThreadId> = candidates.iter().map(|c| c.thread_id.clone()).collect();
        threads.sort_by_key(ThreadId::as_str);
        threads.dedup();
        let mut messages = Vec::new();
        for chunk in threads.chunks(MEMBERSHIP_MAX_THREADS) {
            messages.extend(
                state
                    .store
                    .desk_messages_in_threads(&account_id, chunk)
                    .await?,
            );
        }
        let senders = Senders::load(state, &account_id, &messages).await?;
        let is_self = self_matcher(state, &account_id).await?;
        for candidate in candidates {
            let Some(message) = messages.iter().find(|m| m.id == candidate.message_id) else {
                continue;
            };
            let described = mail_kind::describe(&senders.signals(message, &is_self));
            kinds.insert(
                candidate.message_id.clone(),
                (described.kind, described.reason),
            );
        }
    }
    Ok(kinds)
}

/// Where each email goes now: its arrivals row re-placed by today's rules,
/// else placed live. An email whose home cannot be worked out is absent.
async fn current_homes(
    state: &AppState,
    present: &[(EvalCandidate, &Labelled)],
) -> Result<HashMap<MessageId, Home>, HandlerError> {
    // The same gate the arrivals line reads under, so a rule edit cannot
    // land between rows.
    let _sorting = state.rule_mutation_gate.lock().await;
    let ids: Vec<MessageId> = present.iter().map(|(c, _)| c.message_id.clone()).collect();
    let rows = arrivals::effective_rows(state, state.store.arrivals_by_ids(&ids).await?).await?;
    let mut homes: HashMap<MessageId, Home> = rows
        .into_iter()
        .filter_map(|row| {
            let mode = placed(&row.effective)?.to_string();
            Some((
                row.message_id,
                Home {
                    mode,
                    reason: row.reason.unwrap_or_default(),
                    live: false,
                },
            ))
        })
        .collect();

    let mut without_row: BTreeMap<String, (AccountId, Vec<Wanted>)> = BTreeMap::new();
    for (candidate, _) in present {
        if !homes.contains_key(&candidate.message_id) {
            without_row
                .entry(candidate.account_id.as_str().clone())
                .or_insert_with(|| (candidate.account_id.clone(), Vec::new()))
                .1
                .push((
                    candidate.message_id.clone(),
                    candidate.thread_id.clone(),
                    candidate.spam,
                ));
        }
    }
    for (account_id, wanted) in without_row.into_values() {
        for batch in wanted.chunks(MEMBERSHIP_MAX_THREADS) {
            for placement in arrivals::place_live(state, &account_id, batch).await? {
                homes.insert(
                    placement.message_id,
                    Home {
                        mode: placement.mode,
                        reason: placement.reason,
                        live: true,
                    },
                );
            }
        }
    }
    Ok(homes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_provider_fake::fixtures::scenarios::{Expect, Home as ExpectedHome, Review, Who};

    fn label(expect: Expect) -> Labelled {
        Labelled {
            provider_id: "demo-msg-9".into(),
            from_email: "a@b.example".into(),
            scenario: "s",
            description: "",
            expect,
            twin_of: Some("t"),
            differs_by: "x",
            review: None,
        }
    }

    const EXPECT: Expect = Expect {
        who: Who::Machine,
        home: ExpectedHome::Updates,
        todo: false,
        record: true,
    };

    fn right() -> Actual {
        Actual {
            who: "machine",
            who_reason: "automated sender".into(),
            home: Some(Home {
                mode: "updates".into(),
                reason: "automated sender".into(),
                live: false,
            }),
            todo: false,
            record: true,
        }
    }

    fn graded(expect: Expect, actual: &Actual) -> (Tally, Vec<ClassifyEvalMismatchData>) {
        let (mut tally, mut mismatches) = (Tally::default(), Vec::new());
        grade(&label(expect), actual, &mut tally, &mut mismatches);
        (tally, mismatches)
    }

    #[test]
    fn a_right_answer_everywhere_has_no_mismatch() {
        let (tally, mismatches) = graded(EXPECT, &right());
        assert!(mismatches.is_empty());
        assert_eq!(tally.evaluated, [1; 4]);
        assert_eq!(tally.correct, [1; 4]);
    }

    #[test]
    fn a_to_do_or_record_is_wrong_in_both_directions() {
        // Expected no to-do, got one; expected a record, got none.
        let actual = Actual {
            todo: true,
            record: false,
            ..right()
        };
        let (tally, mismatches) = graded(EXPECT, &actual);
        let wrong: Vec<_> = mismatches
            .iter()
            .map(|m| (m.question.as_str(), m.expected.as_str(), m.actual.as_str()))
            .collect();
        assert_eq!(wrong, [("todo", "no", "yes"), ("record", "yes", "no")]);
        assert_eq!(tally.correct, [1, 1, 0, 0]);
        // And the opposite polarity.
        let expects_todo = Expect {
            todo: true,
            record: false,
            ..EXPECT
        };
        let (_, mismatches) = graded(expects_todo, &right());
        assert_eq!(mismatches.len(), 2);
        assert_eq!(mismatches[0].expected, "yes");
        assert_eq!(mismatches[0].actual, "no");
    }

    #[test]
    fn an_email_with_no_home_yet_is_reported_not_graded() {
        let actual = Actual {
            home: None,
            ..right()
        };
        let (tally, mismatches) = graded(EXPECT, &actual);
        assert!(mismatches.is_empty(), "not placed is not a wrong answer");
        assert_eq!(tally.not_placed, 1);
        assert_eq!(tally.evaluated, [1, 0, 1, 1]);
    }

    #[test]
    fn a_mismatch_on_a_judgement_call_label_says_so() {
        let mut judged = label(EXPECT);
        judged.review = Some(Review {
            questions: &["todo"],
            why: "a policy call",
        });
        let actual = Actual {
            todo: true,
            ..right()
        };
        let (mut tally, mut mismatches) = (Tally::default(), Vec::new());
        grade(&judged, &actual, &mut tally, &mut mismatches);
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].label_review.as_deref(), Some("a policy call"));
        let (_, plain) = graded(EXPECT, &actual);
        assert_eq!(plain[0].label_review, None);
        // The doubt covers the to-do answer only: a wrong record is not it.
        let wrong_record = Actual {
            record: false,
            ..right()
        };
        let (mut tally, mut mismatches) = (Tally::default(), Vec::new());
        grade(&judged, &wrong_record, &mut tally, &mut mismatches);
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].question, "record");
        assert_eq!(mismatches[0].label_review, None);
    }

    #[test]
    fn mail_placed_live_is_graded_and_counted() {
        let actual = Actual {
            home: Some(Home {
                mode: "messages".into(),
                reason: String::new(),
                live: true,
            }),
            ..right()
        };
        let (tally, mismatches) = graded(EXPECT, &actual);
        assert_eq!(tally.placed_live, 1);
        assert_eq!(tally.evaluated, [1; 4], "its home is asked like any other");
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].question, "home");
    }

    #[test]
    fn a_wrong_who_and_home_carry_the_rule_that_decided_them() {
        let actual = Actual {
            who: "person",
            who_reason: "possible personal mail".into(),
            home: Some(Home {
                mode: "messages".into(),
                reason: "possible personal mail".into(),
                live: false,
            }),
            ..right()
        };
        let (_, mismatches) = graded(EXPECT, &actual);
        assert_eq!(mismatches.len(), 2);
        assert!(mismatches
            .iter()
            .all(|m| m.reason == "possible personal mail" && m.twin_of.as_deref() == Some("t")));
    }

    #[test]
    fn mismatches_sort_by_scenario_then_numeric_provider_id_then_question() {
        let make = |scenario: &str, provider_id: &str, question: &str| ClassifyEvalMismatchData {
            scenario: scenario.into(),
            provider_id: provider_id.into(),
            question: question.into(),
            expected: String::new(),
            actual: String::new(),
            reason: String::new(),
            twin_of: None,
            label_review: None,
        };
        let sorted = vec![
            make("a", "demo-msg-9", "who"),
            make("a", "demo-msg-9", "record"),
            make("a", "demo-msg-10", "home"),
            make("b", "demo-msg-2", "who"),
        ];
        for rotation in 0..sorted.len() {
            let mut shuffled = sorted.clone();
            shuffled.rotate_left(rotation);
            shuffled.reverse();
            sort_mismatches(&mut shuffled);
            assert_eq!(shuffled, sorted, "rotation {rotation}");
        }
    }

    #[test]
    fn accuracy_is_a_percentage_and_zero_when_nothing_was_asked() {
        let tally = Tally {
            evaluated: [3, 0, 3, 3],
            correct: [2, 0, 3, 0],
            ..Tally::default()
        };
        let summary = summarise(10, 3, 5, &tally);
        let accuracy: Vec<f64> = summary
            .questions
            .iter()
            .map(|q| q.accuracy_percent)
            .collect();
        assert_eq!(accuracy, [66.7, 0.0, 100.0, 0.0]);
        assert_eq!(summary.message, None);
        let empty = summarise(10, 0, 5, &Tally::default());
        assert_eq!(empty.message.as_deref(), Some(NO_LABELLED_MAIL));
    }

    #[test]
    fn sorting_is_not_a_home() {
        assert_eq!(placed("sorting"), None);
        assert_eq!(placed("updates"), Some("updates"));
    }
}
