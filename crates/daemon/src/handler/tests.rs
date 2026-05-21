use super::*;
use async_trait::async_trait;
use chrono::TimeZone;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FolderCopyReanchorMode {
    Normal,
    MissingAfterArchive,
}

struct FolderCopyProvider {
    account_id: mxr_core::AccountId,
    reanchor_mode: FolderCopyReanchorMode,
    folders: StdMutex<Vec<String>>,
    last_synced_provider_ids: StdMutex<Vec<String>>,
}

impl FolderCopyProvider {
    fn with_reanchor_mode(
        account_id: mxr_core::AccountId,
        reanchor_mode: FolderCopyReanchorMode,
    ) -> Self {
        Self {
            account_id,
            reanchor_mode,
            folders: StdMutex::new(vec!["INBOX".to_string()]),
            last_synced_provider_ids: StdMutex::new(Vec::new()),
        }
    }

    fn current_provider_ids(&self) -> Vec<String> {
        self.folders
            .lock()
            .unwrap()
            .iter()
            .map(|folder| format!("{folder}:1"))
            .collect()
    }

    fn synced_messages(&self) -> Vec<mxr_core::SyncedMessage> {
        self.folders
            .lock()
            .unwrap()
            .iter()
            .map(|folder| {
                let provider_id = format!("{folder}:1");
                let message_id = mxr_core::MessageId::from_provider_id("folder-copy", &provider_id);
                let envelope = mxr_core::Envelope {
                    id: message_id.clone(),
                    account_id: self.account_id.clone(),
                    provider_id,
                    thread_id: mxr_core::ThreadId::from_provider_id("folder-copy", "thread-1"),
                    message_id_header: Some("<folder-copy@example.com>".to_string()),
                    in_reply_to: None,
                    references: vec![],
                    from: mxr_core::Address {
                        name: Some("Folder Provider".to_string()),
                        email: "folder-provider@example.com".to_string(),
                    },
                    to: vec![mxr_core::Address {
                        name: Some("Receiver".to_string()),
                        email: "receiver@example.com".to_string(),
                    }],
                    cc: vec![],
                    bcc: vec![],
                    subject: "Folder-backed message".to_string(),
                    date: chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                    flags: mxr_core::MessageFlags::READ,
                    snippet: format!("copy in {folder}"),
                    has_attachments: false,
                    size_bytes: 128,
                    unsubscribe: mxr_core::UnsubscribeMethod::None,
                    link_count: 0,
                    body_word_count: 0,
                    label_provider_ids: vec![folder.clone()],
                    keywords: std::collections::BTreeSet::new(),
                };
                let body = mxr_core::MessageBody {
                    message_id,
                    text_plain: Some(format!("body in {folder}")),
                    text_html: None,
                    attachments: vec![],
                    fetched_at: chrono::Utc::now(),
                    metadata: mxr_core::MessageMetadata::default(),
                };
                mxr_core::SyncedMessage { envelope, body }
            })
            .collect()
    }

    fn sync_labels_for_account(&self) -> Vec<mxr_core::Label> {
        let folders = self.folders.lock().unwrap().clone();
        ["INBOX", "Archive"]
            .into_iter()
            .map(|name| {
                let kind = if name == "INBOX" {
                    mxr_core::LabelKind::System
                } else {
                    mxr_core::LabelKind::Folder
                };
                let count = folders
                    .iter()
                    .filter(|folder| folder.eq_ignore_ascii_case(name))
                    .count() as u32;
                mxr_core::Label {
                    id: mxr_core::LabelId::from_provider_id("folder-copy", name),
                    account_id: self.account_id.clone(),
                    name: name.to_string(),
                    kind,
                    color: None,
                    provider_id: name.to_string(),
                    unread_count: 0,
                    total_count: count,
                    role: None,
                }
            })
            .collect()
    }
}

struct FailingSendProvider {
    message: &'static str,
}

struct UnsupportedServerDraftProvider;

struct FailingSyncProvider {
    account_id: mxr_core::AccountId,
    message: &'static str,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl mxr_core::MailSendProvider for FailingSendProvider {
    fn name(&self) -> &str {
        "failing-send"
    }

    async fn send(
        &self,
        _draft: &mxr_core::Draft,
        _from: &mxr_core::Address,
        _rfc2822_message_id: &str,
    ) -> Result<mxr_core::SendReceipt, mxr_core::MxrError> {
        Err(mxr_core::MxrError::Provider(self.message.to_string()))
    }
}

#[async_trait]
impl mxr_core::MailSendProvider for UnsupportedServerDraftProvider {
    fn name(&self) -> &str {
        "unsupported-server-draft"
    }

    async fn send(
        &self,
        _draft: &mxr_core::Draft,
        _from: &mxr_core::Address,
        _rfc2822_message_id: &str,
    ) -> Result<mxr_core::SendReceipt, mxr_core::MxrError> {
        unreachable!("save_draft_to_server fallback test must not send")
    }
}

#[async_trait]
impl mxr_core::MailSyncProvider for FailingSyncProvider {
    fn name(&self) -> &str {
        "failing-sync"
    }

    fn account_id(&self) -> &mxr_core::AccountId {
        &self.account_id
    }

    fn capabilities(&self) -> mxr_core::SyncCapabilities {
        mxr_core::SyncCapabilities {
            sync: mxr_core::SyncCaps {
                native_threading: true,
                ..Default::default()
            },
            mutate: mxr_core::MutateCaps {
                labels: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    async fn authenticate(&mut self) -> Result<(), mxr_core::MxrError> {
        Ok(())
    }

    async fn refresh_auth(&mut self) -> Result<(), mxr_core::MxrError> {
        Ok(())
    }

    async fn sync_labels(&self) -> Result<Vec<mxr_core::Label>, mxr_core::MxrError> {
        Ok(Vec::new())
    }

    async fn sync_messages(
        &self,
        _cursor: &mxr_core::SyncCursor,
    ) -> Result<mxr_core::SyncBatch, mxr_core::MxrError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(mxr_core::MxrError::Provider(self.message.to_string()))
    }

    async fn fetch_attachment(
        &self,
        _provider_message_id: &str,
        _provider_attachment_id: &str,
    ) -> Result<Vec<u8>, mxr_core::MxrError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(mxr_core::MxrError::Provider(self.message.to_string()))
    }

    async fn apply_mutation(
        &self,
        _mutation_id: &str,
        _mutation: &mxr_core::Mutation,
    ) -> Result<(), mxr_core::MxrError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(mxr_core::MxrError::Provider(self.message.to_string()))
    }
}

#[async_trait]
impl mxr_core::MailSyncProvider for FolderCopyProvider {
    fn name(&self) -> &str {
        "folder-copy"
    }

    fn account_id(&self) -> &mxr_core::AccountId {
        &self.account_id
    }

    fn capabilities(&self) -> mxr_core::SyncCapabilities {
        mxr_core::SyncCapabilities {
            sync: mxr_core::SyncCaps {
                native_threading: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    async fn authenticate(&mut self) -> Result<(), mxr_core::MxrError> {
        Ok(())
    }

    async fn refresh_auth(&mut self) -> Result<(), mxr_core::MxrError> {
        Ok(())
    }

    async fn sync_labels(&self) -> Result<Vec<mxr_core::Label>, mxr_core::MxrError> {
        Ok(self.sync_labels_for_account())
    }

    async fn sync_messages(
        &self,
        _cursor: &mxr_core::SyncCursor,
    ) -> Result<mxr_core::SyncBatch, mxr_core::MxrError> {
        let current_provider_ids = self.current_provider_ids();
        let mut last_synced = self.last_synced_provider_ids.lock().unwrap();
        let deleted_provider_ids = last_synced
            .iter()
            .filter(|provider_id| !current_provider_ids.contains(provider_id))
            .cloned()
            .collect();
        *last_synced = current_provider_ids;

        Ok(mxr_core::SyncBatch {
            upserted: self.synced_messages(),
            deleted_provider_ids,
            label_changes: vec![],
            next_cursor: mxr_core::SyncCursor::empty(),
            has_more: false,
            threads_changed: vec![],
        })
    }

    async fn fetch_attachment(
        &self,
        _provider_message_id: &str,
        _provider_attachment_id: &str,
    ) -> Result<Vec<u8>, mxr_core::MxrError> {
        Ok(vec![])
    }

    async fn apply_mutation(
        &self,
        _mutation_id: &str,
        mutation: &mxr_core::Mutation,
    ) -> Result<(), mxr_core::MxrError> {
        let (provider_message_id, add, remove): (&str, &[String], &[String]) = match mutation {
            mxr_core::Mutation::ModifyLabels {
                provider_message_id,
                add,
                remove,
            } => (
                provider_message_id.as_str(),
                add.as_slice(),
                remove.as_slice(),
            ),
            // Trash / SetRead / SetStarred are no-ops for this folder-tracking mock.
            _ => return Ok(()),
        };

        let source_folder = provider_message_id
            .rsplit_once(':')
            .map(|(folder, _)| folder.to_string())
            .unwrap_or_else(|| "INBOX".to_string());
        let mut folders = self.folders.lock().unwrap();

        let added_folders: Vec<String> = add
            .iter()
            .filter(|label| {
                !matches!(
                    label.to_ascii_uppercase().as_str(),
                    "READ" | "SEEN" | "STARRED" | "FLAGGED" | "DRAFT" | "DRAFTS" | "ANSWERED"
                )
            })
            .cloned()
            .collect();
        let removed_folders: Vec<String> = remove
            .iter()
            .filter(|label| {
                !matches!(
                    label.to_ascii_uppercase().as_str(),
                    "READ" | "SEEN" | "STARRED" | "FLAGGED" | "DRAFT" | "DRAFTS" | "ANSWERED"
                )
            })
            .cloned()
            .collect();

        if removed_folders
            .iter()
            .any(|folder| folder.eq_ignore_ascii_case("INBOX"))
            && added_folders.is_empty()
        {
            if self.reanchor_mode == FolderCopyReanchorMode::MissingAfterArchive {
                folders.clear();
                return Ok(());
            }

            folders.retain(|folder| !folder.eq_ignore_ascii_case("INBOX"));
            if !folders
                .iter()
                .any(|folder| folder.eq_ignore_ascii_case("Archive"))
            {
                folders.push("Archive".to_string());
            }
            return Ok(());
        }

        if added_folders
            .iter()
            .any(|folder| folder.eq_ignore_ascii_case("INBOX"))
            && folders
                .iter()
                .all(|folder| !folder.eq_ignore_ascii_case("INBOX"))
            && folders
                .iter()
                .any(|folder| folder.eq_ignore_ascii_case("Archive"))
            && removed_folders.is_empty()
        {
            folders.clear();
            folders.push("INBOX".to_string());
            return Ok(());
        }

        folders.retain(|folder| {
            !removed_folders
                .iter()
                .any(|removed| removed.eq_ignore_ascii_case(folder))
        });

        for folder in added_folders {
            if !folders
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&folder))
            {
                folders.push(folder);
            }
        }

        if folders.is_empty() {
            folders.push(source_folder);
        }

        Ok(())
    }
}

async fn folder_copy_state() -> Arc<AppState> {
    folder_copy_state_with_mode(FolderCopyReanchorMode::Normal).await
}

async fn folder_copy_state_with_mode(reanchor_mode: FolderCopyReanchorMode) -> Arc<AppState> {
    let account_id = mxr_core::AccountId::from_provider_id("imap", "folder-copy@example.com");
    let account = mxr_core::Account {
        id: account_id.clone(),
        name: "Folder Copy".to_string(),
        email: "folder-copy@example.com".to_string(),
        sync_backend: Some(mxr_core::BackendRef {
            provider_kind: mxr_core::ProviderKind::Imap,
            config_key: "folder-copy".to_string(),
        }),
        send_backend: None,
        enabled: true,
    };
    let provider = Arc::new(FolderCopyProvider::with_reanchor_mode(
        account_id,
        reanchor_mode,
    ));
    let provider: Arc<dyn mxr_core::MailSyncProvider> = provider;
    Arc::new(
        AppState::in_memory_with_sync_provider(account, provider, None)
            .await
            .unwrap(),
    )
}

async fn insert_request_invite_body(state: &AppState) -> mxr_core::MessageId {
    let account_id = state.default_account_id_opt().unwrap();
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("calendar-request-1")
        .subject("Planning session")
        .sender_address("Organizer", "organizer@example.com")
        .recipient_address(Some("User"), "user@example.com")
        .has_attachments(true)
        .build();
    let message_id = envelope.id.clone();
    state.store.upsert_envelope(&envelope).await.unwrap();
    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: message_id.clone(),
            text_plain: Some("Planning session invite".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                calendar: Some(mxr_core::types::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Planning session".into()),
                    uid: Some("planning-uid@example.com".into()),
                    sequence: Some(3),
                    organizer: Some(mxr_core::types::CalendarPerson {
                        email: "organizer@example.com".into(),
                        name: Some("Organizer".into()),
                        uri: Some("mailto:organizer@example.com".into()),
                    }),
                    attendees: vec![mxr_core::types::CalendarAttendee {
                        email: "user@example.com".into(),
                        name: Some("User".into()),
                        uri: Some("mailto:user@example.com".into()),
                        partstat: Some("NEEDS-ACTION".into()),
                        role: Some("REQ-PARTICIPANT".into()),
                        rsvp: Some(true),
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    message_id
}

/// Helper: sync, list envelopes, return first envelope's id.
async fn sync_and_get_first_id(state: &Arc<AppState>) -> mxr_core::MessageId {
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 100,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 1,
            offset: 0,
        }),
    };
    let resp = handle_request(state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => {
            assert_eq!(envelopes.len(), 1);
            envelopes[0].id.clone()
        }
        other => panic!("Expected Envelopes, got {:?}", other),
    }
}

fn assert_mutation_succeeded(payload: IpcPayload) -> MutationResultData {
    match payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::MutationResult { result },
        }) => {
            assert!(
                result.succeeded > 0,
                "expected mutation success: {result:?}"
            );
            result
        }
        other => panic!("Expected MutationResult success, got {:?}", other),
    }
}

async fn add_failing_sync_account(
    state: &AppState,
    calls: Arc<AtomicUsize>,
) -> (mxr_core::AccountId, mxr_core::MessageId) {
    let account_id = mxr_core::AccountId::from_provider_id("imap", "hello@bhekani.com");
    let account = mxr_core::Account {
        id: account_id.clone(),
        name: "consulting".to_string(),
        email: "hello@bhekani.com".to_string(),
        sync_backend: Some(mxr_core::BackendRef {
            provider_kind: mxr_core::ProviderKind::Imap,
            config_key: "consulting".to_string(),
        }),
        send_backend: None,
        enabled: true,
    };
    state.store.insert_account(&account).await.unwrap();
    state.add_sync_provider_for_test(Arc::new(FailingSyncProvider {
        account_id: account_id.clone(),
        message: "Keyring error: Failed to read password from keychain",
        calls,
    }));

    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .provider_id("bad-provider-id")
        .subject("bad account message")
        .build();
    let message_id = envelope.id.clone();
    state.store.upsert_envelope(&envelope).await.unwrap();
    (account_id, message_id)
}

fn tiny_png_bytes() -> Vec<u8> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO9xw1QAAAAASUVORK5CYII=")
            .expect("valid 1x1 png")
}

mod body_and_invites;
mod mutations_and_delivery;
mod platform_and_export;
mod routing_and_search;
