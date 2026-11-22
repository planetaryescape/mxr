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

async fn stamp_messages(
    state: &AppState,
    count: usize,
    method: impl Fn(usize) -> UnsubscribeMethod,
) -> Vec<MessageId> {
    stamp_range(state, 0, count, method).await
}

async fn stamp_range(
    state: &AppState,
    start: usize,
    count: usize,
    method: impl Fn(usize) -> UnsubscribeMethod,
) -> Vec<MessageId> {
    let account = state.default_account_id();
    let mut envelopes = state
        .store
        .list_envelopes_by_account(&account, SYNCED_FOR_TEST as u32, 0)
        .await
        .unwrap();
    let mut envelopes: Vec<_> = envelopes.drain(start..).take(count).collect();
    assert_eq!(
        envelopes.len(),
        count,
        "the fake mailbox must hold at least {} messages",
        start + count
    );
    let base = 1_700_000_000i64 + start as i64 * 3_600;
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
