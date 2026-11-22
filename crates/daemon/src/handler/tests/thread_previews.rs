use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::ThreadId;
use mxr_core::types::MessageDirection;
use mxr_protocol::ThreadPreviewData;

async fn previews(fx: &Fixture, thread_ids: Vec<ThreadId>) -> Vec<ThreadPreviewData> {
    match request(fx, Request::GetThreadPreviews { thread_ids }).await {
        ResponseData::ThreadPreviews { previews } => previews,
        other => panic!("expected previews, got {other:?}"),
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

    let wire = serde_json::to_string(&found).unwrap();
    assert!(!wire.contains("review the launch plan"), "{wire}");
    assert!(!wire.contains("text_plain"), "{wire}");
}
