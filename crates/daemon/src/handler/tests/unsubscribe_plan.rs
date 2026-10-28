use super::*;
use chrono::TimeZone;
use mxr_core::types::UnsubscribeMethod;
use mxr_core::MessageId;
use mxr_protocol::UnsubscribePlanData;

const SYNCED_FOR_TEST: usize = 20;

async fn synced_state() -> (Arc<AppState>, Arc<mxr_provider_fake::FakeProvider>) {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();
    (Arc::new(state), fake)
}

fn mailto(address: &str) -> UnsubscribeMethod {
    UnsubscribeMethod::Mailto {
        address: address.into(),
        subject: None,
    }
}

/// Gives the first `count` synced messages a List-Unsubscribe method chosen by
/// index, with dates rising by index. Returns their ids, oldest first.
async fn stamp_messages(
    state: &AppState,
    count: usize,
    method: impl Fn(usize) -> UnsubscribeMethod,
) -> Vec<MessageId> {
    let account = state.default_account_id();
    let mut envelopes = state
        .store
        .list_envelopes_by_account(&account, SYNCED_FOR_TEST as u32, 0)
        .await
        .unwrap();
    envelopes.truncate(count);
    assert_eq!(
        envelopes.len(),
        count,
        "the fake mailbox must hold at least {count} messages"
    );
    let base = 1_700_000_000i64;
    for (index, envelope) in envelopes.iter_mut().enumerate() {
        envelope.unsubscribe = method(index);
        envelope.date = chrono::Utc
            .timestamp_opt(base + index as i64 * 60, 0)
            .unwrap();
        state.store.upsert_envelope(envelope).await.unwrap();
    }
    envelopes.into_iter().map(|envelope| envelope.id).collect()
}

async fn plan(state: &Arc<AppState>, message_ids: &[MessageId]) -> UnsubscribePlanData {
    let msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::PlanUnsubscribe {
            message_ids: message_ids.to_vec(),
        }),
    };
    match handle_request(state, &msg).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::UnsubscribePlan { plan },
        }) => plan,
        other => panic!("expected an unsubscribe plan, got {other:?}"),
    }
}

/// What the CLI does for the planned request.
async fn send_unsubscribe(state: &Arc<AppState>, message_id: &MessageId) -> Response {
    let msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsubscribe {
            message_id: message_id.clone(),
        }),
    };
    match handle_request(state, &msg).await.payload {
        IpcPayload::Response(response) => response,
        other => panic!("expected a response, got {other:?}"),
    }
}

#[tokio::test]
async fn one_target_across_many_messages_sends_one_request() {
    let (state, fake) = synced_state().await;
    let ids = stamp_messages(&state, 12, |_| mailto("news@list.example")).await;

    let plan = plan(&state, &ids).await;
    assert_eq!(plan.matched_messages, 12);
    let request = plan.request.clone().expect("one request is planned");
    assert_eq!(request.message_id, *ids.last().unwrap());
    assert!(!plan.already_unsubscribed);

    assert!(matches!(
        send_unsubscribe(&state, &request.message_id).await,
        Response::Ok { .. }
    ));
    assert_eq!(fake.sent_drafts().len(), 1);
}

#[tokio::test]
async fn distinct_tokenised_urls_still_plan_one_request_from_the_newest_message() {
    let (state, _fake) = synced_state().await;
    let ids = stamp_messages(&state, 15, |index| UnsubscribeMethod::OneClick {
        url: format!("https://promo.example/unsubscribe/{index}"),
    })
    .await;

    let plan = plan(&state, &ids).await;
    assert_eq!(plan.matched_messages, 15);
    let request = plan.request.expect("one request is planned");
    assert_eq!(request.message_id, *ids.last().unwrap());
    assert_eq!(
        request.method,
        UnsubscribeMethod::OneClick {
            url: "https://promo.example/unsubscribe/14".into(),
        }
    );
}

#[tokio::test]
async fn a_receipt_on_the_newest_message_plans_no_request() {
    let (state, fake) = synced_state().await;
    let ids = stamp_messages(&state, 5, |_| mailto("news@list.example")).await;
    let newest = ids.last().unwrap().clone();

    assert!(matches!(
        send_unsubscribe(&state, &newest).await,
        Response::Ok { .. }
    ));
    assert_eq!(fake.sent_drafts().len(), 1);

    let plan = plan(&state, &ids).await;
    assert!(plan.request.is_none(), "{plan:?}");
    assert!(plan.already_unsubscribed);
    assert_eq!(fake.sent_drafts().len(), 1, "planning never sends");
}

#[tokio::test]
async fn an_in_progress_claim_stays_in_the_plan_and_the_real_run_reports_it() {
    let (state, _fake) = synced_state().await;
    let ids = stamp_messages(&state, 3, |_| mailto("news@list.example")).await;
    let newest = ids.last().unwrap().clone();
    let account = state.default_account_id();
    state
        .store
        .claim_unsubscribe(&newest, &account, chrono::Utc::now().timestamp())
        .await
        .unwrap();

    let plan = plan(&state, &ids).await;
    let request = plan.request.expect("the in-flight request stays planned");
    assert_eq!(request.message_id, newest);
    assert!(!plan.already_unsubscribed);
    assert!(matches!(
        send_unsubscribe(&state, &newest).await,
        Response::Error { code, .. } if code == "unsubscribe_in_progress"
    ));
}

#[tokio::test]
async fn explicit_message_ids_still_send_one_request_each() {
    let (state, fake) = synced_state().await;
    let ids = stamp_messages(&state, 3, |_| mailto("news@list.example")).await;

    for id in &ids {
        assert!(matches!(
            send_unsubscribe(&state, id).await,
            Response::Ok { .. }
        ));
    }
    assert_eq!(fake.sent_drafts().len(), 3);
}

#[tokio::test]
async fn the_dry_run_plan_is_the_request_the_real_run_sends() {
    let (state, fake) = synced_state().await;
    let ids = stamp_messages(&state, 6, |index| {
        if index < 3 {
            mailto("a@list.example")
        } else {
            mailto("b@list.example")
        }
    })
    .await;

    let preview = plan(&state, &ids).await;
    assert_eq!(preview, plan(&state, &ids).await, "a second plan agrees");

    let request = preview.request.clone().expect("one request is planned");
    assert!(matches!(
        send_unsubscribe(&state, &request.message_id).await,
        Response::Ok { .. }
    ));
    assert_eq!(fake.sent_drafts().len(), 1);

    let after = plan(&state, &ids).await;
    assert!(after.request.is_none());
    assert!(after.already_unsubscribed);
}

#[tokio::test]
async fn messages_without_a_usable_method_plan_nothing_to_send() {
    let (state, fake) = synced_state().await;
    let ids = stamp_messages(&state, 4, |_| UnsubscribeMethod::None).await;

    let plan = plan(&state, &ids).await;
    assert_eq!(plan.matched_messages, 4);
    assert!(plan.request.is_none());
    assert!(!plan.already_unsubscribed);
    assert!(fake.sent_drafts().is_empty());
}
