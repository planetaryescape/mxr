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

/// What the CLI does for each planned request.
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
async fn an_address_with_many_messages_and_one_target_sends_one_request() {
    let (state, fake) = synced_state().await;
    let ids = stamp_messages(&state, 12, |_| mailto("news@list.example")).await;

    let plan = plan(&state, &ids).await;
    assert_eq!(plan.requests.len(), 1);
    assert_eq!(plan.requests[0].message_id, *ids.last().unwrap());
    assert_eq!(plan.already_unsubscribed, 0);

    for request in &plan.requests {
        assert!(matches!(
            send_unsubscribe(&state, &request.message_id).await,
            Response::Ok { .. }
        ));
    }
    assert_eq!(fake.sent_drafts().len(), 1);
}

#[tokio::test]
async fn two_distinct_targets_from_one_sender_send_two_requests() {
    let (state, fake) = synced_state().await;
    let ids = stamp_messages(&state, 8, |index| {
        if index % 2 == 0 {
            mailto("weekly@list.example")
        } else {
            mailto("daily@list.example")
        }
    })
    .await;

    let plan = plan(&state, &ids).await;
    let mut planned: Vec<MessageId> = plan.requests.iter().map(|r| r.message_id.clone()).collect();
    planned.sort_by_key(MessageId::as_str);
    let mut expected = vec![ids[7].clone(), ids[6].clone()];
    expected.sort_by_key(MessageId::as_str);
    assert_eq!(planned, expected, "each target uses its own newest message");

    for request in &plan.requests {
        send_unsubscribe(&state, &request.message_id).await;
    }
    assert_eq!(fake.sent_drafts().len(), 2);
}

#[tokio::test]
async fn a_newest_message_with_a_receipt_sends_nothing_for_its_target() {
    let (state, fake) = synced_state().await;
    let ids = stamp_messages(&state, 5, |_| mailto("news@list.example")).await;
    let newest = ids.last().unwrap().clone();

    assert!(matches!(
        send_unsubscribe(&state, &newest).await,
        Response::Ok { .. }
    ));
    assert_eq!(fake.sent_drafts().len(), 1);

    let plan = plan(&state, &ids).await;
    assert!(plan.requests.is_empty(), "{plan:?}");
    assert_eq!(plan.already_unsubscribed, 1);
    assert_eq!(fake.sent_drafts().len(), 1, "planning never sends anything");
}

#[tokio::test]
async fn a_target_whose_request_is_in_progress_stays_in_the_plan() {
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
    assert_eq!(plan.requests.len(), 1, "{plan:?}");
    assert_eq!(plan.already_unsubscribed, 0);
    assert!(matches!(
        send_unsubscribe(&state, &newest).await,
        Response::Error { code, .. } if code == "unsubscribe_in_progress"
    ));
}

#[tokio::test]
async fn the_plan_the_dry_run_shows_is_what_the_real_run_leaves_unsent() {
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
    let again = plan(&state, &ids).await;
    assert_eq!(preview, again, "the preview and a second plan agree");

    for request in &preview.requests {
        send_unsubscribe(&state, &request.message_id).await;
    }
    assert_eq!(fake.sent_drafts().len(), preview.requests.len());

    let after = plan(&state, &ids).await;
    assert!(after.requests.is_empty());
    assert_eq!(after.already_unsubscribed as usize, preview.requests.len());
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
