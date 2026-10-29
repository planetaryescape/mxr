//! `GetThreadPreviews`: headline facts for many threads at once, no bodies,
//! at most 100 per request, unknown ids left out.

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::ThreadId;
use mxr_core::types::MessageDirection;
use mxr_protocol::{ThreadPreviewData, THREAD_PREVIEWS_MAX};

async fn previews(fx: &Fixture, thread_ids: Vec<ThreadId>) -> Vec<ThreadPreviewData> {
    match request(
        fx,
        Request::GetThreadPreviews {
            thread_accounts: Vec::new(),
            thread_ids,
        },
    )
    .await
    {
        ResponseData::ThreadPreviews { previews } => previews,
        other => panic!("expected previews, got {other:?}"),
    }
}

async fn error_for(fx: &Fixture, thread_ids: Vec<ThreadId>) -> String {
    let msg = IpcMessage {
        expected_workspace: None,
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetThreadPreviews {
            thread_accounts: Vec::new(),
            thread_ids,
        }),
    };
    match handle_request(&fx.state, &msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => message,
        other => panic!("expected an error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_preview_says_who_wrote_last_when_and_how_many_without_any_body() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    fx.message(&thread, ME, "dana@example.com", Duration::days(2), None)
        .await;
    let mut latest = fx
        .message(&thread, "dana@example.com", ME, Duration::hours(3), None)
        .await;
    latest.subject = "Invoice 4471".into();
    latest.snippet = "Please confirm payment by Friday".into();
    latest.from.name = Some("Dana Ruiz".into());
    latest.has_attachments = true;
    fx.store_envelope(&latest, MessageDirection::Inbound).await;

    let found = previews(&fx, vec![thread.clone()]).await;
    assert_eq!(found.len(), 1);
    let preview = &found[0];
    assert_eq!(preview.thread_id, thread);
    assert_eq!(preview.sender_name.as_deref(), Some("Dana Ruiz"));
    assert_eq!(preview.sender_email, "dana@example.com");
    assert_eq!(preview.latest_at, latest.date.timestamp());
    assert_eq!(preview.message_count, 2);
    assert_eq!(preview.unread_count, 1);
    assert_eq!(preview.snippet, "Please confirm payment by Friday");
    assert!(preview.has_attachments);

    // The fixture stores a body for inbound mail; none of it travels.
    let wire = serde_json::to_string(&found).unwrap();
    assert!(!wire.contains("review the launch plan"), "{wire}");
    assert!(!wire.contains("text_plain"), "{wire}");
}

#[tokio::test]
async fn unknown_ids_are_left_out_and_known_ones_keep_request_order() {
    let fx = Fixture::new().await;
    let (first, second) = (ThreadId::new(), ThreadId::new());
    fx.message(&first, "a@example.com", ME, Duration::hours(2), None)
        .await;
    fx.message(&second, "b@example.com", ME, Duration::hours(1), None)
        .await;

    let found = previews(
        &fx,
        vec![
            second.clone(),
            ThreadId::new(),
            first.clone(),
            second.clone(),
        ],
    )
    .await;
    let ids: Vec<_> = found.iter().map(|p| p.thread_id.clone()).collect();
    assert_eq!(ids, vec![second, first]);
}

#[tokio::test]
async fn a_hundred_threads_are_fine_and_a_hundred_and_one_are_refused() {
    let fx = Fixture::new().await;
    let ids: Vec<ThreadId> = (0..THREAD_PREVIEWS_MAX).map(|_| ThreadId::new()).collect();
    assert!(previews(&fx, ids.clone()).await.is_empty());

    let mut too_many = ids;
    too_many.push(ThreadId::new());
    assert_eq!(
        error_for(&fx, too_many).await,
        "at most 100 threads at once"
    );
    assert_eq!(error_for(&fx, Vec::new()).await, "name thread_ids");
}
