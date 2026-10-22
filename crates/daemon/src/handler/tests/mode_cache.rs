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
async fn an_unchanged_view_is_still_served_minutes_later() {
    let fx = Fixture::new().await;
    let first = now_built_at(&fx).await;
    fx.state
        .mode_cache
        .age(std::time::Duration::from_secs(5 * 60));
    fx.state.mode_cache.refresh(&fx.state).await;
    assert_eq!(now_built_at(&fx).await, first);
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

#[tokio::test]
async fn a_view_built_in_an_earlier_quarter_hour_is_rebuilt_before_it_answers() {
    let fx = Fixture::new().await;
    let first = now_built_at(&fx).await;
    fx.state.mode_cache.previous_slot();
    assert_ne!(now_built_at(&fx).await, first);
}

/// A build that starts while a commit may still be landing is rebuilt once
/// the change feed has been quiet, even though the count did not move.
#[tokio::test]
async fn a_view_built_while_a_change_settles_is_rebuilt_once_it_has() {
    let fx = Fixture::new().await;
    fx.message(
        &ThreadId::new(),
        "maya@example.com",
        ME,
        Duration::minutes(5),
        None,
    )
    .await;
    let unsettled = now_built_at(&fx).await;
    fx.state.mode_cache.refresh(&fx.state).await;
    assert_eq!(now_built_at(&fx).await, unsettled, "not settled yet");

    tokio::time::sleep(std::time::Duration::from_millis(2_100)).await;
    fx.state.mode_cache.refresh(&fx.state).await;
    let refreshed = now_built_at(&fx).await;
    assert_ne!(refreshed, unsettled);
    assert_eq!(now_built_at(&fx).await, refreshed, "and is served after");
}

/// The read itself must not serve a view that may predate new mail once
/// the change has landed, even before the refresher gets to it.
#[tokio::test]
async fn a_read_after_a_change_settles_never_gets_the_unsettled_view() {
    let fx = Fixture::new().await;
    fx.message(
        &ThreadId::new(),
        "maya@example.com",
        ME,
        Duration::minutes(5),
        None,
    )
    .await;
    let unsettled = now_built_at(&fx).await;
    assert_eq!(now_built_at(&fx).await, unsettled, "still settling");

    tokio::time::sleep(std::time::Duration::from_millis(2_100)).await;
    let settled = now_built_at(&fx).await;
    assert_ne!(settled, unsettled);
    assert_eq!(
        now_built_at(&fx).await,
        settled,
        "the settled build is served"
    );
}

#[tokio::test]
async fn client_chosen_keys_keep_at_most_sixteen_views() {
    let fx = Fixture::new().await;
    for limit in 1..=20 {
        request(
            &fx,
            Request::ListMessages {
                account_id: None,
                turn: None,
                limit,
            },
        )
        .await;
    }
    assert_eq!(fx.state.mode_cache.len(), 16);
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
