mod account;
mod analytics;
mod body;
mod contacts;
mod diagnostics;
mod draft;
mod event_log;
mod label;
mod message;
mod message_events;
mod pool;
mod reply_pairs;
mod rules;
mod search;
mod semantic;
mod snooze;
mod sync_cursor;
mod sync_log;
mod sync_runtime_status;
#[cfg(test)]
mod test_fixtures;
mod thread;

pub use diagnostics::StoreRecordCounts;
pub use event_log::{EventLogEntry, EventLogRefs};
pub use pool::Store;
pub use rules::{row_to_rule_json, row_to_rule_log_json, RuleLogInput, RuleRecordInput};
pub use sync_log::{SyncLogEntry, SyncStatus};
pub use sync_runtime_status::{SyncRuntimeStatus, SyncRuntimeStatusUpdate};

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::error::Error as StdError;
use std::str::FromStr;
use std::time::Instant;

pub(crate) fn encode_json<T: Serialize>(value: &T) -> Result<String, sqlx::Error> {
    serde_json::to_string(value).map_err(|error| sqlx::Error::Encode(Box::new(error)))
}

pub(crate) fn decode_json<T: DeserializeOwned>(value: &str) -> Result<T, sqlx::Error> {
    serde_json::from_str(value).map_err(sqlx::Error::decode)
}

pub(crate) fn decode_id<T>(value: &str) -> Result<T, sqlx::Error>
where
    T: FromStr,
    T::Err: StdError + Send + Sync + 'static,
{
    value.parse().map_err(sqlx::Error::decode)
}

pub(crate) fn decode_timestamp(value: i64) -> Result<DateTime<Utc>, sqlx::Error> {
    DateTime::from_timestamp(value, 0)
        .ok_or_else(|| sqlx::Error::Protocol(format!("invalid unix timestamp: {value}")))
}

pub(crate) fn decode_optional_timestamp(
    value: Option<i64>,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    value.map(decode_timestamp).transpose()
}

pub(crate) fn trace_query(operation: &'static str, started_at: Instant, row_count: usize) {
    tracing::trace!(
        operation,
        row_count,
        elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
        "store query"
    );
}

pub(crate) fn trace_lookup(operation: &'static str, started_at: Instant, found: bool) {
    tracing::trace!(
        operation,
        found,
        elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
        "store lookup"
    );
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::test_fixtures::*;
    use chrono::TimeZone;
    use mxr_core::*;

    fn test_envelope(account_id: &AccountId) -> Envelope {
        TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .flags(MessageFlags::READ | MessageFlags::STARRED)
            .build()
    }

    #[tokio::test]
    async fn account_roundtrip() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let fetched = store.get_account(&account.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, account.name);
        assert_eq!(fetched.email, account.email);
    }

    #[tokio::test]
    async fn account_insert_upserts_existing_runtime_record() {
        let store = Store::in_memory().await.unwrap();
        let mut account = test_account();
        store.insert_account(&account).await.unwrap();

        account.name = "Updated".to_string();
        account.email = "updated@example.com".to_string();
        store.insert_account(&account).await.unwrap();

        let fetched = store.get_account(&account.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "Updated");
        assert_eq!(fetched.email, "updated@example.com");
    }

    #[tokio::test]
    async fn disabled_account_is_hidden_from_enabled_account_list_but_keeps_row() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        store.set_account_enabled(&account.id, false).await.unwrap();

        assert!(store.list_accounts().await.unwrap().is_empty());
        let fetched = store.get_account(&account.id).await.unwrap().unwrap();
        assert!(!fetched.enabled);
    }

    #[tokio::test]
    async fn delete_account_cascades_owned_messages() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let env = test_envelope(&account.id);
        store.upsert_envelope(&env).await.unwrap();

        let deleted = store.delete_account(&account.id).await.unwrap();

        assert_eq!(deleted, 1);
        assert!(store.get_account(&account.id).await.unwrap().is_none());
        assert!(store.get_envelope(&env.id).await.unwrap().is_none());
        assert!(store
            .list_message_ids_by_account(&account.id)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn envelope_upsert_and_query() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let env = test_envelope(&account.id);
        store.upsert_envelope(&env).await.unwrap();

        let fetched = store.get_envelope(&env.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, env.id);
        assert_eq!(fetched.subject, env.subject);
        assert_eq!(fetched.from.email, env.from.email);
        assert_eq!(fetched.flags, env.flags);

        let list = store
            .list_envelopes_by_account(&account.id, 100, 0)
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn count_messages_grouped_by_account_batches_totals() {
        let store = Store::in_memory().await.unwrap();
        let first = test_account();
        let mut second = test_account();
        second.id = AccountId::new();
        second.name = "Second".into();
        second.email = "second@example.com".into();
        store.insert_account(&first).await.unwrap();
        store.insert_account(&second).await.unwrap();

        let first_message = test_envelope(&first.id);
        let mut second_message = test_envelope(&second.id);
        second_message.id = MessageId::new();
        second_message.provider_id = "second-provider".into();
        let mut third_message = test_envelope(&second.id);
        third_message.id = MessageId::new();
        third_message.provider_id = "third-provider".into();
        store.upsert_envelope(&first_message).await.unwrap();
        store.upsert_envelope(&second_message).await.unwrap();
        store.upsert_envelope(&third_message).await.unwrap();

        let counts = store.count_messages_grouped_by_account().await.unwrap();

        assert_eq!(counts.get(&first.id), Some(&1));
        assert_eq!(counts.get(&second.id), Some(&2));
    }

    #[tokio::test]
    async fn list_sync_cursors_returns_all_persisted_cursors() {
        let store = Store::in_memory().await.unwrap();
        let first = test_account();
        let mut second = test_account();
        second.id = AccountId::new();
        second.name = "Second".into();
        second.email = "second@example.com".into();
        store.insert_account(&first).await.unwrap();
        store.insert_account(&second).await.unwrap();

        let first_cursor = SyncCursor::Gmail { history_id: 42 };
        let second_cursor = SyncCursor::GmailBackfill {
            history_id: 77,
            page_token: "page-2".into(),
        };
        store
            .set_sync_cursor(&first.id, &first_cursor)
            .await
            .unwrap();
        store
            .set_sync_cursor(&second.id, &second_cursor)
            .await
            .unwrap();

        let cursors = store.list_sync_cursors().await.unwrap();

        assert!(matches!(
            cursors.get(&first.id),
            Some(SyncCursor::Gmail { history_id: 42 })
        ));
        assert!(matches!(
            cursors.get(&second.id),
            Some(SyncCursor::GmailBackfill {
                history_id: 77,
                page_token
            }) if page_token == "page-2"
        ));
    }

    #[tokio::test]
    async fn list_envelopes_by_ids_roundtrip() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let mut first = test_envelope(&account.id);
        first.provider_id = "first".to_string();
        first.subject = "First".to_string();

        let mut second = test_envelope(&account.id);
        second.id = MessageId::new();
        second.provider_id = "second".to_string();
        second.subject = "Second".to_string();

        store.upsert_envelope(&first).await.unwrap();
        store.upsert_envelope(&second).await.unwrap();

        let listed = store
            .list_envelopes_by_ids(&[second.id.clone(), first.id.clone()])
            .await
            .unwrap();

        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, second.id);
        assert_eq!(listed[0].subject, "Second");
        assert_eq!(listed[1].id, first.id);
        assert_eq!(listed[1].subject, "First");
    }

    #[tokio::test]
    async fn list_envelopes_by_ids_roundtrip_on_disk() {
        let temp_dir = std::env::temp_dir().join(format!(
            "mxr-store-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("mxr.db");
        let store = Store::new(&db_path).await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let mut first = test_envelope(&account.id);
        first.provider_id = "first".to_string();
        first.subject = "First".to_string();

        let mut second = test_envelope(&account.id);
        second.id = MessageId::new();
        second.provider_id = "second".to_string();
        second.subject = "Second".to_string();

        store.upsert_envelope(&first).await.unwrap();
        store.upsert_envelope(&second).await.unwrap();

        let listed = store
            .list_envelopes_by_ids(&[second.id.clone(), first.id.clone()])
            .await
            .unwrap();

        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, second.id);
        assert_eq!(listed[0].subject, "Second");
        assert_eq!(listed[1].id, first.id);
        assert_eq!(listed[1].subject, "First");

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn list_envelopes_by_account_sinks_impossible_future_dates() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let mut poisoned = test_envelope(&account.id);
        poisoned.provider_id = "poisoned-future".to_string();
        poisoned.subject = "Poisoned future".to_string();
        poisoned.date = chrono::Utc
            .timestamp_opt(236_816_444_325, 0)
            .single()
            .unwrap();

        let mut recent = test_envelope(&account.id);
        recent.id = MessageId::new();
        recent.provider_id = "real-recent".to_string();
        recent.subject = "Real recent".to_string();
        recent.date = chrono::Utc::now();

        store.upsert_envelope(&poisoned).await.unwrap();
        store.upsert_envelope(&recent).await.unwrap();

        let listed = store
            .list_envelopes_by_account(&account.id, 100, 0)
            .await
            .unwrap();

        assert_eq!(listed[0].subject, "Real recent");
        assert_eq!(listed[1].subject, "Poisoned future");
    }

    #[tokio::test]
    async fn label_crud() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let label = Label {
            id: LabelId::new(),
            account_id: account.id.clone(),
            name: "Inbox".to_string(),
            kind: LabelKind::System,
            color: None,
            provider_id: "INBOX".to_string(),
            unread_count: 5,
            total_count: 20,
        };
        store.upsert_label(&label).await.unwrap();

        let labels = store.list_labels_by_account(&account.id).await.unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].name, "Inbox");
        assert_eq!(labels[0].unread_count, 5);

        store.update_label_counts(&label.id, 3, 25).await.unwrap();
        let labels = store.list_labels_by_account(&account.id).await.unwrap();
        assert_eq!(labels[0].unread_count, 3);
        assert_eq!(labels[0].total_count, 25);
    }

    #[tokio::test]
    async fn body_cache() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let env = test_envelope(&account.id);
        store.upsert_envelope(&env).await.unwrap();

        let body = MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Hello world".to_string()),
            text_html: Some("<p>Hello world</p>".to_string()),
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
                filename: "report.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                disposition: AttachmentDisposition::Inline,
                content_id: Some("report@example.com".to_string()),
                content_location: Some("https://example.com/report.pdf".to_string()),
                size_bytes: 50000,
                local_path: None,
                provider_id: "att-1".to_string(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                text_plain_format: Some(TextPlainFormat::Flowed { delsp: true }),
                text_plain_source: Some(BodyPartSource::Exact),
                text_html_source: Some(BodyPartSource::Exact),
                ..MessageMetadata::default()
            },
        };
        store.insert_body(&body).await.unwrap();

        let fetched = store.get_body(&env.id).await.unwrap().unwrap();
        assert_eq!(fetched.text_plain, body.text_plain);
        assert_eq!(fetched.text_html, body.text_html);
        assert_eq!(
            fetched.metadata.text_plain_format,
            body.metadata.text_plain_format
        );
        assert_eq!(
            fetched.metadata.text_plain_source,
            body.metadata.text_plain_source
        );
        assert_eq!(
            fetched.metadata.text_html_source,
            body.metadata.text_html_source
        );
        assert_eq!(fetched.attachments.len(), 1);
        assert_eq!(fetched.attachments[0].filename, "report.pdf");
        assert_eq!(
            fetched.attachments[0].disposition,
            AttachmentDisposition::Inline
        );
        assert_eq!(
            fetched.attachments[0].content_id.as_deref(),
            Some("report@example.com")
        );
        assert_eq!(
            fetched.attachments[0].content_location.as_deref(),
            Some("https://example.com/report.pdf")
        );
    }

    #[tokio::test]
    async fn sync_runtime_status_roundtrip() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let now = chrono::Utc::now();
        store
            .upsert_sync_runtime_status(
                &account.id,
                &SyncRuntimeStatusUpdate {
                    last_attempt_at: Some(now),
                    last_success_at: Some(now),
                    sync_in_progress: Some(true),
                    current_cursor_summary: Some(Some("gmail history_id=42".to_string())),
                    last_synced_count: Some(42),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let fetched = store
            .get_sync_runtime_status(&account.id)
            .await
            .unwrap()
            .expect("runtime status");
        assert_eq!(fetched.account_id, account.id);
        assert_eq!(fetched.last_synced_count, 42);
        assert!(fetched.sync_in_progress);
        assert_eq!(
            fetched.current_cursor_summary.as_deref(),
            Some("gmail history_id=42")
        );

        let listed = store.list_sync_runtime_statuses().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].account_id, account.id);
    }

    #[tokio::test]
    async fn message_labels_junction() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let label = Label {
            id: LabelId::new(),
            account_id: account.id.clone(),
            name: "Inbox".to_string(),
            kind: LabelKind::System,
            color: None,
            provider_id: "INBOX".to_string(),
            unread_count: 0,
            total_count: 0,
        };
        store.upsert_label(&label).await.unwrap();

        let env = test_envelope(&account.id);
        store.upsert_envelope(&env).await.unwrap();
        store
            .set_message_labels(&env.id, std::slice::from_ref(&label.id), EventSource::User)
            .await
            .unwrap();

        let by_label = store
            .list_envelopes_by_label(&label.id, 100, 0)
            .await
            .unwrap();
        assert_eq!(by_label.len(), 1);
        assert_eq!(by_label[0].id, env.id);
        assert_eq!(by_label[0].label_provider_ids, vec!["INBOX".to_string()]);

        let by_account = store
            .list_envelopes_by_account(&account.id, 100, 0)
            .await
            .unwrap();
        assert_eq!(by_account.len(), 1);
        assert_eq!(by_account[0].label_provider_ids, vec!["INBOX".to_string()]);

        let fetched = store.get_envelope(&env.id).await.unwrap().unwrap();
        assert_eq!(fetched.label_provider_ids, vec!["INBOX".to_string()]);
    }

    #[tokio::test]
    async fn thread_aggregation() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let thread_id = ThreadId::new();
        for i in 0..3 {
            let mut env = test_envelope(&account.id);
            env.provider_id = format!("fake-thread-{}", i);
            env.thread_id = thread_id.clone();
            env.date = chrono::Utc::now() - chrono::Duration::hours(i);
            if i == 0 {
                env.flags = MessageFlags::empty(); // unread
            }
            store.upsert_envelope(&env).await.unwrap();
        }

        let thread = store.get_thread(&thread_id).await.unwrap().unwrap();
        assert_eq!(thread.message_count, 3);
        assert_eq!(thread.unread_count, 1);
    }

    #[tokio::test]
    async fn thread_latest_date_ignores_impossible_future_dates() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let thread_id = ThreadId::new();

        let mut poisoned = test_envelope(&account.id);
        poisoned.provider_id = "poisoned-thread".to_string();
        poisoned.thread_id = thread_id.clone();
        poisoned.date = chrono::Utc
            .timestamp_opt(236_816_444_325, 0)
            .single()
            .unwrap();

        let mut recent = test_envelope(&account.id);
        recent.id = MessageId::new();
        recent.provider_id = "recent-thread".to_string();
        recent.thread_id = thread_id.clone();
        recent.subject = "Recent thread message".to_string();
        recent.date = chrono::Utc::now();

        store.upsert_envelope(&poisoned).await.unwrap();
        store.upsert_envelope(&recent).await.unwrap();

        let thread = store.get_thread(&thread_id).await.unwrap().unwrap();
        assert_eq!(thread.latest_date.timestamp(), recent.date.timestamp());
    }

    #[tokio::test]
    async fn list_subscriptions_groups_by_sender_and_skips_trash() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let mut latest = test_envelope(&account.id);
        latest.from.name = Some("Readwise".into());
        latest.from.email = "hello@readwise.io".into();
        latest.subject = "Latest digest".into();
        latest.unsubscribe = UnsubscribeMethod::HttpLink {
            url: "https://example.com/unsub".into(),
        };
        latest.date = chrono::Utc::now();

        let mut older = latest.clone();
        older.id = MessageId::new();
        older.provider_id = "fake-older".into();
        older.subject = "Older digest".into();
        older.date = latest.date - chrono::Duration::days(3);

        let mut trashed = latest.clone();
        trashed.id = MessageId::new();
        trashed.provider_id = "fake-trash".into();
        trashed.subject = "Trashed digest".into();
        trashed.date = latest.date + chrono::Duration::hours(1);
        trashed.flags.insert(MessageFlags::TRASH);

        let mut no_unsub = test_envelope(&account.id);
        no_unsub.from.email = "plain@example.com".into();
        no_unsub.provider_id = "fake-none".into();
        no_unsub.unsubscribe = UnsubscribeMethod::None;

        store.upsert_envelope(&older).await.unwrap();
        store.upsert_envelope(&latest).await.unwrap();
        store.upsert_envelope(&trashed).await.unwrap();
        store.upsert_envelope(&no_unsub).await.unwrap();

        let subscriptions = store.list_subscriptions(None, 10).await.unwrap();
        assert_eq!(subscriptions.len(), 1);
        assert_eq!(subscriptions[0].sender_email, "hello@readwise.io");
        assert_eq!(subscriptions[0].message_count, 2);
        assert_eq!(subscriptions[0].latest_subject, "Latest digest");
    }

    #[tokio::test]
    async fn draft_crud() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let draft = Draft {
            id: DraftId::new(),
            account_id: account.id.clone(),
            reply_headers: None,
            to: vec![Address {
                name: None,
                email: "bob@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Draft subject".to_string(),
            body_markdown: "# Hello".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        store.insert_draft(&draft).await.unwrap();

        let drafts = store.list_drafts(&account.id).await.unwrap();
        assert_eq!(drafts.len(), 1);

        store.delete_draft(&draft.id).await.unwrap();
        let drafts = store.list_drafts(&account.id).await.unwrap();
        assert_eq!(drafts.len(), 0);
    }

    #[tokio::test]
    async fn snooze_lifecycle() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let env = test_envelope(&account.id);
        store.upsert_envelope(&env).await.unwrap();

        let snoozed = Snoozed {
            message_id: env.id.clone(),
            account_id: account.id.clone(),
            snoozed_at: chrono::Utc::now(),
            wake_at: chrono::Utc::now() - chrono::Duration::hours(1), // already due
            original_labels: vec![],
        };
        store.insert_snooze(&snoozed).await.unwrap();

        let due = store.get_due_snoozes(chrono::Utc::now()).await.unwrap();
        assert_eq!(due.len(), 1);

        store.remove_snooze(&env.id).await.unwrap();
        let due = store.get_due_snoozes(chrono::Utc::now()).await.unwrap();
        assert_eq!(due.len(), 0);
    }

    #[tokio::test]
    async fn sync_log_lifecycle() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let log_id = store
            .insert_sync_log(&account.id, &SyncStatus::Running)
            .await
            .unwrap();
        assert!(log_id > 0);

        store
            .complete_sync_log(log_id, &SyncStatus::Success, 55, None)
            .await
            .unwrap();

        let last = store.get_last_sync(&account.id).await.unwrap().unwrap();
        assert_eq!(last.status, SyncStatus::Success);
        assert_eq!(last.messages_synced, 55);
    }

    #[tokio::test]
    async fn event_log_insert_and_query() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let env = test_envelope(&account.id);
        let env_id = env.id.as_str();
        store.upsert_envelope(&env).await.unwrap();

        store
            .insert_event("info", "sync", "Sync completed", Some(&account.id), None)
            .await
            .unwrap();
        store
            .insert_event(
                "error",
                "sync",
                "Sync failed",
                Some(&account.id),
                Some("timeout"),
            )
            .await
            .unwrap();
        store
            .insert_event("info", "rule", "Rule applied", None, None)
            .await
            .unwrap();
        store
            .insert_event_refs(
                "info",
                "mutation",
                "Archived a message",
                EventLogRefs {
                    account_id: Some(&account.id),
                    message_id: Some(env_id.as_str()),
                    rule_id: None,
                },
                Some("from=test@example.com"),
            )
            .await
            .unwrap();

        let all = store.list_events(10, None, None).await.unwrap();
        assert_eq!(all.len(), 4);

        let errors = store.list_events(10, Some("error"), None).await.unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].summary, "Sync failed");

        let sync_events = store.list_events(10, None, Some("sync")).await.unwrap();
        assert_eq!(sync_events.len(), 2);

        let mutation_events = store.list_events(10, None, Some("mutation")).await.unwrap();
        assert_eq!(mutation_events.len(), 1);
        assert_eq!(
            mutation_events[0].message_id.as_deref(),
            Some(env_id.as_str())
        );

        let latest_sync = store
            .latest_event_timestamp("sync", Some("Sync"))
            .await
            .unwrap();
        assert!(latest_sync.is_some());
    }

    #[tokio::test]
    async fn prune_events_before_removes_old_rows() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        store
            .insert_event("info", "sync", "recent", Some(&account.id), None)
            .await
            .unwrap();
        sqlx::query("UPDATE event_log SET timestamp = ? WHERE summary = 'recent'")
            .bind(chrono::Utc::now().timestamp())
            .execute(store.writer())
            .await
            .unwrap();

        store
            .insert_event("info", "sync", "old", Some(&account.id), None)
            .await
            .unwrap();
        sqlx::query("UPDATE event_log SET timestamp = ? WHERE summary = 'old'")
            .bind((chrono::Utc::now() - chrono::Duration::days(120)).timestamp())
            .execute(store.writer())
            .await
            .unwrap();

        let removed = store
            .prune_events_before((chrono::Utc::now() - chrono::Duration::days(90)).timestamp())
            .await
            .unwrap();
        assert_eq!(removed, 1);

        let events = store.list_events(10, None, None).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "recent");
    }

    #[tokio::test]
    async fn get_message_id_by_provider_id() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let env = test_envelope(&account.id);
        store.upsert_envelope(&env).await.unwrap();

        let found = store
            .get_message_id_by_provider_id(&account.id, &env.provider_id)
            .await
            .unwrap();
        assert_eq!(found, Some(env.id.clone()));

        let not_found = store
            .get_message_id_by_provider_id(&account.id, "nonexistent")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn recalculate_label_counts() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let label = Label {
            id: LabelId::new(),
            account_id: account.id.clone(),
            name: "Inbox".to_string(),
            kind: LabelKind::System,
            color: None,
            provider_id: "INBOX".to_string(),
            unread_count: 0,
            total_count: 0,
        };
        store.upsert_label(&label).await.unwrap();

        // Insert 3 messages: 2 read, 1 unread
        for i in 0..3 {
            let mut env = test_envelope(&account.id);
            env.provider_id = format!("fake-label-{}", i);
            if i < 2 {
                env.flags = MessageFlags::READ;
            } else {
                env.flags = MessageFlags::empty();
            }
            store.upsert_envelope(&env).await.unwrap();
            store
                .set_message_labels(&env.id, std::slice::from_ref(&label.id), EventSource::User)
                .await
                .unwrap();
        }

        store.recalculate_label_counts(&account.id).await.unwrap();

        let labels = store.list_labels_by_account(&account.id).await.unwrap();
        assert_eq!(labels[0].total_count, 3);
        assert_eq!(labels[0].unread_count, 1);
    }

    #[tokio::test]
    async fn replace_label_moves_message_associations_when_id_changes() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let original = Label {
            id: LabelId::from_provider_id("imap", "Projects"),
            account_id: account.id.clone(),
            name: "Projects".to_string(),
            kind: LabelKind::Folder,
            color: None,
            provider_id: "Projects".to_string(),
            unread_count: 0,
            total_count: 0,
        };
        store.upsert_label(&original).await.unwrap();

        let env = test_envelope(&account.id);
        store.upsert_envelope(&env).await.unwrap();
        store
            .set_message_labels(
                &env.id,
                std::slice::from_ref(&original.id),
                EventSource::User,
            )
            .await
            .unwrap();

        let renamed = Label {
            id: LabelId::from_provider_id("imap", "Client Work"),
            account_id: account.id.clone(),
            name: "Client Work".to_string(),
            kind: LabelKind::Folder,
            color: None,
            provider_id: "Client Work".to_string(),
            unread_count: 0,
            total_count: 0,
        };
        store.replace_label(&original.id, &renamed).await.unwrap();

        let labels = store.list_labels_by_account(&account.id).await.unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].name, "Client Work");
        assert_eq!(labels[0].id, renamed.id);

        let by_new_label = store
            .list_envelopes_by_label(&renamed.id, 100, 0)
            .await
            .unwrap();
        assert_eq!(by_new_label.len(), 1);
        assert_eq!(by_new_label[0].id, env.id);
        assert!(store
            .list_envelopes_by_label(&original.id, 100, 0)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn label_mutations_do_not_leak_between_accounts() {
        // v1 ship-gate: confirm a junction-table mutation on account A's
        // message doesn't accidentally touch account B's labels. The data
        // model is isolated by message_id (which is account-scoped via
        // UUIDv5), but a regression here would be silent and subtle.
        let store = Store::in_memory().await.unwrap();
        let account_a = test_account();
        store.insert_account(&account_a).await.unwrap();
        let mut account_b = test_account();
        account_b.id = AccountId::new();
        account_b.email = "b@example.com".to_string();
        account_b.name = "B".to_string();
        store.insert_account(&account_b).await.unwrap();

        let label_a = Label {
            id: LabelId::from_provider_id("imap", &format!("A-{}", account_a.id.as_str())),
            account_id: account_a.id.clone(),
            name: "WorkA".to_string(),
            kind: LabelKind::User,
            color: None,
            provider_id: "WorkA".to_string(),
            unread_count: 0,
            total_count: 0,
        };
        let label_b = Label {
            id: LabelId::from_provider_id("imap", &format!("B-{}", account_b.id.as_str())),
            account_id: account_b.id.clone(),
            name: "WorkB".to_string(),
            kind: LabelKind::User,
            color: None,
            provider_id: "WorkB".to_string(),
            unread_count: 0,
            total_count: 0,
        };
        store.upsert_label(&label_a).await.unwrap();
        store.upsert_label(&label_b).await.unwrap();

        let env_a = test_envelope(&account_a.id);
        store.upsert_envelope(&env_a).await.unwrap();
        let mut env_b = test_envelope(&account_b.id);
        env_b.id = mxr_core::MessageId::from_scoped_provider_id(&account_b.id, "imap", "msg-b");
        env_b.provider_id = "msg-b".to_string();
        store.upsert_envelope(&env_b).await.unwrap();

        store
            .set_message_labels(
                &env_a.id,
                std::slice::from_ref(&label_a.id),
                EventSource::User,
            )
            .await
            .unwrap();
        store
            .set_message_labels(
                &env_b.id,
                std::slice::from_ref(&label_b.id),
                EventSource::User,
            )
            .await
            .unwrap();

        let by_a = store
            .list_envelopes_by_label(&label_a.id, 100, 0)
            .await
            .unwrap();
        assert_eq!(by_a.len(), 1);
        assert_eq!(by_a[0].account_id, account_a.id);

        let by_b = store
            .list_envelopes_by_label(&label_b.id, 100, 0)
            .await
            .unwrap();
        assert_eq!(by_b.len(), 1);
        assert_eq!(by_b[0].account_id, account_b.id);

        // Removing all labels from account A's message must not affect
        // account B's labels or messages.
        store
            .set_message_labels(&env_a.id, &[], EventSource::User)
            .await
            .unwrap();
        let by_a_after = store
            .list_envelopes_by_label(&label_a.id, 100, 0)
            .await
            .unwrap();
        assert!(by_a_after.is_empty());
        let by_b_after = store
            .list_envelopes_by_label(&label_b.id, 100, 0)
            .await
            .unwrap();
        assert_eq!(by_b_after.len(), 1, "account B unaffected");
    }

    #[tokio::test]
    async fn set_message_labels_is_atomic_under_constraint_violation() {
        // Regression: a mid-loop INSERT failure used to delete every existing
        // junction row before bailing, leaving the message with zero labels and
        // permanent corruption. The DELETE+INSERT must roll back as one tx.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let label_a = Label {
            id: LabelId::from_provider_id("imap", "A"),
            account_id: account.id.clone(),
            name: "A".to_string(),
            kind: LabelKind::User,
            color: None,
            provider_id: "A".to_string(),
            unread_count: 0,
            total_count: 0,
        };
        let label_b = Label {
            id: LabelId::from_provider_id("imap", "B"),
            account_id: account.id.clone(),
            name: "B".to_string(),
            kind: LabelKind::User,
            color: None,
            provider_id: "B".to_string(),
            unread_count: 0,
            total_count: 0,
        };
        store.upsert_label(&label_a).await.unwrap();
        store.upsert_label(&label_b).await.unwrap();

        let env = test_envelope(&account.id);
        store.upsert_envelope(&env).await.unwrap();
        store
            .set_message_labels(
                &env.id,
                &[label_a.id.clone(), label_b.id.clone()],
                EventSource::User,
            )
            .await
            .unwrap();

        // Duplicate label_id in the input list triggers PRIMARY KEY violation on
        // the second INSERT. Without the wrapping transaction, the prior DELETE
        // would have already wiped [A, B] and we'd be stuck with one of them
        // (or none). The transaction must roll back the DELETE.
        let result = store
            .set_message_labels(
                &env.id,
                &[label_a.id.clone(), label_a.id.clone()],
                EventSource::User,
            )
            .await;
        assert!(
            result.is_err(),
            "set_message_labels with duplicate label_ids should fail (got {:?})",
            result
        );

        let by_a = store
            .list_envelopes_by_label(&label_a.id, 100, 0)
            .await
            .unwrap();
        assert_eq!(by_a.len(), 1, "label A should still be associated");
        let by_b = store
            .list_envelopes_by_label(&label_b.id, 100, 0)
            .await
            .unwrap();
        assert_eq!(by_b.len(), 1, "label B must survive the failed mutation");
    }

    #[tokio::test]
    async fn rules_roundtrip_and_history() {
        let store = Store::in_memory().await.unwrap();
        let now = chrono::Utc::now();

        store
            .upsert_rule(crate::RuleRecordInput {
                id: "rule-1",
                name: "Archive newsletters",
                enabled: true,
                priority: 10,
                conditions_json: r#"{"type":"field","field":"has_label","label":"newsletters"}"#,
                actions_json: r#"[{"type":"archive"}]"#,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();

        let rules = store.list_rules().await.unwrap();
        assert_eq!(rules.len(), 1);
        let rule_json = crate::rules::row_to_rule_json(&rules[0]);
        assert_eq!(rule_json["name"], "Archive newsletters");
        assert_eq!(rule_json["priority"], 10);

        store
            .insert_rule_log(crate::RuleLogInput {
                rule_id: "rule-1",
                rule_name: "Archive newsletters",
                message_id: "msg-1",
                actions_applied_json: r#"["archive"]"#,
                timestamp: now,
                success: true,
                error: None,
            })
            .await
            .unwrap();

        let logs = store.list_rule_logs(Some("rule-1"), 10).await.unwrap();
        assert_eq!(logs.len(), 1);
        let log_json = crate::rules::row_to_rule_log_json(&logs[0]);
        assert_eq!(log_json["rule_name"], "Archive newsletters");
        assert_eq!(log_json["message_id"], "msg-1");
    }

    #[tokio::test]
    async fn get_saved_search_by_name() {
        let store = Store::in_memory().await.unwrap();

        let search = SavedSearch {
            id: SavedSearchId::new(),
            account_id: None,
            name: "Unread".to_string(),
            query: "is:unread".to_string(),
            search_mode: SearchMode::Lexical,
            sort: SortOrder::DateDesc,
            icon: None,
            position: 0,
            created_at: chrono::Utc::now(),
        };
        store.insert_saved_search(&search).await.unwrap();

        let found = store.get_saved_search_by_name("Unread").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().query, "is:unread");

        let not_found = store.get_saved_search_by_name("Nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn saved_search_crud() {
        let store = Store::in_memory().await.unwrap();

        let s1 = SavedSearch {
            id: SavedSearchId::new(),
            account_id: None,
            name: "Unread".to_string(),
            query: "is:unread".to_string(),
            search_mode: SearchMode::Lexical,
            sort: SortOrder::DateDesc,
            icon: None,
            position: 0,
            created_at: chrono::Utc::now(),
        };
        let s2 = SavedSearch {
            id: SavedSearchId::new(),
            account_id: None,
            name: "Starred".to_string(),
            query: "is:starred".to_string(),
            search_mode: SearchMode::Hybrid,
            sort: SortOrder::DateDesc,
            icon: Some("star".to_string()),
            position: 1,
            created_at: chrono::Utc::now(),
        };

        store.insert_saved_search(&s1).await.unwrap();
        store.insert_saved_search(&s2).await.unwrap();

        // List returns both, ordered by position
        let all = store.list_saved_searches().await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "Unread");
        assert_eq!(all[1].name, "Starred");

        // Get by name
        let found = store.get_saved_search_by_name("Starred").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().query, "is:starred");

        // Delete
        store.delete_saved_search(&s1.id).await.unwrap();
        let remaining = store.list_saved_searches().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "Starred");

        // Delete by name
        let deleted = store.delete_saved_search_by_name("Starred").await.unwrap();
        assert!(deleted);
        let empty = store.list_saved_searches().await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn list_contacts_ordered_by_frequency() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        // Insert 3 messages from alice, 2 from bob, 1 from carol
        for i in 0..3 {
            let mut env = test_envelope(&account.id);
            env.provider_id = format!("fake-alice-{}", i);
            env.from = Address {
                name: Some("Alice".to_string()),
                email: "alice@example.com".to_string(),
            };
            store.upsert_envelope(&env).await.unwrap();
        }
        for i in 0..2 {
            let mut env = test_envelope(&account.id);
            env.provider_id = format!("fake-bob-{}", i);
            env.from = Address {
                name: Some("Bob".to_string()),
                email: "bob@example.com".to_string(),
            };
            store.upsert_envelope(&env).await.unwrap();
        }
        {
            let mut env = test_envelope(&account.id);
            env.provider_id = "fake-carol-0".to_string();
            env.from = Address {
                name: Some("Carol".to_string()),
                email: "carol@example.com".to_string(),
            };
            store.upsert_envelope(&env).await.unwrap();
        }

        let contacts = store.list_contacts(10).await.unwrap();
        assert_eq!(contacts.len(), 3);
        // Ordered by frequency: alice (3), bob (2), carol (1)
        assert_eq!(contacts[0].1, "alice@example.com");
        assert_eq!(contacts[1].1, "bob@example.com");
        assert_eq!(contacts[2].1, "carol@example.com");
    }

    #[tokio::test]
    async fn sync_cursor_persistence() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let cursor = store.get_sync_cursor(&account.id).await.unwrap();
        assert!(cursor.is_none());

        let new_cursor = SyncCursor::Gmail { history_id: 12345 };
        store
            .set_sync_cursor(&account.id, &new_cursor)
            .await
            .unwrap();

        let fetched = store.get_sync_cursor(&account.id).await.unwrap().unwrap();
        let json = serde_json::to_string(&fetched).unwrap();
        assert!(json.contains("12345"));
    }

    #[tokio::test]
    async fn set_read_persists_read_state_with_event_source() {
        // Pins the new contract: set_read accepts an EventSource and toggles the
        // READ flag in both directions. The argument is unused at this slice; later
        // slices observe it through `message_events`. Test stays correct because
        // it asserts on observable flag state, which survives any later refactor of
        // how source is recorded.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let env = test_envelope(&account.id);
        store.upsert_envelope(&env).await.unwrap();
        assert!(env.flags.contains(MessageFlags::READ));

        store
            .set_read(&env.id, false, EventSource::User)
            .await
            .unwrap();
        let after_unread = store.get_envelope(&env.id).await.unwrap().unwrap();
        assert!(!after_unread.flags.contains(MessageFlags::READ));

        store
            .set_read(&env.id, true, EventSource::User)
            .await
            .unwrap();
        let after_read = store.get_envelope(&env.id).await.unwrap().unwrap();
        assert!(after_read.flags.contains(MessageFlags::READ));
    }

    #[tokio::test]
    async fn set_read_emits_message_event_on_transition() {
        // Three cases: real transition emits, no-op repeat does not, opposite
        // transition with different source emits with that source. Asserts on the
        // observable contract of `message_events` rows — survives any rewrite of
        // how the hook is wired so long as transitions still produce typed rows.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let env = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&env).await.unwrap();
        assert!(!env.flags.contains(MessageFlags::READ));

        // Transition: unread -> read.
        store
            .set_read(&env.id, true, EventSource::User)
            .await
            .unwrap();
        let events = store.list_message_events(&env.id).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, MessageEventType::Read);
        assert_eq!(events[0].source, EventSource::User);
        assert_eq!(events[0].account_id, account.id);

        // No-op: read -> read with same source. Must not emit.
        store
            .set_read(&env.id, true, EventSource::User)
            .await
            .unwrap();
        let events_after_noop = store.list_message_events(&env.id).await.unwrap();
        assert_eq!(events_after_noop.len(), 1);

        // Transition: read -> unread with a different source.
        store
            .set_read(&env.id, false, EventSource::Sync)
            .await
            .unwrap();
        let events_after_unread = store.list_message_events(&env.id).await.unwrap();
        assert_eq!(events_after_unread.len(), 2);
        assert_eq!(events_after_unread[1].event_type, MessageEventType::Unread);
        assert_eq!(events_after_unread[1].source, EventSource::Sync);
    }

    #[tokio::test]
    async fn mutations_emit_typed_message_events_with_source() {
        // Single test drives the source-attribution refactor across the rest of
        // the mutation surface. Each row in `message_events` proves: (a) the
        // method now takes EventSource, (b) it emits on actual transition, (c)
        // the source is recorded as given. No-op repeats and bulk methods are
        // covered by separate tests above / below.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let env = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&env).await.unwrap();

        let label = mxr_core::Label {
            id: LabelId::new(),
            account_id: account.id.clone(),
            name: "Important".to_string(),
            kind: LabelKind::User,
            color: None,
            provider_id: "provider-Important".to_string(),
            unread_count: 0,
            total_count: 0,
        };
        store.upsert_label(&label).await.unwrap();

        store
            .set_starred(&env.id, true, EventSource::RuleEngine)
            .await
            .unwrap();
        store
            .move_to_trash(&env.id, EventSource::User)
            .await
            .unwrap();
        store
            .add_message_label(&env.id, &label.id, EventSource::Sync)
            .await
            .unwrap();
        store
            .remove_message_label(&env.id, &label.id, EventSource::User)
            .await
            .unwrap();

        let events = store.list_message_events(&env.id).await.unwrap();
        let types: Vec<MessageEventType> = events.iter().map(|e| e.event_type).collect();
        let sources: Vec<EventSource> = events.iter().map(|e| e.source).collect();

        assert_eq!(
            types,
            vec![
                MessageEventType::Starred,
                MessageEventType::Trashed,
                MessageEventType::Labeled,
                MessageEventType::Unlabeled,
            ],
        );
        assert_eq!(
            sources,
            vec![
                EventSource::RuleEngine,
                EventSource::User,
                EventSource::Sync,
                EventSource::User,
            ],
        );
        // Label events must carry the label_id; flag events must not.
        assert_eq!(events[0].label_id, None);
        assert_eq!(events[1].label_id, None);
        assert_eq!(events[2].label_id, Some(label.id.clone()));
        assert_eq!(events[3].label_id, Some(label.id));
    }

    #[tokio::test]
    async fn storage_breakdown_groups_by_mimetype_sender_and_label() {
        // Pins the StorageBucket contract for all three group-by modes. Inputs
        // are deliberate so each ordering and arithmetic check is grounded in
        // the fixture, not the SQL — the query can be rewritten and this test
        // still proves the contract.
        use mxr_core::{
            AttachmentDisposition, AttachmentId, AttachmentMeta, MessageBody, MessageMetadata,
        };

        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let label_work = Label {
            id: LabelId::new(),
            account_id: account.id.clone(),
            name: "Work".to_string(),
            kind: LabelKind::User,
            color: None,
            provider_id: "provider-Work".to_string(),
            unread_count: 0,
            total_count: 0,
        };
        store.upsert_label(&label_work).await.unwrap();

        // Two messages from alice (1000 + 2000 bytes) under "Work";
        // one message from bob (5000 bytes) with no label.
        let mut msg_a = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        msg_a.from = Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        };
        msg_a.size_bytes = 1000;
        store.upsert_envelope(&msg_a).await.unwrap();
        store
            .set_message_labels(
                &msg_a.id,
                std::slice::from_ref(&label_work.id),
                EventSource::User,
            )
            .await
            .unwrap();

        let mut msg_a2 = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        msg_a2.from = Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        };
        msg_a2.size_bytes = 2000;
        msg_a2.provider_id = "second-msg".into();
        store.upsert_envelope(&msg_a2).await.unwrap();
        store
            .set_message_labels(
                &msg_a2.id,
                std::slice::from_ref(&label_work.id),
                EventSource::User,
            )
            .await
            .unwrap();

        let mut msg_b = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        msg_b.from = Address {
            name: Some("Bob".into()),
            email: "bob@example.com".into(),
        };
        msg_b.size_bytes = 5000;
        msg_b.provider_id = "third-msg".into();
        store.upsert_envelope(&msg_b).await.unwrap();

        // Attachments: 2 PNGs (10k each) on msg_a, 1 PDF (50k) on msg_b.
        let body_a = MessageBody {
            message_id: msg_a.id.clone(),
            text_plain: Some("body".into()),
            text_html: None,
            attachments: vec![
                AttachmentMeta {
                    id: AttachmentId::from_provider_id("test", "png-1"),
                    message_id: msg_a.id.clone(),
                    filename: "photo1.png".into(),
                    mime_type: "image/png".into(),
                    disposition: AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 10_000,
                    local_path: None,
                    provider_id: "png-1".into(),
                },
                AttachmentMeta {
                    id: AttachmentId::from_provider_id("test", "png-2"),
                    message_id: msg_a.id.clone(),
                    filename: "photo2.png".into(),
                    mime_type: "image/png".into(),
                    disposition: AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 10_000,
                    local_path: None,
                    provider_id: "png-2".into(),
                },
            ],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };
        store.insert_body(&body_a).await.unwrap();

        let body_b = MessageBody {
            message_id: msg_b.id.clone(),
            text_plain: Some("body".into()),
            text_html: None,
            attachments: vec![AttachmentMeta {
                id: AttachmentId::from_provider_id("test", "pdf-1"),
                message_id: msg_b.id.clone(),
                filename: "doc.pdf".into(),
                mime_type: "application/pdf".into(),
                disposition: AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 50_000,
                local_path: None,
                provider_id: "pdf-1".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };
        store.insert_body(&body_b).await.unwrap();

        // by mimetype: PDF leads at 50k×1, PNG follows at 20k×2.
        let by_mime = store
            .storage_breakdown(None, StorageGroupBy::Mimetype, 10)
            .await
            .unwrap();
        assert_eq!(by_mime.len(), 2);
        assert_eq!(by_mime[0].key, "application/pdf");
        assert_eq!(by_mime[0].bytes, 50_000);
        assert_eq!(by_mime[0].count, 1);
        assert_eq!(by_mime[1].key, "image/png");
        assert_eq!(by_mime[1].bytes, 20_000);
        assert_eq!(by_mime[1].count, 2);

        // by sender: bob 5000×1 leads alice 3000×2.
        let by_sender = store
            .storage_breakdown(None, StorageGroupBy::Sender, 10)
            .await
            .unwrap();
        assert_eq!(by_sender.len(), 2);
        assert_eq!(by_sender[0].key, "bob@example.com");
        assert_eq!(by_sender[0].bytes, 5000);
        assert_eq!(by_sender[0].count, 1);
        assert_eq!(by_sender[1].key, "alice@example.com");
        assert_eq!(by_sender[1].bytes, 3000);
        assert_eq!(by_sender[1].count, 2);

        // by label: only Work shows (bob's message is unlabeled).
        let by_label = store
            .storage_breakdown(None, StorageGroupBy::Label, 10)
            .await
            .unwrap();
        assert_eq!(by_label.len(), 1);
        assert_eq!(by_label[0].key, "Work");
        assert_eq!(by_label[0].bytes, 3000);
        assert_eq!(by_label[0].count, 2);
    }

    #[tokio::test]
    async fn reclassify_unknown_directions_uses_lookup_closure() {
        // Pins Slice 15's reclassification: rows with direction='unknown'
        // get reclassified per the lookup; rows with concrete direction are
        // untouched.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let mut from_me = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        from_me.id = MessageId::new();
        from_me.provider_id = "out-1".into();
        from_me.from = Address {
            name: None,
            email: "me@example.com".into(),
        };
        store.upsert_envelope(&from_me).await.unwrap();

        let mut from_other = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        from_other.id = MessageId::new();
        from_other.provider_id = "in-1".into();
        from_other.from = Address {
            name: None,
            email: "alice@example.com".into(),
        };
        store.upsert_envelope(&from_other).await.unwrap();

        let mut already_classified = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        already_classified.id = MessageId::new();
        already_classified.provider_id = "in-2".into();
        already_classified.from = Address {
            name: None,
            email: "bob@example.com".into(),
        };
        store
            .upsert_envelope_with_direction(&already_classified, MessageDirection::Inbound)
            .await
            .unwrap();

        let updated = store
            .reclassify_unknown_directions(|email| email.eq_ignore_ascii_case("me@example.com"))
            .await
            .unwrap();
        assert_eq!(updated, 2);

        let directions: Vec<(String, String)> =
            sqlx::query_as("SELECT id, direction FROM messages ORDER BY provider_id")
                .fetch_all(store.reader())
                .await
                .unwrap();
        let by_id: std::collections::HashMap<String, String> = directions.into_iter().collect();
        assert_eq!(
            by_id.get(&from_me.id.as_str()).map(String::as_str),
            Some("outbound")
        );
        assert_eq!(
            by_id.get(&from_other.id.as_str()).map(String::as_str),
            Some("inbound")
        );
        assert_eq!(
            by_id
                .get(&already_classified.id.as_str())
                .map(String::as_str),
            Some("inbound")
        );
    }

    #[test]
    fn compute_business_hours_seconds_excludes_weekends_and_off_hours() {
        use crate::reply_pairs::compute_business_hours_seconds;
        use chrono::TimeZone;
        // Friday 17:00 UTC -> Monday 09:00 UTC: business-hours latency is 0.
        let fri_5pm = chrono::Utc
            .with_ymd_and_hms(2026, 5, 1, 17, 0, 0)
            .unwrap()
            .timestamp();
        let mon_9am = chrono::Utc
            .with_ymd_and_hms(2026, 5, 4, 9, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(compute_business_hours_seconds(fri_5pm, mon_9am), 0);

        // Same-day window inside business hours: 10:00 -> 12:00 = 2h = 7200s.
        let mon_10am = chrono::Utc
            .with_ymd_and_hms(2026, 5, 4, 10, 0, 0)
            .unwrap()
            .timestamp();
        let mon_12pm = chrono::Utc
            .with_ymd_and_hms(2026, 5, 4, 12, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(compute_business_hours_seconds(mon_10am, mon_12pm), 7200);

        // Window straddles a weekend: Mon 16:00 -> Tue 10:00 with weekend
        // intervening NOT in the test (consecutive weekdays). Mon: 16-17 = 1h.
        // Tue: 9-10 = 1h. Total = 7200.
        let mon_4pm = chrono::Utc
            .with_ymd_and_hms(2026, 5, 4, 16, 0, 0)
            .unwrap()
            .timestamp();
        let tue_10am = chrono::Utc
            .with_ymd_and_hms(2026, 5, 5, 10, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(compute_business_hours_seconds(mon_4pm, tue_10am), 2 * 3600);

        // Reverse range: returns 0.
        assert_eq!(compute_business_hours_seconds(mon_12pm, mon_10am), 0);
    }

    #[tokio::test]
    async fn list_response_time_returns_clock_and_business_percentiles() {
        // Three i_replied pairs with latencies 1h/2h/3h all on a Monday so
        // business hours match clock.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        // Anchor parents/replies on Monday in business hours.
        let monday: chrono::DateTime<chrono::Utc> =
            chrono::Utc.with_ymd_and_hms(2026, 5, 4, 10, 0, 0).unwrap();
        for (i, latency_h) in [1i64, 2, 3].iter().enumerate() {
            let mut parent = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .build();
            parent.id = MessageId::new();
            parent.provider_id = format!("parent-{i}");
            parent.message_id_header = Some(format!("<parent-{i}@x>"));
            parent.from = Address {
                name: None,
                email: format!("alice-{i}@example.com"),
            };
            parent.date = monday;
            store
                .upsert_envelope_with_direction(&parent, MessageDirection::Inbound)
                .await
                .unwrap();

            let mut reply = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .build();
            reply.id = MessageId::new();
            reply.provider_id = format!("reply-{i}");
            reply.from = Address {
                name: None,
                email: "me@example.com".into(),
            };
            reply.to = vec![Address {
                name: None,
                email: format!("alice-{i}@example.com"),
            }];
            reply.in_reply_to = Some(format!("<parent-{i}@x>"));
            reply.date = monday + chrono::Duration::hours(*latency_h);
            store
                .upsert_envelope_with_direction(&reply, MessageDirection::Outbound)
                .await
                .unwrap();
            assert!(store
                .try_create_reply_pair(&reply, MessageDirection::Outbound)
                .await
                .unwrap());
        }
        let backfilled = store.backfill_business_hours_latency().await.unwrap();
        assert_eq!(backfilled, 3);

        let summary = store
            .list_response_time(None, ResponseTimeDirection::IReplied, None, None)
            .await
            .unwrap();
        assert_eq!(summary.sample_count, 3);
        assert_eq!(summary.clock_p50_seconds, 7200);
        assert_eq!(summary.clock_p90_seconds, 10_800);
        assert_eq!(summary.business_hours_p50_seconds, Some(7200));
        assert_eq!(summary.business_hours_p90_seconds, Some(10_800));
    }

    #[tokio::test]
    async fn refresh_contacts_aggregates_inbound_and_outbound_per_email() {
        // Pins the contacts materialization contract: per (account, email),
        // inbound/outbound counts roll up correctly. asymmetry ratios derive
        // from the materialized rows and must surface the imbalance.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let now = chrono::Utc::now();

        async fn seed(
            store: &Store,
            account_id: &AccountId,
            sender: &str,
            direction: MessageDirection,
            count: usize,
            now: chrono::DateTime<chrono::Utc>,
        ) {
            for i in 0..count {
                let mut env = TestEnvelopeBuilder::new()
                    .account_id(account_id.clone())
                    .build();
                env.id = MessageId::new();
                let dir_label = match direction {
                    MessageDirection::Inbound => "in",
                    MessageDirection::Outbound => "out",
                    MessageDirection::Unknown => "unk",
                };
                env.provider_id = format!("p-{sender}-{dir_label}-{i}");
                env.from = match direction {
                    MessageDirection::Inbound => Address {
                        name: Some(sender.into()),
                        email: format!("{sender}@example.com"),
                    },
                    _ => Address {
                        name: None,
                        email: "me@example.com".into(),
                    },
                };
                env.to = match direction {
                    MessageDirection::Inbound => vec![Address {
                        name: None,
                        email: "me@example.com".into(),
                    }],
                    _ => vec![Address {
                        name: None,
                        email: format!("{sender}@example.com"),
                    }],
                };
                env.date = now - chrono::Duration::days(i as i64);
                store
                    .upsert_envelope_with_direction(&env, direction)
                    .await
                    .unwrap();
            }
        }

        // alice: 10 inbound, 1 outbound (high asymmetry)
        seed(
            &store,
            &account.id,
            "alice",
            MessageDirection::Inbound,
            10,
            now,
        )
        .await;
        seed(
            &store,
            &account.id,
            "alice",
            MessageDirection::Outbound,
            1,
            now,
        )
        .await;
        // bob: 5 inbound, 5 outbound (balanced)
        seed(
            &store,
            &account.id,
            "bob",
            MessageDirection::Inbound,
            5,
            now,
        )
        .await;
        seed(
            &store,
            &account.id,
            "bob",
            MessageDirection::Outbound,
            5,
            now,
        )
        .await;
        // carol: 0 inbound, 8 outbound (extreme asymmetry, but min_inbound filter)
        seed(
            &store,
            &account.id,
            "carol",
            MessageDirection::Outbound,
            8,
            now,
        )
        .await;

        store.refresh_contacts().await.unwrap();

        // min_inbound = 1: carol filtered out, alice + bob remain.
        let asym = store.list_contact_asymmetry(None, 1, 50).await.unwrap();
        let by_email: std::collections::HashMap<_, _> =
            asym.iter().map(|r| (r.email.clone(), r)).collect();
        let alice = by_email.get("alice@example.com").unwrap();
        assert_eq!(alice.total_inbound, 10);
        assert_eq!(alice.total_outbound, 1);
        // 9/10 = 0.9
        assert!((alice.asymmetry - 0.9).abs() < 1e-6);

        let bob = by_email.get("bob@example.com").unwrap();
        assert_eq!(bob.total_inbound, 5);
        assert_eq!(bob.total_outbound, 5);
        assert!(bob.asymmetry.abs() < 1e-6);

        assert!(by_email.get("carol@example.com").is_none());

        // First row should be alice (largest asymmetry).
        assert_eq!(asym[0].email, "alice@example.com");
    }

    #[tokio::test]
    async fn list_contact_decay_excludes_30_day_boundary() {
        // Three contacts:
        //   A: last inbound 60d ago / no outbound -> appears (cold > 30 days)
        //   B: last inbound 5d ago / outbound 1d ago -> excluded (recent reply)
        //   C: last inbound exactly 30d ago / no outbound -> excluded (boundary)
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let now = chrono::Utc::now();

        async fn seed_inbound(
            store: &Store,
            account_id: &AccountId,
            sender: &str,
            days_ago: i64,
            now: chrono::DateTime<chrono::Utc>,
        ) {
            let mut env = TestEnvelopeBuilder::new()
                .account_id(account_id.clone())
                .build();
            env.id = MessageId::new();
            env.provider_id = format!("in-{sender}-{days_ago}");
            env.from = Address {
                name: None,
                email: format!("{sender}@example.com"),
            };
            env.date = now - chrono::Duration::days(days_ago);
            store
                .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
                .await
                .unwrap();
        }

        async fn seed_outbound(
            store: &Store,
            account_id: &AccountId,
            recipient: &str,
            days_ago: i64,
            now: chrono::DateTime<chrono::Utc>,
        ) {
            let mut env = TestEnvelopeBuilder::new()
                .account_id(account_id.clone())
                .build();
            env.id = MessageId::new();
            env.provider_id = format!("out-{recipient}-{days_ago}");
            env.from = Address {
                name: None,
                email: "me@example.com".into(),
            };
            env.to = vec![Address {
                name: None,
                email: format!("{recipient}@example.com"),
            }];
            env.date = now - chrono::Duration::days(days_ago);
            store
                .upsert_envelope_with_direction(&env, MessageDirection::Outbound)
                .await
                .unwrap();
        }

        seed_inbound(&store, &account.id, "alice", 60, now).await;
        seed_inbound(&store, &account.id, "bob", 5, now).await;
        seed_outbound(&store, &account.id, "bob", 1, now).await;
        // Tip: feeding `30 * 86400` exact seconds produces a boundary at the
        // cutoff, which the SQL excludes (`<` cutoff, not `<=`).
        seed_inbound(&store, &account.id, "carol", 30, now).await;

        store.refresh_contacts().await.unwrap();
        let decay = store.list_contact_decay(None, 30, 50).await.unwrap();
        let emails: Vec<_> = decay.iter().map(|r| r.email.clone()).collect();
        assert_eq!(emails, vec!["alice@example.com"]);
        assert!(decay[0].days_since_inbound >= 59);
        assert!(decay[0].last_outbound_at.is_none());
    }

    #[tokio::test]
    async fn list_stale_threads_filters_by_perspective_and_age() {
        // Three threads:
        //   A: latest message inbound 30d ago     -> should appear with --mine
        //   B: latest message outbound 30d ago    -> should appear with --theirs
        //   C: latest message inbound 5d ago      -> excluded by older-than-14d
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let now = chrono::Utc::now();
        async fn seed(
            store: &Store,
            account_id: &AccountId,
            sender: &str,
            days_ago: i64,
            direction: MessageDirection,
            thread: ThreadId,
            now: chrono::DateTime<chrono::Utc>,
        ) {
            let mut env = TestEnvelopeBuilder::new()
                .account_id(account_id.clone())
                .build();
            env.id = MessageId::new();
            env.provider_id = format!("p-{}-{}", sender, days_ago);
            env.thread_id = thread;
            env.from = Address {
                name: None,
                email: format!("{sender}@example.com"),
            };
            env.to = vec![Address {
                name: None,
                email: "me@example.com".into(),
            }];
            env.date = now - chrono::Duration::days(days_ago);
            env.subject = format!("subject-{sender}-{days_ago}");
            store
                .upsert_envelope_with_direction(&env, direction)
                .await
                .unwrap();
        }

        let thread_a = ThreadId::new();
        let thread_b = ThreadId::new();
        let thread_c = ThreadId::new();
        seed(
            &store,
            &account.id,
            "alice",
            30,
            MessageDirection::Inbound,
            thread_a.clone(),
            now,
        )
        .await;
        seed(
            &store,
            &account.id,
            "bob",
            30,
            MessageDirection::Outbound,
            thread_b.clone(),
            now,
        )
        .await;
        seed(
            &store,
            &account.id,
            "carol",
            5,
            MessageDirection::Inbound,
            thread_c,
            now,
        )
        .await;

        let cutoff = now.timestamp() - 14 * 86_400;
        let mine = store
            .list_stale_threads(None, StaleBallInCourt::Mine, cutoff, 50)
            .await
            .unwrap();
        let mine_threads: Vec<_> = mine.iter().map(|r| r.thread_id.clone()).collect();
        assert_eq!(mine_threads, vec![thread_a.clone()]);
        assert_eq!(mine[0].counterparty_email, "alice@example.com");
        assert!(mine[0].days_stale >= 29);

        let theirs = store
            .list_stale_threads(None, StaleBallInCourt::Theirs, cutoff, 50)
            .await
            .unwrap();
        let theirs_threads: Vec<_> = theirs.iter().map(|r| r.thread_id.clone()).collect();
        assert_eq!(theirs_threads, vec![thread_b]);
        assert_eq!(theirs[0].counterparty_email, "me@example.com");
    }

    #[tokio::test]
    async fn try_create_reply_pair_links_outbound_reply_to_inbound_parent() {
        // Pins the i_replied contract: outbound message with in_reply_to
        // pointing at an inbound parent creates a reply_pairs row with
        // accurate latency. Negative case (unknown parent) returns false and
        // leaves the table untouched, then enqueues into the pending queue.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let inbound_at: chrono::DateTime<chrono::Utc> =
            chrono::Utc.with_ymd_and_hms(2026, 5, 1, 9, 0, 0).unwrap();
        let outbound_at = inbound_at + chrono::Duration::seconds(3600);

        let mut inbound = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        inbound.from = Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        };
        inbound.message_id_header = Some("<parent@example.com>".to_string());
        inbound.date = inbound_at;
        store
            .upsert_envelope_with_direction(&inbound, MessageDirection::Inbound)
            .await
            .unwrap();

        let mut outbound = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        outbound.id = MessageId::new();
        outbound.provider_id = "outbound-1".into();
        outbound.from = Address {
            name: None,
            email: "me@example.com".into(),
        };
        outbound.to = vec![Address {
            name: None,
            email: "alice@example.com".into(),
        }];
        outbound.in_reply_to = Some("<parent@example.com>".to_string());
        outbound.date = outbound_at;
        store
            .upsert_envelope_with_direction(&outbound, MessageDirection::Outbound)
            .await
            .unwrap();

        let created = store
            .try_create_reply_pair(&outbound, MessageDirection::Outbound)
            .await
            .unwrap();
        assert!(created);

        let row: (String, String, i64, String) = sqlx::query_as(
            "SELECT direction, counterparty_email, latency_seconds, parent_message_id
             FROM reply_pairs WHERE reply_message_id = ?",
        )
        .bind(outbound.id.as_str())
        .fetch_one(store.reader())
        .await
        .unwrap();
        assert_eq!(row.0, "i_replied");
        assert_eq!(row.1, "alice@example.com");
        assert_eq!(row.2, 3600);
        assert_eq!(row.3, inbound.id.as_str());
    }

    #[tokio::test]
    async fn try_create_reply_pair_enqueues_when_parent_unknown() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let mut outbound = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        outbound.from = Address {
            name: None,
            email: "me@example.com".into(),
        };
        outbound.in_reply_to = Some("<unseen-parent@example.com>".to_string());
        store
            .upsert_envelope_with_direction(&outbound, MessageDirection::Outbound)
            .await
            .unwrap();

        let created = store
            .try_create_reply_pair(&outbound, MessageDirection::Outbound)
            .await
            .unwrap();
        assert!(!created, "no parent should mean no pair");

        store.enqueue_reply_pair_pending(&outbound).await.unwrap();

        let pending: Vec<(String,)> =
            sqlx::query_as("SELECT in_reply_to_header FROM reply_pair_pending")
                .fetch_all(store.reader())
                .await
                .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, "<unseen-parent@example.com>");
    }

    #[tokio::test]
    async fn reconcile_reply_pair_pending_resolves_when_parent_arrives() {
        // Pins Slice 10's reconciler contract: a pending reply gets migrated
        // into `reply_pairs` once its parent lands in `messages`. Function-
        // level test — the daemon-loop wiring is exercised separately.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let inbound_at: chrono::DateTime<chrono::Utc> =
            chrono::Utc.with_ymd_and_hms(2026, 5, 2, 10, 0, 0).unwrap();
        let outbound_at = inbound_at + chrono::Duration::seconds(7200);

        let mut outbound = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        outbound.from = Address {
            name: None,
            email: "me@example.com".into(),
        };
        outbound.in_reply_to = Some("<late-parent@example.com>".to_string());
        outbound.date = outbound_at;
        store
            .upsert_envelope_with_direction(&outbound, MessageDirection::Outbound)
            .await
            .unwrap();
        store.enqueue_reply_pair_pending(&outbound).await.unwrap();

        // Parent arrives later.
        let mut inbound = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        inbound.id = MessageId::new();
        inbound.provider_id = "inbound-late".into();
        inbound.message_id_header = Some("<late-parent@example.com>".to_string());
        inbound.from = Address {
            name: None,
            email: "alice@example.com".into(),
        };
        inbound.date = inbound_at;
        store
            .upsert_envelope_with_direction(&inbound, MessageDirection::Inbound)
            .await
            .unwrap();

        let migrated = store.reconcile_reply_pair_pending().await.unwrap();
        assert_eq!(migrated, 1);

        let pending_after: Vec<(String,)> =
            sqlx::query_as("SELECT reply_message_id FROM reply_pair_pending")
                .fetch_all(store.reader())
                .await
                .unwrap();
        assert!(pending_after.is_empty());

        let row: (String, i64) = sqlx::query_as(
            "SELECT direction, latency_seconds FROM reply_pairs WHERE reply_message_id = ?",
        )
        .bind(outbound.id.as_str())
        .fetch_one(store.reader())
        .await
        .unwrap();
        assert_eq!(row.0, "i_replied");
        assert_eq!(row.1, 7200);
    }

    #[tokio::test]
    async fn upsert_envelope_with_direction_writes_direction_column() {
        // Pins the direction-write contract: explicit Inbound/Outbound lands
        // in the column on insert, and re-upserting with `Unknown` preserves
        // the previously classified value (sticky direction). Survives any
        // future refactor of how the column is plumbed because it asserts on
        // the persisted value.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let env = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store
            .upsert_envelope_with_direction(&env, MessageDirection::Outbound)
            .await
            .unwrap();

        let direction: (String,) = sqlx::query_as("SELECT direction FROM messages WHERE id = ?")
            .bind(env.id.as_str())
            .fetch_one(store.reader())
            .await
            .unwrap();
        assert_eq!(direction.0, "outbound");

        // Re-upserting with `Unknown` must NOT downgrade a classified row.
        store
            .upsert_envelope_with_direction(&env, MessageDirection::Unknown)
            .await
            .unwrap();
        let direction_after: (String,) =
            sqlx::query_as("SELECT direction FROM messages WHERE id = ?")
                .bind(env.id.as_str())
                .fetch_one(store.reader())
                .await
                .unwrap();
        assert_eq!(direction_after.0, "outbound");

        // Re-upserting with an explicit Inbound flips it (reclassification
        // is allowed, only Unknown is sticky-skipped).
        store
            .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
            .await
            .unwrap();
        let direction_flipped: (String,) =
            sqlx::query_as("SELECT direction FROM messages WHERE id = ?")
                .bind(env.id.as_str())
                .fetch_one(store.reader())
                .await
                .unwrap();
        assert_eq!(direction_flipped.0, "inbound");
    }

    #[tokio::test]
    async fn account_addresses_roundtrip_with_primary_uniqueness() {
        // Pins the account_addresses contract: insert_account seeds primary,
        // adding aliases preserves a single primary, set_primary_address
        // atomically demotes the previous primary, removal works, and the
        // global snapshot returns everything for the AppState cache.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        // insert_account auto-seeds the canonical email as primary.
        let initial = store.list_account_addresses(&account.id).await.unwrap();
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].email, account.email);
        assert!(initial[0].is_primary);

        // Add an alias as non-primary.
        store
            .add_account_address(&account.id, "alias@example.com", false)
            .await
            .unwrap();
        let after_alias = store.list_account_addresses(&account.id).await.unwrap();
        assert_eq!(after_alias.len(), 2);
        let primaries: Vec<_> = after_alias.iter().filter(|a| a.is_primary).collect();
        assert_eq!(primaries.len(), 1);
        assert_eq!(primaries[0].email, account.email);

        // Promote alias to primary; previous primary must demote.
        store
            .set_primary_address(&account.id, "alias@example.com")
            .await
            .unwrap();
        let promoted = store.list_account_addresses(&account.id).await.unwrap();
        let new_primary: Vec<_> = promoted.iter().filter(|a| a.is_primary).collect();
        assert_eq!(new_primary.len(), 1);
        assert_eq!(new_primary[0].email, "alias@example.com");

        // Adding a third with is_primary=true also demotes the current primary.
        store
            .add_account_address(&account.id, "third@example.com", true)
            .await
            .unwrap();
        let after_third = store.list_account_addresses(&account.id).await.unwrap();
        let third_primaries: Vec<_> = after_third.iter().filter(|a| a.is_primary).collect();
        assert_eq!(third_primaries.len(), 1);
        assert_eq!(third_primaries[0].email, "third@example.com");

        // Removal works.
        store
            .remove_account_address(&account.id, "alias@example.com")
            .await
            .unwrap();
        let after_remove = store.list_account_addresses(&account.id).await.unwrap();
        assert_eq!(after_remove.len(), 2);
        assert!(!after_remove.iter().any(|a| a.email == "alias@example.com"));

        // Setting primary on a non-existent address fails loudly.
        let err = store
            .set_primary_address(&account.id, "ghost@example.com")
            .await
            .unwrap_err();
        assert!(matches!(err, sqlx::Error::RowNotFound));

        // Global snapshot returns everything for the cache.
        let all = store.list_all_account_addresses().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn list_subscriptions_computes_open_and_archive_counts_per_sender() {
        // Pins the new ROI fields on SubscriptionSummary. Three senders, each
        // with a distinct read/archive mix. Test asserts counts arithmetic;
        // survives any later refactor of the SQL window functions.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        // Helper closure builds an envelope with the desired flags + sender +
        // an unsubscribe header so it qualifies as a "subscription".
        let mut next_id = 0u32;
        let mut make_msg = |sender: &str, flags: MessageFlags| {
            next_id += 1;
            let mut env = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .flags(flags)
                .build();
            env.from = Address {
                name: Some(sender.into()),
                email: format!("{sender}@list.example.com"),
            };
            env.provider_id = format!("msg-{next_id}");
            env.unsubscribe = UnsubscribeMethod::HttpLink {
                url: "https://example.com/unsub".into(),
            };
            env
        };

        // alice: 3 messages — 1 read, 0 archived-unread.
        for i in 0..3 {
            let flags = if i == 0 {
                MessageFlags::READ
            } else {
                MessageFlags::empty()
            };
            store
                .upsert_envelope(&make_msg("alice", flags))
                .await
                .unwrap();
        }
        // bob: 5 messages — 5 read, 0 archived-unread.
        for _ in 0..5 {
            store
                .upsert_envelope(&make_msg("bob", MessageFlags::READ))
                .await
                .unwrap();
        }
        // carol: 4 messages — 0 read, 3 archived-unread.
        for i in 0..4 {
            let flags = if i < 3 {
                MessageFlags::ARCHIVED
            } else {
                MessageFlags::empty()
            };
            store
                .upsert_envelope(&make_msg("carol", flags))
                .await
                .unwrap();
        }

        let mut subs = store.list_subscriptions(None, 100).await.unwrap();
        subs.sort_by(|a, b| a.sender_email.cmp(&b.sender_email));
        assert_eq!(subs.len(), 3);

        let by_email = |email: &str| {
            subs.iter()
                .find(|s| s.sender_email == email)
                .unwrap_or_else(|| panic!("missing sender {email}"))
        };

        let alice = by_email("alice@list.example.com");
        assert_eq!(alice.message_count, 3);
        assert_eq!(alice.opened_count, 1);
        assert_eq!(alice.archived_unread_count, 0);
        assert_eq!(alice.replied_count, 0);

        let bob = by_email("bob@list.example.com");
        assert_eq!(bob.message_count, 5);
        assert_eq!(bob.opened_count, 5);
        assert_eq!(bob.archived_unread_count, 0);

        let carol = by_email("carol@list.example.com");
        assert_eq!(carol.message_count, 4);
        assert_eq!(carol.opened_count, 0);
        assert_eq!(carol.archived_unread_count, 3);
    }

    #[tokio::test]
    async fn insert_body_promotes_list_id_to_messages_column() {
        // Pins the sync-time list_id promotion contract. Body metadata's
        // `list_id` field flows into `messages.list_id` so the indexed grouping
        // works in `mxr unsub --rank`. Survives any future split of the
        // promotion path (e.g. moving it into upsert_envelope) because it
        // asserts on the column value, not the call shape.
        use mxr_core::{MessageBody, MessageMetadata};

        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let env = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&env).await.unwrap();

        // Initial state: list_id is NULL because migration 007 default applies.
        let pre: Vec<(Option<String>,)> =
            sqlx::query_as("SELECT list_id FROM messages WHERE id = ?")
                .bind(env.id.as_str())
                .fetch_all(store.reader())
                .await
                .unwrap();
        assert_eq!(pre[0].0, None);

        // Body with list_id set.
        let body = MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("body".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                list_id: Some("readwise.io".into()),
                ..MessageMetadata::default()
            },
        };
        store.insert_body(&body).await.unwrap();

        let post: Vec<(Option<String>,)> =
            sqlx::query_as("SELECT list_id FROM messages WHERE id = ?")
                .bind(env.id.as_str())
                .fetch_all(store.reader())
                .await
                .unwrap();
        assert_eq!(post[0].0.as_deref(), Some("readwise.io"));

        // Body without list_id leaves the column untouched (idempotent — the
        // column does not get cleared by a subsequent body that lacks the
        // header). This is the intended behavior; doctor --rebuild-analytics
        // can clear stale promotions explicitly if ever needed.
        let body_no_list = MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("body".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };
        store.insert_body(&body_no_list).await.unwrap();
        let post2: Vec<(Option<String>,)> =
            sqlx::query_as("SELECT list_id FROM messages WHERE id = ?")
                .bind(env.id.as_str())
                .fetch_all(store.reader())
                .await
                .unwrap();
        assert_eq!(post2[0].0.as_deref(), Some("readwise.io"));
    }

    #[tokio::test]
    async fn upsert_envelope_writes_direction_unknown_and_null_list_id_by_default() {
        // Pins migration 007's column contract: every inserted message has a
        // non-null `direction` defaulting to 'unknown' and a nullable `list_id`
        // defaulting to NULL. Slice 8 will wire actual inference and Slice 6
        // will populate list_id from body metadata; this test stays green
        // through both because it asserts only the schema-level guarantee.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let env_a = test_envelope(&account.id);
        let mut env_b = test_envelope(&account.id);
        env_b.id = MessageId::new();
        env_b.provider_id = "second".to_string();
        store.upsert_envelope(&env_a).await.unwrap();
        store.upsert_envelope(&env_b).await.unwrap();

        let directions: Vec<(String,)> =
            sqlx::query_as("SELECT direction FROM messages ORDER BY id")
                .fetch_all(store.reader())
                .await
                .unwrap();
        assert_eq!(directions.len(), 2);
        for (direction,) in &directions {
            assert_eq!(direction, "unknown");
        }

        let list_ids: Vec<(Option<String>,)> = sqlx::query_as("SELECT list_id FROM messages")
            .fetch_all(store.reader())
            .await
            .unwrap();
        assert!(list_ids.iter().all(|(v,)| v.is_none()));
    }

    #[tokio::test]
    async fn add_message_label_does_not_emit_on_duplicate() {
        // INSERT OR IGNORE is idempotent; the hook must respect that and skip
        // emitting an event for a no-op insert. Otherwise rule re-runs and sync
        // reconciles would double-count label additions.
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let env = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&env).await.unwrap();

        let label = mxr_core::Label {
            id: LabelId::new(),
            account_id: account.id.clone(),
            name: "Pinned".to_string(),
            kind: LabelKind::User,
            color: None,
            provider_id: "provider-Pinned".to_string(),
            unread_count: 0,
            total_count: 0,
        };
        store.upsert_label(&label).await.unwrap();

        store
            .add_message_label(&env.id, &label.id, EventSource::User)
            .await
            .unwrap();
        store
            .add_message_label(&env.id, &label.id, EventSource::User)
            .await
            .unwrap();

        let events = store.list_message_events(&env.id).await.unwrap();
        assert_eq!(events.len(), 1);
    }
}
