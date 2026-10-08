//! Subscriptions over the daemon: detection from filed receipts, field
//! provenance, totals per currency, the signals on Now and Archive, a
//! cancellation email, and an approaching yearly renewal (a suggestion
//! only; nothing files a to-do from it).

use super::desk::{request, Fixture};
use super::records::put;
use super::*;
use chrono::{Duration, Utc};
use mxr_core::id::MessageId;
use mxr_protocol::{RecordFilterData, RecordSubscriptionsData};
use mxr_store::TodoState;

async fn subscriptions(fx: &Fixture) -> RecordSubscriptionsData {
    match request(fx, Request::ListRecordSubscriptions { account_id: None }).await {
        ResponseData::RecordSubscriptions { subscriptions } => subscriptions,
        other => panic!("expected subscriptions, got {other:?}"),
    }
}

/// Receipts from `from` with `subject` and `total`, `days_ago` each.
async fn receipts(
    fx: &Fixture,
    from: (&str, &str),
    subject: &str,
    total: &str,
    days_ago: &[i64],
) -> Vec<MessageId> {
    let now = Utc::now();
    let mut ids = Vec::new();
    for days in days_ago {
        ids.push(
            put(
                fx,
                from,
                subject,
                &format!("Thanks for your payment.\nTotal: {total}"),
                None,
                now - Duration::days(*days),
            )
            .await,
        );
    }
    ids
}

async fn scan(fx: &Fixture, ids: &[MessageId]) {
    crate::handler::records::scan_messages(&fx.state, ids).await;
}

#[tokio::test]
async fn monthly_receipts_are_a_subscription_and_a_one_off_is_not() {
    let fx = Fixture::new().await;
    let mut ids = receipts(
        &fx,
        ("Spotify", "no-reply@spotify.com"),
        "Your Spotify Premium receipt",
        "£11.99",
        &[92, 61, 31, 1],
    )
    .await;
    ids.extend(
        receipts(
            &fx,
            ("Spotify", "no-reply@spotify.com"),
            "Your Spotify receipt",
            "£30.00",
            &[45],
        )
        .await,
    );
    scan(&fx, &ids).await;

    let data = subscriptions(&fx).await;
    assert_eq!(data.subscriptions.len(), 1, "{data:#?}");
    let sub = &data.subscriptions[0];
    assert_eq!(sub.title, "Spotify Premium");
    assert_eq!(sub.cadence, "monthly");
    assert_eq!(sub.status, "active");
    assert_eq!(sub.charge_count, 4);
    assert_eq!(sub.one_offs, 1, "the £30 receipt is not part of it");
    assert_eq!(
        sub.amount.as_ref().map(|a| a.display.as_str()),
        Some("£11.99")
    );
    assert_eq!(
        sub.yearly_cost.as_ref().map(|a| a.display.as_str()),
        Some("£143.88")
    );
    // A rule read the amount, so it stays unchecked, and so does what is
    // worked out from it.
    let field = |name: &str| {
        sub.fields
            .iter()
            .find(|f| f.field == name)
            .unwrap_or_else(|| panic!("no {name} field"))
    };
    assert_eq!(field("amount").source, "rule");
    assert!(!field("amount").checked);
    assert_eq!(field("yearly_cost").source, "derived");
    assert!(!field("yearly_cost").checked);
    assert!(!field("next_expected").checked);
    assert_eq!(data.live, 1);
    assert_eq!(data.totals.len(), 1);
    assert_eq!(data.totals[0].per_year.display, "£143.88");
    assert_eq!(data.totals[0].per_month.display, "£11.99");
    assert!(data.empty_state.is_none());
}

#[tokio::test]
async fn no_subscriptions_says_how_they_appear() {
    let fx = Fixture::new().await;
    let data = subscriptions(&fx).await;
    assert!(data.subscriptions.is_empty());
    assert_eq!(
        data.empty_state.as_deref(),
        Some(mxr_protocol::subscription_copy::NONE_YET)
    );
}

#[tokio::test]
async fn a_missed_charge_shows_on_now_and_in_archives_strip() {
    let fx = Fixture::new().await;
    let ids = receipts(
        &fx,
        ("Netflix", "info@account.netflix.com"),
        "Your Netflix receipt",
        "£10.99",
        &[100, 70, 40],
    )
    .await;
    scan(&fx, &ids).await;

    let data = subscriptions(&fx).await;
    assert_eq!(data.subscriptions[0].status, "overdue");
    assert_eq!(data.signals.len(), 1);
    assert_eq!(data.signals[0].kind, "missed_charge");
    assert!(
        data.signals[0].label.starts_with("No Netflix charge since"),
        "{}",
        data.signals[0].label
    );

    let ledger = match request(
        &fx,
        Request::ListRecords {
            account_id: None,
            filter: RecordFilterData::default(),
            limit: 200,
            offset: 0,
        },
    )
    .await
    {
        ResponseData::RecordLedger { ledger } => ledger,
        other => panic!("expected a ledger, got {other:?}"),
    };
    assert!(ledger.coming_up.iter().any(|m| m.kind == "missed_charge"
        && m.group_id.as_deref() == Some(data.subscriptions[0].id.as_str())));

    let now = match request(&fx, Request::GetNow { account_id: None }).await {
        ResponseData::Now { now } => now,
        other => panic!("expected Now, got {other:?}"),
    };
    assert_eq!(
        now.coming_up.first().map(|m| m.kind.as_str()),
        Some("missed_charge")
    );
}

#[tokio::test]
async fn a_price_change_on_the_newest_charge_is_a_signal() {
    let fx = Fixture::new().await;
    let mut ids = receipts(
        &fx,
        ("Netflix", "info@account.netflix.com"),
        "Your Netflix receipt",
        "£10.99",
        &[92, 61, 31],
    )
    .await;
    ids.extend(
        receipts(
            &fx,
            ("Netflix", "info@account.netflix.com"),
            "Your Netflix receipt",
            "£12.99",
            &[1],
        )
        .await,
    );
    scan(&fx, &ids).await;
    let data = subscriptions(&fx).await;
    assert_eq!(data.subscriptions.len(), 1, "{data:#?}");
    let sub = &data.subscriptions[0];
    assert_eq!(sub.price_changes.len(), 1);
    assert!(sub.price_changes[0]
        .label
        .starts_with("£10.99 to £12.99 on "));
    assert_eq!(data.signals.len(), 1);
    assert_eq!(data.signals[0].kind, "price_change");
    assert!(
        data.signals[0]
            .label
            .starts_with("Netflix went up from £10.99 to £12.99"),
        "{}",
        data.signals[0].label
    );
}

#[tokio::test]
async fn a_cancellation_email_from_the_same_sender_ends_it() {
    let fx = Fixture::new().await;
    let ids = receipts(
        &fx,
        ("Spotify", "no-reply@spotify.com"),
        "Your Spotify Premium receipt",
        "£11.99",
        &[62, 32, 2],
    )
    .await;
    scan(&fx, &ids).await;
    put(
        &fx,
        ("Spotify", "no-reply@spotify.com"),
        "Your Premium subscription has been cancelled",
        "We're sorry to see you go.",
        None,
        Utc::now() - Duration::hours(3),
    )
    .await;
    let data = subscriptions(&fx).await;
    let sub = &data.subscriptions[0];
    assert_eq!(sub.status, "ended");
    assert!(sub.status_reason.starts_with("Cancellation email on "));
    assert!(sub.next_expected.is_none());
    assert_eq!(data.live, 0);
    assert!(data.totals.is_empty(), "an ended one costs nothing now");
    assert_eq!(
        data.empty_state.as_deref(),
        Some(mxr_protocol::subscription_copy::ALL_ENDED)
    );
}

#[tokio::test]
async fn a_yearly_charge_due_soon_is_a_renewal_signal_not_a_to_do() {
    let fx = Fixture::new().await;
    // Last charged 357 days ago: the next is due in about a week, inside
    // the renewal signal's 14-day lead time.
    let ids = receipts(
        &fx,
        ("Admiral", "noreply@admiral.com"),
        "Your Admiral receipt",
        "£412.00",
        &[722, 357],
    )
    .await;
    scan(&fx, &ids).await;
    let data = subscriptions(&fx).await;
    assert_eq!(data.subscriptions[0].cadence, "yearly");
    assert!(!data.subscriptions[0].confirmed);
    assert_eq!(data.signals.len(), 1);
    assert_eq!(data.signals[0].kind, "renewal_approaching");
    assert!(
        data.signals[0].label.starts_with("Admiral renews around "),
        "{}",
        data.signals[0].label
    );

    // A suggestion only: nothing is ever filed as a to-do from it.
    let todos = fx
        .state
        .store
        .list_todos_in_state(None, TodoState::Open, 50)
        .await
        .expect("todos");
    assert!(todos.is_empty(), "{todos:#?}");
}

#[tokio::test]
async fn a_yearly_charge_months_away_is_not_a_signal_yet() {
    let fx = Fixture::new().await;
    let ids = receipts(
        &fx,
        ("Admiral", "noreply@admiral.com"),
        "Your Admiral receipt",
        "£412.00",
        &[465, 100],
    )
    .await;
    scan(&fx, &ids).await;
    let data = subscriptions(&fx).await;
    assert!(data.signals.is_empty(), "{:#?}", data.signals);
}
