use super::desk::{request, thread_ids, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::ThreadId;
use mxr_core::types::{EventSource, MessageDirection, UnsubscribeMethod};
use mxr_protocol::{DeskLaneData, OwedReplyRowData};

async fn owed(fx: &Fixture, limit: u32, all: bool) -> Vec<OwedReplyRowData> {
    owed_within(fx, limit, all, None, None).await
}

async fn owed_within(
    fx: &Fixture,
    limit: u32,
    all: bool,
    older_than_days: Option<u32>,
    within_days: Option<u32>,
) -> Vec<OwedReplyRowData> {
    match request(
        fx,
        Request::ListOwedReplies {
            account_id: fx.account.clone(),
            older_than_days,
            within_days,
            limit,
            all,
        },
    )
    .await
    {
        ResponseData::OwedReplies { rows } => rows,
        other => panic!("expected owed replies, got {other:?}"),
    }
}

async fn desk_owed(fx: &Fixture) -> DeskLaneData {
    match request(
        fx,
        Request::GetDesk {
            account_id: Some(fx.account.clone()),
            lane_limit: 1000,
        },
    )
    .await
    {
        ResponseData::Desk { owed, .. } => owed,
        other => panic!("expected a desk, got {other:?}"),
    }
}

fn owed_threads(rows: &[OwedReplyRowData]) -> Vec<ThreadId> {
    rows.iter().map(|row| row.thread_id.clone()).collect()
}

#[tokio::test]
async fn owed_is_the_desk_owed_lane_and_all_keeps_the_raw_list() {
    let fx = Fixture::new().await;

    // You answered Maya within an hour twice: your usual pace with her.
    for days in [40, 45] {
        let old = ThreadId::new();
        let asked = fx
            .message(&old, "maya@example.com", ME, Duration::days(days), None)
            .await;
        fx.message(
            &old,
            ME,
            "maya@example.com",
            Duration::days(days) - Duration::hours(1),
            asked.message_id_header.as_deref(),
        )
        .await;
    }
    // Owed: Maya answered you two days ago.
    let maya = ThreadId::new();
    let mine = fx
        .message(&maya, ME, "maya@example.com", Duration::days(3), None)
        .await;
    fx.message(
        &maya,
        "maya@example.com",
        ME,
        Duration::days(2),
        mine.message_id_header.as_deref(),
    )
    .await;
    // Owed: you wrote Priya once before; her new thread waits on you.
    let earlier = ThreadId::new();
    fx.message(&earlier, ME, "priya@example.com", Duration::days(9), None)
        .await;
    let priya = ThreadId::new();
    fx.message(&priya, "priya@example.com", ME, Duration::hours(5), None)
        .await;

    // Not owed by the desk's rule, all in the raw list: a stranger (new
    // from people), a newsletter, archived mail from a person, and a
    // thread from before the desk's window.
    let stranger = ThreadId::new();
    fx.message(&stranger, "theo@example.com", ME, Duration::hours(3), None)
        .await;
    let newsletter_thread = ThreadId::new();
    let mut newsletter = fx
        .message(
            &newsletter_thread,
            "weekly@lists.example.com",
            ME,
            Duration::hours(4),
            None,
        )
        .await;
    newsletter.unsubscribe = UnsubscribeMethod::OneClick {
        url: "https://lists.example.com/unsubscribe".into(),
    };
    fx.store_envelope(&newsletter, MessageDirection::Inbound)
        .await;
    let archived = ThreadId::new();
    let archived_msg = fx
        .message(&archived, "maya@example.com", ME, Duration::days(1), None)
        .await;
    fx.state
        .store
        .set_message_labels(&archived_msg.id, &[], EventSource::User)
        .await
        .unwrap();
    let ancient = ThreadId::new();
    fx.message(&ancient, "jon@example.com", ME, Duration::days(90), None)
        .await;

    // The default is the desk's You owe lane: same rows, same order.
    let lane = desk_owed(&fx).await;
    let rows = owed(&fx, 50, false).await;
    let threads = owed_threads(&rows);
    assert_eq!(threads, thread_ids(&lane));
    assert_eq!(threads.len(), 2);
    assert!(threads.contains(&maya));
    assert!(threads.contains(&priya));
    for (row, desk) in rows.iter().zip(&lane.rows) {
        assert_eq!(row.latest_inbound_msg_id, desk.message_id);
        assert_eq!(row.from_email, desk.counterparty_email);
        assert_eq!(row.latest_inbound_at, desk.since);
        assert_eq!(row.usual_seconds, desk.usual_seconds);
    }
    let maya_row = rows.iter().find(|row| row.thread_id == maya).unwrap();
    assert_eq!(maya_row.usual_seconds, Some(3600));
    assert!((maya_row.expected_days - 1.0 / 24.0).abs() < 1e-9);
    assert!(maya_row.overdue_score > 40.0, "two days against an hour");
    let priya_row = rows.iter().find(|row| row.thread_id == priya).unwrap();
    assert_eq!(priya_row.usual_seconds, None);
    assert!(
        (priya_row.expected_days - 1.0).abs() < 1e-9,
        "a day by default"
    );

    // The limit keeps the lane's order.
    assert_eq!(owed_threads(&owed(&fx, 1, false).await), threads[..1]);

    // --all is the store's raw list, exactly as before.
    let raw = owed(&fx, 50, true).await;
    let before = fx
        .state
        .store
        .list_owed_replies(&fx.account, None, None, 50)
        .await
        .unwrap();
    assert_eq!(raw.len(), before.len());
    for (row, old) in raw.iter().zip(&before) {
        assert_eq!(row.thread_id, old.thread_id);
        assert_eq!(row.latest_inbound_msg_id, old.latest_inbound_msg_id);
        assert_eq!(row.from_email, old.from_email);
        assert_eq!(row.latest_inbound_at, old.latest_inbound_at);
        assert_eq!(row.expected_days, old.expected_days);
        assert!((row.waiting_days - old.waiting_days).abs() < 0.01);
        assert_eq!(row.usual_seconds, None);
    }
    for thread in [&stranger, &newsletter_thread, &archived, &ancient, &priya] {
        assert!(
            owed_threads(&raw).contains(thread),
            "the raw list keeps {thread}"
        );
    }
}

#[tokio::test]
async fn owed_windows_apply_to_the_latest_message_from_them() {
    let fx = Fixture::new().await;
    for (thread, age) in [
        (ThreadId::new(), Duration::days(5)),
        (ThreadId::new(), Duration::hours(6)),
    ] {
        let mine = fx
            .message(
                &thread,
                ME,
                "maya@example.com",
                age + Duration::days(1),
                None,
            )
            .await;
        fx.message(
            &thread,
            "maya@example.com",
            ME,
            age,
            mine.message_id_header.as_deref(),
        )
        .await;
    }
    let count =
        |older_than_days, within_days| owed_within(&fx, 50, false, older_than_days, within_days);
    assert_eq!(count(None, None).await.len(), 2);
    assert_eq!(count(Some(2), None).await.len(), 1);
    assert_eq!(count(None, Some(2)).await.len(), 1);
}

#[tokio::test]
async fn an_allowed_sender_is_new_from_people_until_you_write() {
    let fx = Fixture::new().await;
    // An address the rules read as automated: Allow makes it a person.
    let sender = "notifications@studio.example";
    fx.state
        .store
        .set_screener_decision(&mxr_store::ScreenerDecision {
            account_id: fx.account.clone(),
            sender_email: sender.into(),
            disposition: mxr_store::ScreenerDisposition::Allow,
            route_label: None,
            decided_at: chrono::Utc::now(),
        })
        .await
        .unwrap();
    let thread = ThreadId::new();
    let first = fx
        .message(&thread, sender, ME, Duration::hours(6), None)
        .await;

    let ResponseData::Desk {
        owed: owed_lane,
        people_new,
        ..
    } = fx.desk(25).await
    else {
        panic!("expected a desk");
    };
    assert_eq!(thread_ids(&people_new), vec![thread.clone()]);
    assert_eq!(owed_lane.total, 0, "allowing them is not an exchange");
    assert!(owed(&fx, 50, false).await.is_empty());

    // You reply and they write back: now it is a conversation you owe.
    let reply = fx
        .message(
            &thread,
            ME,
            sender,
            Duration::hours(4),
            first.message_id_header.as_deref(),
        )
        .await;
    fx.message(
        &thread,
        sender,
        ME,
        Duration::hours(2),
        reply.message_id_header.as_deref(),
    )
    .await;
    let ResponseData::Desk {
        owed: owed_lane,
        people_new,
        ..
    } = fx.desk(25).await
    else {
        panic!("expected a desk");
    };
    assert_eq!(thread_ids(&owed_lane), vec![thread.clone()]);
    assert_eq!(people_new.total, 0);
    assert_eq!(owed_threads(&owed(&fx, 50, false).await), vec![thread]);
}

/// Times `mxr owed` both ways on a disposable copy of a real store and
/// checks the default against the desk's You owe lane. Prints counts and
/// timings only, never mail content. The copy is migrated, so never point
/// it at a live store:
///
/// ```sh
/// cp -c "$HOME/Library/Application Support/mxr/mxr.db" /tmp/owed-copy.db
/// MXR_OWED_BENCH_DB=/tmp/owed-copy.db MXR_OWED_BENCH_ACCOUNT=<account id> \
///   cargo test -p mxr --release --lib owed_on_a_store_copy -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "needs MXR_OWED_BENCH_DB pointing at a disposable copy of a real store"]
async fn owed_on_a_store_copy() {
    let path = std::env::var("MXR_OWED_BENCH_DB").expect("MXR_OWED_BENCH_DB");
    let account_id: mxr_core::AccountId = std::env::var("MXR_OWED_BENCH_ACCOUNT")
        .expect("MXR_OWED_BENCH_ACCOUNT")
        .parse()
        .unwrap();
    let mut state = AppState::in_memory_without_accounts().await.unwrap();
    state.store = Arc::new(
        mxr_store::Store::new(std::path::Path::new(&path))
            .await
            .unwrap(),
    );
    let addresses = state.store.list_all_account_addresses().await.unwrap();
    state
        .account_addresses
        .replace(addresses.into_iter().map(|a| (a.account_id, a.email)));
    let state = Arc::new(state);
    let call = |request: Request| {
        let state = state.clone();
        async move {
            let msg = IpcMessage {
                id: 3,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Request(request),
            };
            let started = std::time::Instant::now();
            match handle_request(&state, &msg).await.payload {
                IpcPayload::Response(Response::Ok { data }) => (data, started.elapsed()),
                other => panic!("expected data, got {other:?}"),
            }
        }
    };
    let owed_request = |limit, all| Request::ListOwedReplies {
        account_id: account_id.clone(),
        older_than_days: None,
        within_days: None,
        limit,
        all,
    };
    let rows = |data: ResponseData| match data {
        ResponseData::OwedReplies { rows } => rows,
        other => panic!("expected owed replies, got {other:?}"),
    };

    for all in [false, true] {
        for run in 0..3 {
            let (data, elapsed) = call(owed_request(50, all)).await;
            eprintln!(
                "all={all} limit 50, run {run}: {} rows in {elapsed:?}",
                rows(data).len()
            );
        }
        let (data, elapsed) = call(owed_request(u32::MAX, all)).await;
        eprintln!(
            "all={all} no limit: {} rows in {elapsed:?}",
            rows(data).len()
        );
    }
    let (desk, _) = call(Request::GetDesk {
        account_id: Some(account_id.clone()),
        lane_limit: u32::MAX,
    })
    .await;
    let ResponseData::Desk { owed: lane, .. } = desk else {
        panic!("expected a desk");
    };
    let (data, _) = call(owed_request(u32::MAX, false)).await;
    assert_eq!(owed_threads(&rows(data)), thread_ids(&lane));
    eprintln!(
        "default equals the desk's You owe lane: {} rows",
        lane.total
    );
}
