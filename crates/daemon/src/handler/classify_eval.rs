//! `mxr classify eval`: today's classifier measured against labelled
//! scenarios (blueprint 32, build order 1). Stored mail that a scenario
//! labels is read back the way the product reads it, and each of four
//! answers is compared with the label. The quick set is the hand-written
//! demo mail and the near-miss twins, named by provider id and sender; the
//! full set is every labelled email in the store, which on a demo adds the
//! generated bulk, labelled by where the generator put each email.
//!
//! * who: the stored assessment's sender judgement, including mail in Spam;
//! * home: the persisted assessment used by the modes, arrivals and chips.
//!   Missing or stale answers remain pending; evaluation never repairs them.
//! * record: a record row includes this email.
//!
//! What an email asks of you (its to-do rows, and whether a reply is owed) is
//! not a yes or no. It is graded per kind of ask and per field in
//! `classify_eval_asks`, against the items each label expects.
//!
//! Read-only and local. It changes no classification and writes nothing,
//! including no activity row.

use super::classify_eval_asks::{AskGrader, AskInput, Row, ASK_QUESTIONS};
use super::desk::{self_matcher, Senders};
use super::desk_lanes;
use super::modes::MEMBERSHIP_MAX_THREADS;
use super::{mail_kind, HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_protocol::{
    ClassifyEvalAsksData, ClassifyEvalMismatchData, ClassifyEvalQuestionData, ClassifyEvalSetData,
    ClassifyEvalSummaryData, ResponseData, SenderKindData,
};
use mxr_provider_fake::fixtures::scenarios::{self, Labelled, LABEL_SCHEMA_VERSION};
use mxr_store::EvalCandidate;
use std::collections::{BTreeMap, HashMap};

/// What `home` reads, for the output.
const HOME_PROJECTION: &str =
    "stored assessment; missing or stale answers are pending, never created by evaluation";
const NO_LABELLED_MAIL: &str = "No labelled mail here; run mxr demo";
/// An email to place live: its id, thread and whether it is in Spam.
/// Stored emails with the labels that grade them.
type Present = Vec<(EvalCandidate, Labelled)>;

/// The questions graded as accuracy. What an email asks of you is not one.
const QUESTIONS: [&str; 3] = ["who", "home", "record"];

/// What today's code answers for one email.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Actual {
    who: Option<&'static str>,
    who_reason: String,
    /// `None`: no current stored assessment is available to compare.
    home: Option<Home>,
    record: bool,
}

/// What the shared `describe` path says about one email, and whether the desk
/// reads it as asking for a reply.
#[derive(Debug, Clone)]
struct MailRead {
    kind: SenderKindData,
    who_pending: bool,
    home: Option<Home>,
    reason: String,
    reply_owed: bool,
    /// The email's own date.
    sent: chrono::DateTime<chrono::Utc>,
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
    evaluated: [u32; 3],
    correct: [u32; 3],
    /// The same counts over the emails worded in a held-out phrasing.
    held_out_evaluated: [u32; 3],
    held_out_correct: [u32; 3],
    held_out_present: u32,
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

/// Compare one email's answers with its label. Pending `who` and `home`
/// answers are skipped; a missing home counts as not placed. Mail you sent is not graded
/// here: who, home and record are about mail that arrives.
fn grade(
    label: &Labelled,
    actual: &Actual,
    tally: &mut Tally,
    mismatches: &mut Vec<ClassifyEvalMismatchData>,
) {
    if label.outbound {
        return;
    }
    tally.held_out_present += u32::from(label.held_out);
    let mut check = |question: usize, expected: &str, got: &str, reason: &str| {
        tally.evaluated[question] += 1;
        tally.held_out_evaluated[question] += u32::from(label.held_out);
        if expected == got {
            tally.correct[question] += 1;
            tally.held_out_correct[question] += u32::from(label.held_out);
        } else {
            mismatches.push(ClassifyEvalMismatchData {
                scenario: label.scenario.to_string(),
                provider_id: label.provider_id.clone(),
                question: QUESTIONS[question].to_string(),
                expected: expected.to_string(),
                actual: got.to_string(),
                reason: reason.to_string(),
                twin_of: label.twin_of.map(str::to_string),
                held_out: label.held_out,
                label_review: label
                    .review
                    .filter(|review| review.questions.contains(&QUESTIONS[question]))
                    .map(|review| review.why.to_string()),
            });
        }
    };
    if let Some(who) = actual.who {
        check(0, label.expect.who.as_str(), who, &actual.who_reason);
    }
    match &actual.home {
        Some(home) => {
            tally.placed_live += u32::from(home.live);
            check(1, label.expect.home.as_str(), &home.mode, &home.reason);
        }
        None => tally.not_placed += 1,
    }
    check(2, yes_no(label.expect.record), yes_no(actual.record), "");
}

/// Stable order whatever order the store returned rows in: scenario, then
/// provider id in numeric order, then question (the accuracy questions, then
/// the asks), then what was expected and got.
fn sort_mismatches(mismatches: &mut [ClassifyEvalMismatchData]) {
    mismatches.sort_by(|a, b| {
        let question = |m: &ClassifyEvalMismatchData| {
            QUESTIONS
                .iter()
                .chain(&ASK_QUESTIONS)
                .position(|q| *q == m.question)
                .unwrap_or(QUESTIONS.len() + ASK_QUESTIONS.len())
        };
        (
            &a.scenario,
            a.provider_id.len(),
            &a.provider_id,
            question(a),
            &a.expected,
            &a.actual,
        )
            .cmp(&(
                &b.scenario,
                b.provider_id.len(),
                &b.provider_id,
                question(b),
                &b.expected,
                &b.actual,
            ))
    });
}

fn accuracy(question: &str, evaluated: u32, correct: u32) -> ClassifyEvalQuestionData {
    ClassifyEvalQuestionData {
        question: question.to_string(),
        evaluated,
        correct,
        accuracy_percent: if evaluated == 0 {
            0.0
        } else {
            (f64::from(correct) / f64::from(evaluated) * 1000.0).round() / 10.0
        },
    }
}

fn summarise(
    set: ClassifyEvalSetData,
    labelled_in_set: u32,
    present: u32,
    unlabelled: u32,
    tally: &Tally,
    asks: Option<ClassifyEvalAsksData>,
) -> ClassifyEvalSummaryData {
    ClassifyEvalSummaryData {
        set,
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
                accuracy(question, tally.evaluated[index], tally.correct[index])
            })
            .collect(),
        held_out_present: tally.held_out_present,
        held_out_questions: if tally.held_out_present == 0 {
            Vec::new()
        } else {
            QUESTIONS
                .iter()
                .enumerate()
                .map(|(index, question)| {
                    accuracy(
                        question,
                        tally.held_out_evaluated[index],
                        tally.held_out_correct[index],
                    )
                })
                .collect()
        },
        home_projection: HOME_PROJECTION.to_string(),
        asks,
        message: (present == 0).then(|| NO_LABELLED_MAIL.to_string()),
    }
}

/// The stored emails a set labels, with their labels.
async fn labelled_mail(
    state: &AppState,
    accounts: &[AccountId],
    set: ClassifyEvalSetData,
) -> Result<(Present, u32), HandlerError> {
    match set {
        ClassifyEvalSetData::Quick => {
            let labels: Vec<&Labelled> = scenarios::quick_set().collect();
            let provider_ids: Vec<&str> = labels.iter().map(|l| l.provider_id.as_str()).collect();
            // Both demo accounts number their mail from 1 and reuse some of
            // the same people, and a smaller demo numbers its fillers
            // differently, so only the scenario account's email from the
            // scenario's sender is the email the label means.
            let scenario_account = scenarios::account_id();
            let present = state
                .store
                .classify_eval_candidates(accounts, &provider_ids)
                .await?
                .into_iter()
                .filter(|candidate| candidate.account_id == scenario_account)
                .filter_map(|candidate| {
                    let label = scenarios::expectation_for(&candidate.provider_id)?;
                    label
                        .from_email
                        .eq_ignore_ascii_case(&candidate.from_email)
                        .then(|| (candidate, label.clone()))
                })
                .collect();
            Ok((present, u32::try_from(labels.len()).unwrap_or(u32::MAX)))
        }
        ClassifyEvalSetData::Full => {
            // Every stored email of a demo account that the generator or the
            // quick set labels. The set is whatever the store holds, so its
            // size is the number found.
            let present: Present = state
                .store
                .classify_eval_all_candidates(accounts)
                .await?
                .into_iter()
                .filter_map(|candidate| {
                    let label = scenarios::full_label_for(
                        &candidate.account_id,
                        &candidate.provider_id,
                        &candidate.thread_id,
                        &candidate.from_email,
                    )?;
                    Some((candidate, label))
                })
                .collect();
            let labelled = u32::try_from(present.len()).unwrap_or(u32::MAX);
            Ok((present, labelled))
        }
    }
}

pub(super) async fn classify_eval(
    state: &AppState,
    account_id: Option<&AccountId>,
    set: ClassifyEvalSetData,
) -> HandlerResult {
    let accounts: Vec<AccountId> = account_id.cloned().into_iter().collect();
    let (present, labelled_in_set) = labelled_mail(state, &accounts, set).await?;
    let total = state.store.classify_eval_message_count(&accounts).await?;
    if present.is_empty() {
        return Ok(ResponseData::ClassifyEval {
            summary: summarise(set, labelled_in_set, 0, total, &Tally::default(), None),
            mismatches: Vec::new(),
        });
    }

    let reads = mail_reads(state, &present).await?;
    // Mail deleted between the lookup and the read has nothing to grade.
    let present: Vec<_> = present
        .into_iter()
        .filter(|(candidate, _)| reads.contains_key(&candidate.message_id))
        .collect();
    let present_count = u32::try_from(present.len()).unwrap_or(u32::MAX);
    let unlabelled = total.saturating_sub(present_count);
    let ids: Vec<MessageId> = present
        .iter()
        .map(|(candidate, _)| candidate.message_id.clone())
        .collect();
    let aspects = state.store.classify_eval_aspects(&ids).await?;
    let mut rows: HashMap<&MessageId, Vec<Row>> = HashMap::new();
    for todo in &aspects.todos {
        rows.entry(&todo.source_message_id)
            .or_default()
            .push(Row::from_todo(todo));
    }
    let mut tally = Tally::default();
    let mut asks = AskGrader::default();
    let mut mismatches = Vec::new();
    for (candidate, label) in &present {
        let Some(read) = reads.get(&candidate.message_id) else {
            continue;
        };
        let actual = Actual {
            who: (!read.who_pending).then(|| who_name(read.kind)),
            who_reason: read.reason.clone(),
            home: read.home.clone(),
            record: aspects.record.contains(&candidate.message_id),
        };
        grade(label, &actual, &mut tally, &mut mismatches);
        asks.grade(
            AskInput {
                label,
                // The store returns rows in key order; the grader does not
                // rely on it.
                rows: rows.remove(&candidate.message_id).unwrap_or_default(),
                reply_found: (!label.outbound).then_some(read.reply_owed),
                sent: read.sent,
            },
            &candidate.from_email,
        );
    }
    let (asks, ask_mismatches) = asks.finish();
    mismatches.extend(ask_mismatches);
    sort_mismatches(&mut mismatches);
    Ok(ResponseData::ClassifyEval {
        summary: summarise(
            set,
            labelled_in_set,
            present_count,
            unlabelled,
            &tally,
            asks,
        ),
        mismatches,
    })
}

/// Each email's mail kind and the reason for it, over the sender signals
/// placement reads: the thread, the sender's history, your choices. Also
/// whether the desk reads it as asking for a reply.
async fn mail_reads(
    state: &AppState,
    present: &[(EvalCandidate, Labelled)],
) -> Result<HashMap<MessageId, MailRead>, HandlerError> {
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
        let senders = Senders::load_raw(state, &account_id, &messages).await?;
        let assessments = super::assessments::fresh_stored(
            state,
            &account_id,
            state
                .store
                .assessments(&account_id, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
                .await?,
        )
        .await?;
        let is_self = self_matcher(state, &account_id).await?;
        // Looked up once per email: a scan per email is quadratic on a demo
        // of tens of thousands.
        let by_id: HashMap<&MessageId, _> = messages.iter().map(|m| (&m.id, m)).collect();
        // The newest message you sent in each thread, by the order mail was
        // stored (`seq`), as the desk judges who wrote last
        // (`desk_lanes::last_stored`).
        let mut last_sent: HashMap<&ThreadId, i64> = HashMap::new();
        for message in &messages {
            if desk_lanes::is_outbound(message, &is_self) {
                let newest = last_sent.entry(&message.thread_id).or_insert(message.seq);
                *newest = (*newest).max(message.seq);
            }
        }
        for candidate in candidates {
            let Some(message) = by_id.get(&candidate.message_id) else {
                continue;
            };
            let signals = senders.signals(message, &is_self);
            let assessment = assessments.get(&candidate.message_id);
            let kind = assessment.map_or(SenderKindData::ScreenedOut, |a| match a.who.as_str() {
                "person" => SenderKindData::People,
                "list" => SenderKindData::Reading,
                "machine" => SenderKindData::PaperTrail,
                _ => SenderKindData::ScreenedOut,
            });
            kinds.insert(
                candidate.message_id.clone(),
                MailRead {
                    kind,
                    who_pending: !assessments.contains_key(&candidate.message_id),
                    home: assessments.get(&candidate.message_id).map(|a| Home {
                        mode: a.home.clone(),
                        reason: a.deciding.reason.clone(),
                        live: false,
                    }),
                    reason: assessment.map_or_else(String::new, |a| a.deciding.reason.clone()),
                    // Your turn: a request directed to you that you have not
                    // answered. The desk puts a thread in You owe only when
                    // its latest stored message is not yours
                    // (`desk_lanes.rs:438`: an outbound latest is Waiting)
                    // and the inbound one passes `requires_reply`
                    // (`desk_lanes.rs:483`), which is `reply_request` and a
                    // sender that classifies as a person
                    // (`mail_kind.rs:408-418`). Per message that is:
                    // requires_reply on the signals built above, and
                    // nothing sent after it. The lane's other gates (copied
                    // threads, decay, the screener, snoozes) are not mirrored.
                    reply_owed: mail_kind::requires_reply(&signals)
                        && last_sent
                            .get(&message.thread_id)
                            .is_none_or(|sent| *sent < message.seq),
                    sent: message.date,
                },
            );
        }
    }
    Ok(kinds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_provider_fake::fixtures::scenarios::{
        Expect, Home as ExpectedHome, Review, SenderKind, Who,
    };

    fn label(expect: Expect) -> Labelled {
        Labelled {
            provider_id: "demo-msg-9".into(),
            from_email: "a@b.example".into(),
            scenario: "s",
            description: "",
            expect,
            items: Vec::new(),
            outbound: false,
            sender: SenderKind::Organisation,
            locale: "en-GB",
            family: "s",
            twin_of: Some("t"),
            differs_by: "x",
            review: None,
            held_out: false,
        }
    }

    const EXPECT: Expect = Expect::new(Who::Machine, ExpectedHome::Updates, true);

    fn right() -> Actual {
        Actual {
            who: Some("machine"),
            who_reason: "automated sender".into(),
            home: Some(Home {
                mode: "updates".into(),
                reason: "automated sender".into(),
                live: false,
            }),
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
        assert_eq!(tally.evaluated, [1; 3]);
        assert_eq!(tally.correct, [1; 3]);
    }

    #[test]
    fn a_record_is_wrong_in_both_directions() {
        // Expected a record, got none.
        let actual = Actual {
            record: false,
            ..right()
        };
        let (tally, mismatches) = graded(EXPECT, &actual);
        let wrong: Vec<_> = mismatches
            .iter()
            .map(|m| (m.question.as_str(), m.expected.as_str(), m.actual.as_str()))
            .collect();
        assert_eq!(wrong, [("record", "yes", "no")]);
        assert_eq!(tally.correct, [1, 1, 0]);
        // And the opposite polarity.
        let expects_none = Expect::new(Who::Machine, ExpectedHome::Updates, false);
        let (_, mismatches) = graded(expects_none, &right());
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].expected, "no");
        assert_eq!(mismatches[0].actual, "yes");
    }

    #[test]
    fn what_an_email_asks_is_not_an_accuracy_question() {
        assert_eq!(QUESTIONS, ["who", "home", "record"]);
        let (tally, _) = graded(EXPECT, &right());
        let summary = summarise(ClassifyEvalSetData::Quick, 1, 1, 0, &tally, None);
        assert!(summary.questions.iter().all(|q| q.question != "todo"));
    }

    #[test]
    fn mail_you_sent_is_not_graded_on_who_home_or_record() {
        let sent = Labelled {
            outbound: true,
            ..label(EXPECT)
        };
        let wrong = Actual {
            who: Some("person"),
            record: false,
            ..right()
        };
        let (mut tally, mut mismatches) = (Tally::default(), Vec::new());
        grade(&sent, &wrong, &mut tally, &mut mismatches);
        assert!(mismatches.is_empty());
        assert_eq!(tally.evaluated, [0; 3]);
        assert_eq!(tally.held_out_present, 0);
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
        assert_eq!(tally.evaluated, [1, 0, 1]);
    }

    #[test]
    fn a_pending_assessment_is_not_a_wrong_who_answer() {
        let actual = Actual {
            who: None,
            home: None,
            ..right()
        };
        let (mut tally, mut mismatches) = (Tally::default(), Vec::new());
        let held_out = Labelled {
            held_out: true,
            ..label(EXPECT)
        };
        grade(&held_out, &actual, &mut tally, &mut mismatches);
        assert!(mismatches.is_empty());
        assert_eq!(tally.not_placed, 1);
        assert_eq!(tally.evaluated, [0, 0, 1]);
        assert_eq!(tally.held_out_evaluated, [0, 0, 1]);
        assert_eq!(tally.correct, [0, 0, 1]);
    }

    #[test]
    fn a_mismatch_on_a_judgement_call_label_says_so() {
        let mut judged = label(EXPECT);
        judged.review = Some(Review {
            questions: &["todo"],
            why: "a policy call",
        });
        let actual = Actual {
            record: false,
            ..right()
        };
        let mut judged_record = judged.clone();
        judged_record.review = Some(Review {
            questions: &["record"],
            why: "a policy call",
        });
        let (mut tally, mut mismatches) = (Tally::default(), Vec::new());
        grade(&judged_record, &actual, &mut tally, &mut mismatches);
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].label_review.as_deref(), Some("a policy call"));
        let (_, plain) = graded(EXPECT, &actual);
        assert_eq!(plain[0].label_review, None);
        // The doubt covers its own question only: a wrong who is not it.
        let wrong_who = Actual {
            who: Some("person"),
            ..right()
        };
        let (mut tally, mut mismatches) = (Tally::default(), Vec::new());
        grade(&judged_record, &wrong_who, &mut tally, &mut mismatches);
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].question, "who");
        assert_eq!(mismatches[0].label_review, None);
        // And a to-do doubt does not reach the accuracy questions.
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
        assert_eq!(tally.evaluated, [1; 3], "its home is asked like any other");
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].question, "home");
    }

    #[test]
    fn a_wrong_who_and_home_carry_the_rule_that_decided_them() {
        let actual = Actual {
            who: Some("person"),
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
            held_out: false,
            label_review: None,
        };
        let sorted = vec![
            make("a", "demo-msg-9", "who"),
            make("a", "demo-msg-9", "record"),
            make("a", "demo-msg-10", "home"),
            make("a", "demo-msg-10", "ask"),
            make("a", "demo-msg-10", "reply"),
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
            evaluated: [3, 0, 3],
            correct: [2, 0, 0],
            ..Tally::default()
        };
        let summary = summarise(ClassifyEvalSetData::Quick, 10, 3, 5, &tally, None);
        let accuracy: Vec<f64> = summary
            .questions
            .iter()
            .map(|q| q.accuracy_percent)
            .collect();
        assert_eq!(accuracy, [66.7, 0.0, 0.0]);
        assert_eq!(summary.message, None);
        let empty = summarise(
            ClassifyEvalSetData::Quick,
            10,
            0,
            5,
            &Tally::default(),
            None,
        );
        assert_eq!(empty.message.as_deref(), Some(NO_LABELLED_MAIL));
    }

    #[test]
    fn a_held_out_email_is_counted_apart_and_its_mismatches_are_flagged() {
        let held_out = Labelled {
            held_out: true,
            ..label(EXPECT)
        };
        let wrong_record = Actual {
            record: false,
            ..right()
        };
        let (mut tally, mut mismatches) = (Tally::default(), Vec::new());
        grade(&held_out, &wrong_record, &mut tally, &mut mismatches);
        // An ordinary email beside it, answered right.
        grade(&label(EXPECT), &right(), &mut tally, &mut mismatches);

        assert_eq!(tally.held_out_present, 1);
        assert_eq!(tally.held_out_evaluated, [1; 3]);
        assert_eq!(tally.held_out_correct, [1, 1, 0]);
        assert_eq!(tally.evaluated, [2; 3]);
        assert_eq!(tally.correct, [2, 2, 1]);
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches[0].held_out);

        let summary = summarise(ClassifyEvalSetData::Full, 2, 2, 0, &tally, None);
        assert_eq!(summary.set, ClassifyEvalSetData::Full);
        assert_eq!(summary.held_out_present, 1);
        let held: Vec<f64> = summary
            .held_out_questions
            .iter()
            .map(|q| q.accuracy_percent)
            .collect();
        assert_eq!(held, [100.0, 100.0, 0.0]);
        let all: Vec<f64> = summary
            .questions
            .iter()
            .map(|q| q.accuracy_percent)
            .collect();
        assert_eq!(all, [100.0, 100.0, 50.0]);
    }

    #[test]
    fn a_set_with_nothing_held_out_reports_no_held_out_line() {
        let (mut tally, mut mismatches) = (Tally::default(), Vec::new());
        grade(&label(EXPECT), &right(), &mut tally, &mut mismatches);
        let summary = summarise(ClassifyEvalSetData::Quick, 1, 1, 0, &tally, None);
        assert_eq!(summary.held_out_present, 0);
        assert!(summary.held_out_questions.is_empty());
    }
}
