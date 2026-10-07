//! Freshness: the newest arrival across accounts, where each arrival went,
//! and sync failures surfaced with their kind and retry time.

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::ThreadId;
use mxr_core::types::{BackendRef, MessageDirection, ProviderKind};
use mxr_protocol::{FreshnessData, ModeKindData, SyncErrorKindData, SyncHealthData};
use mxr_store::SyncRuntimeStatusUpdate;

async fn freshness(fx: &Fixture, account_id: Option<mxr_core::AccountId>) -> FreshnessData {
    match request(
        fx,
        Request::GetFreshness {
            account_id,
            limit: None,
        },
    )
    .await
    {
        ResponseData::Freshness { freshness } => freshness,
        other => panic!("expected freshness, got {other:?}"),
    }
}

async fn second_account(fx: &Fixture, name: &str, kind: ProviderKind) -> mxr_core::Account {
    let account = mxr_core::Account {
        id: mxr_core::AccountId::new(),
        name: name.into(),
        email: format!("{name}@example.com"),
        sync_backend: Some(BackendRef {
            provider_kind: kind,
            config_key: name.into(),
        }),
        send_backend: None,
        enabled: true,
    };
    fx.state.store.insert_account(&account).await.unwrap();
    account
}

fn close(a: Option<DateTime<Utc>>, b: DateTime<Utc>) -> bool {
    a.is_some_and(|a| (a - b).num_seconds().abs() <= 1)
}

#[tokio::test]
async fn newest_mail_is_the_freshest_arrival_across_accounts_in_any_mailbox() {
    let fx = Fixture::new().await;
    let older = fx
        .message(
            &ThreadId::new(),
            "ana@example.com",
            ME,
            Duration::hours(3),
            None,
        )
        .await;
    // Your own mail never arrives, however recent.
    fx.message(
        &ThreadId::new(),
        ME,
        "ana@example.com",
        Duration::minutes(1),
        None,
    )
    .await;
    // A newer message on another account, archived on arrival: no inbox
    // label, still the newest mail.
    let other = second_account(&fx, "home", ProviderKind::Gmail).await;
    // Built from a copy, not through the fixture, which stores into the
    // default account.
    let mut newer = older.clone();
    newer.from.email = "bo@example.com".into();
    newer.date = Utc::now() - Duration::minutes(10);
    newer.id = mxr_core::id::MessageId::new();
    newer.provider_id = format!("home-{}", newer.id);
    newer.account_id = other.id.clone();
    newer.thread_id = ThreadId::new();
    fx.state
        .store
        .upsert_envelope_with_direction(&newer, MessageDirection::Inbound)
        .await
        .unwrap();

    let all = freshness(&fx, None).await;
    assert!(close(all.newest_message_at, newer.date), "{all:?}");
    assert_eq!(all.accounts.len(), 2);
    assert_eq!(all.arrivals[0].message_id, newer.id);
    assert!(!all.arrivals[0].in_inbox);
    assert!(all.arrivals[0].modes.is_empty());
    assert!(
        all.arrivals.iter().all(|arrival| arrival.from.email != ME),
        "sent mail is not an arrival"
    );

    // One account's scope sees only its own newest.
    let mine = freshness(&fx, Some(fx.account.clone())).await;
    assert!(close(mine.newest_message_at, older.date), "{mine:?}");
    assert_eq!(mine.accounts.len(), 1);
}

#[tokio::test]
async fn each_arrival_says_the_mode_it_went_to_in_one_word() {
    let fx = Fixture::new().await;
    let build = fx
        .message(
            &ThreadId::new(),
            "notifications@github.com",
            ME,
            Duration::minutes(4),
            None,
        )
        .await;
    let data = freshness(&fx, None).await;
    let arrival = data
        .arrivals
        .iter()
        .find(|arrival| arrival.message_id == build.id)
        .expect("the notification arrived");
    assert!(arrival.in_inbox);
    let mode = &arrival.modes[0];
    assert_eq!(mode.mode, ModeKindData::Updates);
    assert_eq!(mode.tag.as_deref(), Some("automated"));
}

#[tokio::test]
async fn sync_failures_surface_with_their_kind_and_the_worst_speaks_for_all() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let store = &fx.state.store;
    // Rate limited: paused, with the retry the sync loop scheduled.
    let retry = now + Duration::minutes(8);
    store
        .upsert_sync_runtime_status(
            &fx.account,
            &SyncRuntimeStatusUpdate {
                last_attempt_at: Some(now),
                last_success_at: Some(now - Duration::minutes(3)),
                last_error: Some(Some("Rate limited, retry after 470s".into())),
                failure_class: Some(Some("rate_limit".into())),
                consecutive_failures: Some(1),
                backoff_until: Some(Some(retry)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // Signed out, and offline: both failing.
    let gmail = second_account(&fx, "work", ProviderKind::Gmail).await;
    let imap = second_account(&fx, "home", ProviderKind::Imap).await;
    for (account, error, class) in [
        (&gmail, "Provider error: oauth token revoked", "auth"),
        (&imap, "Provider error: connection refused", "network"),
    ] {
        store
            .upsert_sync_runtime_status(
                &account.id,
                &SyncRuntimeStatusUpdate {
                    last_attempt_at: Some(now),
                    last_success_at: Some(now - Duration::hours(1)),
                    last_error: Some(Some(error.into())),
                    failure_class: Some(Some(class.into())),
                    consecutive_failures: Some(2),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }

    let data = freshness(&fx, None).await;
    let by_id = |id: &mxr_core::AccountId| {
        data.accounts
            .iter()
            .find(|account| &account.account_id == id)
            .unwrap()
    };
    let paused = by_id(&fx.account);
    assert_eq!(paused.health, SyncHealthData::Paused);
    let error = paused.last_sync_error.as_ref().unwrap();
    assert_eq!(error.kind, SyncErrorKindData::RateLimited);
    assert!(close(error.retry_at, retry));

    let signed_out = by_id(&gmail.id);
    assert_eq!(signed_out.health, SyncHealthData::Failing);
    assert_eq!(signed_out.label, "work", "several accounts: named by name");
    assert_eq!(
        signed_out.last_sync_error.as_ref().unwrap().kind,
        SyncErrorKindData::Auth
    );

    let offline = by_id(&imap.id);
    assert_eq!(offline.health, SyncHealthData::Failing);
    assert_eq!(offline.label, "home");
    assert_eq!(
        offline.last_sync_error.as_ref().unwrap().kind,
        SyncErrorKindData::Offline
    );

    let worst = data.worst_account_id.as_ref().unwrap();
    assert_eq!(by_id(worst).health, SyncHealthData::Failing);
}

#[tokio::test]
async fn your_own_recent_mail_never_hides_the_last_arrival() {
    let fx = Fixture::new().await;
    let arrival = fx
        .message(
            &ThreadId::new(),
            "ana@example.com",
            ME,
            Duration::hours(2),
            None,
        )
        .await;
    // Thirty newer messages from your own address with no stored
    // direction: sent mail synced before the address table knew it.
    for minutes in 0..30 {
        let mut own = arrival.clone();
        own.id = mxr_core::id::MessageId::new();
        own.provider_id = format!("own-{}", own.id);
        own.thread_id = ThreadId::new();
        own.from.email = ME.into();
        own.date = Utc::now() - Duration::minutes(minutes);
        fx.state
            .store
            .upsert_envelope_with_direction(&own, MessageDirection::Unknown)
            .await
            .unwrap();
    }
    let data = freshness(&fx, None).await;
    assert_eq!(
        data.arrivals
            .iter()
            .map(|a| a.message_id.clone())
            .collect::<Vec<_>>(),
        vec![arrival.id]
    );
}
