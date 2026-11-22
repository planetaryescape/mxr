use super::classify_eval_diagnostics::emit_ask_diagnostics;
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
use std::time::{Duration, Instant};

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

async fn demo_state(total: usize) -> (Arc<AppState>, AccountId, AccountId) {
    let (state, personal, work, _) = demo_state_timed(total).await;
    (state, personal, work)
}

const TIMING_PREFIX: &str = "classify-eval-timing: ";

fn timing_row(stage: &str, pages: &str, sync: Option<Duration>, wall: Duration) {
    let sync_secs = sync.map_or(String::new(), |sync| format!("{:.1}", sync.as_secs_f64()));
    let other_secs = wall.saturating_sub(sync.unwrap_or_default()).as_secs_f64();
    eprintln!(
        "{TIMING_PREFIX}| {stage} | {pages} | {sync_secs} | {other_secs:.1} | {:.1} |",
        wall.as_secs_f64()
    );
}

async fn demo_state_timed(total: usize) -> (Arc<AppState>, AccountId, AccountId, Duration) {
    let mut total_sync = Duration::ZERO;
    eprintln!(
        "{TIMING_PREFIX}| Stage | Pages | Sync (s) | Classify and other work (s) | Wall (s) |"
    );
    eprintln!("{TIMING_PREFIX}|---|---|---|---|---|");
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
            if pages % 5 == 0 {
                eprintln!("  {id}: {pages} pages in {:?} (sync {sync_time:?}, classify {classify_time:?})", started.elapsed());
            }
            if !outcome.has_more {
                break;
            }
        }
        eprintln!("synced and classified {} in {:?}", id, started.elapsed());
        timing_row(
            &format!("sync and classify {id}"),
            &pages.to_string(),
            Some(sync_time),
            started.elapsed(),
        );
        total_sync += sync_time;
    }
    let started = Instant::now();
    crate::loops::run_analytics_repair(&state, &personal.id).await;
    state.store.refresh_contacts().await.unwrap();
    timing_row("analytics repair and contacts", "", None, started.elapsed());
    super::classify_eval::authenticate_demo_payment_alerts(&state, &personal.id, now).await;
    super::classify_eval::authenticate_demo_payment_alerts(&state, &work.id, now).await;
    assert_directions_are_known(&state).await;
    (state, personal.id, work.id, total_sync)
}

fn pinned_now() -> chrono::DateTime<chrono::Utc> {
    static NOW: std::sync::OnceLock<chrono::DateTime<chrono::Utc>> = std::sync::OnceLock::new();
    *NOW.get_or_init(|| {
        chrono::Utc::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .expect("noon is a valid time")
            .and_utc()
    })
}

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
            crate::handler::assessments::reconcile_account_at(state, &account.id, pinned_now())
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

fn shape<'a>(question: &str, value: &'a str) -> &'a str {
    match question {
        "amount" | "due" | "counterparty" if value != "none" => "a value",
        _ => value,
    }
}

type ClusterKey<'a> = (&'a str, &'a str, &'a str, &'a str);
type Cluster = (u32, u32, bool);

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

#[tokio::test]
#[ignore = "builds the whole 50,000-message demo; CI's Classifier Eval job runs it"]
async fn the_full_set_is_graded_on_the_whole_demo() {
    let started = Instant::now();
    let (state, _, _, sync) = demo_state_timed(50_000).await;
    let built = started.elapsed();
    let (summary, mismatches) = eval(&state, None, ClassifyEvalSetData::Full).await;
    let graded = started.elapsed() - built;
    eprintln!("full-set eval: built and classified the demo in {built:?}, graded in {graded:?}");
    timing_row("reconcile and grade", "", None, graded);
    let elapsed = started.elapsed();
    timing_row("whole test", "", Some(sync), elapsed);
    let work_per_sync = work_per_sync(sync, elapsed);
    eprintln!("{TIMING_PREFIX}");
    eprintln!(
        "{TIMING_PREFIX}Classify and other work took {work_per_sync:.1} times the sync time \
         (budget {WORK_PER_SYNC_BUDGET:.0}). The sync time scales with the runner, so a slow \
         runner moves this ratio far less than a slower classifier does."
    );
    assert_eq!(summary.set, ClassifyEvalSetData::Full);
    assert_eq!(summary.message, None);
    assert!(
        summary.present > 30_000,
        "only {} of the demo's emails are labelled",
        summary.present
    );
    assert!(summary.held_out_present > 5_000);
    emit_ask_diagnostics(&state, &summary, &mismatches).await;
    classify_eval_floor::check("full", summary.asks.as_ref().expect("asks"));
    insta::assert_snapshot!(report(&summary, &mismatches));
    assert!(
        work_per_sync <= WORK_PER_SYNC_BUDGET,
        "classify and other work took {work_per_sync:.1} times the sync time, over the \
         budget of {WORK_PER_SYNC_BUDGET:.0}: the classifier, repair or grading got slower \
         than the sync it follows. If sync itself got faster, re-measure and lower the budget"
    );
}

fn work_per_sync(sync: Duration, elapsed: Duration) -> f64 {
    elapsed.saturating_sub(sync).as_secs_f64() / sync.as_secs_f64().max(f64::EPSILON)
}

const WORK_PER_SYNC_BUDGET: f64 = 25.0;

const SHARED_DEMO_MESSAGES: usize = 1_500;

#[tokio::test]
async fn a_generated_demo_grades_the_same_twice_both_accounts_and_the_quick_set_on_curated_mail() {
    let (state, personal, work) = demo_state(SHARED_DEMO_MESSAGES).await;
    grading_the_full_set_twice_gives_the_same_answer(&state).await;
    the_full_set_labels_the_curated_mail_with_the_quick_sets_labels(&state, &personal).await;
    the_full_set_grades_both_demo_accounts_and_reports_held_out_apart(&state, &personal, &work)
        .await;
    the_quick_set_on_a_generated_demo_still_grades_only_the_curated_mail(&state).await;
}

async fn the_full_set_grades_both_demo_accounts_and_reports_held_out_apart(
    state: &AppState,
    personal: &AccountId,
    work: &AccountId,
) {
    let (summary, mismatches) = eval(state, None, ClassifyEvalSetData::Full).await;
    assert_eq!(summary.set, ClassifyEvalSetData::Full);
    assert_eq!(summary.message, None);
    assert_eq!(
        summary.present, summary.labelled_in_set,
        "the full set is whatever the store holds that is labelled"
    );
    assert!(summary.present > 1_000, "{}", summary.present);
    assert!(summary.unlabelled > 100, "{}", summary.unlabelled);
    assert_eq!(summary.not_placed, 0);

    assert!(summary.held_out_present > 100);
    assert!(summary.held_out_present < summary.present);
    assert_eq!(summary.held_out_questions.len(), summary.questions.len());
    for (held, all) in summary.held_out_questions.iter().zip(&summary.questions) {
        assert_eq!(held.question, all.question);
        assert!(held.evaluated > 0 && held.evaluated < all.evaluated);
        assert!(held.correct <= all.correct);
    }
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

    let (personal_only, _) = eval(state, Some(personal), ClassifyEvalSetData::Full).await;
    let (work_only, _) = eval(state, Some(work), ClassifyEvalSetData::Full).await;
    assert!(personal_only.present > 0 && work_only.present > 0);
    assert_eq!(personal_only.present + work_only.present, summary.present);
    assert_eq!(
        personal_only.held_out_present + work_only.held_out_present,
        summary.held_out_present
    );
}

async fn the_quick_set_on_a_generated_demo_still_grades_only_the_curated_mail(state: &AppState) {
    let (quick, quick_mismatches) = eval(state, None, ClassifyEvalSetData::Quick).await;
    let (full, _) = eval(state, None, ClassifyEvalSetData::Full).await;
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

async fn grading_the_full_set_twice_gives_the_same_answer(state: &AppState) {
    let first = eval(state, None, ClassifyEvalSetData::Full).await;
    emit_ask_diagnostics(state, &first.0, &first.1).await;
    let before: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(state.store.writer())
        .await
        .unwrap();
    let pending_before: i64 = sqlx::query_scalar("SELECT count(*) FROM asks_projection_outbox")
        .fetch_one(state.store.reader())
        .await
        .unwrap();
    let read_only = classify_eval(state, None, ClassifyEvalSetData::Full)
        .await
        .unwrap();
    let after: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(state.store.writer())
        .await
        .unwrap();
    let pending_after: i64 = sqlx::query_scalar("SELECT count(*) FROM asks_projection_outbox")
        .fetch_one(state.store.reader())
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(pending_before, pending_after);
    assert_eq!(
        pending_after, 0,
        "fixture must finish projection before grading"
    );
    let ResponseData::ClassifyEval {
        summary,
        mismatches,
    } = read_only
    else {
        panic!("grading response")
    };
    assert_eq!(
        first,
        (summary, mismatches),
        "production reads are stable without repair"
    );
    let second = eval(state, None, ClassifyEvalSetData::Full).await;
    assert_eq!(first, second, "no state carries over between runs");
}

async fn the_full_set_labels_the_curated_mail_with_the_quick_sets_labels(
    state: &AppState,
    personal: &AccountId,
) {
    let (_, quick_mismatches) = eval(state, Some(personal), ClassifyEvalSetData::Quick).await;
    let (_, full_mismatches) = eval(state, Some(personal), ClassifyEvalSetData::Full).await;
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
