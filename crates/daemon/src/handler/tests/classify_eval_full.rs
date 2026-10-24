//! `mxr classify eval --set full` over generated demos: both demo accounts
//! synced from the scenario families, run through the same post-sync
//! classification a real sync runs (to-dos, records, deliveries, arrivals
//! placement), then graded.
//!
//! * `the_full_set_is_graded_on_the_whole_demo` builds the whole
//!   50,000-message demo, takes minutes and is `#[ignore]`d so the workspace
//!   test jobs skip it. CI's "Classifier Eval" job runs it, and a change that
//!   moves any answer shows up as a reviewed snapshot diff. Locally:
//!   `scripts/cargo-test -p mxr --lib -- --ignored handler::tests::classify_eval_full`
//!   (once; it is heavy).
//! * The other tests run the same code over a demo of a few thousand, so the
//!   set selection, the held-out accounting and the per-account scoping are
//!   checked on every run.

use super::classify_eval_floor;
use super::*;
use crate::handler::classify_eval::classify_eval;
use mxr_core::id::AccountId;
use mxr_core::types::{Account, BackendRef, ProviderKind};
use mxr_protocol::{
    ClassifyEvalMismatchData, ClassifyEvalSetData, ClassifyEvalSummaryData, ResponseData,
};
use mxr_provider_fake::fixtures::scenarios::{SELF_EMAIL, WORK_EMAIL};
use mxr_provider_fake::FakeProvider;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

/// The sync page the demo is fed through. The daemon pages a demo by the
/// provider's own page size; a larger page only changes how much post-sync
/// work one call batches, never what it concludes.
const PAGE_SIZE: usize = 1000;

fn account(email: &str, name: &str) -> Account {
    Account {
        id: AccountId::from_provider_id("fake", email),
        name: name.to_string(),
        email: email.to_string(),
        sync_backend: Some(BackendRef {
            provider_kind: ProviderKind::Fake,
            config_key: "fake".to_string(),
        }),
        send_backend: None,
        enabled: true,
    }
}

/// Both demo accounts synced from a demo of `total` messages (split 55/45
/// between them, as `mxr demo` does), set up as the daemon sets up a sync.
///
/// * The sync engine is the daemon's: built with `with_address_lookup` over the
///   account address registry, loaded from the stored accounts
///   (`load_account_address_registry`). The test state's own engine has no
///   address lookup and leaves every message's direction unknown, and then
///   nothing aggregates into contacts or sender histories.
/// * Each page gets the post-sync classification the daemon runs.
/// * At the end of the backfill the daemon's whole-table analytics repair runs
///   (`run_analytics_repair`: directions, list ids, reply pairs), then the
///   contacts refresh its worker does.
///
/// Skipped, because none of it is read by who, home, to-do or record: the
/// semantic ingest, the relationship-profile and voice-profile workers, and
/// the search warm-up (all beside the classification, and three quarters of the
/// post-sync time on a 12,000-message demo); and the demo's seeded rules and
/// screener decisions, which `mxr demo` adds over IPC and the quick-set test
/// does not have either (CONTRIBUTING says its numbers differ slightly).
///
/// The demo is dated, and its to-dos and records are found, as of today at
/// noon UTC ([`pinned_now`]), not the moment the run starts.
async fn demo_state(total: usize) -> (Arc<AppState>, AccountId, AccountId) {
    let personal = account(SELF_EMAIL, "Alex Demo");
    let work = account(WORK_EMAIL, "Alex Work");
    let now = pinned_now();
    let personal_provider = Arc::new(
        FakeProvider::with_demo_dataset_at(personal.id.clone(), total, now, &[])
            .with_page_size(PAGE_SIZE),
    );
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(personal.clone(), personal_provider.clone(), None)
            .await
            .unwrap(),
    );
    state.store.insert_account(&work).await.unwrap();
    let work_provider = Arc::new(
        FakeProvider::with_demo_dataset_at(work.id.clone(), total, now, &[])
            .with_page_size(PAGE_SIZE),
    );
    state.add_sync_provider_for_test(work_provider.clone());
    // Both accounts are stored, so the registry holds both primaries.
    crate::state::load_account_address_registry(&state.store, &state.account_addresses)
        .await
        .unwrap();
    let engine = mxr_sync::SyncEngine::with_address_lookup(
        state.store.clone(),
        state.search.clone(),
        state.account_addresses.clone() as Arc<dyn mxr_core::types::AccountAddressLookup>,
    );
    for (id, provider) in [(&personal.id, personal_provider), (&work.id, work_provider)] {
        let started = Instant::now();
        let mut pages = 0;
        let mut sync_time = std::time::Duration::ZERO;
        let mut classify_time = std::time::Duration::ZERO;
        loop {
            let stage = Instant::now();
            let outcome = engine
                .sync_account_with_outcome(provider.as_ref())
                .await
                .unwrap();
            sync_time += stage.elapsed();
            let stage = Instant::now();
            // As the daemon seeds a demo: every page but the last is part of
            // the initial backfill, which leaves Reading's whole-account
            // sweep to the last page. Run on every page it is quadratic and
            // was over half of the time.
            crate::loops::post_sync_classification(
                &state,
                id,
                provider.as_ref(),
                &outcome.upserted_message_ids,
                outcome.has_more,
                now,
            )
            .await;
            classify_time += stage.elapsed();
            pages += 1;
            // A line per few pages: a queued job that prints nothing for
            // minutes is taken for stuck.
            if pages % 5 == 0 {
                eprintln!("  {id}: {pages} pages in {:?} (sync {sync_time:?}, classify {classify_time:?})", started.elapsed());
            }
            if !outcome.has_more {
                break;
            }
        }
        eprintln!("synced and classified {} in {:?}", id, started.elapsed());
    }
    crate::loops::run_analytics_repair(&state, &personal.id).await;
    state.store.refresh_contacts().await.unwrap();
    assert_directions_are_known(&state).await;
    (state, personal.id, work.id)
}

/// Today at 12:00 UTC. A demo bill is due a whole day ("due by Friday 24
/// October") counted from its email's date and time, and stays open until
/// that day ends. On the wall clock, which bills had already lapsed depended
/// on the hour the job ran, and the snapshot moved with it. Pinned to a time
/// of day, the same mail gets the same answers on any run.
fn pinned_now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
        .date_naive()
        .and_hms_opt(12, 0, 0)
        .expect("noon is a valid time")
        .and_utc()
}

/// The correspondence history the classifier reads exists: every message has
/// a direction, the accounts' own mail is outbound, and contacts and the
/// addresses written to were aggregated from it. Without an address-aware
/// sync none of this holds and the eval grades a mailbox the daemon never has.
async fn assert_directions_are_known(state: &AppState) {
    let count = |sql: &'static str| async move {
        sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(state.store.reader())
            .await
            .unwrap()
    };
    assert_eq!(
        count("SELECT COUNT(*) FROM messages WHERE direction NOT IN ('inbound', 'outbound')").await,
        0,
        "some messages have no direction"
    );
    assert!(count("SELECT COUNT(*) FROM messages WHERE direction = 'outbound'").await > 0);
    assert!(count("SELECT COUNT(*) FROM messages WHERE direction = 'inbound'").await > 0);
    assert!(
        count("SELECT COUNT(*) FROM contacts").await > 0,
        "no contacts"
    );
}

async fn eval(
    state: &AppState,
    account_id: Option<&AccountId>,
    set: ClassifyEvalSetData,
) -> (ClassifyEvalSummaryData, Vec<ClassifyEvalMismatchData>) {
    for account in state.store.list_accounts().await.unwrap() {
        if account_id.is_none_or(|id| id == &account.id) {
            crate::handler::assessments::reconcile_account(state, &account.id)
                .await
                .unwrap();
        }
    }

    let started = Instant::now();
    let response = classify_eval(state, account_id, set).await.unwrap();
    eprintln!("graded the {set:?} set in {:?}", started.elapsed());
    match response {
        ResponseData::ClassifyEval {
            summary,
            mismatches,
        } => (summary, mismatches),
        other => panic!("{other:?}"),
    }
}

fn question_lines(questions: &[mxr_protocol::ClassifyEvalQuestionData]) -> String {
    questions
        .iter()
        .map(|q| {
            format!(
                "{:<7} {:>6}/{:<6} {:>5.1}%\n",
                q.question, q.correct, q.evaluated, q.accuracy_percent
            )
        })
        .collect()
}

/// A field mismatch is clustered by its shape, not its value: 98 different
/// amounts are one cluster.
fn shape<'a>(question: &str, value: &'a str) -> &'a str {
    match question {
        "amount" | "due" | "counterparty" if value != "none" => "a value",
        _ => value,
    }
}

/// Scenario, question, expected and actual answer.
type ClusterKey<'a> = (&'a str, &'a str, &'a str, &'a str);
/// How many, how many of them held out, and whether any label is a judgement
/// call.
type Cluster = (u32, u32, bool);

/// Counts, per-question accuracy, held-out accuracy and the mismatches
/// grouped by what was expected and what was said. Never one line per email:
/// a 50,000-email set is reviewed by cluster. Rule wording is left out so a
/// reworded reason is not a diff but a changed answer is.
///
/// `placed_live` is left out on purpose. A message counts as an arrival during
/// an account's first sync only if its date is within 30 days of the moment it
/// is stored (the `arrivals_on_message_insert` trigger), and the demo's mail is
/// dated from the moment the provider was built, so the number of emails
/// dated near that line depends on how long the run took: 34,121 and 34,122 on
/// the same code. Those emails are placed live by the same rules, so no graded
/// answer changes. The CLI still prints the count.
fn report(summary: &ClassifyEvalSummaryData, mismatches: &[ClassifyEvalMismatchData]) -> String {
    let mut out = format!(
        "label schema v{}\n{} labelled emails graded, {} stored emails unlabelled, {} not placed\n\n",
        summary.label_schema_version, summary.present, summary.unlabelled, summary.not_placed
    );
    out.push_str(&question_lines(&summary.questions));
    out.push('\n');
    if let Some(asks) = &summary.asks {
        out.push_str(&classify_eval_floor::report(asks));
    }
    out.push_str(&format!(
        "\nheld-out phrasings: {} emails\n",
        summary.held_out_present
    ));
    out.push_str(&question_lines(&summary.held_out_questions));

    // (scenario, question, expected, actual) -> (all, held out, label to check)
    let mut clusters: BTreeMap<ClusterKey, Cluster> = BTreeMap::new();
    for m in mismatches {
        let entry = clusters
            .entry((
                m.scenario.as_str(),
                m.question.as_str(),
                shape(&m.question, &m.expected),
                shape(&m.question, &m.actual),
            ))
            .or_default();
        entry.0 += 1;
        entry.1 += u32::from(m.held_out);
        entry.2 |= m.label_review.is_some();
    }
    out.push_str(&format!(
        "\n{} mismatches in {} clusters\n",
        mismatches.len(),
        clusters.len()
    ));
    for ((scenario, question, expected, actual), (count, held_out, to_check)) in clusters {
        out.push_str(&format!(
            "{scenario} {question}: expected {expected}, got {actual}: {count} ({held_out} held out){}\n",
            if to_check { " [label to check]" } else { "" }
        ));
    }
    out
}

/// Grades all 50,000 emails of the demo. CI's "Classifier Eval" job runs it.
/// The snapshot is the classifier's measured behaviour on the whole demo:
/// any change to sorting shows up as a reviewed diff.
#[tokio::test]
#[ignore = "builds the whole 50,000-message demo; CI's Classifier Eval job runs it"]
async fn the_full_set_is_graded_on_the_whole_demo() {
    let started = Instant::now();
    let (state, _, _) = demo_state(50_000).await;
    let built = started.elapsed();
    let (summary, mismatches) = eval(&state, None, ClassifyEvalSetData::Full).await;
    eprintln!(
        "full-set eval: built and classified the demo in {built:?}, graded in {:?}",
        started.elapsed() - built
    );
    assert_eq!(summary.set, ClassifyEvalSetData::Full);
    assert_eq!(summary.message, None);
    // Everything the account did not send is labelled, so what is left
    // unlabelled is the mail the demo's accounts wrote.
    assert!(
        summary.present > 30_000,
        "only {} of the demo's emails are labelled",
        summary.present
    );
    assert!(summary.held_out_present > 5_000);
    classify_eval_floor::check("full", summary.asks.as_ref().expect("asks"));
    insta::assert_snapshot!(report(&summary, &mismatches));
}

#[tokio::test]
async fn the_full_set_grades_both_demo_accounts_and_reports_held_out_apart() {
    let (state, personal, work) = demo_state(2_500).await;
    let (summary, mismatches) = eval(&state, None, ClassifyEvalSetData::Full).await;
    assert_eq!(summary.set, ClassifyEvalSetData::Full);
    assert_eq!(summary.message, None);
    assert_eq!(
        summary.present, summary.labelled_in_set,
        "the full set is whatever the store holds that is labelled"
    );
    assert!(summary.present > 1_000, "{}", summary.present);
    // Mail the accounts wrote is context, not labelled.
    assert!(summary.unlabelled > 100, "{}", summary.unlabelled);
    assert_eq!(summary.not_placed, 0);

    // Held-out phrasings are graded on their own line, per question, and
    // their share is a fraction of all.
    assert!(summary.held_out_present > 100);
    assert!(summary.held_out_present < summary.present);
    assert_eq!(summary.held_out_questions.len(), summary.questions.len());
    for (held, all) in summary.held_out_questions.iter().zip(&summary.questions) {
        assert_eq!(held.question, all.question);
        assert!(held.evaluated > 0 && held.evaluated < all.evaluated);
        assert!(held.correct <= all.correct);
    }
    // A mismatch on a held-out email is flagged, and the flags add up to the
    // held-out tallies.
    for (index, question) in summary.questions.iter().enumerate() {
        let held_wrong = mismatches
            .iter()
            .filter(|m| m.question == question.question && m.held_out)
            .count() as u32;
        let held = &summary.held_out_questions[index];
        assert_eq!(
            held.evaluated - held.correct,
            held_wrong,
            "{}",
            question.question
        );
    }

    // Asked about one account, only that account's mail is graded; together
    // they are the whole.
    let (personal_only, _) = eval(&state, Some(&personal), ClassifyEvalSetData::Full).await;
    let (work_only, _) = eval(&state, Some(&work), ClassifyEvalSetData::Full).await;
    assert!(personal_only.present > 0 && work_only.present > 0);
    assert_eq!(personal_only.present + work_only.present, summary.present);
    assert_eq!(
        personal_only.held_out_present + work_only.held_out_present,
        summary.held_out_present
    );
}

#[tokio::test]
async fn the_quick_set_on_a_generated_demo_still_grades_only_the_curated_mail() {
    let (state, _, _) = demo_state(1_500).await;
    let (quick, quick_mismatches) = eval(&state, None, ClassifyEvalSetData::Quick).await;
    let (full, _) = eval(&state, None, ClassifyEvalSetData::Full).await;
    // The quick set is the hand-written mail of the personal account: the
    // twins are not in a demo, and none of it is held out.
    assert_eq!(quick.set, ClassifyEvalSetData::Quick);
    assert!(quick.present > 90 && quick.present < full.present);
    assert_eq!(quick.held_out_present, 0);
    assert!(quick.held_out_questions.is_empty());
    assert!(quick_mismatches.iter().all(|m| !m.held_out));
    assert!(
        quick.present < quick.labelled_in_set,
        "twins are not in a demo"
    );
}

#[tokio::test]
async fn grading_the_full_set_twice_gives_the_same_answer() {
    let (state, _, _) = demo_state(1_200).await;
    let first = eval(&state, None, ClassifyEvalSetData::Full).await;
    let second = eval(&state, None, ClassifyEvalSetData::Full).await;
    assert_eq!(first, second, "no state carries over between runs");
}

#[tokio::test]
async fn the_full_set_labels_the_curated_mail_with_the_quick_sets_labels() {
    let (state, personal, _) = demo_state(1_200).await;
    // Every mismatch the quick set finds on the curated mail is also found,
    // with the same expected and actual answer, by the full set.
    let (_, quick_mismatches) = eval(&state, Some(&personal), ClassifyEvalSetData::Quick).await;
    let (_, full_mismatches) = eval(&state, Some(&personal), ClassifyEvalSetData::Full).await;
    assert!(!quick_mismatches.is_empty());
    for quick in &quick_mismatches {
        assert!(
            full_mismatches.iter().any(|full| full == quick),
            "the full set missed {} {}",
            quick.provider_id,
            quick.question
        );
    }
}

#[test]
fn the_snapshot_report_does_not_depend_on_how_long_the_run_took() {
    let summary = |placed_live| ClassifyEvalSummaryData {
        set: ClassifyEvalSetData::Full,
        label_schema_version: 3,
        labelled_in_set: 10,
        present: 10,
        placed_live,
        not_placed: 0,
        unlabelled: 3,
        questions: Vec::new(),
        held_out_present: 0,
        held_out_questions: Vec::new(),
        home_projection: String::new(),
        asks: None,
        message: None,
    };
    assert_eq!(report(&summary(7), &[]), report(&summary(8), &[]));
    assert!(!report(&summary(7), &[]).contains("placed live"));
}
