use super::*;
use crate::test_fixtures::*;
use crate::{ScreenerDecision, ScreenerDisposition, Store};
use chrono::Duration;
use mxr_core::types::{Envelope, MessageDirection};

async fn store_with_account() -> (Store, AccountId) {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    (store, account.id)
}

fn envelope(account: &AccountId, n: usize, from: &str, date: DateTime<Utc>) -> Envelope {
    let mut envelope = TestEnvelopeBuilder::new()
        .account_id(account.clone())
        .build();
    envelope.provider_id = format!("p-{n}");
    envelope.from.email = from.to_string();
    envelope.date = date;
    envelope
}

async fn arrive(
    store: &Store,
    account: &AccountId,
    n: usize,
    from: &str,
    date: DateTime<Utc>,
    direction: MessageDirection,
) -> MessageId {
    store
        .upsert_envelope_with_direction(&envelope(account, n, from, date), direction)
        .await
        .unwrap()
}

async fn first_seen(store: &Store, id: &MessageId) -> Option<i64> {
    sqlx::query_scalar("SELECT first_seen_at FROM arrivals WHERE message_id = ?1")
        .bind(id.as_str())
        .fetch_optional(store.reader())
        .await
        .unwrap()
}

fn placement(id: &MessageId, mode: &str) -> ArrivalPlacement {
    ArrivalPlacement {
        message_id: id.clone(),
        mode: mode.to_string(),
        rule: "person".to_string(),
        reason: "from a person".to_string(),
        not_sure: None,
    }
}

#[tokio::test]
async fn storing_an_inbound_message_records_its_arrival_once() {
    let (store, account) = store_with_account().await;
    let now = Utc::now();
    let new = arrive(&store, &account, 1, "maya@example.com", now, MessageDirection::Inbound).await;
    let seen = first_seen(&store, &new).await.expect("an arrival row");
    assert!((seen - now.timestamp()).abs() <= 5, "first seen is when it was stored");

    // Sent mail is not an arrival.
    let sent = arrive(&store, &account, 2, "test@example.com", now, MessageDirection::Outbound).await;
    assert_eq!(first_seen(&store, &sent).await, None);

    // An old message a first sync stores keeps its date: history, not news.
    let old_date = now - Duration::days(5);
    let old = arrive(&store, &account, 3, "a@example.com", old_date, MessageDirection::Inbound).await;
    assert_eq!(first_seen(&store, &old).await, Some(old_date.timestamp()));

    // Mail older than the ledger's 30 days gets no row at all.
    let ancient = arrive(
        &store,
        &account,
        4,
        "b@example.com",
        now - Duration::days(45),
        MessageDirection::Inbound,
    )
    .await;
    assert_eq!(first_seen(&store, &ancient).await, None);

    // A re-sync of the same message updates it and keeps the first sighting.
    sqlx::query("UPDATE arrivals SET first_seen_at = 100 WHERE message_id = ?1")
        .bind(new.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    arrive(&store, &account, 1, "maya@example.com", now, MessageDirection::Inbound).await;
    assert_eq!(first_seen(&store, &new).await, Some(100));
}

#[tokio::test]
async fn the_upgrade_backfills_thirty_days_at_their_dates() {
    let (store, account) = store_with_account().await;
    let now = Utc::now();
    let recent = arrive(&store, &account, 1, "m@example.com", now - Duration::days(3), MessageDirection::Inbound).await;
    let future = arrive(&store, &account, 2, "f@example.com", now + Duration::days(4), MessageDirection::Inbound).await;
    let old = arrive(&store, &account, 3, "o@example.com", now - Duration::days(31), MessageDirection::Inbound).await;
    sqlx::query("DELETE FROM arrivals").execute(store.writer()).await.unwrap();

    let migration = include_str!("../../migrations/070_arrivals.sql");
    let backfill = migration
        .split("-- Backfill:")
        .nth(1)
        .and_then(|rest| rest.split("CREATE TABLE").next())
        .unwrap();
    let statement = &backfill[backfill.find("INSERT").unwrap()..];
    sqlx::raw_sql(sqlx::AssertSqlSafe(statement.to_string()))
        .execute(store.writer())
        .await
        .unwrap();

    assert_eq!(
        first_seen(&store, &recent).await,
        Some((now - Duration::days(3)).timestamp())
    );
    let future_seen = first_seen(&store, &future).await.unwrap();
    assert!(future_seen <= Utc::now().timestamp(), "never first seen in the future");
    assert_eq!(first_seen(&store, &old).await, None);
    let pending = store.pending_arrivals(&account, 100).await.unwrap();
    assert_eq!(pending.len(), 2, "backfilled rows wait to be placed");
}

#[tokio::test]
async fn counts_sum_and_each_list_matches_its_count() {
    let (store, account) = store_with_account().await;
    let now = Utc::now();
    let mut ids = Vec::new();
    for n in 0..7 {
        ids.push(arrive(&store, &account, n, &format!("s{n}@example.com"), now, MessageDirection::Inbound).await);
    }
    // One stays sorting; the rest are placed, one in Spam.
    let modes = ["messages", "messages", "updates", "reading", "reading", "spam"];
    let placements: Vec<_> = ids.iter().zip(modes).map(|(id, mode)| placement(id, mode)).collect();
    store.set_arrival_placements(&placements, now).await.unwrap();
    // A second placement never rewrites the first.
    store
        .set_arrival_placements(&[placement(&ids[0], "reading")], now)
        .await
        .unwrap();

    let since = now - Duration::hours(1);
    let until = now + Duration::hours(1);
    let counts = store.arrival_counts(&[account.clone()], since, until).await.unwrap();
    assert_eq!(counts.total, 7);
    assert_eq!(counts.by_mode.values().sum::<u32>(), counts.total);
    assert_eq!(counts.by_mode["messages"], 2);
    assert_eq!(counts.by_mode["sorting"], 1);
    assert_eq!(counts.by_mode["spam"], 1);

    for (bucket, count) in &counts.by_mode {
        let (rows, total) = store
            .list_arrivals(&[account.clone()], since, until, Some(bucket), 100)
            .await
            .unwrap();
        assert_eq!(total, *count, "{bucket}");
        assert_eq!(rows.len() as u32, *count, "{bucket}");
        assert!(rows.iter().all(|row| &row.effective == bucket));
    }
    let (all, total) = store
        .list_arrivals(&[account.clone()], since, until, None, 100)
        .await
        .unwrap();
    assert_eq!((all.len(), total), (7, 7));

    // Outside the window nothing counts.
    let later = store
        .arrival_counts(&[account], until, until + Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(later.total, 0);
}

#[tokio::test]
async fn a_newer_sender_decision_overrides_a_move_and_clearing_it_restores_the_move() {
    let (store, account) = store_with_account().await;
    let now = Utc::now();
    let id = arrive(&store, &account, 1, "Editor@Weekly.example", now, MessageDirection::Inbound).await;
    store.set_arrival_placements(&[placement(&id, "reading")], now).await.unwrap();

    let moved_at = now - Duration::minutes(10);
    store.set_arrival_move(&id, Some("messages"), Some(moved_at)).await.unwrap();
    let moves = store.arrival_moves_in_force(&account).await.unwrap();
    assert_eq!(moves.get(&id).map(String::as_str), Some("messages"));

    store
        .set_screener_decision(&ScreenerDecision {
            account_id: account.clone(),
            sender_email: "editor@weekly.example".into(),
            disposition: ScreenerDisposition::Feed,
            route_label: None,
            decided_at: now,
        })
        .await
        .unwrap();
    assert!(store.arrival_moves_in_force(&account).await.unwrap().is_empty());
    // The sender's mode re-placed the row; the move is kept for undo.
    store
        .set_arrival_now_modes(&[(id.clone(), Some("reading".into()))])
        .await
        .unwrap();
    let row = store.arrivals_by_ids(std::slice::from_ref(&id)).await.unwrap().remove(0);
    assert_eq!(row.effective, "reading");
    assert_eq!(row.moved_to.as_deref(), Some("messages"));
    assert!(!row.moved);

    store
        .delete_screener_decision(&account, "editor@weekly.example")
        .await
        .unwrap();
    let moves = store.arrival_moves_in_force(&account).await.unwrap();
    assert_eq!(moves.get(&id).map(String::as_str), Some("messages"));
}

#[tokio::test]
async fn corrections_log_undo_once_and_count_only_real_moves() {
    let (store, account) = store_with_account().await;
    let now = Utc::now();
    let correction = |from: &str, to: &str| NewCorrection {
        account_id: account.clone(),
        scope: "email".into(),
        message_id: Some(MessageId::new()),
        sender_email: "Maya@Example.com".into(),
        from_mode: from.into(),
        to_mode: to.into(),
        rule: Some("copied".into()),
        source: "not_sure".into(),
        created_at: now,
        prior_moved_to: None,
        prior_moved_at: None,
        prior_disposition: None,
        aspect_id: None,
    };
    assert!(!store.has_moves(&[account.clone()]).await.unwrap());
    // Keeping it where it was is an answer, not a move.
    store.insert_correction(&correction("updates", "updates")).await.unwrap();
    assert!(!store.has_moves(&[account.clone()]).await.unwrap());
    let id = store.insert_correction(&correction("updates", "messages")).await.unwrap();
    assert_eq!(store.count_moves_since(&[account.clone()], now - Duration::days(7)).await.unwrap(), 1);
    let stored = store.get_correction(id).await.unwrap().unwrap();
    assert_eq!(stored.fields.sender_email, "maya@example.com");

    assert!(store.mark_correction_undone(id, now).await.unwrap());
    assert!(!store.mark_correction_undone(id, now).await.unwrap(), "a second undo is a no-op");
    assert_eq!(store.count_moves_since(&[account.clone()], now - Duration::days(7)).await.unwrap(), 0);
    assert_eq!(store.list_corrections(&[account], 10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn deleting_a_message_takes_its_arrival_and_keeps_the_correction_log() {
    let (store, account) = store_with_account().await;
    let now = Utc::now();
    let id = arrive(&store, &account, 1, "maya@example.com", now, MessageDirection::Inbound).await;
    store
        .insert_correction(&NewCorrection {
            account_id: account.clone(),
            scope: "email".into(),
            message_id: Some(id.clone()),
            sender_email: "maya@example.com".into(),
            from_mode: "reading".into(),
            to_mode: "messages".into(),
            rule: None,
            source: "move".into(),
            created_at: now,
            prior_moved_to: None,
            prior_moved_at: None,
            prior_disposition: None,
            aspect_id: None,
        })
        .await
        .unwrap();
    store
        .delete_messages_and_derived(&account, &["p-1".to_string()])
        .await
        .unwrap();
    assert_eq!(first_seen(&store, &id).await, None);
    assert_eq!(store.list_corrections(&[account], 10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn reads_use_indexes_not_scans() {
    let (store, _account) = store_with_account().await;
    let plan = |sql: String| {
        let store = &store;
        async move {
            sqlx::query(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}")))
                .bind("[]")
                .bind(0)
                .bind(1)
                .fetch_all(store.reader())
                .await
                .unwrap()
                .into_iter()
                .map(|row| row.get::<String, _>("detail"))
                .collect::<Vec<_>>()
        }
    };
    let steps = plan(format!(
        "SELECT {EFFECTIVE}, COUNT(*) FROM arrivals a
         WHERE a.account_id IN (SELECT value FROM json_each(?1))
           AND a.first_seen_at >= ?2 AND a.first_seen_at < ?3"
    ))
    .await;
    assert!(
        steps.iter().any(|s| s.contains("idx_arrivals_account_seen")),
        "{steps:#?}"
    );
}
