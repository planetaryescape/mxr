use super::desk::{request, Fixture, ME};
use super::*;
use crate::handler::now;
use chrono::{Duration, FixedOffset, Utc};
use mxr_core::id::ThreadId;
use mxr_core::types::MessageDirection;
use mxr_protocol::{MessagesData, PeopleSplitData};

fn utc() -> FixedOffset {
    FixedOffset::east_opt(0).unwrap()
}

async fn you_write_with(fx: &Fixture, who: &str) -> ThreadId {
    let thread = ThreadId::new();
    let mine = fx
        .attention_message(&thread, ME, who, Duration::days(3), None)
        .await;
    fx.attention_message(
        &thread,
        who,
        ME,
        Duration::hours(5),
        mine.message_id_header.as_deref(),
    )
    .await;
    thread
}

async fn wrote_to_you(fx: &Fixture, who: &str, age: Duration) -> ThreadId {
    let thread = ThreadId::new();
    fx.attention_message(&thread, who, ME, age, None).await;
    thread
}

struct Seeded {
    fx: Fixture,
    maya_threads: [ThreadId; 2],
    first_time: ThreadId,
}

async fn seed() -> Seeded {
    let fx = Fixture::new().await;
    let maya_a = you_write_with(&fx, "maya@example.com").await;
    let maya_b = you_write_with(&fx, "maya@example.com").await;
    you_write_with(&fx, "jon@example.com").await;

    let group = ThreadId::new();
    let started = fx
        .attention_message(&group, ME, "ruth@example.com", Duration::days(2), None)
        .await;
    let mut ruth = fx
        .attention_message(
            &group,
            "ruth@example.com",
            ME,
            Duration::hours(8),
            started.message_id_header.as_deref(),
        )
        .await;
    ruth.cc = vec![mxr_core::types::Address {
        name: None,
        email: "sam@example.com".into(),
    }];
    fx.store_envelope(&ruth, MessageDirection::Inbound).await;

    let first_time = wrote_to_you(&fx, "stranger@example.com", Duration::hours(2)).await;
    wrote_to_you(&fx, "repeat1@example.com", Duration::hours(30)).await;
    wrote_to_you(&fx, "repeat1@example.com", Duration::days(16)).await;
    wrote_to_you(&fx, "repeat2@example.com", Duration::hours(26)).await;
    wrote_to_you(&fx, "repeat2@example.com", Duration::days(16)).await;

    let waiting = ThreadId::new();
    fx.attention_message(&waiting, ME, "waits@example.com", Duration::days(3), None)
        .await;

    let old_owed = ThreadId::new();
    let mine = fx
        .attention_message(&old_owed, ME, "old@example.com", Duration::days(25), None)
        .await;
    fx.attention_message(
        &old_owed,
        "old@example.com",
        ME,
        Duration::days(20),
        mine.message_id_header.as_deref(),
    )
    .await;

    fx.state.store.refresh_contacts().await.unwrap();

    Seeded {
        fx,
        maya_threads: [maya_a, maya_b],
        first_time,
    }
}

async fn messages(fx: &Fixture, turn: Option<mxr_protocol::MessagesTurnData>) -> MessagesData {
    match request(
        fx,
        Request::ListMessages {
            account_id: None,
            turn,
            filter: Default::default(),
            limit: 50,
        },
    )
    .await
    {
        ResponseData::Messages { messages } => messages,
        other => panic!("expected messages, got {other:?}"),
    }
}

async fn rail_count(fx: &Fixture, id: &str) -> Option<u32> {
    match request(fx, Request::GetRail { account_id: None }).await {
        ResponseData::Rail { rail } => rail.entries.iter().find(|e| e.id == id).unwrap().count,
        other => panic!("expected the rail, got {other:?}"),
    }
}

async fn thread_owes(fx: &Fixture, thread: &ThreadId) -> bool {
    match request(
        fx,
        Request::GetThreadContext {
            thread_id: thread.clone(),
        },
    )
    .await
    {
        ResponseData::ThreadContext { context } => context.owed_reply.is_some(),
        other => panic!("expected thread context, got {other:?}"),
    }
}

#[tokio::test]
async fn every_surface_counts_the_same_people() {
    let seeded = seed().await;
    let fx = &seeded.fx;
    let expected = PeopleSplitData {
        write_with: 3,
        others: 3,
    };

    let now = now::now_at(&fx.state, None, Utc::now(), &utc())
        .await
        .unwrap();
    assert_eq!(now.people.split, expected);
    assert_eq!(now.people.total, expected.total());
    assert!(
        now.headline.contains("6 people waiting on you"),
        "{}",
        now.headline
    );

    let messages = messages(fx, None).await;
    assert_eq!(messages.your_turn.len() as u32, now.people.total);
    assert_eq!(messages.your_turn_split, expected);

    assert_eq!(rail_count(fx, "messages").await, Some(now.people.total));

    let ResponseData::Desk { owed, people, .. } = request(
        fx,
        Request::GetDesk {
            account_id: None,
            lane_limit: 50,
        },
    )
    .await
    else {
        panic!("expected the desk");
    };
    assert_eq!(people, expected);
    assert_eq!(owed.total, 4, "four owed conversations are three people");
    assert_eq!(owed.rows.len(), 4);

    let ResponseData::OwedReplies { rows, people } = request(
        fx,
        Request::ListOwedReplies {
            account_id: fx.account.clone(),
            older_than_days: None,
            within_days: None,
            limit: 50,
            all: false,
        },
    )
    .await
    else {
        panic!("expected owed replies");
    };
    assert_eq!(people, Some(expected));
    assert_eq!(
        rows.len(),
        4,
        "the page lists conversations; `people` counts them"
    );
}

#[tokio::test]
async fn the_screener_page_is_the_set_its_badge_counts() {
    let seeded = seed().await;
    let fx = &seeded.fx;
    let ResponseData::Desk { elsewhere, .. } = request(
        fx,
        Request::GetDesk {
            account_id: None,
            lane_limit: 50,
        },
    )
    .await
    else {
        panic!("expected the desk");
    };
    let queue = |all_senders| Request::ListScreenerQueue {
        account_id: fx.account.clone(),
        limit: 100,
        all_senders,
    };
    let ResponseData::ScreenerQueue { entries } = request(fx, queue(false)).await else {
        panic!("expected the queue");
    };
    let ResponseData::ScreenerQueue { entries: all } = request(fx, queue(true)).await else {
        panic!("expected the queue");
    };
    let page: Vec<&str> = entries.iter().map(|e| e.sender_email.as_str()).collect();
    assert_eq!(entries.len() as u32, elsewhere.screener, "{page:?}");
    assert_eq!(
        page,
        ["stranger@example.com"],
        "repeat1/2 first seen 16 days ago"
    );
    let everyone: Vec<&str> = all.iter().map(|e| e.sender_email.as_str()).collect();
    for repeat in ["repeat1@example.com", "repeat2@example.com"] {
        assert!(
            everyone.contains(&repeat),
            "{repeat} stays reachable under all senders"
        );
    }
}

#[tokio::test]
async fn done_here_moves_the_badge_the_page_and_the_headline_together() {
    let seeded = seed().await;
    let fx = &seeded.fx;
    let count = |people: PeopleSplitData| people.total();
    let before = now::now_at(&fx.state, None, Utc::now(), &utc())
        .await
        .unwrap();
    assert_eq!(count(before.people.split), 6);

    let done = |thread: &ThreadId| Request::ResolveDeskItems {
        items: vec![mxr_protocol::DeskDoneItemData {
            thread_id: thread.clone(),
            lane: None,
            commitment_id: None,
        }],
        dry_run: false,
    };
    request(fx, done(&seeded.maya_threads[0])).await;
    let after_one = now::now_at(&fx.state, None, Utc::now(), &utc())
        .await
        .unwrap();
    assert_eq!(after_one.people.total, 6, "Maya still has a thread");

    request(fx, done(&seeded.maya_threads[1])).await;
    request(fx, done(&seeded.first_time)).await;
    let after = now::now_at(&fx.state, None, Utc::now(), &utc())
        .await
        .unwrap();
    assert_eq!(
        after.people.split,
        PeopleSplitData {
            write_with: 2,
            others: 2
        }
    );
    assert_eq!(
        messages(fx, None).await.your_turn.len() as u32,
        after.people.total
    );
    assert_eq!(rail_count(fx, "messages").await, Some(after.people.total));
    assert!(
        !thread_owes(fx, &seeded.first_time).await,
        "a thread put away is not owed"
    );
}
