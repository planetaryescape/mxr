//! Timed deferral (`DeferThreads`): reply later until a time, and "bring it
//! back if nobody replies", on a moved clock. Each comes back at its time,
//! exactly once across a restart, a reply cancels the wait, and Done puts
//! both away for good.

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::Envelope;
use mxr_protocol::{DeferKindData, DeferredThreadData, DeskDoneItemData, DeskRowData};

struct Deferred {
    items: Vec<DeferredThreadData>,
    mutation_id: Option<String>,
}

async fn defer(
    fx: &Fixture,
    threads: &[ThreadId],
    until: DateTime<Utc>,
    dry_run: bool,
) -> Deferred {
    match request(
        fx,
        Request::DeferThreads {
            thread_ids: threads.to_vec(),
            until,
            dry_run,
        },
    )
    .await
    {
        ResponseData::ThreadsDeferred {
            items,
            until: echoed,
            dry_run: echoed_dry_run,
            mutation_id,
            undo_unavailable,
        } => {
            assert_eq!((echoed, echoed_dry_run), (until, dry_run));
            assert!(!undo_unavailable);
            Deferred { items, mutation_id }
        }
        other => panic!("expected ThreadsDeferred, got {other:?}"),
    }
}

/// The desk's four lanes at `now`.
async fn lanes_at(fx: &Fixture, now: DateTime<Utc>) -> [Vec<DeskRowData>; 4] {
    match super::super::desk::get_desk_at(&fx.state, None, 25, now)
        .await
        .unwrap()
    {
        ResponseData::Desk {
            owed,
            due,
            waiting,
            people_new,
            ..
        } => [owed.rows, due.rows, waiting.rows, people_new.rows],
        other => panic!("expected a desk, got {other:?}"),
    }
}

/// Times are stored to the second.
fn whole_seconds(at: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::from_timestamp(at.timestamp(), 0).unwrap()
}

fn threads_of(rows: &[DeskRowData]) -> Vec<ThreadId> {
    rows.iter().map(|row| row.thread_id.clone()).collect()
}

/// The messages named by the return events sent so far (reply later
/// returned, reminder fired), then nothing more.
fn drain(events: &mut tokio::sync::broadcast::Receiver<IpcMessage>) -> Vec<MessageId> {
    let mut seen = Vec::new();
    while let Ok(message) = events.try_recv() {
        if let IpcPayload::Event(
            DaemonEvent::ReplyLaterReturned { message_id }
            | DaemonEvent::ReminderTriggered {
                sent_message_id: message_id,
            },
        ) = message.payload
        {
            seen.push(message_id);
        }
    }
    seen
}

/// You wrote to Maya and she answered: you owe her a reply.
async fn owed(fx: &Fixture, age: Duration) -> (ThreadId, Envelope) {
    let thread = ThreadId::new();
    let mine = fx
        .message(
            &thread,
            ME,
            "maya@example.com",
            age + Duration::days(1),
            None,
        )
        .await;
    let answer = fx
        .message(
            &thread,
            "maya@example.com",
            ME,
            age,
            mine.message_id_header.as_deref(),
        )
        .await;
    (thread, answer)
}

/// Jon asked, you answered: waiting on Jon.
async fn waiting(fx: &Fixture, age: Duration) -> (ThreadId, Envelope, Envelope) {
    let thread = ThreadId::new();
    let question = fx
        .message(
            &thread,
            "jon@example.com",
            ME,
            age + Duration::days(1),
            None,
        )
        .await;
    let mine = fx
        .message(
            &thread,
            ME,
            "jon@example.com",
            age,
            question.message_id_header.as_deref(),
        )
        .await;
    (thread, question, mine)
}

#[tokio::test]
async fn reply_later_at_a_time_leaves_the_desk_and_comes_back_once_across_a_restart() {
    let fx = Fixture::new().await;
    let (thread, answer) = owed(&fx, Duration::hours(3)).await;
    let now = Utc::now();
    assert_eq!(
        threads_of(&lanes_at(&fx, now).await[0]),
        vec![thread.clone()]
    );
    let until = now + Duration::days(2);

    // The preview names what the run does and changes nothing.
    let preview = defer(&fx, std::slice::from_ref(&thread), until, true).await;
    assert!(preview.mutation_id.is_none());
    assert_eq!(preview.items[0].kind, Some(DeferKindData::ReplyLater));
    assert_eq!(preview.items[0].message_id.as_ref(), Some(&answer.id));
    assert_eq!(
        threads_of(&lanes_at(&fx, now).await[0]),
        vec![thread.clone()]
    );

    let run = defer(&fx, std::slice::from_ref(&thread), until, false).await;
    assert_eq!(run.items, preview.items, "the run matches its preview");
    assert!(run.mutation_id.is_some());

    // Away: off the desk and out of the queue until the time.
    let just_before = until - Duration::minutes(1);
    assert!(lanes_at(&fx, just_before).await.iter().all(Vec::is_empty));
    let store = &fx.state.store;
    assert!(store
        .list_reply_later(just_before)
        .await
        .unwrap()
        .is_empty());
    let ResponseData::ReplyQueue { messages } = request(&fx, Request::ListReplyQueue).await else {
        panic!("expected the reply queue");
    };
    assert!(messages.is_empty());

    // Back at the time, in You owe and the queue, before the loop has run.
    let after = until + Duration::minutes(1);
    let [owed, ..] = lanes_at(&fx, after).await;
    assert_eq!(threads_of(&owed), vec![thread.clone()]);
    assert_eq!(owed[0].reason, "back from reply later");
    assert_eq!(owed[0].back_at, Some(whole_seconds(until)));
    assert!(owed[0].overdue);
    assert_eq!(
        store.list_reply_later(after).await.unwrap(),
        vec![answer.id.clone()]
    );

    // Announced once. A second tick, or a restarted daemon's first tick
    // over the same store, announces nothing.
    let mut events = fx.state.event_tx.subscribe();
    assert_eq!(
        crate::loops::process_due_reply_later(&fx.state, after).await,
        Ok(1)
    );
    assert_eq!(drain(&mut events), vec![answer.id.clone()]);
    let later = after + Duration::hours(6);
    assert_eq!(
        crate::loops::process_due_reply_later(&fx.state, later).await,
        Ok(0)
    );
    assert!(drain(&mut events).is_empty());
    assert_eq!(threads_of(&lanes_at(&fx, later).await[0]), vec![thread]);
}

#[tokio::test]
async fn a_reply_later_older_than_the_desk_window_still_comes_back() {
    let fx = Fixture::new().await;
    let (thread, _) = owed(&fx, Duration::days(45)).await;
    let now = Utc::now();
    assert!(lanes_at(&fx, now).await.iter().all(Vec::is_empty));

    let until = now + Duration::days(1);
    defer(&fx, std::slice::from_ref(&thread), until, false).await;
    let [owed, ..] = lanes_at(&fx, until).await;
    assert_eq!(threads_of(&owed), vec![thread]);
}

#[tokio::test]
async fn waiting_with_a_time_leaves_waiting_and_comes_back_once_if_nobody_replies() {
    let fx = Fixture::new().await;
    let (thread, _, mine) = waiting(&fx, Duration::days(2)).await;
    let now = Utc::now();
    assert_eq!(
        threads_of(&lanes_at(&fx, now).await[2]),
        vec![thread.clone()]
    );
    let until = now + Duration::days(3);

    let run = defer(&fx, std::slice::from_ref(&thread), until, false).await;
    assert_eq!(run.items[0].kind, Some(DeferKindData::Waiting));
    assert_eq!(run.items[0].message_id.as_ref(), Some(&mine.id));
    assert!(lanes_at(&fx, until - Duration::minutes(1))
        .await
        .iter()
        .all(Vec::is_empty));

    let after = until + Duration::minutes(1);
    let [_, _, waiting, _] = lanes_at(&fx, after).await;
    assert_eq!(threads_of(&waiting), vec![thread.clone()]);
    assert_eq!(waiting[0].reason, "no reply by the time you set");
    assert_eq!(waiting[0].back_at, Some(whole_seconds(until)));

    let mut events = fx.state.event_tx.subscribe();
    assert_eq!(
        crate::loops::process_due_reminders(&fx.state, after).await,
        Ok(1)
    );
    assert_eq!(drain(&mut events), vec![mine.id.clone()]);
    assert_eq!(
        crate::loops::process_due_reminders(&fx.state, after + Duration::hours(1)).await,
        Ok(0),
        "a restarted loop never fires it again"
    );
    assert!(drain(&mut events).is_empty());
    assert!(fx.state.store.is_reply_later(&mine.id).await.unwrap());
    assert_eq!(threads_of(&lanes_at(&fx, after).await[2]), vec![thread]);
}

#[tokio::test]
async fn a_short_wait_on_a_fresh_message_still_comes_back() {
    let fx = Fixture::new().await;
    // Sent an hour ago: too fresh for Waiting on by itself.
    let (thread, _, _) = waiting(&fx, Duration::hours(1)).await;
    let now = Utc::now();
    let until = now + Duration::hours(2);
    defer(&fx, std::slice::from_ref(&thread), until, false).await;
    assert_eq!(threads_of(&lanes_at(&fx, until).await[2]), vec![thread]);
}

#[tokio::test]
async fn a_reply_cancels_the_wait() {
    let fx = Fixture::new().await;
    let (thread, _, mine) = waiting(&fx, Duration::days(2)).await;
    let until = Utc::now() + Duration::days(3);
    defer(&fx, std::slice::from_ref(&thread), until, false).await;

    // Jon answers before the time: owed at once, never brought back.
    fx.message(
        &thread,
        "jon@example.com",
        ME,
        Duration::minutes(5),
        mine.message_id_header.as_deref(),
    )
    .await;
    let now = Utc::now();
    assert_eq!(
        threads_of(&lanes_at(&fx, now).await[0]),
        vec![thread.clone()]
    );

    let after = until + Duration::minutes(1);
    assert_eq!(
        crate::loops::process_due_reminders(&fx.state, after).await,
        Ok(0)
    );
    assert!(!fx.state.store.is_reply_later(&mine.id).await.unwrap());
    let [owed, _, waiting, _] = lanes_at(&fx, after).await;
    assert_eq!(threads_of(&owed), vec![thread]);
    assert!(owed[0].back_at.is_none());
    assert!(waiting.is_empty());
}

#[tokio::test]
async fn done_cancels_a_pending_wait_and_undo_puts_it_back() {
    let fx = Fixture::new().await;
    let (thread, _, mine) = waiting(&fx, Duration::days(2)).await;
    let until = Utc::now() + Duration::days(3);
    defer(&fx, std::slice::from_ref(&thread), until, false).await;

    let ResponseData::DeskItemsResolved {
        items, mutation_id, ..
    } = request(
        &fx,
        Request::ResolveDeskItems {
            items: vec![DeskDoneItemData {
                thread_id: thread.clone(),
                lane: None,
                commitment_id: None,
            }],
            dry_run: false,
        },
    )
    .await
    else {
        panic!("expected DeskItemsResolved");
    };
    assert_eq!(items[0].reminders_cancelled, 1);
    let after = until + Duration::minutes(1);
    assert!(lanes_at(&fx, after).await.iter().all(Vec::is_empty));

    // Undo: the wait is pending again and fires at its time.
    request(
        &fx,
        Request::UndoMutation {
            mutation_id: mutation_id.unwrap(),
        },
    )
    .await;
    assert!(lanes_at(&fx, Utc::now()).await.iter().all(Vec::is_empty));
    assert_eq!(
        crate::loops::process_due_reminders(&fx.state, after).await,
        Ok(1)
    );
    assert!(fx.state.store.is_reply_later(&mine.id).await.unwrap());
}

#[tokio::test]
async fn done_on_a_returned_wait_puts_it_away_and_out_of_the_queue() {
    let fx = Fixture::new().await;
    let (thread, _, mine) = waiting(&fx, Duration::days(2)).await;
    let until = Utc::now() + Duration::days(1);
    defer(&fx, std::slice::from_ref(&thread), until, false).await;
    let after = until + Duration::minutes(1);
    assert_eq!(
        crate::loops::process_due_reminders(&fx.state, after).await,
        Ok(1)
    );
    assert_eq!(
        threads_of(&lanes_at(&fx, after).await[2]),
        vec![thread.clone()]
    );

    let ResponseData::DeskItemsResolved { items, .. } = request(
        &fx,
        Request::ResolveDeskItems {
            items: vec![DeskDoneItemData {
                thread_id: thread.clone(),
                lane: Some(mxr_protocol::DeskLaneKind::Waiting),
                commitment_id: None,
            }],
            dry_run: false,
        },
    )
    .await
    else {
        panic!("expected DeskItemsResolved");
    };
    assert_eq!(items[0].reply_later_cleared, 1);
    assert!(items[0].dismissed);
    assert!(!fx.state.store.is_reply_later(&mine.id).await.unwrap());
    assert!(lanes_at(&fx, after).await.iter().all(Vec::is_empty));
}

#[tokio::test]
async fn undo_puts_an_earlier_untimed_flag_back_exactly() {
    let fx = Fixture::new().await;
    let (thread, answer) = owed(&fx, Duration::hours(3)).await;
    let flagged_at = Utc::now() - Duration::hours(1);
    fx.state
        .store
        .set_reply_later(&answer.id, flagged_at)
        .await
        .unwrap();

    let run = defer(
        &fx,
        std::slice::from_ref(&thread),
        Utc::now() + Duration::days(1),
        false,
    )
    .await;
    assert!(fx
        .state
        .store
        .list_reply_later(Utc::now())
        .await
        .unwrap()
        .is_empty());

    request(
        &fx,
        Request::UndoMutation {
            mutation_id: run.mutation_id.unwrap(),
        },
    )
    .await;
    let restored = fx
        .state
        .store
        .reply_later_states(std::slice::from_ref(&answer.id))
        .await
        .unwrap()
        .remove(&answer.id)
        .expect("flagged again");
    assert_eq!(restored.due_at, None);
    assert_eq!(restored.set_at.timestamp(), flagged_at.timestamp());
    assert_eq!(
        threads_of(&lanes_at(&fx, Utc::now()).await[0]),
        vec![thread]
    );
}

#[tokio::test]
async fn a_time_that_has_passed_is_refused_and_unknown_threads_say_why() {
    let fx = Fixture::new().await;
    let (thread, _) = owed(&fx, Duration::hours(3)).await;
    let past = IpcMessage {
        id: 9,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::DeferThreads {
            thread_ids: vec![thread.clone()],
            until: Utc::now() - Duration::minutes(1),
            dry_run: false,
        }),
    };
    assert!(matches!(
        handle_request(&fx.state, &past).await.payload,
        IpcPayload::Response(Response::Error { .. })
    ));

    let missing = ThreadId::new();
    let run = defer(
        &fx,
        &[thread.clone(), missing.clone(), thread],
        Utc::now() + Duration::days(1),
        false,
    )
    .await;
    assert!(run.items[0].error.is_none());
    assert_eq!(
        run.items[1].error.as_deref(),
        Some("conversation not found")
    );
    assert!(run.items[2].error.as_deref().unwrap().contains("already"));
    assert_eq!(run.items[1].kind, None);
}
