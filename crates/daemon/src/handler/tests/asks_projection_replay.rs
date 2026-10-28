use super::desk::{Fixture, ME};
use chrono::Duration;
use mxr_core::ThreadId;

async fn pending(fx: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM asks_projection_outbox WHERE account_id = ?")
        .bind(fx.account.as_str())
        .fetch_one(fx.state.store.reader())
        .await
        .unwrap()
}

#[tokio::test]
async fn projection_only_pages_resume_after_failure_and_finish_without_idle_writes() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    for _ in 0..205 {
        fx.message(
            &thread,
            "reviewer@example.com",
            ME,
            Duration::minutes(5),
            None,
        )
        .await;
    }
    super::super::assessments::materialize_threads(
        &fx.state,
        &fx.account,
        std::slice::from_ref(&thread),
        || async { Ok(()) },
    )
    .await
    .unwrap();
    assert_eq!(pending(&fx).await, 105);
    assert!(fx
        .state
        .store
        .dirty_assessment_threads(&fx.account, None)
        .await
        .unwrap()
        .is_empty());
    sqlx::raw_sql("CREATE TRIGGER projection_failure BEFORE INSERT ON todos BEGIN SELECT RAISE(ABORT, 'injected projection failure'); END").execute(fx.state.store.writer()).await.unwrap();
    assert!(
        super::super::assessments::reconcile_dirty_page(&fx.state, &fx.account, None)
            .await
            .is_err()
    );
    assert_eq!(pending(&fx).await, 105, "failed tickets stay durable");
    sqlx::raw_sql("DROP TRIGGER projection_failure")
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    let cursor = super::super::assessments::reconcile_dirty_page(&fx.state, &fx.account, None)
        .await
        .unwrap();
    assert!(
        cursor.is_some(),
        "a projection-only full page must continue"
    );
    assert_eq!(pending(&fx).await, 5);
    assert!(super::super::assessments::reconcile_dirty_page(
        &fx.state,
        &fx.account,
        cursor.as_deref()
    )
    .await
    .unwrap()
    .is_none());
    assert_eq!(pending(&fx).await, 0);
    let before: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(fx.state.store.writer())
        .await
        .unwrap();
    assert!(
        super::super::assessments::reconcile_dirty_page(&fx.state, &fx.account, None)
            .await
            .unwrap()
            .is_none()
    );
    let after: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(fx.state.store.writer())
        .await
        .unwrap();
    assert_eq!(before, after, "settled reconciliation writes nothing");
}
