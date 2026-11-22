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

fn whole_seconds(at: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::from_timestamp(at.timestamp(), 0).unwrap()
}

fn threads_of(rows: &[DeskRowData]) -> Vec<ThreadId> {
    rows.iter().map(|row| row.thread_id.clone()).collect()
}

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

async fn owed(fx: &Fixture, age: Duration) -> (ThreadId, Envelope) {
    let thread = ThreadId::new();
    let mine = fx
        .attention_message(
            &thread,
            ME,
            "maya@example.com",
            age + Duration::days(1),
            None,
        )
        .await;
    let answer = fx
        .attention_message(
            &thread,
            "maya@example.com",
            ME,
            age,
            mine.message_id_header.as_deref(),
        )
        .await;
    (thread, answer)
}

async fn waiting(fx: &Fixture, age: Duration) -> (ThreadId, Envelope, Envelope) {
    let thread = ThreadId::new();
    let question = fx
        .attention_message(
            &thread,
            "jon@example.com",
            ME,
            age + Duration::days(1),
            None,
        )
        .await;
    let mine = fx
        .attention_message(
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

    let mut events = fx.state.event_tx.subscribe();
    assert_eq!(
        crate::loops::process_due_timers(&fx.state, after).await,
        Ok(1)
    );
    assert_eq!(drain(&mut events), vec![answer.id.clone()]);
    let later = after + Duration::hours(6);
    assert_eq!(
        crate::loops::process_due_timers(&fx.state, later).await,
        Ok(0)
    );
    assert!(drain(&mut events).is_empty());
    assert_eq!(threads_of(&lanes_at(&fx, later).await[0]), vec![thread]);
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
        crate::loops::process_due_timers(&fx.state, after).await,
        Ok(1)
    );
    assert_eq!(drain(&mut events), vec![mine.id.clone()]);
    assert_eq!(
        crate::loops::process_due_timers(&fx.state, after + Duration::hours(1)).await,
        Ok(0),
        "a restarted loop never fires it again"
    );
    assert!(drain(&mut events).is_empty());
    assert!(fx.state.store.is_reply_later(&mine.id).await.unwrap());
    assert_eq!(threads_of(&lanes_at(&fx, after).await[2]), vec![thread]);
}

#[tokio::test]
async fn a_reply_cancels_the_wait() {
    let fx = Fixture::new().await;
    let (thread, _, mine) = waiting(&fx, Duration::days(2)).await;
    let until = Utc::now() + Duration::days(3);
    defer(&fx, std::slice::from_ref(&thread), until, false).await;

    fx.attention_message(
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
        crate::loops::process_due_timers(&fx.state, after).await,
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

    request(
        &fx,
        Request::UndoMutation {
            mutation_id: mutation_id.unwrap(),
        },
    )
    .await;
    assert!(lanes_at(&fx, Utc::now()).await.iter().all(Vec::is_empty));
    assert_eq!(
        crate::loops::process_due_timers(&fx.state, after).await,
        Ok(1)
    );
    assert!(fx.state.store.is_reply_later(&mine.id).await.unwrap());
}

async fn due_reminder(fx: &Fixture, sent: &Envelope, remind_at: DateTime<Utc>) {
    fx.state
        .store
        .set_auto_reminder(&sent.id, &fx.account, remind_at, sent.date)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_reply_filed_under_another_thread_still_answers_by_its_headers() {
    let fx = Fixture::new().await;
    let (_, _, mine) = waiting(&fx, Duration::days(2)).await;
    due_reminder(&fx, &mine, Utc::now() - Duration::minutes(1)).await;
    fx.attention_message(
        &ThreadId::new(),
        "jon@example.com",
        ME,
        Duration::hours(3),
        mine.message_id_header.as_deref(),
    )
    .await;

    assert_eq!(
        crate::loops::process_due_timers(&fx.state, Utc::now()).await,
        Ok(0)
    );
    let stored = fx
        .state
        .store
        .get_auto_reminder(&mine.id)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.cancelled_at.is_some());
}

#[tokio::test]
async fn a_backdated_reply_brings_reply_later_back_to_you_owe() {
    let fx = Fixture::new().await;
    let (thread, _, mine) = waiting(&fx, Duration::days(1)).await;
    fx.attention_message(
        &thread,
        "jon@example.com",
        ME,
        Duration::days(2),
        mine.message_id_header.as_deref(),
    )
    .await;
    let now = Utc::now();
    assert_eq!(
        threads_of(&lanes_at(&fx, now).await[0]),
        vec![thread.clone()],
        "the desk agrees they wrote last"
    );

    let until = now + Duration::days(1);
    let run = defer(&fx, std::slice::from_ref(&thread), until, false).await;
    assert_eq!(run.items[0].kind, Some(DeferKindData::ReplyLater));
    let [owed, _, waiting, _] = lanes_at(&fx, until + Duration::minutes(1)).await;
    assert_eq!(threads_of(&owed), vec![thread]);
    assert_eq!(owed[0].reason, "back from reply later");
    assert!(waiting.is_empty());
}
