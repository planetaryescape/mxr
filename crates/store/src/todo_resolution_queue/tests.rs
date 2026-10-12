use super::*;
use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
use chrono::Duration;
use mxr_core::types::{MessageBody, MessageDirection, MessageMetadata};

async fn seed(store: &Store) -> (AccountId, MessageId, MessageId) {
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let now = Utc::now();
    let mut source = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    source.date = now - Duration::days(2);
    let mut later = source.clone();
    later.id = MessageId::new();
    later.provider_id = "later".into();
    later.date = now - Duration::days(1);
    for envelope in [&source, &later] {
        store
            .upsert_envelope_with_direction(envelope, MessageDirection::Inbound)
            .await
            .unwrap();
        store
            .insert_body(&MessageBody {
                message_id: envelope.id.clone(),
                text_plain: Some("Task update".into()),
                text_html: None,
                attachments: vec![],
                fetched_at: now,
                metadata: MessageMetadata::default(),
            })
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO todos (id, account_id, thread_id, source_message_id, source_date,
        kind, verb, title, origin, reason, rules_version, dedup_key, created_at, updated_at)
        VALUES ('task', ?1, ?2, ?3, ?4, 'bill', 'pay', 'Pay council tax', 'manual', 'test', 1, 'task', ?4, ?4)")
        .bind(account.id.as_str()).bind(source.thread_id.as_str()).bind(source.id.as_str())
        .bind(source.date.timestamp()).execute(store.writer()).await.unwrap();
    (account.id, source.id, later.id)
}

async fn enqueue(store: &Store, later: &MessageId, now: DateTime<Utc>) -> TodoResolutionWork {
    let input = store
        .todo_resolution_input("task", later)
        .await
        .unwrap()
        .unwrap();
    store.enqueue_todo_resolution(&input, now).await.unwrap();
    store
        .list_due_todo_resolutions(now, 10)
        .await
        .unwrap()
        .pop()
        .unwrap()
}

#[tokio::test]
async fn reopen_retains_reservation_backoff_and_completed_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("queue.db");
    let now = Utc::now();
    let (later, revision) = {
        let store = Store::new(&path).await.unwrap();
        let (_, _, later) = seed(&store).await;
        let work = enqueue(&store, &later, now).await;
        assert!(store
            .reserve_todo_resolution_attempt(&work, now)
            .await
            .unwrap());
        assert!(store
            .list_due_todo_resolutions(now, 10)
            .await
            .unwrap()
            .is_empty());
        (later, work.revision)
    };
    let store = Store::new(&path).await.unwrap();
    let input = store
        .todo_resolution_input("task", &later)
        .await
        .unwrap()
        .unwrap();
    store.enqueue_todo_resolution(&input, now).await.unwrap();
    assert_eq!(
        store.todo_resolution_queue_counts(now).await.unwrap(),
        (1, 0)
    );
    let work = store
        .list_due_todo_resolutions(now + Duration::hours(1), 10)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(work.revision, revision);
    assert_eq!(work.attempts, 1);
    store.finish_todo_resolution_work(&work).await.unwrap();
    store
        .enqueue_todo_resolution(&input, now + Duration::hours(2))
        .await
        .unwrap();
    assert_eq!(
        store
            .todo_resolution_queue_counts(now + Duration::hours(2))
            .await
            .unwrap(),
        (0, 0)
    );
}

#[tokio::test]
async fn changed_input_survives_old_finish_even_when_contents_return_to_original() {
    let store = Store::in_memory().await.unwrap();
    let (_, _, later) = seed(&store).await;
    let now = Utc::now();
    let first = enqueue(&store, &later, now).await;
    assert!(store
        .reserve_todo_resolution_attempt(&first, now)
        .await
        .unwrap());
    assert!(!store
        .reserve_todo_resolution_attempt(&first, now)
        .await
        .unwrap());
    let original = store
        .todo_resolution_input("task", &later)
        .await
        .unwrap()
        .unwrap();
    store.enqueue_todo_resolution(&original, now).await.unwrap();
    assert_eq!(
        store.todo_resolution_queue_counts(now).await.unwrap(),
        (1, 0)
    );
    sqlx::query("UPDATE bodies SET text_plain = 'Changed' WHERE message_id = ?")
        .bind(later.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    let changed = enqueue(&store, &later, now).await;
    assert_ne!(first.revision, changed.revision);
    sqlx::query("UPDATE bodies SET text_plain = 'Task update' WHERE message_id = ?")
        .bind(later.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    let returned = enqueue(&store, &later, now).await;
    assert_eq!(first.input_fingerprint, returned.input_fingerprint);
    assert_ne!(first.revision, returned.revision);
    store.finish_todo_resolution_work(&first).await.unwrap();
    store
        .defer_unattempted_todo_resolution(&first, now)
        .await
        .unwrap();
    let due = store.list_due_todo_resolutions(now, 10).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].revision, returned.revision);
    assert_eq!(due[0].attempts, 0);
}

#[tokio::test]
async fn retries_stop_after_five_without_sync_replay_resetting_them() {
    let store = Store::in_memory().await.unwrap();
    let (_, _, later) = seed(&store).await;
    let mut now = Utc::now();
    enqueue(&store, &later, now).await;
    for (attempt, hours) in [(0, 1), (1, 4), (2, 16), (3, 24), (4, 24)] {
        let work = store
            .list_due_todo_resolutions(now, 10)
            .await
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(work.attempts, attempt);
        assert!(store
            .reserve_todo_resolution_attempt(&work, now)
            .await
            .unwrap());
        let input = store
            .todo_resolution_input("task", &later)
            .await
            .unwrap()
            .unwrap();
        store.enqueue_todo_resolution(&input, now).await.unwrap();
        assert!(store
            .list_due_todo_resolutions(now + Duration::hours(hours) - Duration::seconds(1), 10)
            .await
            .unwrap()
            .is_empty());
        now += Duration::hours(hours);
    }
    assert!(store
        .list_due_todo_resolutions(now + Duration::days(10), 10)
        .await
        .unwrap()
        .is_empty());
    let attempts: i64 = sqlx::query_scalar("SELECT attempts FROM todo_resolution_queue")
        .fetch_one(store.reader())
        .await
        .unwrap();
    assert_eq!(attempts, 5);
    sqlx::query("UPDATE todos SET title = 'Pay new council tax' WHERE id = 'task'")
        .execute(store.writer())
        .await
        .unwrap();
    assert_eq!(enqueue(&store, &later, now).await.attempts, 0);
}

#[tokio::test]
async fn offer_gate_rechecks_both_bodies_account_flags_and_revision() {
    let store = Store::in_memory().await.unwrap();
    let (account, original, later) = seed(&store).await;
    let now = Utc::now();
    let work = enqueue(&store, &later, now).await;
    let mut connection = store.reader().acquire().await.unwrap();
    assert!(input_is_current(&mut connection, &work).await.unwrap());
    drop(connection);
    for message in [&original, &later] {
        sqlx::query("UPDATE bodies SET text_plain = 'Edited' WHERE message_id = ?")
            .bind(message.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        let mut connection = store.reader().acquire().await.unwrap();
        assert!(!input_is_current(&mut connection, &work).await.unwrap());
        drop(connection);
        sqlx::query("UPDATE bodies SET text_plain = 'Task update' WHERE message_id = ?")
            .bind(message.as_str())
            .execute(store.writer())
            .await
            .unwrap();
    }
    sqlx::query("UPDATE accounts SET enabled = 0 WHERE id = ?")
        .bind(account.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    assert_eq!(
        store.todo_resolution_queue_counts(now).await.unwrap(),
        (0, 0)
    );
    assert!(store
        .todo_resolution_input("task", &later)
        .await
        .unwrap()
        .is_none());
    sqlx::query("UPDATE accounts SET enabled = 1 WHERE id = ?")
        .bind(account.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    sqlx::query("UPDATE messages SET flags = ? WHERE id = ?")
        .bind(i64::from(MessageFlags::TRASH.bits()))
        .bind(later.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    assert!(store
        .todo_resolution_input("task", &later)
        .await
        .unwrap()
        .is_none());
    sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(later.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM todo_resolution_queue")
        .fetch_one(store.reader())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn disabled_account_rows_do_not_fill_the_workers_bounded_page() {
    let store = Store::in_memory().await.unwrap();
    let (disabled, _, later) = seed(&store).await;
    sqlx::query("UPDATE todos SET id = 'disabled-task' WHERE id = 'task'")
        .execute(store.writer())
        .await
        .unwrap();
    let now = Utc::now();
    let input = store
        .todo_resolution_input("disabled-task", &later)
        .await
        .unwrap()
        .unwrap();
    store.enqueue_todo_resolution(&input, now).await.unwrap();
    store.set_account_enabled(&disabled, false).await.unwrap();
    let (enabled, _, later) = seed(&store).await;
    let work = enqueue(&store, &later, now).await;
    assert_eq!(work.account_id, enabled);
    let page = store.list_due_todo_resolutions(now, 1).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].account_id, enabled);
    store.set_account_enabled(&disabled, true).await.unwrap();
    assert_eq!(
        store.todo_resolution_queue_counts(now).await.unwrap(),
        (2, 2)
    );
}

#[tokio::test]
async fn finish_refreshes_edited_input_without_a_rescan_and_respects_disabled_account() {
    let store = Store::in_memory().await.unwrap();
    let (account, source, later) = seed(&store).await;
    let now = Utc::now();
    let first = enqueue(&store, &later, now).await;
    assert!(store
        .reserve_todo_resolution_attempt(&first, now)
        .await
        .unwrap());
    sqlx::query("UPDATE bodies SET text_plain = 'Late source update' WHERE message_id = ?")
        .bind(source.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    store.finish_todo_resolution_work(&first).await.unwrap();
    let refreshed = store
        .list_due_todo_resolutions(Utc::now(), 10)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_ne!(refreshed.revision, first.revision);
    assert_eq!(refreshed.attempts, 0);
    let input = store
        .todo_resolution_input("task", &later)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refreshed.input_fingerprint, input.fingerprint);
    store.set_account_enabled(&account, false).await.unwrap();
    store.finish_todo_resolution_work(&refreshed).await.unwrap();
    store.set_account_enabled(&account, true).await.unwrap();
    assert_eq!(
        store
            .todo_resolution_queue_counts(Utc::now())
            .await
            .unwrap(),
        (1, 1)
    );
    // Success-with-none and deterministic paths share this final CAS.
    store.finish_todo_resolution_work(&refreshed).await.unwrap();
    assert_eq!(
        store
            .todo_resolution_queue_counts(Utc::now())
            .await
            .unwrap(),
        (0, 0)
    );
}
