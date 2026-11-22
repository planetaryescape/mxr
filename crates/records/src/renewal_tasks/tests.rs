use super::*;
use crate::pass::{self, PassConfig};
use chrono::TimeZone;
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{
    Address, BackendRef, Envelope, MessageBody, MessageDirection, MessageFlags, MessageMetadata,
    ProviderKind, UnsubscribeMethod,
};
use mxr_core::Account;

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
        task.due_at.expect("fixture").date_naive(),
        at(2026, 10, 20).date_naive()
    );
    assert!(task
        .due_words
        .as_ref()
        .expect("fixture")
        .starts_with("Estimated renewal"));
    assert!(task.source_message_id.is_some());
    let fields = FieldSources::from_json(&task.field_sources);
    assert!(!fields.get("due_at").expect("fixture").checked);
    assert!(!fields.get("amount").expect("fixture").checked);
    assert!(task.relevant_until.is_none());
    assert_eq!(
        fx.store
            .count_todos_in_state(None, TodoState::Open)
            .await
            .expect("fixture"),
        0
    );
    apply(&fx.store, candidates).await.expect("fixture");
    let task = fx
        .store
        .list_todos_in_state(None, TodoState::Open, 10)
        .await
        .expect("fixture")
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
        .expect("fixture"),
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
        .expect("fixture")
        .remove(0);
    let original_id = task.id.clone();
    task.title = "Compare alternatives first".into();
    task.due_at = Some(at(2026, 10, 15));
    fx.store
        .update_todo_by_user(&task, now)
        .await
        .expect("fixture");
    fx.store
        .set_todos_state(
            std::slice::from_ref(&original_id),
            &[TodoState::Open],
            TodoState::Dismissed,
            now,
        )
        .await
        .expect("fixture");
    fx.receipt("Premium", at(2023, 10, 20), false).await;
    fx.receipt("Premium", at(2025, 10, 21), true).await;
    fx.apply(now).await;
    fx.apply(now).await;
    let saved = fx
        .store
        .get_todo(&original_id)
        .await
        .expect("fixture")
        .expect("fixture");
    assert_eq!(saved.title, task.title);
    assert_eq!(saved.due_at, task.due_at);
    assert_eq!(saved.state, TodoState::Dismissed);
    assert_eq!(
        fx.store
            .count_todos_in_state(None, TodoState::Open)
            .await
            .expect("fixture"),
        0
    );
    assert_eq!(
        fx.store
            .count_todos_in_state(None, TodoState::Dismissed)
            .await
            .expect("fixture"),
        1
    );
    fx.store
        .set_todos_state(
            std::slice::from_ref(&original_id),
            &[TodoState::Dismissed],
            TodoState::Open,
            now,
        )
        .await
        .expect("fixture");
    fx.apply(now).await;
    fx.store
        .set_todos_state(
            std::slice::from_ref(&original_id),
            &[TodoState::Open],
            TodoState::Done,
            now,
        )
        .await
        .expect("fixture");
    fx.apply(now).await;
    assert_eq!(
        fx.store
            .get_todo(&original_id)
            .await
            .expect("fixture")
            .expect("fixture")
            .state,
        TodoState::Done
    );
    fx.receipt("Premium", at(2026, 10, 20), false).await;
    fx.apply(at(2027, 10, 11)).await;
    let open = fx
        .store
        .list_todos_in_state(None, TodoState::Open, 10)
        .await
        .expect("fixture");
    assert_eq!(open.len(), 1);
    assert_ne!(open[0].id, original_id);
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
            .expect("fixture"),
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
    a.expect("fixture");
    b.expect("fixture");
    assert_eq!(
        fx.store
            .count_todos_in_state(None, TodoState::Open)
            .await
            .expect("fixture"),
        1
    );
}
