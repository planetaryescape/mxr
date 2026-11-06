//! Bounded conversation reads: the reader's newest page, walking older and
//! newer pages without losing a message, and membership evidence capped to
//! the page the reader actually shows.

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::ThreadId;
use mxr_protocol::{ModeKindData, ThreadWindow};

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

    // Omitted window stays the whole thread for CLI, TUI and MCP.
    assert_eq!(page(&fx, &thread, None).await, ids);
}

#[tokio::test]
async fn focus_and_after_open_a_middle_page_without_loading_the_rest() {
    let fx = Fixture::new().await;
    let (thread, ids) = thread_of(&fx, 12).await;

    let focused = page(
        &fx,
        &thread,
        Some(ThreadWindow {
            limit: 3,
            before: None,
            after: None,
            focus: Some(ids[5].clone()),
        }),
    )
    .await;
    assert_eq!(
        focused,
        ids[3..6],
        "a deep link gets the page ending at its message"
    );

    let newer = page(
        &fx,
        &thread,
        Some(ThreadWindow {
            limit: 3,
            before: None,
            after: Some(focused[focused.len() - 1].clone()),
            focus: None,
        }),
    )
    .await;
    assert_eq!(newer, ids[6..9], "the newer cursor walks forward");

    let message = IpcMessage {
        expected_workspace: None,
        id: 2,
        source: mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetThread {
            thread_id: thread,
            window: Some(ThreadWindow {
                limit: 3,
                before: None,
                after: None,
                focus: Some(mxr_core::MessageId::new()),
            }),
        }),
    };
    assert!(
        matches!(
            handle_request(&fx.state, &message).await.payload,
            IpcPayload::Response(Response::Error { .. })
        ),
        "an unknown cursor is a request failure, never a silent wrong page"
    );
}

#[tokio::test]
async fn list_bodies_returns_every_body_and_a_per_id_failure() {
    let fx = Fixture::new().await;
    let (thread, ids) = thread_of(&fx, 2).await;
    // Outbound mail has an envelope but no stored body in this fixture.
    let without_body = fx
        .message(&thread, ME, "sam@example.com", Duration::minutes(1), None)
        .await
        .id;
    let mut requested = ids.clone();
    requested.push(without_body.clone());
    let ResponseData::Bodies { bodies, failures } = request(
        &fx,
        Request::ListBodies {
            message_ids: requested,
        },
    )
    .await
    else {
        panic!("expected bodies");
    };
    assert_eq!(bodies.len(), 2, "both stored bodies come back");
    assert_eq!(failures.len(), 1, "the missing body is one per-id failure");
    assert_eq!(failures[0].message_id, without_body);
}

#[tokio::test]
async fn membership_evidence_is_capped_but_thread_level_modes_stay_whole() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    for index in 0..3 {
        fx.message(
            &thread,
            "sam@example.com",
            ME,
            Duration::minutes(20 - index),
            None,
        )
        .await;
    }
    // Sent mail carries no ask; it is the evidence a reader page can drop.
    for index in 0..3 {
        fx.message(
            &thread,
            ME,
            "sam@example.com",
            Duration::minutes(10 - index),
            None,
        )
        .await;
    }

    let membership = |limit| {
        let fx = &fx;
        let thread = thread.clone();
        async move {
            match request(
                fx,
                Request::GetModeMembership {
                    message_id: None,
                    thread_id: Some(thread),
                    thread_ids: Vec::new(),
                    limit,
                },
            )
            .await
            {
                ResponseData::ModeMembership { mut threads } => threads.remove(0),
                other => panic!("expected membership, got {other:?}"),
            }
        }
    };

    let full = membership(None).await;
    let capped = membership(Some(4)).await;
    let none = membership(Some(0)).await;

    assert_eq!(
        capped.assessments.len(),
        1,
        "only the newest assessed message keeps its evidence"
    );
    assert!(full.assessments.len() > capped.assessments.len());
    assert_eq!(capped.asks.len(), 4, "asks are capped to the page");
    assert!(full.asks.len() > capped.asks.len());
    assert!(none.asks.is_empty(), "a zero cap carries no evidence");
    assert!(none.assessments.is_empty());
    assert!(none.asks_pending.is_empty());
    assert_eq!(capped.modes, full.modes, "the thread's modes are unchanged");
    assert_eq!(capped.held_by, full.held_by);
    assert_eq!(capped.done_in, full.done_in);
    assert_eq!(capped.in_inbox, full.in_inbox);
    assert!(capped.asks_more, "the reader is told evidence was left out");
    // A thread-level mode line still names the whole thread.
    assert!(capped
        .modes
        .iter()
        .any(|mode| mode.mode == ModeKindData::Messages));
}
