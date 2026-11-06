//! One undo id applies at most once, even when two clients send it in the same
//! moment, and a partly failed undo stays retryable with the same id.

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::{MessageId, ThreadId};
use mxr_protocol::{MutationCommand, MutationResultData};

async fn archive(fx: &Fixture, ids: &[MessageId]) -> MutationResultData {
    match request(
        fx,
        Request::mutation(MutationCommand::Archive {
            message_ids: ids.to_vec(),
        }),
    )
    .await
    {
        ResponseData::MutationResult { result } => {
            assert_eq!(result.failed, 0, "{result:?}");
            result
        }
        other => panic!("expected a mutation result, got {other:?}"),
    }
}

fn undo_message(mutation_id: &str) -> IpcMessage {
    IpcMessage {
        expected_workspace: None,
        id: 9,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UndoMutation {
            mutation_id: mutation_id.to_owned(),
        }),
    }
}

fn is_ack(payload: &IpcPayload) -> bool {
    matches!(
        payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    )
}

fn refusal(payload: &IpcPayload) -> Option<&str> {
    match payload {
        IpcPayload::Response(Response::Error { message, .. }) => Some(message.as_str()),
        _ => None,
    }
}

async fn in_inbox(fx: &Fixture, id: &MessageId) -> bool {
    fx.state
        .store
        .get_envelope(id)
        .await
        .unwrap()
        .unwrap()
        .label_provider_ids
        .iter()
        .any(|label| label == "INBOX")
}

#[tokio::test]
async fn an_undo_id_applies_once_even_when_two_clients_send_it_at_once() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let message = fx
        .message(&thread, "maya@example.com", ME, Duration::days(1), None)
        .await;
    let result = archive(&fx, std::slice::from_ref(&message.id)).await;
    let mutation_id = result.mutation_id.expect("Archive is undoable");
    assert!(!in_inbox(&fx, &message.id).await, "archived");

    let first_message = undo_message(&mutation_id);
    let second_message = undo_message(&mutation_id);
    let (first, second) = tokio::join!(
        handle_request(&fx.state, &first_message),
        handle_request(&fx.state, &second_message),
    );
    let payloads = [first.payload, second.payload];
    assert_eq!(
        payloads.iter().filter(|payload| is_ack(payload)).count(),
        1,
        "exactly one undo applies: {payloads:?}"
    );
    let refused = payloads
        .iter()
        .filter_map(|payload| refusal(payload))
        .collect::<Vec<_>>();
    assert_eq!(refused.len(), 1, "{payloads:?}");
    assert!(
        refused[0].contains("not found") && refused[0].contains("already undone"),
        "the loser is refused as already undone: {}",
        refused[0]
    );
    assert!(in_inbox(&fx, &message.id).await, "restored exactly once");
}

#[tokio::test]
async fn a_partly_failed_undo_stays_retryable_with_the_same_id() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let one = fx
        .message(&thread, "maya@example.com", ME, Duration::days(2), None)
        .await;
    let two = fx
        .message(&thread, "maya@example.com", ME, Duration::days(1), None)
        .await;
    let result = archive(&fx, &[one.id.clone(), two.id.clone()]).await;
    assert_eq!(result.succeeded, 2);
    let mutation_id = result.mutation_id.expect("Archive is undoable");

    // The second message's provider call fails, so the undo restores one and
    // keeps the other under the same id.
    fx.fake.fail_mutations_of(Some(&two.provider_id));
    let partial = handle_request(&fx.state, &undo_message(&mutation_id))
        .await
        .payload;
    let message = refusal(&partial).expect("a partial undo reports its failure");
    assert!(message.contains("partly done"), "{message}");
    assert!(in_inbox(&fx, &one.id).await, "the first came back");
    assert!(!in_inbox(&fx, &two.id).await, "the second did not");
    assert!(
        fx.state
            .store
            .read_undo_entry(&mutation_id)
            .await
            .unwrap()
            .is_some(),
        "the id still retries"
    );

    fx.fake.fail_mutations_of(None);
    let retried = handle_request(&fx.state, &undo_message(&mutation_id))
        .await
        .payload;
    assert!(is_ack(&retried), "the same id retries: {retried:?}");
    assert!(in_inbox(&fx, &two.id).await, "the second came back");
    assert!(
        fx.state
            .store
            .read_undo_entry(&mutation_id)
            .await
            .unwrap()
            .is_none(),
        "and the retry spends it"
    );
}
