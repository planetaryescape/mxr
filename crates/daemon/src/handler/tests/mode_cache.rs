use super::desk::{request, Fixture, ME};
use super::*;
use crate::handler::mode_cache::ModeView;
use chrono::Duration;
use mxr_core::id::ThreadId;

async fn now_built_at(fx: &Fixture) -> chrono::DateTime<chrono::Utc> {
    built_at(request(fx, Request::GetNow { account_id: None }).await)
}

fn built_at(data: ResponseData) -> chrono::DateTime<chrono::Utc> {
    match data {
        ResponseData::Now { now } => now.generated_at,
        other => panic!("expected Now, got {other:?}"),
    }
}

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
