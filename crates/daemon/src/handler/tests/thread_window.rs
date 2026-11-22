use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::ThreadId;
use mxr_protocol::ThreadWindow;

async fn thread_of(fx: &Fixture, count: usize) -> (ThreadId, Vec<mxr_core::MessageId>) {
    let thread = ThreadId::new();
    let mut ids = Vec::with_capacity(count);
    for index in 0..count {
        let envelope = fx
            .message(
                &thread,
                "sam@example.com",
                ME,
                Duration::minutes((count - index) as i64),
                None,
            )
            .await;
        ids.push(envelope.id);
    }
    (thread, ids)
}

async fn page(
    fx: &Fixture,
    thread: &ThreadId,
    window: Option<ThreadWindow>,
) -> Vec<mxr_core::MessageId> {
    match request(
        fx,
        Request::GetThread {
            thread_id: thread.clone(),
            window,
        },
    )
    .await
    {
        ResponseData::Thread { messages, .. } => {
            messages.into_iter().map(|message| message.id).collect()
        }
        other => panic!("expected a thread, got {other:?}"),
    }
}

#[tokio::test]
async fn newest_page_and_cursors_leave_no_message_unreachable() {
    let fx = Fixture::new().await;
    let (thread, ids) = thread_of(&fx, 12).await;

    let newest = page(
        &fx,
        &thread,
        Some(ThreadWindow {
            limit: 5,
            before: None,
            after: None,
            focus: None,
        }),
    )
    .await;
    assert_eq!(
        newest,
        ids[7..],
        "the newest page ends at the newest message"
    );

    let older = page(
        &fx,
        &thread,
        Some(ThreadWindow {
            limit: 5,
            before: Some(newest[0].clone()),
            after: None,
            focus: None,
        }),
    )
    .await;
    assert_eq!(older, ids[2..7], "the cursor walks back one page");

    let first = page(
        &fx,
        &thread,
        Some(ThreadWindow {
            limit: 5,
            before: Some(older[0].clone()),
            after: None,
            focus: None,
        }),
    )
    .await;
    assert_eq!(first, ids[..2], "the oldest page stops at the start");

    let mut walked = Vec::new();
    walked.extend(first);
    walked.extend(older);
    walked.extend(newest);
    assert_eq!(walked, ids, "walking the cursors covers every message once");

    assert_eq!(page(&fx, &thread, None).await, ids);
}
