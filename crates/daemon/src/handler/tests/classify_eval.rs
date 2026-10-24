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
    scenario_state_at(Utc::now()).await
}

async fn scenario_state_at(now: chrono::DateTime<Utc>) -> (Arc<AppState>, AccountId) {
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
        mxr_provider_fake::FakeProvider::with_scenario_set(account_id.clone(), now, &[])
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
    for account in state.store.list_accounts().await.unwrap() {
        if account_id.is_none_or(|id| id == &account.id) {
            crate::handler::assessments::reconcile_account(state, &account.id)
                .await
                .unwrap();
        }
    }

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
    if std::env::var_os("MXR_CLASSIFY_TRACE").is_some() {
        use sqlx::Row;
        eprintln!("{}", report(&summary, &mismatches));
        for row in sqlx::query("SELECT m.provider_id, m.subject, a.assessment_json, (SELECT json_group_array(json_object('reason', t.reason, 'state', t.state, 'expired_at_birth', t.expired_at_birth, 'field_sources', json(t.field_sources))) FROM todos t WHERE t.source_message_id = m.id) AS todo_json FROM messages m JOIN mail_assessments a ON a.message_id = m.id WHERE a.policy_version = ?1 ORDER BY m.provider_id")
            .bind(mxr_core::asks::ASKS_POLICY_VERSION)
            .fetch_all(state.store.reader()).await.unwrap()
        {
            eprintln!("ASK_TRACE {}", serde_json::json!({
                "provider_id": row.get::<String, _>("provider_id"),
                "subject": row.get::<String, _>("subject"),
                    "todos": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("todo_json")).expect("synthetic projection JSON"),
                "asks": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("assessment_json")).unwrap(),
            }));
        }
    }
    classify_eval_floor::check("quick", summary.asks.as_ref().expect("asks"));
    insta::assert_snapshot!(report(&summary, &mismatches));
}

#[tokio::test]
async fn the_quick_snapshot_survives_crossing_midnight_utc() {
    let midnight = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    for now in [
        midnight - chrono::Duration::hours(3),
        midnight + chrono::Duration::hours(3),
    ] {
        let (state, _) = scenario_state_at(now).await;
        let (summary, mismatches) = eval(&state, None).await;
        classify_eval_floor::check("quick", summary.asks.as_ref().expect("asks"));
        insta::assert_snapshot!(
            "the_quick_set_is_graded_and_every_mismatch_is_listed",
            report(&summary, &mismatches)
        );
    }
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
async fn an_email_with_no_arrivals_row_keeps_its_stored_assessment() {
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
    assert_eq!(after.placed_live, 0);
    assert_eq!(after.not_placed, 0);
    assert_eq!(after.questions, before.questions, "its home is still asked");
    assert_eq!(
        after_mismatches, before_mismatches,
        "placing live by the same rules gives the same answer"
    );
}

#[tokio::test]
async fn a_missing_assessment_is_pending_without_being_repaired_or_graded_as_wrong() {
    let (state, account_id) = scenario_state().await;
    let (before, _) = eval(&state, Some(&account_id)).await;
    let victim = scenarios::quick_set()
        .find(|label| !label.outbound)
        .unwrap();
    let removed = sqlx::query(
        "DELETE FROM mail_assessments WHERE policy_version = ?4 AND message_id IN
         (SELECT id FROM messages WHERE account_id = ?1 AND provider_id = ?2 AND from_email = ?3)",
    )
    .bind(account_id.as_str())
    .bind(&victim.provider_id)
    .bind(&victim.from_email)
    .bind(mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
    .execute(state.store.writer())
    .await
    .unwrap()
    .rows_affected();
    assert_eq!(removed, 1);
    let stored_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mail_assessments")
        .fetch_one(state.store.reader())
        .await
        .unwrap();

    let ResponseData::ClassifyEval {
        summary,
        mismatches,
    } = classify_eval(&state, Some(&account_id), ClassifyEvalSetData::Quick)
        .await
        .unwrap()
    else {
        panic!("expected evaluation");
    };
    assert_eq!(summary.not_placed, before.not_placed + 1);
    for question in ["who", "home"] {
        let old = before
            .questions
            .iter()
            .find(|q| q.question == question)
            .unwrap();
        let new = summary
            .questions
            .iter()
            .find(|q| q.question == question)
            .unwrap();
        assert_eq!(new.evaluated + 1, old.evaluated);
        assert!(!mismatches
            .iter()
            .any(|m| { m.provider_id == victim.provider_id && m.question == question }));
    }
    let stored_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mail_assessments")
        .fetch_one(state.store.reader())
        .await
        .unwrap();
    assert_eq!(
        stored_after, stored_before,
        "evaluation must not repair the missing answer"
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

#[tokio::test]
async fn a_stored_ask_without_its_item_projection_is_not_graded_as_delivered() {
    let (state, _) = scenario_state().await;
    let (before, _) = eval(&state, None).await;
    // The production read must not be credited with the fixture workers' writes.
    state
        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
        .await;
    let removed = sqlx::query("DELETE FROM todos WHERE source_message_id IN (SELECT id FROM messages WHERE provider_id = 'demo-msg-9')")
        .execute(state.store.writer()).await.expect("remove only the synthetic projection").rows_affected();
    assert_eq!(removed, 1);
    let changes: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(state.store.writer())
        .await
        .expect("changes");
    let ResponseData::ClassifyEval {
        summary,
        mismatches,
    } = classify_eval(&state, None, ClassifyEvalSetData::Quick)
        .await
        .expect("read-only evaluation")
    else {
        panic!("eval response");
    };
    assert_eq!(
        summary
            .asks
            .as_ref()
            .expect("asks")
            .kinds
            .iter()
            .find(|k| k.kind == "pay" && k.direction == "asks_you")
            .expect("pay")
            .counts
            .found
            + 1,
        before
            .asks
            .as_ref()
            .expect("asks")
            .kinds
            .iter()
            .find(|k| k.kind == "pay" && k.direction == "asks_you")
            .expect("pay")
            .counts
            .found
    );
    assert!(mismatches
        .iter()
        .any(|m| m.provider_id == "demo-msg-9" && m.question == "ask" && m.actual == "no row"));
    let after: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(state.store.writer())
        .await
        .expect("changes");
    assert_eq!(
        changes, after,
        "evaluation must not regenerate the missing task"
    );
}

#[tokio::test]
async fn linked_product_fields_are_measured_from_the_row_without_repairing_them() {
    let (state, _) = scenario_state().await;
    eval(&state, None).await;
    // The production read must not be credited with the fixture workers' writes.
    state
        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
        .await;
    let changed = sqlx::query("UPDATE todos SET amount_minor = amount_minor + 100, due_at = due_at + 172800 WHERE user_edited = 0 AND source_message_id IN (SELECT id FROM messages WHERE provider_id = 'demo-msg-9')")
        .execute(state.store.writer()).await.expect("change only projected fields").rows_affected();
    assert_eq!(changed, 1);
    let before: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(state.store.writer())
        .await
        .expect("changes");
    let ResponseData::ClassifyEval { mismatches, .. } =
        classify_eval(&state, None, ClassifyEvalSetData::Quick)
            .await
            .expect("stored evaluation")
    else {
        panic!("eval response");
    };
    for field in ["amount", "due"] {
        assert!(
            mismatches
                .iter()
                .any(|m| m.provider_id == "demo-msg-9" && m.question == field),
            "wrong stored {field} must be visible"
        );
    }
    let after: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(state.store.writer())
        .await
        .expect("changes");
    assert_eq!(
        before, after,
        "evaluation leaves the erroneous projection unchanged"
    );
}
