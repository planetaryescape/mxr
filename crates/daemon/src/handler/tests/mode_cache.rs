//! The ready-made Now and mode views: served from the cache only while
//! nothing they show can have changed.

use super::desk::{request, Fixture, ME};
use super::*;
use crate::handler::mode_cache::ModeView;
use chrono::Duration;
use mxr_core::id::ThreadId;

/// When the daemon built the Now it answered with.
async fn now_built_at(fx: &Fixture) -> chrono::DateTime<chrono::Utc> {
    built_at(request(fx, Request::GetNow { account_id: None }).await)
}

fn built_at(data: ResponseData) -> chrono::DateTime<chrono::Utc> {
    match data {
        ResponseData::Now { now } => now.generated_at,
        other => panic!("expected Now, got {other:?}"),
    }
}

/// When the cached Now a read would get was built, without building one.
fn cached_now_built_at(fx: &Fixture) -> Option<chrono::DateTime<chrono::Utc>> {
    fx.state
        .mode_cache
        .cached(&fx.state, &ModeView::Now { account_id: None })
        .map(built_at)
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
    let refreshed = cached_now_built_at(&fx).expect("the refresher rebuilt it");
    assert_ne!(refreshed, first);
    assert_eq!(
        now_built_at(&fx).await,
        refreshed,
        "the refreshed view answers the read"
    );
}

/// Unchanged views are not rebuilt in the background as they age; the
/// first read past `MAX_AGE` builds instead, so an idle daemon does no work.
#[tokio::test]
async fn the_refresher_leaves_an_unchanged_view_to_age_out() {
    let fx = Fixture::new().await;
    let first = now_built_at(&fx).await;
    fx.state.mode_cache.age(std::time::Duration::from_secs(50));
    fx.state.mode_cache.refresh(&fx.state).await;
    assert_eq!(cached_now_built_at(&fx), Some(first), "not rebuilt by age");
    fx.state.mode_cache.age(std::time::Duration::from_secs(11));
    assert_eq!(cached_now_built_at(&fx), None, "too old to answer");
}

#[tokio::test]
async fn a_view_built_in_an_earlier_hour_is_rebuilt_before_it_answers() {
    let fx = Fixture::new().await;
    let first = now_built_at(&fx).await;
    fx.state.mode_cache.previous_hour();
    assert_ne!(now_built_at(&fx).await, first);
}

async fn store_new_mail(fx: &Fixture) {
    fx.message(
        &ThreadId::new(),
        "maya@example.com",
        ME,
        Duration::minutes(5),
        None,
    )
    .await;
}

/// A build that could not wait out a write in flight may have missed a
/// commit, so no later read gets it: each builds afresh until a build
/// reads the count with the writer free, and that one is served.
#[tokio::test]
async fn a_view_built_during_a_write_in_flight_is_never_served_again() {
    let fx = Fixture::new().await;
    fx.state.mode_cache.hold_write(true);
    store_new_mail(&fx).await;
    let unsettled = now_built_at(&fx).await;
    assert_eq!(cached_now_built_at(&fx), None, "not served");
    assert_ne!(now_built_at(&fx).await, unsettled, "the next read builds");

    fx.state.mode_cache.hold_write(false);
    let settled = now_built_at(&fx).await;
    assert_eq!(cached_now_built_at(&fx), Some(settled));
    assert_eq!(
        now_built_at(&fx).await,
        settled,
        "the settled build is served"
    );
}

/// Readers that arrive together during a long write share builds instead
/// of each building again in turn: at most the build already running, and
/// one that starts after they all arrived.
#[tokio::test]
async fn readers_queued_during_a_write_in_flight_share_a_build() {
    let fx = Fixture::new().await;
    fx.state.mode_cache.hold_write(true);
    let reads = (0..6).map(|_| now_built_at(&fx));
    let built: std::collections::BTreeSet<_> =
        futures::future::join_all(reads).await.into_iter().collect();
    assert!(built.len() <= 2, "{} builds for 6 readers", built.len());
}

#[tokio::test]
async fn the_refresher_replaces_a_view_built_during_a_write_in_flight() {
    let fx = Fixture::new().await;
    fx.state.mode_cache.hold_write(true);
    let _unsettled = now_built_at(&fx).await;
    fx.state.mode_cache.hold_write(false);
    fx.state.mode_cache.age(std::time::Duration::from_secs(6));
    fx.state.mode_cache.refresh(&fx.state).await;
    assert!(
        cached_now_built_at(&fx).is_some(),
        "the refresher built one a read can get"
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
