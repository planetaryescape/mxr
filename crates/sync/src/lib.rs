#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "sync tests unwrap fixture setup for direct failures"
    )
)]

mod engine;
pub mod links;
pub use engine::{ProgressSink, SyncEngine, SyncOutcome, SyncProgress};

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::*;
    use mxr_core::types::*;
    use mxr_core::{MailSyncProvider, MutateCaps, MxrError, SyncCapabilities, SyncCaps};
    use mxr_search::{SearchIndex, SearchServiceHandle};
    use mxr_store::{ScreenerDecision, ScreenerDisposition, Store};
    use std::collections::HashSet;
    use std::sync::Arc;

    fn in_memory_search() -> SearchServiceHandle {
        SearchServiceHandle::start(SearchIndex::in_memory().unwrap()).0
    }

    struct LoadedAddressLookup {
        by_account: std::collections::HashMap<AccountId, HashSet<String>>,
    }

    impl LoadedAddressLookup {
        fn for_account(
            account_id: AccountId,
            emails: impl IntoIterator<Item = impl Into<String>>,
        ) -> Self {
            let set = emails
                .into_iter()
                .map(|email| email.into().to_lowercase())
                .collect();
            Self {
                by_account: std::collections::HashMap::from([(account_id, set)]),
            }
        }
    }

    impl AccountAddressLookup for LoadedAddressLookup {
        fn is_account_address(&self, account_id: &AccountId, email: &str) -> bool {
            self.by_account
                .get(account_id)
                .is_some_and(|set| set.contains(&email.to_lowercase()))
        }

        fn is_loaded(&self) -> bool {
            true
        }
    }

    struct ErrorProvider {
        account_id: AccountId,
    }

    #[async_trait::async_trait]
    impl MailSyncProvider for ErrorProvider {
        fn name(&self) -> &str {
            "error"
        }
        fn account_id(&self) -> &AccountId {
            &self.account_id
        }
        fn capabilities(&self) -> SyncCapabilities {
            SyncCapabilities {
                sync: SyncCaps {
                    native_threading: true,
                    ..Default::default()
                },
                ..Default::default()
            }
        }
        async fn authenticate(&mut self) -> Result<(), MxrError> {
            Ok(())
        }
        async fn refresh_auth(&mut self) -> Result<(), MxrError> {
            Ok(())
        }
        async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
            Ok(vec![])
        }
        async fn sync_messages(&self, _cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
            Err(MxrError::Provider("simulated sync error".into()))
        }
        async fn fetch_attachment(&self, _mid: &str, _aid: &str) -> Result<Vec<u8>, MxrError> {
            Err(MxrError::Provider("simulated attachment error".into()))
        }
        async fn apply_mutation(
            &self,
            _mutation_id: &str,
            _mutation: &Mutation,
        ) -> Result<(), MxrError> {
            Err(MxrError::Provider("simulated error".into()))
        }
    }

    /// Collects the engine's progress milestones so a test can assert on
    /// them after the pass returns.
    #[derive(Default)]
    struct CollectedProgress {
        milestones: std::sync::Mutex<Vec<SyncProgress>>,
    }

    impl CollectedProgress {
        fn sink(&self) -> impl Fn(SyncProgress) + Send + Sync + '_ {
            move |milestone| self.milestones.lock().unwrap().push(milestone)
        }
    }

    fn test_account(account_id: AccountId) -> mxr_core::Account {
        mxr_core::Account {
            id: account_id,
            name: "Fake Account".to_string(),
            email: "user@example.com".to_string(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".to_string(),
            }),
            send_backend: None,
            enabled: true,
        }
    }

    struct DeltaLabelProvider {
        account_id: AccountId,
        messages: Vec<SyncedMessage>,
        labels: Vec<Label>,
        label_changes: Vec<LabelChange>,
    }

    impl DeltaLabelProvider {
        fn new(
            account_id: AccountId,
            messages: Vec<Envelope>,
            labels: Vec<Label>,
            label_changes: Vec<LabelChange>,
        ) -> Self {
            let messages = messages
                .into_iter()
                .map(|env| SyncedMessage {
                    body: make_empty_body(&env.id),
                    envelope: env,
                    arrival: Default::default(),
                })
                .collect();
            Self {
                account_id,
                messages,
                labels,
                label_changes,
            }
        }
    }

    struct ThreadingProvider {
        account_id: AccountId,
        messages: Vec<SyncedMessage>,
    }

    /// Finite provider responses make an accidental repair loop fail instead
    /// of hanging the suite. Cursors describe the provider paging contract.
    struct LabelRepairProvider {
        account_id: AccountId,
        supports_labels: bool,
        labels: Vec<Label>,
        pages:
            std::sync::Mutex<std::collections::VecDeque<(SyncCursor, Result<SyncBatch, MxrError>)>>,
    }

    fn label_repair_page(messages: Vec<Envelope>, next_cursor: &[u8], has_more: bool) -> SyncBatch {
        SyncBatch {
            upserted: messages
                .into_iter()
                .map(|envelope| SyncedMessage {
                    body: make_empty_body(&envelope.id),
                    envelope,
                    arrival: Default::default(),
                })
                .collect(),
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor: SyncCursor::from_bytes(next_cursor.to_vec()),
            has_more,
            threads_changed: vec![],
            remaining_estimate: None,
        }
    }

    #[async_trait::async_trait]
    impl MailSyncProvider for LabelRepairProvider {
        fn name(&self) -> &str {
            "label-repair"
        }
        fn account_id(&self) -> &AccountId {
            &self.account_id
        }
        fn capabilities(&self) -> SyncCapabilities {
            let mut caps = SyncCapabilities::default();
            caps.sync.native_threading = true;
            caps.mutate.labels = self.supports_labels;
            caps
        }
        fn is_backfill_cursor(&self, cursor: &SyncCursor) -> bool {
            cursor.as_bytes().starts_with(b"page-")
        }
        async fn authenticate(&mut self) -> Result<(), MxrError> {
            Ok(())
        }
        async fn refresh_auth(&mut self) -> Result<(), MxrError> {
            Ok(())
        }
        async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
            Ok(self.labels.clone())
        }
        async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
            let (expected_cursor, response) =
                self.pages.lock().unwrap().pop_front().ok_or_else(|| {
                    MxrError::Provider("label repair exceeded fixture page budget".into())
                })?;
            assert_eq!(
                cursor.as_bytes(),
                expected_cursor.as_bytes(),
                "sync must preserve provider paging cursors"
            );
            response
        }
        async fn fetch_attachment(&self, _mid: &str, _aid: &str) -> Result<Vec<u8>, MxrError> {
            Err(MxrError::NotFound("no attachment".into()))
        }
        #[expect(
            clippy::panic,
            reason = "fixture fails if label recovery mutates provider mail"
        )]
        async fn apply_mutation(&self, _id: &str, _mutation: &Mutation) -> Result<(), MxrError> {
            panic!("label recovery must not mutate provider mail")
        }
    }

    struct RecoveringExpiredCursorProvider {
        account_id: AccountId,
        message: SyncedMessage,
        calls: std::sync::Mutex<Vec<SyncCursor>>,
    }

    #[async_trait::async_trait]
    impl MailSyncProvider for ThreadingProvider {
        fn name(&self) -> &str {
            "threading"
        }

        fn account_id(&self) -> &AccountId {
            &self.account_id
        }

        fn capabilities(&self) -> SyncCapabilities {
            SyncCapabilities::default()
        }

        async fn authenticate(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn refresh_auth(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
            Ok(vec![])
        }

        async fn sync_messages(&self, _cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
            Ok(SyncBatch {
                upserted: self.messages.clone(),
                deleted_provider_ids: vec![],
                label_changes: vec![],
                next_cursor: SyncCursor::empty(),
                has_more: false,
                threads_changed: vec![],
                remaining_estimate: None,
            })
        }

        async fn fetch_attachment(&self, _mid: &str, _aid: &str) -> Result<Vec<u8>, MxrError> {
            Err(MxrError::NotFound("no attachment".into()))
        }

        async fn apply_mutation(
            &self,
            _mutation_id: &str,
            _mutation: &Mutation,
        ) -> Result<(), MxrError> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl MailSyncProvider for RecoveringExpiredCursorProvider {
        fn name(&self) -> &str {
            "recovering-expired-cursor"
        }

        fn account_id(&self) -> &AccountId {
            &self.account_id
        }

        fn capabilities(&self) -> SyncCapabilities {
            SyncCapabilities {
                sync: SyncCaps {
                    delta: true,
                    native_threading: true,
                },
                ..Default::default()
            }
        }

        async fn authenticate(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn refresh_auth(&mut self) -> Result<(), MxrError> {
            Ok(())
        }

        async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
            Ok(vec![])
        }

        async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
            self.calls.lock().unwrap().push(cursor.clone());
            if cursor.is_empty() {
                Ok(SyncBatch {
                    upserted: vec![self.message.clone()],
                    deleted_provider_ids: vec![],
                    label_changes: vec![],
                    next_cursor: SyncCursor::from_bytes(b"recovered-cursor".to_vec()),
                    has_more: false,
                    threads_changed: vec![],
                    remaining_estimate: None,
                })
            } else {
                Err(MxrError::SyncCursorExpired {
                    reason: "test: history cursor past retention".into(),
                })
            }
        }

        async fn fetch_attachment(&self, _mid: &str, _aid: &str) -> Result<Vec<u8>, MxrError> {
            Err(MxrError::NotFound("no attachment".into()))
        }

        async fn apply_mutation(
            &self,
            _mutation_id: &str,
            _mutation: &Mutation,
        ) -> Result<(), MxrError> {
            Ok(())
        }
    }

    fn make_empty_body(message_id: &MessageId) -> MessageBody {
        MessageBody {
            message_id: message_id.clone(),
            text_plain: Some("test body".to_string()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    #[async_trait::async_trait]
    impl MailSyncProvider for DeltaLabelProvider {
        fn name(&self) -> &str {
            "delta-label"
        }
        fn account_id(&self) -> &AccountId {
            &self.account_id
        }
        fn capabilities(&self) -> SyncCapabilities {
            SyncCapabilities {
                sync: SyncCaps {
                    delta: true,
                    native_threading: true,
                },
                mutate: MutateCaps {
                    labels: true,
                    ..Default::default()
                },
                ..Default::default()
            }
        }
        async fn authenticate(&mut self) -> Result<(), MxrError> {
            Ok(())
        }
        async fn refresh_auth(&mut self) -> Result<(), MxrError> {
            Ok(())
        }
        async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
            Ok(self.labels.clone())
        }
        async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
            if cursor.is_empty() {
                Ok(SyncBatch {
                    upserted: self.messages.clone(),
                    deleted_provider_ids: vec![],
                    label_changes: vec![],
                    next_cursor: SyncCursor::from_bytes(b"delta-initial".to_vec()),
                    has_more: false,
                    threads_changed: vec![],
                    remaining_estimate: None,
                })
            } else {
                Ok(SyncBatch {
                    upserted: vec![],
                    deleted_provider_ids: vec![],
                    label_changes: self.label_changes.clone(),
                    next_cursor: SyncCursor::from_bytes(b"delta-follow-up".to_vec()),
                    has_more: false,
                    threads_changed: vec![],
                    remaining_estimate: None,
                })
            }
        }
        async fn fetch_attachment(&self, _mid: &str, _aid: &str) -> Result<Vec<u8>, MxrError> {
            Err(MxrError::NotFound("no attachment".into()))
        }
        async fn apply_mutation(
            &self,
            _mutation_id: &str,
            _mutation: &Mutation,
        ) -> Result<(), MxrError> {
            Ok(())
        }
    }

    fn make_test_label(account_id: &AccountId, name: &str, provider_id: &str) -> Label {
        Label {
            id: LabelId::new(),
            account_id: account_id.clone(),
            name: name.to_string(),
            kind: LabelKind::System,
            color: None,
            provider_id: provider_id.to_string(),
            unread_count: 0,
            total_count: 0,
            role: None,
        }
    }

    fn make_test_envelope(
        account_id: &AccountId,
        provider_id: &str,
        label_provider_ids: Vec<String>,
    ) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
            provider_id: provider_id.to_string(),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: mxr_core::Address {
                name: Some("Test".to_string()),
                email: "test@example.com".to_string(),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "Test message".to_string(),
            date: chrono::Utc::now(),
            flags: MessageFlags::empty(),
            snippet: "Test snippet".to_string(),
            has_attachments: false,
            size_bytes: 1000,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids,
            keywords: std::collections::BTreeSet::new(),
        }
    }

    /// Upserts its messages on the first sync, then reports the given
    /// provider ids deleted on every later one: a Gmail expunge or an IMAP
    /// `VANISHED`.
    struct ExpungingProvider {
        account_id: AccountId,
        messages: Vec<SyncedMessage>,
        expunged: Vec<String>,
    }

    #[async_trait::async_trait]
    impl MailSyncProvider for ExpungingProvider {
        fn name(&self) -> &str {
            "expunging"
        }
        fn account_id(&self) -> &AccountId {
            &self.account_id
        }
        fn capabilities(&self) -> SyncCapabilities {
            SyncCapabilities {
                sync: SyncCaps {
                    delta: true,
                    native_threading: true,
                },
                ..Default::default()
            }
        }
        async fn authenticate(&mut self) -> Result<(), MxrError> {
            Ok(())
        }
        async fn refresh_auth(&mut self) -> Result<(), MxrError> {
            Ok(())
        }
        async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
            Ok(vec![])
        }
        async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
            let initial = cursor.is_empty();
            Ok(SyncBatch {
                upserted: if initial {
                    self.messages.clone()
                } else {
                    vec![]
                },
                deleted_provider_ids: if initial {
                    vec![]
                } else {
                    self.expunged.clone()
                },
                label_changes: vec![],
                next_cursor: SyncCursor::from_bytes(b"expunging".to_vec()),
                has_more: false,
                threads_changed: vec![],
                remaining_estimate: None,
            })
        }
        async fn fetch_attachment(&self, _mid: &str, _aid: &str) -> Result<Vec<u8>, MxrError> {
            Err(MxrError::NotFound("no attachment".into()))
        }
        async fn apply_mutation(
            &self,
            _mutation_id: &str,
            _mutation: &Mutation,
        ) -> Result<(), MxrError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_delete_only_pass_reports_the_deleted_mail_and_the_threads_it_changed() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();
        let engine = SyncEngine::with_address_lookup(
            store.clone(),
            in_memory_search(),
            Arc::new(LoadedAddressLookup::for_account(
                account_id.clone(),
                ["user@example.com"],
            )),
        );
        let shared_thread = ThreadId::new();
        let gone = Envelope {
            thread_id: shared_thread.clone(),
            ..make_test_envelope(&account_id, "gone", vec![])
        };
        let kept = Envelope {
            thread_id: shared_thread.clone(),
            ..make_test_envelope(&account_id, "kept", vec![])
        };
        let alone = make_test_envelope(&account_id, "alone", vec![]);
        let provider = ExpungingProvider {
            account_id: account_id.clone(),
            messages: [&gone, &kept, &alone]
                .into_iter()
                .map(|envelope| SyncedMessage {
                    envelope: envelope.clone(),
                    body: make_empty_body(&envelope.id),
                    arrival: Default::default(),
                })
                .collect(),
            expunged: vec!["gone".to_string(), "alone".to_string()],
        };
        engine.sync_account_with_outcome(&provider).await.unwrap();

        let outcome = engine.sync_account_with_outcome(&provider).await.unwrap();

        assert_eq!(outcome.synced_count, 0);
        let deleted: HashSet<_> = outcome.deleted.message_ids.iter().cloned().collect();
        assert_eq!(deleted, HashSet::from([gone.id.clone(), alone.id.clone()]));
        assert!(store.get_envelope(&kept.id).await.unwrap().is_some());
        let threads: std::collections::HashMap<_, _> = outcome
            .threads_changed
            .iter()
            .map(|thread| (thread.id.clone(), thread.message_ids.clone()))
            .collect();
        assert_eq!(threads.get(&shared_thread), Some(&vec![kept.id.clone()]));
        assert_eq!(
            threads.get(&alone.thread_id),
            Some(&vec![]),
            "an emptied thread is reported as a tombstone"
        );
        assert_eq!(
            outcome
                .deleted
                .counterparties
                .iter()
                .map(|(_, email)| email.as_str())
                .collect::<Vec<_>>(),
            vec!["test@example.com"]
        );
    }

    #[tokio::test]
    async fn sync_ingest_applies_deny_screener_decision() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let account_id = AccountId::new();
        let engine = SyncEngine::with_address_lookup(
            store.clone(),
            search,
            Arc::new(LoadedAddressLookup::for_account(
                account_id.clone(),
                ["user@example.com"],
            )),
        );

        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();
        let inbox = make_test_label(&account_id, "Inbox", "INBOX");
        let trash = make_test_label(&account_id, "Trash", "TRASH");
        let labels = vec![inbox.clone(), trash.clone()];
        store
            .set_screener_decision(&ScreenerDecision {
                account_id: account_id.clone(),
                sender_email: "test@example.com".into(),
                disposition: ScreenerDisposition::Deny,
                route_label: None,
                decided_at: chrono::Utc::now(),
            })
            .await
            .unwrap();

        let mut msg = make_test_envelope(&account_id, "denied-msg", vec!["INBOX".to_string()]);
        msg.flags.remove(MessageFlags::READ);
        let provider = DeltaLabelProvider::new(account_id.clone(), vec![msg], labels, vec![]);

        engine.sync_account(&provider).await.unwrap();

        let msg_id = store
            .get_message_id_by_provider_id(&account_id, "denied-msg")
            .await
            .unwrap()
            .unwrap();
        let envelope = store.get_envelope(&msg_id).await.unwrap().unwrap();
        let label_ids = store.get_message_label_ids(&msg_id).await.unwrap();
        assert!(envelope.flags.contains(MessageFlags::READ));
        assert!(!label_ids.contains(&inbox.id));
        assert!(label_ids.contains(&trash.id));
    }

    #[tokio::test]
    async fn provider_sent_mail_from_an_unowned_sender_is_outbound() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let account_id = AccountId::new();
        let engine = SyncEngine::with_address_lookup(
            store.clone(),
            search,
            Arc::new(LoadedAddressLookup::for_account(
                account_id.clone(),
                ["me@x.com"],
            )),
        );
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();
        let inbox = make_test_label(&account_id, "Inbox", "INBOX");

        let mut sent = make_test_envelope(&account_id, "sent-as-alias", vec!["SENT".to_string()]);
        sent.from.email = "alias@send-as.example".to_string();
        sent.flags = MessageFlags::SENT;
        let mut received = make_test_envelope(&account_id, "received", vec!["INBOX".to_string()]);
        received.from.email = "alias@send-as.example".to_string();

        let provider = DeltaLabelProvider::new(
            account_id.clone(),
            vec![sent, received],
            vec![inbox],
            vec![],
        );
        engine.sync_account(&provider).await.unwrap();

        let sent_id = store
            .get_message_id_by_provider_id(&account_id, "sent-as-alias")
            .await
            .unwrap()
            .unwrap();
        let received_id = store
            .get_message_id_by_provider_id(&account_id, "received")
            .await
            .unwrap()
            .unwrap();
        let directions = store
            .list_message_directions_by_ids(&[sent_id.clone(), received_id.clone()])
            .await
            .unwrap();

        assert_eq!(directions.get(&sent_id), Some(&MessageDirection::Outbound));
        assert_eq!(
            directions.get(&received_id),
            Some(&MessageDirection::Inbound),
            "without the SENT flag an unowned sender is still inbound"
        );
    }

    #[tokio::test]
    async fn delta_sync_applies_label_additions() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search.clone());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();

        let inbox = make_test_label(&account_id, "Inbox", "INBOX");
        let starred = make_test_label(&account_id, "Starred", "STARRED");
        let labels = vec![inbox.clone(), starred.clone()];

        let msg = make_test_envelope(&account_id, "prov-msg-1", vec!["INBOX".to_string()]);
        let msg_provider_id = msg.provider_id.clone();

        let provider = DeltaLabelProvider::new(
            account_id.clone(),
            vec![msg],
            labels,
            vec![LabelChange {
                provider_message_id: msg_provider_id.clone(),
                added_labels: vec!["STARRED".to_string()],
                removed_labels: vec![],
            }],
        );

        engine.sync_account(&provider).await.unwrap();

        let msg_id = store
            .get_message_id_by_provider_id(&account_id, &msg_provider_id)
            .await
            .unwrap()
            .unwrap();
        let labels_before = store.get_message_label_ids(&msg_id).await.unwrap();
        assert!(labels_before.contains(&inbox.id));
        assert!(!labels_before.contains(&starred.id));

        engine.sync_account(&provider).await.unwrap();

        let labels_after = store.get_message_label_ids(&msg_id).await.unwrap();
        assert!(
            labels_after.contains(&inbox.id),
            "INBOX should still be present"
        );
        assert!(
            labels_after.contains(&starred.id),
            "STARRED should be added by delta"
        );
    }

    #[tokio::test]
    async fn sync_rethreads_messages_when_provider_lacks_native_thread_ids() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search);

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();

        let first_id = MessageId::new();
        let second_id = MessageId::new();
        let first = SyncedMessage {
            envelope: Envelope {
                id: first_id.clone(),
                account_id: account_id.clone(),
                provider_id: "prov-thread-1".into(),
                thread_id: ThreadId::new(),
                message_id_header: Some("<root@example.com>".into()),
                in_reply_to: None,
                references: vec![],
                from: mxr_core::Address {
                    name: Some("Alice".into()),
                    email: "alice@example.com".into(),
                },
                to: vec![],
                cc: vec![],
                bcc: vec![],
                subject: "Topic".into(),
                date: chrono::Utc::now() - chrono::Duration::minutes(5),
                flags: MessageFlags::empty(),
                snippet: "first".into(),
                has_attachments: false,
                size_bytes: 100,
                unsubscribe: UnsubscribeMethod::None,
                link_count: 0,
                body_word_count: 0,
                label_provider_ids: vec![],
                keywords: std::collections::BTreeSet::new(),
            },
            body: make_empty_body(&first_id),
            arrival: Default::default(),
        };
        let second = SyncedMessage {
            envelope: Envelope {
                id: second_id.clone(),
                account_id: account_id.clone(),
                provider_id: "prov-thread-2".into(),
                thread_id: ThreadId::new(),
                message_id_header: Some("<reply@example.com>".into()),
                in_reply_to: Some("<root@example.com>".into()),
                references: vec!["<root@example.com>".into()],
                from: mxr_core::Address {
                    name: Some("Bob".into()),
                    email: "bob@example.com".into(),
                },
                to: vec![],
                cc: vec![],
                bcc: vec![],
                subject: "Re: Topic".into(),
                date: chrono::Utc::now(),
                flags: MessageFlags::empty(),
                snippet: "second".into(),
                has_attachments: false,
                size_bytes: 100,
                unsubscribe: UnsubscribeMethod::None,
                link_count: 0,
                body_word_count: 0,
                label_provider_ids: vec![],
                keywords: std::collections::BTreeSet::new(),
            },
            body: make_empty_body(&second_id),
            arrival: Default::default(),
        };

        let provider = ThreadingProvider {
            account_id: account_id.clone(),
            messages: vec![first, second],
        };

        let outcome = engine.sync_account_with_outcome(&provider).await.unwrap();

        let first_env = store.get_envelope(&first_id).await.unwrap().unwrap();
        let second_env = store.get_envelope(&second_id).await.unwrap().unwrap();
        assert_eq!(first_env.thread_id, second_env.thread_id);

        let thread = store
            .get_thread(&first_env.thread_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(thread.message_count, 2);

        let survivor_id = first_env.thread_id.clone();
        let survivor = outcome
            .threads_changed
            .iter()
            .find(|t| t.id == survivor_id)
            .expect("survivor thread should be in threads_changed");
        assert_eq!(survivor.message_ids.len(), 2);
        assert!(survivor.message_ids.contains(&first_id));
        assert!(survivor.message_ids.contains(&second_id));

        let tombstone = outcome
            .threads_changed
            .iter()
            .find(|t| t.id != survivor_id)
            .expect("tombstoned (loser) thread should be in threads_changed");
        assert!(
            tombstone.message_ids.is_empty(),
            "loser thread should be tombstoned"
        );
        assert_eq!(tombstone.message_count, 0);
    }

    #[tokio::test]
    async fn sync_populates_store_and_search() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search.clone());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();

        let provider = mxr_provider_fake::FakeProvider::new(account_id.clone());
        let count = engine.sync_account(&provider).await.unwrap();
        assert_eq!(count, 55);

        let envelopes = store
            .list_envelopes_by_account(&account_id, 100, 0)
            .await
            .unwrap();
        assert_eq!(envelopes.len(), 55);

        let results = search
            .search("deployment", 10, 0, SortOrder::DateDesc)
            .await
            .unwrap();
        assert!(!results.has_more);
        assert!(!results.results.is_empty());
        assert!(results.results.len() <= 10);
        assert!(results.results.iter().all(|r| r.score >= 0.0));
    }

    /// A paged provider must land every message exactly once, with its
    /// body, labels, keywords, and lexical index entry — the engine drives
    /// one page per call and the caller re-polls while `has_more`.
    #[tokio::test]
    async fn paged_sync_stores_every_page_once() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search.clone());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();

        const TOTAL: usize = 250;
        let provider =
            mxr_provider_fake::FakeProvider::with_demo_dataset(account_id.clone(), TOTAL)
                .with_page_size(100);

        let mut pages = 0;
        let mut synced = 0u32;
        loop {
            let outcome = engine.sync_account_with_outcome(&provider).await.unwrap();
            pages += 1;
            synced += outcome.synced_count;
            assert!(pages <= 5, "paging should terminate");
            if !outcome.has_more {
                break;
            }
        }

        assert_eq!(pages, 3);
        assert_eq!(synced as usize, TOTAL);
        assert_eq!(
            store.count_messages_by_account(&account_id).await.unwrap() as usize,
            TOTAL
        );
        assert_eq!(search.num_docs().await.unwrap() as usize, TOTAL);

        let envelopes = store
            .list_envelopes_by_account(&account_id, TOTAL as u32, 0)
            .await
            .unwrap();
        let last = envelopes
            .iter()
            .find(|envelope| envelope.provider_id == format!("demo-msg-{TOTAL}"))
            .expect("last message stored");
        assert!(store.get_body(&last.id).await.unwrap().is_some());
        assert!(store
            .get_message_keywords(&last.id)
            .await
            .unwrap()
            .contains("$Forwarded"));
        assert!(!last.label_provider_ids.is_empty());

        let steady = engine.sync_account_with_outcome(&provider).await.unwrap();
        assert_eq!(steady.synced_count, 0);
        assert!(!steady.has_more);
        assert_eq!(
            store.count_messages_by_account(&account_id).await.unwrap() as usize,
            TOTAL
        );
    }

    /// Both demo accounts sync into one store. Attachment rows are keyed by
    /// `attachments.id`, and the body write is a delete-then-`INSERT OR
    /// REPLACE`, so an id that repeats across accounts silently moves the
    /// first account's attachment row onto the second account's message —
    /// the first account just loses it. Counting per account against what
    /// the provider delivered is what catches that; reading a message's
    /// attachments back can't, since the query filters by message id.
    #[tokio::test]
    async fn attachments_stay_with_their_own_account_when_both_demo_accounts_sync() {
        const MESSAGES: usize = 400;
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search);

        let personal = AccountId::from_provider_id("fake", "alex@demo.mxr.local");
        let work = AccountId::from_provider_id("fake", "alex@work.demo.mxr.local");
        let mut expected = Vec::new();
        for account_id in [personal.clone(), work.clone()] {
            store
                .insert_account(&test_account(account_id.clone()))
                .await
                .unwrap();
            let provider =
                mxr_provider_fake::FakeProvider::with_demo_dataset(account_id.clone(), MESSAGES);
            let delivered =
                mxr_core::MailSyncProvider::sync_messages(&provider, &SyncCursor::empty())
                    .await
                    .unwrap()
                    .upserted
                    .iter()
                    .map(|synced| synced.body.attachments.len())
                    .sum::<usize>();
            engine.sync_account(&provider).await.unwrap();
            expected.push(delivered);
        }

        for (account_id, delivered) in [personal, work].into_iter().zip(expected) {
            let envelopes = store
                .list_envelopes_by_account(&account_id, MESSAGES as u32, 0)
                .await
                .unwrap();
            let mut stored = 0;
            for envelope in &envelopes {
                let Some(body) = store.get_body(&envelope.id).await.unwrap() else {
                    continue;
                };
                stored += body.attachments.len();
            }
            assert!(delivered > 0, "the fixture should produce attachments");
            assert_eq!(
                stored, delivered,
                "account {account_id} lost attachment rows to the other account"
            );
        }
    }

    /// Resuming from a persisted mid-backfill cursor continues where the
    /// last page stopped instead of restarting or skipping.
    #[tokio::test]
    async fn paged_sync_resumes_from_persisted_cursor() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();

        const TOTAL: usize = 250;
        let provider =
            mxr_provider_fake::FakeProvider::with_demo_dataset(account_id.clone(), TOTAL)
                .with_page_size(100);

        let first = SyncEngine::new(store.clone(), search.clone());
        let outcome = first.sync_account_with_outcome(&provider).await.unwrap();
        assert!(outcome.has_more);
        assert_eq!(
            store.count_messages_by_account(&account_id).await.unwrap(),
            100
        );

        let resumed = SyncEngine::new(store.clone(), search.clone());
        loop {
            let outcome = resumed.sync_account_with_outcome(&provider).await.unwrap();
            if !outcome.has_more {
                break;
            }
        }
        assert_eq!(
            store.count_messages_by_account(&account_id).await.unwrap() as usize,
            TOTAL
        );
    }

    #[tokio::test]
    async fn snooze_wake() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search.clone());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();

        let provider = mxr_provider_fake::FakeProvider::new(account_id.clone());
        engine.sync_account(&provider).await.unwrap();

        let envelopes = store
            .list_envelopes_by_account(&account_id, 1, 0)
            .await
            .unwrap();

        let snoozed = Snoozed {
            message_id: envelopes[0].id.clone(),
            account_id: account_id.clone(),
            snoozed_at: chrono::Utc::now(),
            wake_at: chrono::Utc::now() - chrono::Duration::hours(1),
            original_labels: vec![],
        };
        store.insert_snooze(&snoozed).await.unwrap();

        let woken = engine.check_snoozes().await.unwrap();
        assert_eq!(woken.len(), 1);

        let woken2 = engine.check_snoozes().await.unwrap();
        assert_eq!(woken2.len(), 0);
    }

    #[tokio::test]
    async fn sync_error_does_not_crash() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search.clone());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();

        let error_provider = ErrorProvider {
            account_id: account_id.clone(),
        };

        let result = engine.sync_account(&error_provider).await;
        assert!(
            result.is_err(),
            "sync_account should return Err for failing provider"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("simulated"),
            "Error should contain provider message, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn delta_sync_no_duplicate_labels() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search.clone());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();

        let provider = mxr_provider_fake::FakeProvider::new(account_id.clone());

        engine.sync_account(&provider).await.unwrap();

        let labels_after_first = store.list_labels_by_account(&account_id).await.unwrap();
        let label_count_first = labels_after_first.len();

        let delta_count = engine.sync_account(&provider).await.unwrap();
        assert_eq!(delta_count, 0, "Delta sync should return 0 new messages");

        let labels_after_second = store.list_labels_by_account(&account_id).await.unwrap();

        assert_eq!(
            label_count_first,
            labels_after_second.len(),
            "Label count should not change after delta sync"
        );

        for label in &labels_after_first {
            let still_exists = labels_after_second
                .iter()
                .any(|l| l.provider_id == label.provider_id && l.name == label.name);
            assert!(
                still_exists,
                "Label '{}' (provider_id='{}') should survive delta sync",
                label.name, label.provider_id
            );
        }

        let envelopes = store
            .list_envelopes_by_account(&account_id, 200, 0)
            .await
            .unwrap();
        assert_eq!(
            envelopes.len(),
            55,
            "All 55 messages should survive delta sync"
        );
    }

    #[tokio::test]
    async fn label_repair_recovers_one_account_without_touching_another() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search.clone());
        let account_a = AccountId::new();
        let account_b = AccountId::new();
        let label_a = make_test_label(&account_a, "Inbox", "INBOX");
        let label_b = make_test_label(&account_b, "Inbox", "INBOX");
        let message_a = make_test_envelope(&account_a, "a", vec!["INBOX".into()]);
        let message_b = make_test_envelope(&account_b, "b", vec!["INBOX".into()]);
        let provider_a = DeltaLabelProvider::new(
            account_a.clone(),
            vec![message_a.clone()],
            vec![label_a.clone()],
            vec![],
        );
        let provider_b = DeltaLabelProvider::new(
            account_b.clone(),
            vec![message_b.clone()],
            vec![label_b.clone()],
            vec![],
        );
        for account_id in [&account_b, &account_a] {
            store
                .insert_account(&test_account(account_id.clone()))
                .await
                .unwrap();
        }
        engine.sync_account(&provider_b).await.unwrap();
        engine.sync_account(&provider_a).await.unwrap();
        let cursor_b = store.get_sync_cursor(&account_b).await.unwrap().unwrap();
        let labels_b = store.get_message_label_ids(&message_b.id).await.unwrap();
        assert_eq!(labels_b, vec![label_b.id.clone()]);

        sqlx::query("DELETE FROM message_labels WHERE message_id IN (SELECT id FROM messages WHERE account_id = ?)")
            .bind(account_a.as_str())
            .execute(store.writer()).await.unwrap();
        assert!(store
            .list_envelopes_by_label(&label_a.id, 10, 0)
            .await
            .unwrap()
            .is_empty());
        let progress = CollectedProgress::default();
        let outcome = engine
            .sync_account_reporting(&provider_a, &progress.sink())
            .await
            .unwrap();

        let restored = store
            .list_envelopes_by_label(&label_a.id, 10, 0)
            .await
            .unwrap();
        assert_eq!(
            restored.iter().map(|env| &env.id).collect::<Vec<_>>(),
            vec![&message_a.id]
        );
        assert_eq!(outcome.upserted_message_ids, vec![message_a.id.clone()]);
        assert_eq!(
            progress
                .milestones
                .lock()
                .unwrap()
                .iter()
                .filter(|step| **step == SyncProgress::Restarted)
                .count(),
            1
        );
        assert_eq!(
            store.get_message_label_ids(&message_b.id).await.unwrap(),
            labels_b
        );
        assert_eq!(
            store
                .get_sync_cursor(&account_b)
                .await
                .unwrap()
                .unwrap()
                .as_bytes(),
            cursor_b.as_bytes()
        );
        for (account, message) in [(&account_a, &message_a), (&account_b, &message_b)] {
            let page = search
                .search_in_account(
                    "body_text:test",
                    Some(&account.as_str()),
                    10,
                    0,
                    SortOrder::Relevance,
                )
                .await
                .unwrap();
            assert_eq!(page.total, 1);
            assert_eq!(page.results[0].message_id, message.id.as_str());
            assert_eq!(store.count_messages_by_account(account).await.unwrap(), 1);
            assert!(store.get_body(&message.id).await.unwrap().is_some());
        }
    }

    #[tokio::test]
    async fn label_repair_finishes_unresolvable_backfill_and_allows_later_retry() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search.clone());
        let account = AccountId::new();
        store
            .insert_account(&test_account(account.clone()))
            .await
            .unwrap();
        let first = make_test_envelope(&account, "first", vec!["missing-label".into()]);
        let last = make_test_envelope(&account, "last", vec!["missing-label".into()]);
        store.upsert_envelope(&first).await.unwrap();
        store
            .set_sync_cursor(&account, &SyncCursor::from_bytes(b"stale-delta".to_vec()))
            .await
            .unwrap();
        let provider = LabelRepairProvider {
            account_id: account.clone(),
            supports_labels: true,
            labels: vec![make_test_label(&account, "Inbox", "INBOX")],
            pages: std::sync::Mutex::new(std::collections::VecDeque::new()),
        };

        for start in [b"stale-delta".as_slice(), b"settled".as_slice()] {
            provider.pages.lock().unwrap().extend([
                (
                    SyncCursor::from_bytes(start.to_vec()),
                    Ok(label_repair_page(vec![], b"delta", false)),
                ),
                (
                    SyncCursor::empty(),
                    Ok(label_repair_page(vec![first.clone()], b"page-final", true)),
                ),
                (
                    SyncCursor::from_bytes(b"page-final".to_vec()),
                    Ok(label_repair_page(vec![last.clone()], b"settled", false)),
                ),
            ]);
            let progress = CollectedProgress::default();
            let mut upserted = Vec::new();
            let mut passes = 0;
            loop {
                let outcome = engine
                    .sync_account_reporting(&provider, &progress.sink())
                    .await
                    .unwrap();
                passes += 1;
                upserted.extend(outcome.upserted_message_ids);
                if !outcome.has_more {
                    break;
                }
            }
            assert_eq!(passes, 2);
            assert_eq!(upserted, vec![first.id.clone(), last.id.clone()]);
            assert_eq!(
                progress
                    .milestones
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|step| **step == SyncProgress::Restarted)
                    .count(),
                1
            );
            assert_eq!(
                store
                    .get_sync_cursor(&account)
                    .await
                    .unwrap()
                    .unwrap()
                    .as_bytes(),
                b"settled"
            );
            assert_eq!(
                store
                    .count_message_labels_by_account(&account)
                    .await
                    .unwrap(),
                0
            );
            assert_eq!(store.count_messages_by_account(&account).await.unwrap(), 2);
            let page = search
                .search_in_account(
                    "body_text:test",
                    Some(&account.as_str()),
                    10,
                    0,
                    SortOrder::Relevance,
                )
                .await
                .unwrap();
            assert_eq!(page.total, 2);
            assert_eq!(
                page.results
                    .into_iter()
                    .map(|hit| hit.message_id)
                    .collect::<HashSet<_>>(),
                HashSet::from([first.id.as_str(), last.id.as_str()])
            );
            for message in [&first, &last] {
                assert_eq!(
                    store
                        .get_message_id_by_provider_id(&account, &message.provider_id)
                        .await
                        .unwrap(),
                    Some(message.id.clone())
                );
                assert!(store.get_body(&message.id).await.unwrap().is_some());
            }
        }
    }

    #[tokio::test]
    async fn expired_sync_cursor_resets_to_initial_and_recovers() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search.clone());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();
        store
            .set_sync_cursor(
                &account_id,
                &SyncCursor::from_bytes(b"stale-history".to_vec()),
            )
            .await
            .unwrap();

        let message_id = MessageId::new();
        let provider = RecoveringExpiredCursorProvider {
            account_id: account_id.clone(),
            message: SyncedMessage {
                envelope: Envelope {
                    id: message_id.clone(),
                    account_id: account_id.clone(),
                    provider_id: "recovered-1".into(),
                    thread_id: ThreadId::new(),
                    message_id_header: None,
                    in_reply_to: None,
                    references: vec![],
                    from: Address {
                        name: Some("Recovered".into()),
                        email: "recovered@example.com".into(),
                    },
                    to: vec![],
                    cc: vec![],
                    bcc: vec![],
                    subject: "Recovered after cursor reset".into(),
                    date: chrono::Utc::now(),
                    flags: MessageFlags::empty(),
                    snippet: "Recovered".into(),
                    has_attachments: false,
                    size_bytes: 42,
                    unsubscribe: UnsubscribeMethod::None,
                    link_count: 0,
                    body_word_count: 0,
                    label_provider_ids: vec![],
                    keywords: std::collections::BTreeSet::new(),
                },
                body: make_empty_body(&message_id),
                arrival: Default::default(),
            },
            calls: std::sync::Mutex::new(Vec::new()),
        };

        let outcome = engine.sync_account_with_outcome(&provider).await.unwrap();

        assert_eq!(outcome.synced_count, 1);
        {
            let calls = provider.calls.lock().unwrap();
            assert_eq!(calls.len(), 2);
            assert_eq!(calls[0].as_bytes(), b"stale-history");
            assert!(calls[1].is_empty(), "recovery dispatches an empty cursor");
        }
        let stored_cursor = store.get_sync_cursor(&account_id).await.unwrap();
        assert_eq!(
            stored_cursor.as_ref().map(SyncCursor::as_bytes),
            Some(b"recovered-cursor".as_slice())
        );
        assert!(
            store.initial_sync_done(&account_id).await.unwrap(),
            "the last page of a first sync marks it done"
        );
    }

    /// A message that already exists under a different `MessageId` — the
    /// pre-0.4.52 unscoped derivation — must not split in two. The natural
    /// key `(account_id, provider_id)` is unique, so the stored id wins and
    /// the whole page has to follow it: rows, the ids the engine hands back,
    /// and the lexical index, which is keyed by message id and would
    /// otherwise end up with two documents for one message.
    #[tokio::test]
    async fn a_page_containing_a_legacy_row_lands_under_the_stored_id() {
        const TOTAL: usize = 3;
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = in_memory_search();
        let engine = SyncEngine::new(store.clone(), search.clone());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();
        let provider =
            mxr_provider_fake::FakeProvider::with_demo_dataset(account_id.clone(), TOTAL);

        let legacy_message = provider
            .fetch_message("demo-msg-2")
            .await
            .unwrap()
            .expect("demo dataset message");
        let derived_id = legacy_message.envelope.id.clone();
        let legacy_id = MessageId::from_provider_id("fake", "demo-msg-2");
        assert_ne!(legacy_id, derived_id, "the two derivations must differ");
        let mut legacy_envelope = legacy_message.envelope;
        let current_subject = legacy_envelope.subject.clone();
        legacy_envelope.id = legacy_id.clone();
        legacy_envelope.subject = "Oldreleasesubjecttoken".to_string();
        let mut legacy_body = legacy_message.body;
        legacy_body.message_id = legacy_id.clone();
        for attachment in &mut legacy_body.attachments {
            attachment.message_id = legacy_id.clone();
        }
        store
            .apply_sync_upserts(&mut [mxr_store::SyncUpsert {
                envelope: legacy_envelope.clone(),
                direction: MessageDirection::Inbound,
                body: legacy_body.clone(),
                label_ids: Vec::new(),
                arrival: Default::default(),
            }])
            .await
            .unwrap();
        search
            .apply_batch(mxr_search::SearchUpdateBatch {
                entries: vec![mxr_search::SearchIndexEntry {
                    envelope: legacy_envelope,
                    body: Some(legacy_body),
                    reply_later: false,
                }],
                removed_message_ids: Vec::new(),
            })
            .await
            .unwrap();

        let outcome = engine.sync_account_with_outcome(&provider).await.unwrap();

        assert_eq!(outcome.synced_count as usize, TOTAL);
        assert_eq!(
            store.count_messages_by_account(&account_id).await.unwrap() as usize,
            TOTAL,
            "the legacy row must be updated, not duplicated"
        );
        assert!(
            outcome.upserted_message_ids.contains(&legacy_id),
            "the engine must report the id the row is stored under"
        );
        assert!(
            !outcome.upserted_message_ids.contains(&derived_id),
            "the id the provider computed no longer names any row"
        );

        assert!(store.get_envelope(&derived_id).await.unwrap().is_none());
        let stored = store
            .get_envelope(&legacy_id)
            .await
            .unwrap()
            .expect("legacy row");
        assert_eq!(stored.subject, current_subject);
        assert!(store.get_body(&legacy_id).await.unwrap().is_some());
        assert!(store.get_body(&derived_id).await.unwrap().is_none());

        assert_eq!(search.num_docs().await.unwrap() as usize, TOTAL);
        let stale = search
            .search("oldreleasesubjecttoken", 10, 0, SortOrder::DateDesc)
            .await
            .unwrap();
        assert!(
            stale.results.is_empty(),
            "the stale document must be replaced, not left alongside the new one"
        );
    }

    /// A stored message the provider now puts in another thread leaves its
    /// old thread behind. Both ends have to reach the client: the thread the
    /// message joined, and the one it left — tombstoned when the move emptied
    /// it — or a client keeps showing a thread that no longer has the mail.
    #[tokio::test]
    async fn a_message_moved_between_threads_reports_both_threads() {
        const TOTAL: usize = 3;
        let store = Arc::new(Store::in_memory().await.unwrap());
        let engine = SyncEngine::new(store.clone(), in_memory_search());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();
        let provider =
            mxr_provider_fake::FakeProvider::with_demo_dataset(account_id.clone(), TOTAL);

        let synced = provider
            .fetch_message("demo-msg-2")
            .await
            .unwrap()
            .expect("demo dataset message");
        let provider_thread = synced.envelope.thread_id.clone();
        let old_thread = ThreadId::from_provider_id("fake", "thread-before-the-move");
        assert_ne!(old_thread, provider_thread);
        let mut stale = synced.envelope;
        stale.thread_id = old_thread.clone();
        store
            .apply_sync_upserts(&mut [mxr_store::SyncUpsert {
                envelope: stale,
                direction: MessageDirection::Inbound,
                body: synced.body,
                label_ids: Vec::new(),
                arrival: Default::default(),
            }])
            .await
            .unwrap();

        let outcome = engine.sync_account_with_outcome(&provider).await.unwrap();

        let changed: HashSet<ThreadId> = outcome
            .threads_changed
            .iter()
            .map(|thread| thread.id.clone())
            .collect();
        assert!(
            changed.contains(&provider_thread),
            "the thread the message moved into must be reported"
        );
        assert!(
            changed.contains(&old_thread),
            "the thread it left must be reported too"
        );
        let tombstone = outcome
            .threads_changed
            .iter()
            .find(|thread| thread.id == old_thread)
            .expect("vacated thread");
        assert!(
            tombstone.message_ids.is_empty(),
            "the move emptied it, so it comes back as a tombstone"
        );
    }
}
