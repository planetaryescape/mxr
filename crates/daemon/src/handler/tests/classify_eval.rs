//! `mxr classify eval` over the labelled scenario set: a daemon that has
//! synced the curated demo mail and every near-miss twin, run through the
//! same post-sync work a real sync runs (to-dos, records, deliveries,
//! arrivals placement), then graded. The snapshot is the classifier's
//! measured behaviour: any change to sorting shows up as a reviewed diff, and
//! no kind of ask may fall below its checked-in floor
//! (`classify_eval_floor.rs`).

use super::classify_eval_floor;
use super::*;
use crate::handler::classify_eval::classify_eval;
use chrono::Utc;
use mxr_core::id::AccountId;
use mxr_core::types::{Account, BackendRef, ProviderKind};
use mxr_protocol::{
    ClassifyEvalMismatchData, ClassifyEvalSetData, ClassifyEvalSummaryData, ResponseData,
};
use mxr_provider_fake::fixtures::scenarios::{self, SELF_EMAIL};
use std::sync::Arc;

/// The personal demo account synced from the scenario set, with the
/// post-sync work the daemon runs after each page and a contacts refresh,
/// which the daemon's background worker does a few seconds after a sync.
async fn scenario_state() -> (Arc<AppState>, AccountId) {
    let account_id = AccountId::from_provider_id("fake", SELF_EMAIL);
    let account = Account {
        id: account_id.clone(),
        name: "Alex Demo".to_string(),
        email: SELF_EMAIL.to_string(),
        sync_backend: Some(BackendRef {
            provider_kind: ProviderKind::Fake,
            config_key: "fake".to_string(),
        }),
        send_backend: None,
        enabled: true,
    };
    let provider = Arc::new(
        mxr_provider_fake::FakeProvider::with_scenario_set(account_id.clone(), Utc::now(), &[])
            .with_page_size(1000),
    );
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, provider.clone(), None)
            .await
            .unwrap(),
    );
    // As the daemon sets up a sync, and as `classify_eval_full.rs` does: an
    // engine that knows the accounts' addresses, so every message has a
    // direction and sender histories and contacts aggregate. The test state's
    // own engine has no address lookup and leaves every direction unknown.
    crate::state::load_account_address_registry(&state.store, &state.account_addresses)
        .await
        .unwrap();
    let engine = mxr_sync::SyncEngine::with_address_lookup(
        state.store.clone(),
        state.search.clone(),
        state.account_addresses.clone() as Arc<dyn mxr_core::types::AccountAddressLookup>,
    );
    loop {
        let outcome = engine
            .sync_account_with_outcome(provider.as_ref())
            .await
            .unwrap();
        crate::loops::post_sync_fanout(
            state.clone(),
            account_id.clone(),
            provider.clone(),
            outcome.upserted_message_ids,
            false,
            false,
        )
        .await;
        if !outcome.has_more {
            break;
        }
    }
    // The whole-table analytics repair the daemon runs at the end of a
    // backfill (directions, list ids, reply pairs), then the contacts
    // refresh its worker does a few seconds later.
    crate::loops::run_analytics_repair(&state, &account_id).await;
    state.store.refresh_contacts().await.unwrap();
    (state, account_id)
}

async fn eval(
    state: &AppState,
    account_id: Option<&AccountId>,
) -> (ClassifyEvalSummaryData, Vec<ClassifyEvalMismatchData>) {
    match classify_eval(state, account_id, ClassifyEvalSetData::Quick)
        .await
        .unwrap()
    {
        ResponseData::ClassifyEval {
            summary,
            mismatches,
        } => (summary, mismatches),
        other => panic!("{other:?}"),
    }
}

/// Counts and the sorted mismatch list, without the rule wording, so a
/// reworded reason is not a diff but a changed answer is.
fn report(summary: &ClassifyEvalSummaryData, mismatches: &[ClassifyEvalMismatchData]) -> String {
    let mut out = format!(
        "label schema v{}; {} labelled in the set, {} in the mailbox, {} not placed\n",
        summary.label_schema_version, summary.labelled_in_set, summary.present, summary.not_placed
    );
    for q in &summary.questions {
        out.push_str(&format!(
            "{:<7} {:>3}/{:<3} {:>5.1}%\n",
            q.question, q.correct, q.evaluated, q.accuracy_percent
        ));
    }
    out.push('\n');
    out.push_str(&classify_eval_floor::report(
        summary.asks.as_ref().expect("the asks were graded"),
    ));
    out.push_str(&format!("\n{} mismatches\n", mismatches.len()));
    for m in mismatches {
        out.push_str(&format!(
            "{} {} {}: expected {}, got {}{}\n",
            m.scenario,
            m.provider_id,
            m.question,
            m.expected,
            m.actual,
            if m.label_review.is_some() {
                " [label to check]"
            } else {
                ""
            }
        ));
    }
    out
}

#[tokio::test]
async fn the_quick_set_is_graded_and_every_mismatch_is_listed() {
    let (state, _) = scenario_state().await;
    let (summary, mismatches) = eval(&state, None).await;
    // The sent context the twins need is mail the account wrote, not
    // labelled mail, so every labelled email is present.
    assert_eq!(summary.present, summary.labelled_in_set);
    assert_eq!(summary.not_placed, 0);
    assert_eq!(summary.message, None);
    // The desk's Your turn needs a sender that classifies as a person, so a
    // question from a no-reply address is not owed a reply.
    assert!(
        mismatches
            .iter()
            .all(|m| !(m.scenario == "no-reply-vendor-question" && m.question == "reply")),
        "{mismatches:?}"
    );
    classify_eval_floor::check("quick", summary.asks.as_ref().expect("asks"));
    insta::assert_snapshot!(report(&summary, &mismatches));
}

#[tokio::test]
async fn grading_twice_gives_the_same_answer() {
    let (state, account_id) = scenario_state().await;
    let first = eval(&state, None).await;
    let second = eval(&state, Some(&account_id)).await;
    assert_eq!(first, second, "no state carries over between runs");
}

#[tokio::test]
async fn a_mailbox_with_no_labelled_mail_says_so_and_lists_nothing() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (summary, mismatches) = eval(&state, None).await;
    assert_eq!(summary.present, 0);
    assert!(mismatches.is_empty());
    assert_eq!(
        summary.message.as_deref(),
        Some("No labelled mail here; run mxr demo")
    );
    assert!(summary.questions.iter().all(|q| q.evaluated == 0));
    // Nothing graded, so no asks section and no zero rates.
    assert_eq!(summary.asks, None);
}

#[tokio::test]
async fn another_accounts_mail_with_the_same_provider_id_and_sender_is_not_graded() {
    let (state, _) = scenario_state().await;
    let (before, before_mismatches) = eval(&state, None).await;
    // The work demo account numbers its mail from 1 as well and reuses some
    // of the same people: here, an email with a scenario's provider id and
    // sender, which the label does not mean.
    let work = Account {
        id: AccountId::from_provider_id("fake", "alex@work.demo.mxr.local"),
        name: "Alex Work".to_string(),
        email: "alex@work.demo.mxr.local".to_string(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    state.store.insert_account(&work).await.unwrap();
    let victim = scenarios::quick_set().next().unwrap();
    let lookalike = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(work.id.clone())
        .provider_id(victim.provider_id.clone())
        .sender_address("Lookalike", &victim.from_email)
        .recipient_address(None, "alex@work.demo.mxr.local")
        .build();
    state.store.upsert_envelope(&lookalike).await.unwrap();

    let (after, after_mismatches) = eval(&state, None).await;
    assert_eq!(after.present, before.present, "the lookalike is not graded");
    assert_eq!(after.unlabelled, before.unlabelled + 1, "it is unlabelled");
    assert_eq!(after.questions, before.questions);
    assert_eq!(after_mismatches, before_mismatches);
    // Asked about that account alone there is nothing labelled to grade.
    let (work_only, mismatches) = eval(&state, Some(&work.id)).await;
    assert_eq!(work_only.present, 0);
    assert!(mismatches.is_empty());
    assert_eq!(
        work_only.message.as_deref(),
        Some("No labelled mail here; run mxr demo")
    );
}

#[tokio::test]
async fn an_email_with_no_arrivals_row_is_placed_live_not_counted_wrong() {
    let (state, _) = scenario_state().await;
    let (before, before_mismatches) = eval(&state, None).await;
    let victim = scenarios::quick_set().next().unwrap();
    let removed = sqlx::query(
        "DELETE FROM arrivals WHERE message_id IN
         (SELECT id FROM messages WHERE provider_id = ?1 AND from_email = ?2)",
    )
    .bind(&victim.provider_id)
    .bind(&victim.from_email)
    .execute(state.store.writer())
    .await
    .unwrap()
    .rows_affected();
    assert_eq!(removed, 1, "the first labelled email had an arrivals row");

    let (after, after_mismatches) = eval(&state, None).await;
    assert_eq!(after.placed_live, before.placed_live + 1);
    assert_eq!(after.not_placed, 0);
    assert_eq!(after.questions, before.questions, "its home is still asked");
    assert_eq!(
        after_mismatches, before_mismatches,
        "placing live by the same rules gives the same answer"
    );
}

#[tokio::test]
async fn a_to_do_that_was_already_over_when_found_is_not_a_to_do() {
    let (state, _) = scenario_state().await;
    // The leaving drinks ended before the mail was seen, so the to-do row
    // exists but was born expired: the product never shows it.
    let born_expired: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM todos t JOIN messages m ON m.id = t.source_message_id
         WHERE m.subject = 'Sam''s leaving drinks' AND t.expired_at_birth = 1",
    )
    .fetch_one(state.store.reader())
    .await
    .unwrap();
    assert_eq!(born_expired, 1, "the premise: a born-expired row exists");
    let (_, mismatches) = eval(&state, None).await;
    assert!(
        mismatches
            .iter()
            .all(|m| !(m.scenario == "todo-event-already-over" && m.question == "ask")),
        "a born-expired to-do is not counted as a to-do: {mismatches:?}"
    );
}
