mod account;
mod body;
mod draft;
mod event_log;
mod label;
mod message;
mod pool;
mod rules;
mod search;
mod snooze;
mod sync_cursor;
mod sync_log;
mod sync_runtime_status;
mod thread;

pub use event_log::EventLogEntry;
pub use pool::Store;
pub use rules::{row_to_rule_json, row_to_rule_log_json, RuleLogInput, RuleRecordInput};
pub use sync_log::{SyncLogEntry, SyncStatus};
pub use sync_runtime_status::{SyncRuntimeStatus, SyncRuntimeStatusUpdate};

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::*;

    fn test_account() -> Account {
        Account {
            id: AccountId::new(),
            name: "Test".to_string(),
            email: "test@example.com".to_string(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".to_string(),
            }),
            send_backend: None,
            enabled: true,
        }
    }

    fn test_envelope(account_id: &AccountId) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
            provider_id: "fake-1".to_string(),
            thread_id: ThreadId::new(),
            message_id_header: Some("<test@example.com>".to_string()),
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some("Alice".to_string()),
                email: "alice@example.com".to_string(),
            },
            to: vec![Address {
                name: None,
                email: "bob@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test subject".to_string(),
            date: chrono::Utc::now(),
            flags: MessageFlags::READ | MessageFlags::STARRED,
            snippet: "Preview text".to_string(),
            has_attachments: false,
            size_bytes: 1024,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec![],
        }
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
                size_bytes: 50000,
                local_path: None,
                provider_id: "att-1".to_string(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };
        store.insert_body(&body).await.unwrap();

        let fetched = store.get_body(&env.id).await.unwrap().unwrap();
        assert_eq!(fetched.text_plain, body.text_plain);
        assert_eq!(fetched.attachments.len(), 1);
        assert_eq!(fetched.attachments[0].filename, "report.pdf");
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
            .set_message_labels(&env.id, std::slice::from_ref(&label.id))
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
                Some(&account.id),
                Some(env_id.as_str()),
                None,
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
                .set_message_labels(&env.id, std::slice::from_ref(&label.id))
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
            .set_message_labels(&env.id, std::slice::from_ref(&original.id))
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
        assert!(store.list_envelopes_by_label(&original.id, 100, 0).await.unwrap().is_empty());
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
}
