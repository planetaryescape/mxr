//! One definition of who is waiting on you: Now, the rail, Messages, the
//! Owed page, the desk, a thread's relationship line and the Screener all
//! count from the same lanes, on one seeded account.

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

/// You wrote to `who`; they replied, so it's your turn.
async fn you_write_with(fx: &Fixture, who: &str) -> ThreadId {
    let thread = ThreadId::new();
    let mine = fx.message(&thread, ME, who, Duration::days(3), None).await;
    fx.message(
        &thread,
        who,
        ME,
        Duration::hours(5),
        mine.message_id_header.as_deref(),
    )
    .await;
    thread
}

/// Mail from a stranger you never wrote to, `age` ago.
async fn wrote_to_you(fx: &Fixture, who: &str, age: Duration) -> ThreadId {
    let thread = ThreadId::new();
    fx.message(&thread, who, ME, age, None).await;
    thread
}

/// Two people, two owed threads with one of them (Maya), a group thread,
/// two first-time senders, two repeat senders first seen 16 days ago,
/// one thread you wait on and one owed thread past its decay.
struct Seeded {
    fx: Fixture,
    maya_threads: [ThreadId; 2],
    group: ThreadId,
    repeat: ThreadId,
    first_time: ThreadId,
    waiting: ThreadId,
    old_owed: ThreadId,
}

async fn seed() -> Seeded {
    let fx = Fixture::new().await;
    let maya_a = you_write_with(&fx, "maya@example.com").await;
    let maya_b = you_write_with(&fx, "maya@example.com").await;
    you_write_with(&fx, "jon@example.com").await;

    // A group thread: you wrote in it, Ruth answered. One unit.
    let group = ThreadId::new();
    let started = fx
        .message(&group, ME, "ruth@example.com", Duration::days(2), None)
        .await;
    let mut ruth = fx
        .message(
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
    // Repeat senders: first seen 16 days ago, wrote again yesterday, and
    // you never replied. They are "others who wrote to you" on the desk
    // and past the Screener's 14 days.
    let repeat = wrote_to_you(&fx, "repeat1@example.com", Duration::hours(30)).await;
    wrote_to_you(&fx, "repeat1@example.com", Duration::days(16)).await;
    wrote_to_you(&fx, "repeat2@example.com", Duration::hours(26)).await;
    wrote_to_you(&fx, "repeat2@example.com", Duration::days(16)).await;

    let waiting = ThreadId::new();
    fx.message(&waiting, ME, "waits@example.com", Duration::days(3), None)
        .await;

    // They replied 20 days ago, you wrote to them 25 days ago: past the
    // turn's decay, so nobody's turn any more.
    let old_owed = ThreadId::new();
    let mine = fx
        .message(&old_owed, ME, "old@example.com", Duration::days(25), None)
        .await;
    fx.message(
        &old_owed,
        "old@example.com",
        ME,
        Duration::days(20),
        mine.message_id_header.as_deref(),
    )
    .await;

    // The contacts table is what first-seen dates come from.
    fx.state.store.refresh_contacts().await.unwrap();

    Seeded {
        fx,
        maya_threads: [maya_a, maya_b],
        group,
        repeat,
        first_time,
        waiting,
        old_owed,
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
    // Maya (two threads), Jon and the group are people you write with; the
    // stranger and the two repeat senders wrote to you.
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
async fn waiting_people_are_not_counted_and_their_view_has_its_own_empty_line() {
    let seeded = seed().await;
    let fx = &seeded.fx;
    let theirs = messages(fx, Some(mxr_protocol::MessagesTurnData::Theirs)).await;
    assert!(theirs.your_turn.is_empty());
    assert_eq!(
        theirs.your_turn_split.total(),
        6,
        "still describes Your turn"
    );
    assert!(theirs.empty_state.is_none(), "one thread waits on them");

    // Nobody to wait on: the view says so in its own words, not that
    // nobody is waiting on you.
    let quiet = Fixture::new().await;
    you_write_with(&quiet, "maya@example.com").await;
    let theirs = messages(&quiet, Some(mxr_protocol::MessagesTurnData::Theirs)).await;
    assert_eq!(
        theirs.empty_state.as_deref(),
        Some(mxr_protocol::messages_copy::WAITING_CLEAR)
    );
    let mine = messages(&quiet, None).await;
    assert!(mine.empty_state.is_none(), "Maya is waiting on you");
}

#[tokio::test]
async fn a_thread_says_you_owe_only_when_it_is_on_the_desk() {
    let seeded = seed().await;
    let fx = &seeded.fx;
    for owed in [
        &seeded.maya_threads[0],
        &seeded.maya_threads[1],
        &seeded.group,
    ] {
        assert!(thread_owes(fx, owed).await, "owed thread {owed}");
    }
    assert!(
        thread_owes(fx, &seeded.repeat).await,
        "a repeat sender wrote to you"
    );
    assert!(thread_owes(fx, &seeded.first_time).await);
    assert!(
        !thread_owes(fx, &seeded.waiting).await,
        "you wrote last: waiting, not owed"
    );
    assert!(
        !thread_owes(fx, &seeded.old_owed).await,
        "past its decay: nobody's turn, so no line"
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
async fn deserialises_without_the_new_fields() {
    let json = serde_json::json!({
        "rows": [], "total": 2, "more_line": "and 2 more in Messages"
    });
    let people: mxr_protocol::NowPeopleData = serde_json::from_value(json).unwrap();
    assert_eq!(people.split, PeopleSplitData::default());
    let request: Request = serde_json::from_value(serde_json::json!({
        "cmd": "ListScreenerQueue",
        "account_id": fx_account_json(),
    }))
    .unwrap();
    assert!(matches!(
        request,
        Request::ListScreenerQueue {
            all_senders: false,
            limit: 100,
            ..
        }
    ));
}

fn fx_account_json() -> serde_json::Value {
    serde_json::to_value(mxr_core::id::AccountId::new()).unwrap()
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

    // Maya has two owed threads: putting one away leaves her a unit.
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

/// Now's total, Messages' Your turn rows and the rail's Messages badge on
/// the same account at the same moment.
async fn three_counts(fx: &Fixture) -> (u32, u32, Option<u32>) {
    let now = now::now_at(&fx.state, None, Utc::now(), &utc())
        .await
        .unwrap();
    (
        now.people.total,
        messages(fx, None).await.your_turn.len() as u32,
        rail_count(fx, "messages").await,
    )
}

#[tokio::test]
async fn the_owed_page_says_how_many_others_wait_when_it_is_empty() {
    let fx = Fixture::new().await;
    wrote_to_you(&fx, "stranger@example.com", Duration::hours(2)).await;
    let ResponseData::OwedReplies { rows, people } = request(
        &fx,
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
    assert!(rows.is_empty());
    assert_eq!(
        people,
        Some(PeopleSplitData {
            write_with: 0,
            others: 1
        })
    );
    assert_eq!(three_counts(&fx).await, (1, 1, Some(1)));
}

/// Alice, watched daily, wrote in a group thread two days ago; Bob, whom
/// you never wrote to, wrote yesterday. The desk keeps the thread on Waiting
/// on (it outranks New from people), so nobody's turn.
async fn watched_alice_group(fx: &Fixture) -> ThreadId {
    let thread = ThreadId::new();
    let mut alice = fx
        .message(&thread, "alice@example.com", ME, Duration::days(2), None)
        .await;
    alice.to = vec![
        mxr_core::types::Address {
            name: None,
            email: ME.into(),
        },
        mxr_core::types::Address {
            name: None,
            email: "bob@example.com".into(),
        },
    ];
    fx.store_envelope(&alice, MessageDirection::Inbound).await;
    let mut bob = fx
        .message(&thread, "bob@example.com", ME, Duration::days(1), None)
        .await;
    bob.cc = vec![mxr_core::types::Address {
        name: None,
        email: "alice@example.com".into(),
    }];
    fx.store_envelope(&bob, MessageDirection::Inbound).await;
    fx.state
        .store
        .upsert_contact(&mxr_core::types::ContactRow {
            account_id: fx.account.clone(),
            email: "alice@example.com".into(),
            display_name: Some("Alice".into()),
            first_seen_at: Utc::now() - Duration::days(200),
            last_seen_at: Utc::now() - Duration::days(2),
            last_inbound_at: Some(Utc::now() - Duration::days(2)),
            last_outbound_at: Some(Utc::now() - Duration::days(30)),
            total_inbound: 5,
            total_outbound: 5,
            replied_count: 0,
            cadence_days_p50: None,
        })
        .await
        .unwrap();
    fx.state
        .store
        .watch_cadence(
            &mxr_store::RelationshipWatchEntry {
                account_id: fx.account.clone(),
                email: "alice@example.com".into(),
                expected_days: Some(1.0),
                note: None,
                added_at: Utc::now(),
            },
            false,
        )
        .await
        .unwrap();
    thread
}

#[tokio::test]
async fn a_watched_persons_waiting_row_outranks_a_new_senders_turn_in_every_count() {
    let fx = Fixture::new().await;
    watched_alice_group(&fx).await;
    let ResponseData::Desk { waiting, .. } = fx.desk(25).await else {
        panic!("expected a desk");
    };
    assert_eq!(
        waiting.total, 1,
        "Alice's drift anchors to the group thread"
    );
    assert_eq!(three_counts(&fx).await, (0, 0, Some(0)));
}

#[tokio::test]
async fn a_small_messages_window_does_not_hide_a_turn_the_desk_counts() {
    let fx = Fixture::new().await;
    let mut config = fx.state.config_snapshot();
    config.messages.recent_days = 1;
    fx.state.set_config_for_test(config).await;
    wrote_to_you(&fx, "stranger@example.com", Duration::days(2)).await;
    assert_eq!(three_counts(&fx).await, (1, 1, Some(1)));
}

#[tokio::test]
async fn a_disabled_restored_account_adds_nothing_to_one_surface_only() {
    let fx = Fixture::new().await;
    wrote_to_you(&fx, "stranger@example.com", Duration::hours(2)).await;
    fx.state
        .store
        .set_account_enabled(&fx.account, false)
        .await
        .unwrap();
    assert_eq!(three_counts(&fx).await, (0, 0, Some(0)));
}

#[tokio::test]
async fn the_screener_page_keeps_an_eligible_sender_behind_five_hundred_newer_ones() {
    let fx = Fixture::new().await;
    let eligible = wrote_to_you(&fx, "eligible@example.com", Duration::days(3)).await;
    for index in 0..500 {
        let thread = ThreadId::new();
        let archived = fx
            .message(
                &thread,
                &format!("archived{index}@example.com"),
                ME,
                Duration::hours(2),
                None,
            )
            .await;
        fx.state
            .store
            .set_message_labels(&archived.id, &[], mxr_core::types::EventSource::User)
            .await
            .unwrap();
    }
    fx.state.store.refresh_contacts().await.unwrap();
    let ResponseData::Desk { elsewhere, .. } = fx.desk(25).await else {
        panic!("expected a desk");
    };
    assert_eq!(elsewhere.screener, 1, "{eligible}: the one inbox sender");
    let ResponseData::ScreenerQueue { entries } = request(
        &fx,
        Request::ListScreenerQueue {
            account_id: fx.account.clone(),
            limit: 100,
            all_senders: false,
        },
    )
    .await
    else {
        panic!("expected the queue");
    };
    let page: Vec<&str> = entries.iter().map(|e| e.sender_email.as_str()).collect();
    assert_eq!(page, ["eligible@example.com"]);
}

#[tokio::test]
async fn a_thread_outranked_by_a_watched_waiting_row_is_not_owed() {
    let fx = Fixture::new().await;
    let thread = watched_alice_group(&fx).await;
    assert!(
        !thread_owes(&fx, &thread).await,
        "Waiting on outranks New from people"
    );
}

#[tokio::test]
async fn list_sender_history_does_not_erase_possible_participants_or_split_surface_counts() {
    let fx = Fixture::new().await;
    let mut config = fx.state.config_snapshot();
    config.messages.recent_days = 1;
    fx.state.set_config_for_test(config).await;
    // A known list address, with its own archived thread three days old.
    sqlx::query(
        "INSERT INTO contacts (account_id, email, display_name, first_seen_at, last_seen_at,
           last_inbound_at, last_outbound_at, total_inbound, total_outbound, replied_count,
           cadence_days_p50, is_list_sender, list_id, refreshed_at)
         VALUES (?, 'crew@partners.example.com', NULL, 0, 0, NULL, NULL, 9, 0, 0, NULL, 1,
           'lists.example.com', 0)",
    )
    .bind(fx.account.as_str())
    .execute(fx.state.store.writer())
    .await
    .unwrap();
    let archived = wrote_to_you(&fx, "crew@partners.example.com", Duration::days(3)).await;
    let _ = archived;
    // Two owed Alice threads, each written with the list copied.
    for _ in 0..2 {
        let thread = you_write_with(&fx, "alice@example.com").await;
        for mut envelope in fx.state.store.get_thread_envelopes(&thread).await.unwrap() {
            if envelope.from.email == ME {
                envelope.cc = vec![mxr_core::types::Address {
                    name: None,
                    email: "crew@partners.example.com".into(),
                }];
                fx.store_envelope(&envelope, MessageDirection::Outbound)
                    .await;
            }
        }
    }
    let (now, rows, rail) = three_counts(&fx).await;
    assert_eq!(
        (now, rows, rail),
        (3, 3, Some(3)),
        "current personal asks and two possible group conversations remain visible consistently; list history alone is not permanent identity"
    );
}

#[tokio::test]
async fn the_screener_page_lists_every_sender_the_badge_counts() {
    let fx = Fixture::new().await;
    for index in 0..101 {
        wrote_to_you(
            &fx,
            &format!("sender{index}@example.com"),
            Duration::hours(2),
        )
        .await;
    }
    fx.state.store.refresh_contacts().await.unwrap();
    let ResponseData::Desk { elsewhere, .. } = fx.desk(25).await else {
        panic!("expected a desk");
    };
    assert_eq!(elsewhere.screener, 101);
    let ResponseData::ScreenerQueue { entries } = request(
        &fx,
        Request::ListScreenerQueue {
            account_id: fx.account.clone(),
            limit: 100,
            all_senders: false,
        },
    )
    .await
    else {
        panic!("expected the queue");
    };
    assert_eq!(entries.len(), 101, "limit never shortens the badge's set");
}

#[tokio::test]
async fn the_screener_page_counts_unknown_direction_mail_as_inbound_like_the_badge() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let envelope = fx
        .message(&thread, "old@example.com", ME, Duration::hours(24), None)
        .await;
    // An older database before repair: the message is stored without a
    // direction.
    sqlx::query("UPDATE messages SET direction = 'unknown' WHERE id = ?")
        .bind(envelope.id.as_str())
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    fx.state.store.refresh_contacts().await.unwrap();
    let ResponseData::Desk { elsewhere, .. } = fx.desk(25).await else {
        panic!("expected a desk");
    };
    assert_eq!(elsewhere.screener, 1);
    let ResponseData::ScreenerQueue { entries } = request(
        &fx,
        Request::ListScreenerQueue {
            account_id: fx.account.clone(),
            limit: 100,
            all_senders: false,
        },
    )
    .await
    else {
        panic!("expected the queue");
    };
    assert_eq!(entries.len(), 1);
}

#[tokio::test]
async fn the_all_accounts_screener_badge_counts_the_account_its_link_opens() {
    let fx = Fixture::new().await;
    wrote_to_you(&fx, "first@example.com", Duration::hours(2)).await;
    // A second account with its own new sender.
    let second = mxr_core::AccountId::new();
    fx.state
        .store
        .insert_account(&crate::test_fixtures::test_account_with_id(second.clone()))
        .await
        .unwrap();
    let inbox = mxr_core::id::LabelId::from_scoped_provider_id(&second, "fake", "INBOX");
    fx.state
        .store
        .upsert_label(&mxr_core::types::Label {
            id: inbox.clone(),
            account_id: second.clone(),
            name: "Inbox".into(),
            kind: mxr_core::types::LabelKind::System,
            color: None,
            provider_id: "INBOX".into(),
            unread_count: 0,
            total_count: 0,
            role: None,
        })
        .await
        .unwrap();
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(second.clone())
        .thread_id(ThreadId::new())
        .provider_id("second-new")
        .sender_address("Newcomer", "second@example.com")
        .date(Utc::now() - Duration::hours(2))
        .build();
    fx.state
        .store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
        .await
        .unwrap();
    fx.state
        .store
        .set_message_labels(
            &envelope.id,
            std::slice::from_ref(&inbox),
            mxr_core::types::EventSource::User,
        )
        .await
        .unwrap();
    fx.state.store.refresh_contacts().await.unwrap();

    let ResponseData::Desk { elsewhere, .. } = fx.desk(25).await else {
        panic!("expected a desk");
    };
    let opens = elsewhere.screener_account.expect("a queue to open");
    let ResponseData::ScreenerQueue { entries } = request(
        &fx,
        Request::ListScreenerQueue {
            account_id: opens,
            limit: 100,
            all_senders: false,
        },
    )
    .await
    else {
        panic!("expected the queue");
    };
    assert_eq!(entries.len(), 1, "one sender waits in each account");
    assert_eq!(
        elsewhere.screener as usize,
        entries.len(),
        "the badge is the page it opens, not the sum of both accounts"
    );
}
