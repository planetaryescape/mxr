use super::*;
use crate::test_fixtures::*;
use crate::Store;
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
    let new = arrive(
        &store,
        &account,
        1,
        "maya@example.com",
        now,
        MessageDirection::Inbound,
    )
    .await;
    let seen = first_seen(&store, &new).await.expect("an arrival row");
    assert!(
        (seen - now.timestamp()).abs() <= 5,
        "first seen is when it was stored"
    );

    let sent = arrive(
        &store,
        &account,
        2,
        "test@example.com",
        now,
        MessageDirection::Outbound,
    )
    .await;
    assert_eq!(first_seen(&store, &sent).await, None);

    let old_date = now - Duration::days(5);
    let old = arrive(
        &store,
        &account,
        3,
        "a@example.com",
        old_date,
        MessageDirection::Inbound,
    )
    .await;
    assert_eq!(first_seen(&store, &old).await, Some(old_date.timestamp()));

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

    sqlx::query("UPDATE arrivals SET first_seen_at = 100 WHERE message_id = ?1")
        .bind(new.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    arrive(
        &store,
        &account,
        1,
        "maya@example.com",
        now,
        MessageDirection::Inbound,
    )
    .await;
    assert_eq!(first_seen(&store, &new).await, Some(100));
}

#[tokio::test]
async fn counts_sum_and_each_list_matches_its_count() {
    let (store, account) = store_with_account().await;
    let now = Utc::now();
    let mut ids = Vec::new();
    for n in 0..7 {
        ids.push(
            arrive(
                &store,
                &account,
                n,
                &format!("s{n}@example.com"),
                now,
                MessageDirection::Inbound,
            )
            .await,
        );
    }
    let modes = [
        "messages", "messages", "updates", "reading", "reading", "spam",
    ];
    let placements: Vec<_> = ids
        .iter()
        .zip(modes)
        .map(|(id, mode)| placement(id, mode))
        .collect();
    store
        .set_arrival_placements(&placements, now)
        .await
        .unwrap();
    store
        .set_arrival_placements(&[placement(&ids[0], "reading")], now)
        .await
        .unwrap();

    let since = now - Duration::hours(1);
    let until = now + Duration::hours(1);
    let counts = store
        .arrival_counts(std::slice::from_ref(&account), since, until)
        .await
        .unwrap();
    assert_eq!(counts.total, 7);
    assert_eq!(counts.by_mode.values().sum::<u32>(), counts.total);
    assert_eq!(counts.by_mode["messages"], 2);
    assert_eq!(counts.by_mode["sorting"], 1);
    assert_eq!(counts.by_mode["spam"], 1);

    for (bucket, count) in &counts.by_mode {
        let (rows, total) = store
            .list_arrivals(
                std::slice::from_ref(&account),
                since,
                until,
                Some(bucket),
                100,
            )
            .await
            .unwrap();
        assert_eq!(total, *count, "{bucket}");
        assert_eq!(rows.len() as u32, *count, "{bucket}");
        assert!(rows.iter().all(|row| &row.effective == bucket));
    }
    let (all, total) = store
        .list_arrivals(std::slice::from_ref(&account), since, until, None, 100)
        .await
        .unwrap();
    assert_eq!((all.len(), total), (7, 7));

    let later = store
        .arrival_counts(&[account], until, until + Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(later.total, 0);
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
        prior_decided_at: None,
        aspect_id: None,
    };
    assert!(!store
        .has_moves(std::slice::from_ref(&account))
        .await
        .unwrap());
    store
        .insert_correction(&correction("updates", "updates"))
        .await
        .unwrap();
    assert!(!store
        .has_moves(std::slice::from_ref(&account))
        .await
        .unwrap());
    let id = store
        .insert_correction(&correction("updates", "messages"))
        .await
        .unwrap();
    assert_eq!(
        store
            .count_moves_since(std::slice::from_ref(&account), now - Duration::days(7))
            .await
            .unwrap(),
        1
    );
    let stored = store.get_correction(id).await.unwrap().unwrap();
    assert_eq!(stored.fields.sender_email, "maya@example.com");

    assert!(store.mark_correction_undone(id, now).await.unwrap());
    assert!(
        !store.mark_correction_undone(id, now).await.unwrap(),
        "a second undo is a no-op"
    );
    assert_eq!(
        store
            .count_moves_since(std::slice::from_ref(&account), now - Duration::days(7))
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        store.list_corrections(&[account], 10).await.unwrap().len(),
        2
    );
}

#[tokio::test]
async fn deleting_a_message_takes_its_arrival_and_keeps_the_correction_log() {
    let (store, account) = store_with_account().await;
    let now = Utc::now();
    let id = arrive(
        &store,
        &account,
        1,
        "maya@example.com",
        now,
        MessageDirection::Inbound,
    )
    .await;
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
            prior_decided_at: None,
            aspect_id: None,
        })
        .await
        .unwrap();
    store
        .delete_messages_and_derived(&account, &["p-1".to_string()])
        .await
        .unwrap();
    assert_eq!(first_seen(&store, &id).await, None);
    assert_eq!(
        store.list_corrections(&[account], 10).await.unwrap().len(),
        1
    );
}
