use super::desk::{request, Fixture};
use super::records::put_in;
use super::*;
use chrono::{Duration, Utc};
use mxr_core::id::MessageId;
use mxr_protocol::RecordSubscriptionsData;
use mxr_store::TodoState;

async fn subscriptions(fx: &Fixture) -> RecordSubscriptionsData {
    match request(fx, Request::ListRecordSubscriptions { account_id: None }).await {
        ResponseData::RecordSubscriptions { subscriptions } => subscriptions,
        other => panic!("expected subscriptions, got {other:?}"),
    }
}

async fn receipts(
    fx: &Fixture,
    from: (&str, &str),
    subject: &str,
    total: &str,
    days_ago: &[i64],
) -> Vec<MessageId> {
    receipts_in(fx, &fx.account, from, subject, total, days_ago).await
}

async fn receipts_in(
    fx: &Fixture,
    account: &mxr_core::AccountId,
    from: (&str, &str),
    subject: &str,
    total: &str,
    days_ago: &[i64],
) -> Vec<MessageId> {
    let now = Utc::now();
    let mut ids = Vec::new();
    for days in days_ago {
        ids.push(
            put_in(
                fx,
                account,
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
async fn a_yearly_charge_due_soon_creates_one_review_task_on_tick() {
    let fx = Fixture::new().await;
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

    crate::handler::records::tick(&fx.state, Utc::now())
        .await
        .unwrap();
    crate::handler::records::tick(&fx.state, Utc::now())
        .await
        .unwrap();
    let todos = fx
        .state
        .store
        .list_todos_in_state(None, TodoState::Open, 50)
        .await
        .expect("todos");
    assert_eq!(todos.len(), 1, "{todos:#?}");
    assert_eq!(todos[0].title, "Review Admiral renewal");
    let ResponseData::Todo { todo } = request(
        &fx,
        Request::GetTodo {
            todo_id: todos[0].id.clone(),
        },
    )
    .await
    else {
        panic!("expected todo")
    };
    assert!(todo.when_label.starts_with("estimated renewal "));
    assert_eq!(
        todo.amount.as_ref().map(|amount| amount.display.as_str()),
        Some("Estimated £412.00")
    );
    assert!(todos[0]
        .due_words
        .as_ref()
        .unwrap()
        .starts_with("Estimated renewal"));
    let set = |action, dry_run| Request::SetTodoState {
        todo_ids: vec![todos[0].id.clone()],
        action,
        dry_run,
    };
    request(&fx, set(mxr_protocol::TodoStateActionData::Dismiss, true)).await;
    assert_eq!(
        fx.state
            .store
            .get_todo(&todos[0].id)
            .await
            .unwrap()
            .unwrap()
            .state,
        TodoState::Open
    );
    request(&fx, set(mxr_protocol::TodoStateActionData::Dismiss, false)).await;
    crate::handler::records::tick(&fx.state, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        fx.state
            .store
            .get_todo(&todos[0].id)
            .await
            .unwrap()
            .unwrap()
            .state,
        TodoState::Dismissed
    );
    request(&fx, set(mxr_protocol::TodoStateActionData::Undo, false)).await;
    request(&fx, set(mxr_protocol::TodoStateActionData::Done, false)).await;
    crate::handler::records::tick(&fx.state, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        fx.state
            .store
            .get_todo(&todos[0].id)
            .await
            .unwrap()
            .unwrap()
            .state,
        TodoState::Done
    );
}
