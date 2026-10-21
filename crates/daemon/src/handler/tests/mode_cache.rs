//! The ready-made Now and mode views: served from the cache only while
//! nothing they show can have changed.

use super::desk::{request, Fixture, ME};
use super::*;
use crate::handler::mode_cache::ModeView;
use chrono::Duration;
use mxr_core::id::ThreadId;

/// When the daemon built the Now it answered with.
async fn now_built_at(fx: &Fixture) -> chrono::DateTime<chrono::Utc> {
    match request(fx, Request::GetNow { account_id: None }).await {
        ResponseData::Now { now } => now.generated_at,
        other => panic!("expected Now, got {other:?}"),
    }
}

#[tokio::test]
async fn a_second_read_is_answered_from_the_cache() {
    let fx = Fixture::new().await;
    let first = now_built_at(&fx).await;
    assert_eq!(now_built_at(&fx).await, first);
}

#[tokio::test]
async fn a_change_through_the_daemon_shows_on_the_next_read() {
    let fx = Fixture::new().await;
    let first = now_built_at(&fx).await;
    let hint = Request::SetHintSeen {
        hint: "todo.runway".into(),
        seen: true,
    };
    assert!(!request_is_read_only(&hint));
    request(&fx, hint).await;
    assert_ne!(now_built_at(&fx).await, first);
}

#[tokio::test]
async fn new_mail_stored_outside_a_request_shows_on_the_next_read() {
    let fx = Fixture::new().await;
    let first = now_built_at(&fx).await;
    fx.message(
        &ThreadId::new(),
        "maya@example.com",
        ME,
        Duration::minutes(5),
        None,
    )
    .await;
    assert_ne!(now_built_at(&fx).await, first);
}

#[tokio::test]
async fn bookkeeping_writes_keep_the_cached_view() {
    let fx = Fixture::new().await;
    let first = now_built_at(&fx).await;
    sqlx::query(
        "INSERT INTO event_log (timestamp, level, category, summary) VALUES (0, 'info', 'sync', 'x')",
    )
    .execute(fx.state.store.writer())
    .await
    .unwrap();
    assert_eq!(now_built_at(&fx).await, first);
}

#[tokio::test]
async fn a_view_past_its_age_is_rebuilt_before_it_answers() {
    let fx = Fixture::new().await;
    let first = now_built_at(&fx).await;
    fx.state.mode_cache.age(std::time::Duration::from_secs(61));
    assert_ne!(now_built_at(&fx).await, first);
}

#[tokio::test]
async fn the_refresher_rebuilds_a_view_after_a_mutation_so_reads_stay_instant() {
    let fx = Fixture::new().await;
    let first = now_built_at(&fx).await;
    fx.state.mode_cache.note_mutation();
    fx.state.mode_cache.refresh(&fx.state).await;
    let refreshed = now_built_at(&fx).await;
    assert_ne!(refreshed, first);
    assert_eq!(
        now_built_at(&fx).await,
        refreshed,
        "the refreshed view answers the read"
    );
}

#[test]
fn requests_that_mark_something_seen_always_run() {
    let updates = |mark_seen| Request::GetUpdatesDigest {
        account_id: None,
        cut: None,
        mark_seen,
        expired: false,
    };
    let reading = |mark_visit| Request::GetReadingEdition {
        account_id: None,
        mark_visit,
    };
    assert!(ModeView::of(&updates(false)).is_some());
    assert!(ModeView::of(&updates(true)).is_none());
    assert!(ModeView::of(&reading(false)).is_some());
    assert!(ModeView::of(&reading(true)).is_none());
    assert!(ModeView::of(&Request::GetArrivals {
        account_id: None,
        mark_seen: false,
        since: None,
    })
    .is_none());
}
