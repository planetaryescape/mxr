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

/// Let commits land again and wait until every counted one has: the test
/// store marks them when its connection goes back to the pool, a moment
/// after the query.
async fn let_commits_land(fx: &Fixture) {
    fx.state.mode_cache.hold_landing(&fx.state, false);
    let feed = fx.state.store.change_feed();
    for _ in 0..1000 {
        if feed.landed() >= feed.seq() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    panic!("counted commits never landed");
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

/// A build that counts a commit before it lands may miss it. It answers
/// only until that commit lands; then the next read builds afresh, without
/// waiting for the refresher, however long the commit took.
#[tokio::test]
async fn a_view_built_before_a_commit_landed_answers_only_until_it_lands() {
    let fx = Fixture::new().await;
    fx.state.mode_cache.hold_landing(&fx.state, true);
    store_new_mail(&fx).await;
    let unsettled = now_built_at(&fx).await;
    assert_eq!(
        now_built_at(&fx).await,
        unsettled,
        "the commit has not landed"
    );

    let_commits_land(&fx).await;
    let settled = now_built_at(&fx).await;
    assert_ne!(settled, unsettled);
    assert_eq!(
        now_built_at(&fx).await,
        settled,
        "the settled build is served"
    );
}

/// The writer going on to another transaction must not keep a view that
/// missed an earlier, now landed, commit.
#[tokio::test]
async fn a_view_that_missed_a_landed_commit_is_not_served_during_the_next_write() {
    let fx = Fixture::new().await;
    fx.state.mode_cache.hold_landing(&fx.state, true);
    store_new_mail(&fx).await;
    let unsettled = now_built_at(&fx).await;
    let_commits_land(&fx).await;
    // The next transaction has the writer but has not committed.
    let next = fx.state.store.writer().begin().await.unwrap();
    assert_ne!(cached_now_built_at(&fx), Some(unsettled));
    drop(next);
}

#[tokio::test]
async fn the_refresher_replaces_a_view_built_before_a_commit_landed_once_it_lands() {
    let fx = Fixture::new().await;
    fx.state.mode_cache.hold_landing(&fx.state, true);
    store_new_mail(&fx).await;
    let unsettled = now_built_at(&fx).await;
    fx.state.mode_cache.refresh(&fx.state).await;
    assert_eq!(
        cached_now_built_at(&fx),
        Some(unsettled),
        "nothing to gain yet"
    );

    let_commits_land(&fx).await;
    fx.state.mode_cache.refresh(&fx.state).await;
    let refreshed = cached_now_built_at(&fx).expect("the refresher rebuilt it");
    assert_ne!(refreshed, unsettled);
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
