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
        ResponseData::OwedReplies { rows, .. } => rows,
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

    for days in [40, 45] {
        let old = ThreadId::new();
        let asked = fx
            .attention_message(&old, "maya@example.com", ME, Duration::days(days), None)
            .await;
        fx.attention_message(
            &old,
            ME,
            "maya@example.com",
            Duration::days(days) - Duration::hours(1),
            asked.message_id_header.as_deref(),
        )
        .await;
    }
    let maya = ThreadId::new();
    let mine = fx
        .attention_message(&maya, ME, "maya@example.com", Duration::days(3), None)
        .await;
    fx.attention_message(
        &maya,
        "maya@example.com",
        ME,
        Duration::days(2),
        mine.message_id_header.as_deref(),
    )
    .await;
    let earlier = ThreadId::new();
    fx.attention_message(&earlier, ME, "priya@example.com", Duration::days(9), None)
        .await;
    let priya = ThreadId::new();
    fx.attention_message(&priya, "priya@example.com", ME, Duration::hours(5), None)
        .await;

    let stranger = ThreadId::new();
    fx.attention_message(&stranger, "theo@example.com", ME, Duration::hours(3), None)
        .await;
    let newsletter_thread = ThreadId::new();
    let mut newsletter = fx
        .attention_message(
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
        .attention_message(&archived, "maya@example.com", ME, Duration::days(1), None)
        .await;
    fx.state
        .store
        .set_message_labels(&archived_msg.id, &[], EventSource::User)
        .await
        .unwrap();
    let ancient = ThreadId::new();
    fx.attention_message(&ancient, "jon@example.com", ME, Duration::days(90), None)
        .await;

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

    assert_eq!(owed_threads(&owed(&fx, 1, false).await), threads[..1]);

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
