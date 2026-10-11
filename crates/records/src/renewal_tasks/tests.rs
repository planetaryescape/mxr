use super::*;
use crate::pass::{self, PassConfig};
use chrono::TimeZone;
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{
    Address, BackendRef, Envelope, MessageBody, MessageDirection, MessageFlags, MessageMetadata,
    ProviderKind, UnsubscribeMethod,
};
use mxr_core::Account;
use mxr_store::RecordQuery;

mod fixtures;
use fixtures::{at, Fx};
mod missing_data;

#[tokio::test]
async fn review_is_inspectable_unchecked_and_preview_does_not_write() {
    let mut fx = Fx::new().await;
    fx.annual("Premium").await;
    let candidates = fx.plan(at(2026, 10, 11)).await;
    assert_eq!(candidates.len(), 1);
    let task = &candidates[0].task;
    assert_eq!(task.title, "Review Acme Premium renewal");
    assert_eq!(task.verb, "review");
    assert_eq!(task.amount_minor, Some(12000));
    assert_eq!(task.currency.as_deref(), Some("GBP"));
    assert_eq!(
        task.due_at.unwrap().date_naive(),
        at(2026, 10, 20).date_naive()
    );
    assert!(task
        .due_words
        .as_ref()
        .unwrap()
        .starts_with("Estimated renewal"));
    assert!(task.source_message_id.is_some());
    let fields = FieldSources::from_json(&task.field_sources);
    assert!(!fields.get("due_at").unwrap().checked);
    assert!(!fields.get("amount").unwrap().checked);
    assert!(task.relevant_until.is_none());
    assert_eq!(
        fx.store
            .count_todos_in_state(None, TodoState::Open)
            .await
            .unwrap(),
        0
    );
    apply(&fx.store, candidates).await.unwrap();
    let task = fx
        .store
        .list_todos_in_state(None, TodoState::Open, 10)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(
        pass::file_from_todo(
            &fx.store,
            &PassConfig {
                now: at(2026, 10, 11),
                tz: Utc
            },
            &task
        )
        .await
        .unwrap(),
        None
    );
}

#[tokio::test]
async fn backfill_duplicates_and_replay_keep_user_edits_and_decisions() {
    let mut fx = Fx::new().await;
    fx.annual("Premium").await;
    let now = at(2026, 10, 11);
    fx.apply(now).await;
    let mut task = fx
        .store
        .list_todos_in_state(None, TodoState::Open, 10)
        .await
        .unwrap()
        .remove(0);
    let original_id = task.id.clone();
    task.title = "Compare alternatives first".into();
    task.due_at = Some(at(2026, 10, 15));
    fx.store.update_todo_by_user(&task, now).await.unwrap();
    fx.store
        .set_todos_state(
            &[original_id.clone()],
            &[TodoState::Open],
            TodoState::Dismissed,
            now,
        )
        .await
        .unwrap();
    fx.receipt("Premium", at(2023, 10, 20), false).await;
    fx.receipt("Premium", at(2025, 10, 21), true).await;
    fx.apply(now).await;
    fx.apply(now).await;
    let saved = fx.store.get_todo(&original_id).await.unwrap().unwrap();
    assert_eq!(saved.title, task.title);
    assert_eq!(saved.due_at, task.due_at);
    assert_eq!(saved.state, TodoState::Dismissed);
    assert_eq!(
        fx.store
            .count_todos_in_state(None, TodoState::Open)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fx.store
            .count_todos_in_state(None, TodoState::Dismissed)
            .await
            .unwrap(),
        1
    );
    fx.store
        .set_todos_state(
            &[original_id.clone()],
            &[TodoState::Dismissed],
            TodoState::Open,
            now,
        )
        .await
        .unwrap();
    fx.apply(now).await;
    fx.store
        .set_todos_state(
            &[original_id.clone()],
            &[TodoState::Open],
            TodoState::Done,
            now,
        )
        .await
        .unwrap();
    fx.apply(now).await;
    assert_eq!(
        fx.store
            .get_todo(&original_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        TodoState::Done
    );
    fx.receipt("Premium", at(2026, 10, 20), false).await;
    fx.apply(at(2027, 10, 11)).await;
    let open = fx
        .store
        .list_todos_in_state(None, TodoState::Open, 10)
        .await
        .unwrap();
    assert_eq!(open.len(), 1);
    assert_ne!(open[0].id, original_id);
}

#[tokio::test]
async fn lead_window_uses_local_calendar_days_and_excludes_past_and_monthly() {
    let mut fx = Fx::new().await;
    fx.annual("Premium").await;
    for month in 7..=9 {
        fx.receipt("Basic", at(2026, month, 20), false).await;
    }
    assert!(fx.plan(at(2026, 10, 5)).await.is_empty());
    assert_eq!(fx.plan(at(2026, 10, 6)).await.len(), 1);
    assert_eq!(fx.plan(at(2026, 10, 20)).await.len(), 1);
    assert!(fx.plan(at(2026, 10, 21)).await.is_empty());
    let boundary = Utc
        .with_ymd_and_hms(2026, 10, 5, 23, 30, 0)
        .single()
        .unwrap();
    let zone = chrono_tz::Europe::London;
    let local = plan(&fx.store, &[fx.account.clone()], boundary, &zone, 9)
        .await
        .unwrap();
    assert_eq!(local.len(), 1);
    assert_eq!(
        local[0]
            .task
            .due_at
            .unwrap()
            .with_timezone(&zone)
            .date_naive(),
        at(2026, 10, 20).date_naive()
    );
}

#[tokio::test]
async fn same_merchant_plans_and_accounts_do_not_coalesce() {
    let mut fx = Fx::new().await;
    fx.annual("Premium").await;
    fx.annual("Family").await;
    fx.apply(at(2026, 10, 11)).await;
    let rows = fx
        .store
        .list_todos_in_state(None, TodoState::Open, 10)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].dedup_key, rows[1].dedup_key);
    let other = Account {
        id: AccountId::new(),
        name: "Second".into(),
        email: "second@example.com".into(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    fx.store.insert_account(&other).await.unwrap();
    let old_account = fx.account.clone();
    fx.account = other.id;
    fx.annual("Premium").await;
    fx.apply(at(2026, 10, 11)).await;
    assert_eq!(
        fx.store
            .count_todos_in_state(None, TodoState::Open)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        fx.store
            .count_todos_in_state(Some(&old_account), TodoState::Open)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn cancellation_prevents_new_tasks_without_closing_existing_reviews() {
    let mut fx = Fx::new().await;
    fx.annual("Premium").await;
    fx.apply(at(2026, 10, 11)).await;
    fx.mail(
        ("Acme", "billing@acme.example"),
        "Your Acme Premium subscription cancelled",
        "Your subscription is cancelled",
        None,
        at(2026, 10, 12),
    )
    .await;
    assert!(fx.plan(at(2026, 10, 12)).await.is_empty());
    fx.apply(at(2026, 10, 12)).await;
    assert_eq!(
        fx.store
            .count_todos_in_state(None, TodoState::Open)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn parallel_and_stale_duplicate_candidates_create_one_task() {
    let mut fx = Fx::new().await;
    fx.annual("Premium").await;
    let now = at(2026, 10, 11);
    let before = fx.plan(now).await;
    fx.receipt("Premium", at(2025, 10, 21), true).await;
    let after = fx.plan(now).await;
    let (a, b) = tokio::join!(apply(&fx.store, before), apply(&fx.store, after));
    a.unwrap();
    b.unwrap();
    assert_eq!(
        fx.store
            .count_todos_in_state(None, TodoState::Open)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn aliases_are_resumable_and_conflicts_fail_without_storage_order_choice() {
    let fx = Fx::new().await;
    let a = vec!["z".into()];
    let key = fx
        .store
        .record_renewal_cycle_key(&fx.account, &a, true)
        .await
        .unwrap();
    let replay = fx
        .store
        .record_renewal_cycle_key(&fx.account, &["a".into(), "z".into()], true)
        .await
        .unwrap();
    assert_eq!(replay, key);
    let other = fx
        .store
        .record_renewal_cycle_key(&fx.account, &["b".into()], true)
        .await
        .unwrap();
    assert_ne!(other, key);
    assert!(fx
        .store
        .record_renewal_cycle_key(&fx.account, &["b".into(), "z".into()], true)
        .await
        .is_err());
    assert!(fx
        .store
        .record_renewal_cycle_key(&fx.account, &[], true)
        .await
        .is_err());
}

#[tokio::test]
async fn reserved_alias_without_task_is_recovered_by_replay() {
    let mut fx = Fx::new().await;
    fx.annual("Premium").await;
    let candidates = fx.plan(at(2026, 10, 11)).await;
    let candidate = &candidates[0];
    let key = fx
        .store
        .record_renewal_cycle_key(&fx.account, &candidate.record_ids, true)
        .await
        .unwrap();
    assert!(fx
        .store
        .get_todo_by_dedup(&fx.account, &key)
        .await
        .unwrap()
        .is_none());
    fx.apply(at(2026, 10, 11)).await;
    assert_eq!(
        fx.store
            .get_todo_by_dedup(&fx.account, &key)
            .await
            .unwrap()
            .unwrap()
            .state,
        TodoState::Open
    );
}

#[tokio::test]
async fn price_title_and_estimated_date_corrections_keep_the_cycle() {
    let mut fx = Fx::new().await;
    fx.annual("Premium").await;
    let now = at(2026, 10, 11);
    fx.apply(now).await;
    let before = fx
        .store
        .list_todos_in_state(None, TodoState::Open, 10)
        .await
        .unwrap()
        .remove(0);
    let records = fx
        .store
        .list_archive_records(&RecordQuery::default())
        .await
        .unwrap();
    let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
    let edit = mxr_store::RecordFieldEdit::Set {
        field: "title".into(),
        value_text: Some("Acme Plus receipt".into()),
        value_int: None,
    };
    fx.store
        .edit_archive_records(&ids, &edit, now, None, true)
        .await
        .unwrap();
    let latest = records.iter().max_by_key(|r| r.ledger_date()).unwrap();
    for (field, text, value) in [
        ("amount", Some("GBP".into()), Some(15000)),
        ("issued_at", None, Some(at(2025, 10, 22).timestamp())),
    ] {
        fx.store
            .edit_archive_records(
                &[latest.id.clone()],
                &mxr_store::RecordFieldEdit::Set {
                    field: field.into(),
                    value_text: text,
                    value_int: value,
                },
                now,
                None,
                true,
            )
            .await
            .unwrap();
    }
    fx.apply(now).await;
    let after = fx
        .store
        .list_todos_in_state(None, TodoState::Open, 10)
        .await
        .unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, before.id);
    assert_eq!(after[0].title, "Review Acme Plus renewal");
    assert_eq!(after[0].amount_minor, Some(15000));
    assert_eq!(
        after[0].due_at.unwrap().date_naive(),
        at(2026, 10, 22).date_naive()
    );
    assert!(
        !FieldSources::from_json(&after[0].field_sources)
            .get("amount")
            .unwrap()
            .checked
    );
}
