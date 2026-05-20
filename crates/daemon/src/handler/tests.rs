use super::*;
use async_trait::async_trait;
use chrono::TimeZone;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;

#[test]
fn request_lane_routes_llm_and_network_requests_to_bulk() {
    // Sample of the slow lane: at least one LLM call, one network
    // round-trip, and one heavy rebuild. Locks the classifier so a
    // refactor that drops a slow request back to Hot will trip the
    // test instead of silently re-introducing head-of-line blocking
    // on fast user-initiated commands.
    let llm = Request::SummarizeThread {
        thread_id: mxr_core::ThreadId::new(),
    };
    assert_eq!(request_lane(&llm), IpcLane::Bulk);

    let download = Request::DownloadAttachment {
        message_id: mxr_core::MessageId::new(),
        attachment_id: mxr_core::AttachmentId::from_provider_id("p", "a"),
        destination: None,
    };
    assert_eq!(request_lane(&download), IpcLane::Bulk);

    let rebuild = Request::RefreshContacts;
    assert_eq!(request_lane(&rebuild), IpcLane::Bulk);

    let remote_assets = Request::GetHtmlImageAssets {
        message_id: mxr_core::MessageId::new(),
        allow_remote: true,
    };
    assert_eq!(request_lane(&remote_assets), IpcLane::Bulk);
}

#[test]
fn request_lane_defaults_user_initiated_commands_to_hot() {
    let list = Request::ListEnvelopes {
        label_id: None,
        account_id: None,
        limit: 50,
        offset: 0,
    };
    assert_eq!(request_lane(&list), IpcLane::Hot);

    let archive = Request::Mutation {
        mutation: mxr_protocol::MutationCommand::Archive {
            message_ids: vec![mxr_core::MessageId::new()],
        },
        client_correlation_id: None,
    };
    assert_eq!(request_lane(&archive), IpcLane::Hot);

    // HTML images without remote fetch is a local-only render and
    // should stay on the hot lane.
    let local_assets = Request::GetHtmlImageAssets {
        message_id: mxr_core::MessageId::new(),
        allow_remote: false,
    };
    assert_eq!(request_lane(&local_assets), IpcLane::Hot);

    let ping = Request::Ping;
    assert_eq!(request_lane(&ping), IpcLane::Hot);
}

#[test]
fn sanitized_attachment_filename_truncates_long_names_preserving_extension() {
    let attachment_id = mxr_core::AttachmentId::from_provider_id("test", "long-pdf");
    let filename = format!("{}.pdf", "a".repeat(400));

    let sanitized = sanitized_attachment_filename(&filename, &attachment_id);

    assert!(
        sanitized.len() <= 220,
        "filename should fit conservative path component limit: {} bytes",
        sanitized.len()
    );
    assert!(sanitized.ends_with(&format!("-{}.pdf", attachment_id.as_str())));
}

#[test]
fn sanitized_attachment_filename_truncates_utf8_on_char_boundary() {
    let attachment_id = mxr_core::AttachmentId::from_provider_id("test", "utf8-pdf");
    let filename = format!("{}.pdf", "é".repeat(200));

    let sanitized = sanitized_attachment_filename(&filename, &attachment_id);

    assert!(
        sanitized.len() <= 220,
        "filename should fit conservative path component limit: {} bytes",
        sanitized.len()
    );
    assert!(sanitized.ends_with(&format!("-{}.pdf", attachment_id.as_str())));
}

#[test]
fn sanitized_attachment_filename_uses_stable_fallback_for_blank_names() {
    let attachment_id = mxr_core::AttachmentId::from_provider_id("test", "blank");

    let sanitized = sanitized_attachment_filename("   ", &attachment_id);

    assert_eq!(sanitized, format!("attachment-{}", attachment_id.as_str()));
}

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

#[tokio::test]
async fn dispatch_ping_returns_pong() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Ping),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Pong,
        }) => {}
        other => panic!("Expected Pong, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_list_envelopes_after_sync() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    // Initial sync
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 100,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => {
            assert_eq!(envelopes.len(), 55);
        }
        other => panic!("Expected Envelopes, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_list_envelopes_by_label() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    // Get labels first
    let labels_msg = IpcMessage {
        id: 10,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels { account_id: None }),
    };
    let resp = handle_request(&state, &labels_msg).await;
    let labels = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => labels,
        other => panic!("Expected Labels, got {:?}", other),
    };

    // Find Inbox label
    let inbox = labels
        .iter()
        .find(|l| l.name == "Inbox")
        .expect("Inbox label missing");

    // Fetch envelopes by Inbox label
    let msg = IpcMessage {
        id: 11,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: Some(inbox.id.clone()),
            account_id: None,
            limit: 100,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => {
            assert!(
                !envelopes.is_empty(),
                "Inbox label should have envelopes, got 0. Inbox label_id={}",
                inbox.id
            );
        }
        IpcPayload::Response(Response::Error { message, .. }) => {
            panic!("Got error response: {message}");
        }
        other => panic!("Expected Envelopes, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_list_labels_without_accounts_returns_empty() {
    let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());

    let msg = IpcMessage {
        id: 12,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels { account_id: None }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => assert!(labels.is_empty()),
        other => panic!("Expected Labels, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_list_envelopes_without_accounts_returns_empty() {
    let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());

    let msg = IpcMessage {
        id: 13,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 100,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => assert!(envelopes.is_empty()),
        other => panic!("Expected Envelopes, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_create_label_persists_and_returns_label() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();

    let create_msg = IpcMessage {
        id: 14,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Urgent".to_string(),
            color: Some("#ff6600".to_string()),
            account_id: Some(account_id.clone()),
        }),
    };
    let resp = handle_request(&state, &create_msg).await;
    let created = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Label { label },
        }) => label,
        other => panic!("Expected Label, got {:?}", other),
    };
    assert_eq!(created.name, "Urgent");
    assert_eq!(created.color.as_deref(), Some("#ff6600"));
    assert_eq!(created.account_id, account_id);

    let list_msg = IpcMessage {
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels {
            account_id: Some(account_id),
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => {
            assert!(labels.iter().any(|label| label.name == "Urgent"));
        }
        other => panic!("Expected Labels, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_upsert_and_list_rules() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let now = chrono::Utc::now();
    let rule = serde_json::json!({
        "id": "rule-1",
        "name": "Archive newsletters",
        "enabled": true,
        "priority": 10,
        "conditions": {"type":"field","field":"has_label","label":"newsletters"},
        "actions": [{"type":"archive"}],
        "created_at": now,
        "updated_at": now
    });

    let upsert_msg = IpcMessage {
        id: 20,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UpsertRule { rule: rule.clone() }),
    };
    let resp = handle_request(&state, &upsert_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::RuleData { rule: returned },
        }) => {
            assert_eq!(returned["name"], "Archive newsletters");
        }
        other => panic!("Expected RuleData, got {:?}", other),
    }

    let list_msg = IpcMessage {
        id: 21,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListRules),
    };
    let resp = handle_request(&state, &list_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Rules { rules },
        }) => {
            assert_eq!(rules.len(), 1);
            assert_eq!(rules[0]["id"], "rule-1");
        }
        other => panic!("Expected Rules, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_dry_run_rules_returns_matching_messages() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();
    let now = chrono::Utc::now();
    let rule = serde_json::json!({
        "id": "rule-1",
        "name": "Mark unread",
        "enabled": true,
        "priority": 10,
        "conditions": {"type":"field","field":"is_unread"},
        "actions": [{"type":"mark_read"}],
        "created_at": now,
        "updated_at": now
    });
    let _ = handle_request(
        &state,
        &IpcMessage {
            id: 22,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::UpsertRule { rule }),
        },
    )
    .await;

    let dry_run_msg = IpcMessage {
        id: 23,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::DryRunRules {
            rule: Some("rule-1".to_string()),
            all: false,
            after: None,
        }),
    };
    let resp = handle_request(&state, &dry_run_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::RuleDryRun { results },
        }) => {
            assert_eq!(results.len(), 1);
            let matches = results[0]["matches"]
                .as_array()
                .expect("matches should be an array");
            assert!(matches.len() >= 1);
        }
        other => panic!("Expected RuleDryRun, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_upsert_rule_form_and_get_rule_form() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let upsert_msg = IpcMessage {
        id: 231,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UpsertRuleForm {
            existing_rule: None,
            name: "Archive unread".into(),
            condition: "is:unread".into(),
            action: "archive".into(),
            priority: 25,
            enabled: true,
        }),
    };
    let resp = handle_request(&state, &upsert_msg).await;
    let rule_id = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::RuleData { rule },
        }) => {
            assert_eq!(rule["name"], "Archive unread");
            rule["id"].as_str().unwrap().to_string()
        }
        other => panic!("Expected RuleData, got {:?}", other),
    };

    let get_form_msg = IpcMessage {
        id: 232,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetRuleForm { rule: rule_id }),
    };
    let resp = handle_request(&state, &get_form_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::RuleFormData { form },
        }) => {
            assert_eq!(form.name, "Archive unread");
            assert_eq!(form.condition, "is:unread");
            assert_eq!(form.action, "archive");
            assert_eq!(form.priority, 25);
            assert!(form.enabled);
        }
        other => panic!("Expected RuleFormData, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_rename_label_updates_visible_label() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();

    let create_msg = IpcMessage {
        id: 14,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Projects".to_string(),
            color: None,
            account_id: Some(account_id.clone()),
        }),
    };
    let _ = handle_request(&state, &create_msg).await;

    let rename_msg = IpcMessage {
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::RenameLabel {
            old: "Projects".to_string(),
            new: "Client Work".to_string(),
            account_id: Some(account_id.clone()),
        }),
    };
    let resp = handle_request(&state, &rename_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Label { label },
        }) => {
            assert_eq!(label.name, "Client Work");
            assert_eq!(label.provider_id, "Client Work");
        }
        other => panic!("Expected Label, got {:?}", other),
    }

    let list_msg = IpcMessage {
        id: 16,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels {
            account_id: Some(account_id),
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => {
            assert!(labels.iter().any(|label| label.name == "Client Work"));
            assert!(!labels.iter().any(|label| label.name == "Projects"));
        }
        other => panic!("Expected Labels, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_delete_label_removes_it_from_store() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();

    let create_msg = IpcMessage {
        id: 17,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Temporary".to_string(),
            color: None,
            account_id: Some(account_id.clone()),
        }),
    };
    let _ = handle_request(&state, &create_msg).await;

    let delete_msg = IpcMessage {
        id: 18,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::DeleteLabel {
            name: "Temporary".to_string(),
            account_id: Some(account_id.clone()),
        }),
    };
    let resp = handle_request(&state, &delete_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {:?}", other),
    }

    let list_msg = IpcMessage {
        id: 19,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListLabels {
            account_id: Some(account_id),
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Labels { labels },
        }) => {
            assert!(!labels.iter().any(|label| label.name == "Temporary"));
        }
        other => panic!("Expected Labels, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_count_after_sync() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Count {
            query: "deployment".to_string(),
            mode: None,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Count { count },
        }) => {
            assert!(count > 0, "Expected non-zero count for 'deployment'");
        }
        other => panic!("Expected Count, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_list_saved_searches_empty() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 4,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSavedSearches),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearches { searches },
        }) => {
            assert!(searches.is_empty());
        }
        other => panic!("Expected empty SavedSearches, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_create_and_list_saved_searches() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    // Create
    let create_msg = IpcMessage {
        id: 5,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateSavedSearch {
            name: "Important".to_string(),
            query: "is:starred".to_string(),
            search_mode: mxr_core::SearchMode::Lexical,
        }),
    };
    let resp = handle_request(&state, &create_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearchData { search },
        }) => {
            assert_eq!(search.name, "Important");
            assert_eq!(search.query, "is:starred");
            assert_eq!(search.search_mode, mxr_core::SearchMode::Lexical);
        }
        other => panic!("Expected SavedSearchData, got {:?}", other),
    }

    // List
    let list_msg = IpcMessage {
        id: 6,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSavedSearches),
    };
    let resp = handle_request(&state, &list_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearches { searches },
        }) => {
            assert_eq!(searches.len(), 1);
            assert_eq!(searches[0].name, "Important");
            assert_eq!(searches[0].search_mode, mxr_core::SearchMode::Lexical);
        }
        other => panic!("Expected SavedSearches, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_create_saved_search_persists_requested_mode() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let create_msg = IpcMessage {
        id: 51,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateSavedSearch {
            name: "Hybrid".to_string(),
            query: "deployment".to_string(),
            search_mode: mxr_core::SearchMode::Hybrid,
        }),
    };

    let resp = handle_request(&state, &create_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearchData { search },
        }) => {
            assert_eq!(search.search_mode, mxr_core::SearchMode::Hybrid);
        }
        other => panic!("Expected SavedSearchData, got {:?}", other),
    }

    let saved = state
        .store
        .get_saved_search_by_name("Hybrid")
        .await
        .unwrap()
        .expect("saved search");
    assert_eq!(saved.search_mode, mxr_core::SearchMode::Hybrid);
}

#[tokio::test]
async fn dispatch_run_saved_search_returns_results() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let create = IpcMessage {
        id: 200,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateSavedSearch {
            name: "Deploy".into(),
            query: "deployment".into(),
            search_mode: mxr_core::SearchMode::Lexical,
        }),
    };
    handle_request(&state, &create).await;

    let msg = IpcMessage {
        id: 201,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::RunSavedSearch {
            name: "Deploy".into(),
            limit: 10,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::SearchResults {
                    results,
                    has_more,
                    explain,
                    ..
                },
        }) => {
            assert_eq!(has_more, false);
            assert_eq!(explain.is_none(), true);
            assert!(results.len() >= 1);
            assert!(results.len() <= 10);
            assert!(
                results
                    .iter()
                    .all(|item| item.mode == mxr_core::SearchMode::Lexical),
                "saved search should return lexical results"
            );
        }
        other => panic!("Expected SearchResults, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_status() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 7,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetStatus),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::Status {
                    uptime_secs: _,
                    accounts,
                    total_messages: _,
                    daemon_pid,
                    sync_statuses,
                    protocol_version,
                    daemon_version,
                    daemon_build_id,
                    repair_required,
                    ..
                },
        }) => {
            assert_eq!(accounts.len(), 1);
            let daemon_pid = daemon_pid.expect("daemon pid should be present");
            assert!(daemon_pid > 0);
            assert_eq!(sync_statuses.len(), 1);
            assert!(protocol_version >= mxr_protocol::IPC_PROTOCOL_VERSION);
            let daemon_version = daemon_version.expect("daemon version should be present");
            assert_ne!(daemon_version, "");
            let daemon_build_id = daemon_build_id.expect("daemon build id should be present");
            assert_ne!(daemon_build_id, "");
            assert_eq!(repair_required, false);
        }
        other => panic!("Expected Status, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_status_reports_degraded_relationship_llm_features_when_llm_disabled() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 7,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetStatus),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::Status {
                    feature_health: Some(feature_health),
                    ..
                },
        }) => {
            assert!(matches!(
                feature_health.relationship_profile,
                FeatureHealth::Degraded { .. }
            ));
            assert!(matches!(
                feature_health.commitments,
                FeatureHealth::Degraded { .. }
            ));
            assert!(matches!(
                feature_health.humanizer,
                FeatureHealth::Degraded { .. }
            ));
        }
        other => panic!("Expected Status with feature health, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_status_does_not_block_when_search_is_busy() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 8,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetStatus),
    };

    let resp = tokio::time::timeout(
        std::time::Duration::from_millis(250),
        handle_request(&state, &msg),
    )
    .await
    .expect("status should not block on a busy search index");

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Status { .. },
        }) => {}
        other => panic!("Expected Status, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_shutdown_acknowledges_without_exiting() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 9,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Shutdown),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {:?}", other),
    }
    assert!(state.shutdown_requested());
}

#[tokio::test]
async fn dispatch_doctor_report() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 81,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetDoctorReport),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::DoctorReport { report },
        }) => {
            assert!(report.database_path.contains("mxr.db"));
            assert!(report.index_path.contains("search_index"));
            let daemon_version = report.daemon_version.expect("doctor report daemon version");
            assert_ne!(daemon_version, "");
            let daemon_build_id = report.daemon_build_id.expect("doctor report build id");
            assert_ne!(daemon_build_id, "");
        }
        other => panic!("Expected DoctorReport, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_sync_status() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();

    let msg = IpcMessage {
        id: 82,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetSyncStatus { account_id }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SyncStatus { sync },
        }) => {
            assert_ne!(sync.account_name, "");
            let summary = sync
                .current_cursor_summary
                .expect("sync status should include cursor summary");
            assert_ne!(summary, "");
        }
        other => panic!("Expected SyncStatus, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_search_returns_results() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    // Sync first so search index is populated
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 10,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "deployment".to_string(),
            limit: 10,
            offset: 0,
            mode: None,
            sort: None,
            explain: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SearchResults { results, .. },
        }) => {
            assert!(
                results.len() >= 1,
                "Search for 'deployment' should return results"
            );
            assert!(results.len() <= 10);
            assert_eq!(results[0].mode, mxr_core::SearchMode::Lexical);
        }
        other => panic!("Expected SearchResults, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_search_explain_returns_execution_details() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 11,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "deployment".to_string(),
            limit: 5,
            offset: 0,
            mode: Some(mxr_core::SearchMode::Lexical),
            sort: Some(mxr_core::types::SortOrder::DateDesc),
            explain: true,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::SearchResults {
                    results,
                    explain: Some(explain),
                    ..
                },
        }) => {
            assert!(results.len() >= 1);
            assert!(results.len() <= 5);
            assert_eq!(explain.requested_mode, mxr_core::SearchMode::Lexical);
            assert_eq!(explain.executed_mode, mxr_core::SearchMode::Lexical);
            assert_eq!(explain.dense_candidates, 0);
            assert_eq!(explain.final_results as usize, results.len());
            assert_eq!(explain.results.len(), results.len());
        }
        other => panic!(
            "Expected SearchResults with explain payload, got {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn dispatch_structured_search_in_semantic_mode_falls_back_to_lexical() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 13,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "is:unread".to_string(),
            limit: 10,
            offset: 0,
            mode: Some(mxr_core::SearchMode::Semantic),
            sort: Some(mxr_core::types::SortOrder::DateDesc),
            explain: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SearchResults { results, .. },
        }) => {
            assert!(results.len() >= 1);
            assert!(results.len() <= 10);
        }
        other => panic!("Expected SearchResults, got {:?}", other),
    }
}

// Requires the local semantic embedder to populate explain.semantic_query;
// gate to the semantic-local lane so the fast lane stays green.
#[cfg(feature = "semantic-local")]
#[tokio::test]
async fn dispatch_structured_search_in_semantic_mode_explains_fallback() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 14,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "is:unread".to_string(),
            limit: 10,
            offset: 0,
            mode: Some(mxr_core::SearchMode::Semantic),
            sort: Some(mxr_core::types::SortOrder::DateDesc),
            explain: true,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::SearchResults {
                    explain: Some(explain),
                    ..
                },
        }) => {
            assert_eq!(explain.requested_mode, mxr_core::SearchMode::Semantic);
            assert_eq!(explain.executed_mode, mxr_core::SearchMode::Lexical);
            assert!(explain
                .notes
                .iter()
                .any(|note| note.contains("no semantic text terms")));
        }
        other => panic!(
            "Expected SearchResults with explain payload, got {:?}",
            other
        ),
    }
}

#[cfg(feature = "semantic-local")]
#[tokio::test]
async fn dispatch_fielded_semantic_query_explains_disabled_fallback() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.search.semantic.enabled = false;
    state.set_config_for_test(config).await;

    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "body:deployment".to_string(),
            limit: 10,
            offset: 0,
            mode: Some(mxr_core::SearchMode::Hybrid),
            sort: Some(mxr_core::types::SortOrder::DateDesc),
            explain: true,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::SearchResults {
                    results,
                    explain: Some(explain),
                    ..
                },
        }) => {
            assert!(!results.is_empty());
            assert_eq!(explain.requested_mode, mxr_core::SearchMode::Hybrid);
            assert_eq!(explain.executed_mode, mxr_core::SearchMode::Lexical);
            assert_eq!(explain.semantic_query.as_deref(), Some("deployment"));
            assert!(explain
                .notes
                .iter()
                .any(|note| note.contains("semantic search disabled in config")));
        }
        other => panic!(
            "Expected SearchResults with explain payload, got {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn dispatch_search_rejects_invalid_structured_query() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let msg = IpcMessage {
        id: 12,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "older:30q".to_string(),
            limit: 10,
            offset: 0,
            mode: None,
            sort: None,
            explain: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("Invalid search query"));
            assert!(message.contains("invalid date"));
        }
        other => panic!("Expected Error, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_get_body_after_sync() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    // Get first envelope
    let envelopes_msg = IpcMessage {
        id: 11,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 1,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &envelopes_msg).await;
    let message_id = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => {
            assert_eq!(envelopes.len(), 1);
            envelopes[0].id.clone()
        }
        other => panic!("Expected Envelopes, got {:?}", other),
    };

    // Get body for that envelope
    let body_msg = IpcMessage {
        id: 12,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: message_id.clone(),
        }),
    };
    let resp = handle_request(&state, &body_msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Body { body },
        }) => {
            assert!(
                body.text_plain.is_some(),
                "Body should have text_plain content"
            );
        }
        other => panic!("Expected Body, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_list_bodies_omits_missing_rows() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let missing_id = mxr_core::MessageId::new();

    let msg = IpcMessage {
        id: 13,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListBodies {
            message_ids: vec![missing_id],
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Bodies { bodies, .. },
        }) => {
            assert!(
                bodies.is_empty(),
                "missing body rows should be omitted so clients can retry"
            );
        }
        other => panic!("Expected Bodies, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_get_body_rehydrates_missing_store_row_from_provider() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    sqlx::query("DELETE FROM bodies WHERE message_id = ?")
        .bind(id.to_string())
        .execute(state.store.writer())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 14,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Body { body },
        }) => {
            assert!(
                body.text_plain.is_some() || body.text_html.is_some(),
                "provider hydration should restore a readable body"
            );
        }
        other => panic!("Expected Body, got {:?}", other),
    }

    let stored = state.store.get_body(&id).await.unwrap().unwrap();
    assert!(
        stored.text_plain.is_some() || stored.text_html.is_some(),
        "hydrated body should be persisted back into the store"
    );
}

#[tokio::test]
async fn dispatch_list_bodies_stays_local_when_store_row_is_missing() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    sqlx::query("DELETE FROM bodies WHERE message_id = ?")
        .bind(id.to_string())
        .execute(state.store.writer())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListBodies {
            message_ids: vec![id.clone()],
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Bodies { bodies, failures },
        }) => {
            assert!(bodies.is_empty());
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].message_id, id);
            assert!(
                state
                    .store
                    .get_body(&failures[0].message_id)
                    .await
                    .unwrap()
                    .is_none(),
                "bulk prefetch must not repair from provider and block the TUI queue"
            );
        }
        other => panic!("Expected Bodies, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_get_body_rehydrates_legacy_best_effort_body_from_provider() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    let stale = mxr_core::types::MessageBody {
        message_id: id.clone(),
        text_plain: Some("No readable body content was available for this message.".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: mxr_core::types::MessageMetadata::default(),
    };
    state.store.insert_body(&stale).await.unwrap();

    let msg = IpcMessage {
        id: 19,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Body { body },
        }) => {
            assert_ne!(body.text_plain, stale.text_plain);
            assert!(
                body.text_plain.is_some() || body.text_html.is_some(),
                "legacy synthesized body should be replaced with provider content"
            );
        }
        other => panic!("Expected Body, got {:?}", other),
    }

    let stored = state.store.get_body(&id).await.unwrap().unwrap();
    assert_ne!(stored.text_plain, stale.text_plain);
    assert!(
        stored.text_plain.is_some() || stored.text_html.is_some(),
        "rehydrated body should be persisted back into the store"
    );
}

#[tokio::test]
async fn dispatch_get_body_rehydrates_best_effort_summary_when_snippet_implies_real_body() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    let stale = mxr_core::types::MessageBody {
        message_id: id.clone(),
        text_plain: Some("No readable body content was available for this message.".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: mxr_core::types::MessageMetadata {
            text_plain_source: Some(mxr_core::types::BodyPartSource::BestEffortSummary),
            raw_headers: Some(
                "Content-Type: multipart/alternative; boundary=\"debug-boundary\"".into(),
            ),
            ..Default::default()
        },
    };
    state.store.insert_body(&stale).await.unwrap();

    let msg = IpcMessage {
        id: 20,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Body { body },
        }) => {
            assert_ne!(body.text_plain, stale.text_plain);
            assert!(
                body.text_plain.is_some() || body.text_html.is_some(),
                "stored best-effort summaries should be repaired when provider content exists"
            );
        }
        other => panic!("Expected Body, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_list_bodies_preserves_attachments() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let attachment_id = mxr_core::AttachmentId::new();

    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: Some("hello".into()),
            text_html: Some("<p>hello</p>".into()),
            attachments: vec![mxr_core::types::AttachmentMeta {
                id: attachment_id.clone(),
                message_id: id.clone(),
                filename: "report.pdf".into(),
                mime_type: "application/pdf".into(),
                disposition: mxr_core::types::AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 1024,
                local_path: None,
                provider_id: "att-1".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        })
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 16,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListBodies {
            message_ids: vec![id.clone()],
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Bodies { bodies, .. },
        }) => {
            assert_eq!(bodies.len(), 1);
            assert_eq!(bodies[0].text_plain.as_deref(), Some("hello"));
            assert_eq!(bodies[0].text_html.as_deref(), Some("<p>hello</p>"));
            assert_eq!(bodies[0].attachments.len(), 1);
            assert_eq!(bodies[0].attachments[0].id, attachment_id);
            assert_eq!(bodies[0].attachments[0].filename, "report.pdf");
        }
        other => panic!("Expected Bodies, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_get_body_synthesizes_readable_summary_for_calendar_only_messages() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let stored = mxr_core::types::MessageBody {
        message_id: id.clone(),
        text_plain: None,
        text_html: None,
        attachments: vec![mxr_core::types::AttachmentMeta {
            id: mxr_core::AttachmentId::new(),
            message_id: id.clone(),
            filename: "invite.ics".into(),
            mime_type: "text/calendar".into(),
            disposition: mxr_core::types::AttachmentDisposition::Attachment,
            content_id: None,
            content_location: None,
            size_bytes: 2048,
            local_path: None,
            provider_id: "att-calendar".into(),
        }],
        fetched_at: chrono::Utc::now(),
        metadata: mxr_core::types::MessageMetadata {
            calendar: Some(mxr_core::types::CalendarMetadata {
                method: Some("REQUEST".into()),
                summary: Some("Demo call".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
    };
    state.store.insert_body(&stored).await.unwrap();

    let msg = IpcMessage {
        id: 17,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Body { body },
        }) => {
            let text = body
                .text_plain
                .expect("calendar-only body should be synthesized");
            assert!(text.contains("Calendar invite"));
            assert!(text.contains("Summary: Demo call"));
            assert!(text.contains("invite.ics"));
        }
        other => panic!("Expected Body, got {:?}", other),
    }

    let repaired = state.store.get_body(&id).await.unwrap().unwrap();
    assert!(repaired
        .text_plain
        .as_deref()
        .is_some_and(|text| text.contains("Calendar invite")));
}

async fn insert_request_invite_body(state: &AppState) -> mxr_core::MessageId {
    let account_id = state.default_account_id_opt().unwrap();
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("calendar-request-1")
        .subject("Planning session")
        .from_address("Organizer", "organizer@example.com")
        .to_address(Some("User"), "user@example.com")
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

#[tokio::test]
async fn dispatch_respond_invite_dry_run_builds_imip_preview_without_sending() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let message_id = insert_request_invite_body(&state).await;
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 18,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id: message_id.clone(),
                action: mxr_protocol::CalendarInviteActionData::Accept,
                dry_run: true,
            }),
        },
    )
    .await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::InviteResponsePreview { preview },
        }) => {
            assert_eq!(preview.message_id, message_id);
            assert_eq!(preview.organizer_email, "organizer@example.com");
            assert_eq!(preview.attendee_email, "user@example.com");
            assert!(preview.subject.contains("Accepted"));
            assert!(preview.ics.contains("METHOD:REPLY"));
            assert!(preview.ics.contains("UID:planning-uid@example.com"));
            assert!(preview.ics.contains("SEQUENCE:3"));
            assert!(preview.ics.contains("PARTSTAT=ACCEPTED"));
        }
        other => panic!("Expected InviteResponsePreview, got {:?}", other),
    }
    assert!(fake.sent_drafts().is_empty());
}

#[tokio::test]
async fn dispatch_respond_invite_sends_reply_and_updates_local_partstat() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let message_id = insert_request_invite_body(&state).await;
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 19,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id: message_id.clone(),
                action: mxr_protocol::CalendarInviteActionData::Decline,
                dry_run: false,
            }),
        },
    )
    .await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::InviteResponseSent { result },
        }) => {
            assert_eq!(result.message_id, message_id);
            assert_eq!(
                result.action,
                mxr_protocol::CalendarInviteActionData::Decline
            );
            assert!(result
                .provider_message_id
                .as_deref()
                .is_some_and(|id| id.starts_with("fake-calendar-sent-")));
        }
        other => panic!("Expected InviteResponseSent, got {:?}", other),
    }

    let sent = fake.sent_drafts();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to[0].email, "organizer@example.com");
    assert!(sent[0].body_markdown.contains("METHOD:REPLY"));
    assert!(sent[0].body_markdown.contains("PARTSTAT=DECLINED"));

    let stored = state
        .store
        .get_calendar_invite_for_message(&message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.metadata.attendees[0].partstat.as_deref(),
        Some("DECLINED")
    );
}

#[tokio::test]
async fn dispatch_respond_invite_matches_account_alias_attendee() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    state
        .store
        .add_account_address(&account_id, "alias@example.com", false)
        .await
        .unwrap();
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("calendar-alias")
        .subject("Alias invite")
        .from_address("Organizer", "organizer@example.com")
        .to_address(Some("Alias"), "alias@example.com")
        .has_attachments(true)
        .build();
    let message_id = envelope.id.clone();
    state.store.upsert_envelope(&envelope).await.unwrap();
    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: message_id.clone(),
            text_plain: Some("Alias invite".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                calendar: Some(mxr_core::types::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Alias invite".into()),
                    uid: Some("alias-uid@example.com".into()),
                    organizer: Some(mxr_core::types::CalendarPerson {
                        email: "organizer@example.com".into(),
                        name: Some("Organizer".into()),
                        uri: Some("mailto:organizer@example.com".into()),
                    }),
                    attendees: vec![mxr_core::types::CalendarAttendee {
                        email: "alias@example.com".into(),
                        name: Some("Alias".into()),
                        uri: Some("mailto:alias@example.com".into()),
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
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 20,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id: message_id.clone(),
                action: mxr_protocol::CalendarInviteActionData::Accept,
                dry_run: false,
            }),
        },
    )
    .await;

    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::InviteResponseSent { .. }
        })
    ));
    assert_eq!(fake.sent_drafts().len(), 1);
    let stored = state
        .store
        .get_calendar_invite_for_message(&message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.metadata.attendees[0].partstat.as_deref(),
        Some("ACCEPTED")
    );
}

#[tokio::test]
async fn dispatch_respond_invite_blocks_stale_sequence_when_newer_invite_exists() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let stale_message_id = insert_request_invite_body(&state).await;
    let account_id = state.default_account_id_opt().unwrap();
    let newer = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("calendar-request-2")
        .subject("Planning session updated")
        .from_address("Organizer", "organizer@example.com")
        .to_address(Some("User"), "user@example.com")
        .has_attachments(true)
        .build();
    state.store.upsert_envelope(&newer).await.unwrap();
    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: newer.id.clone(),
            text_plain: Some("Updated planning session invite".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                calendar: Some(mxr_core::types::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Planning session updated".into()),
                    uid: Some("planning-uid@example.com".into()),
                    sequence: Some(4),
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
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 20,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id: stale_message_id,
                action: mxr_protocol::CalendarInviteActionData::Accept,
                dry_run: false,
            }),
        },
    )
    .await;

    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("newer update"));
        }
        other => panic!("Expected stale invite error, got {:?}", other),
    }
    assert!(fake.sent_drafts().is_empty());
}

#[tokio::test]
async fn dispatch_respond_invite_warns_when_same_uid_has_different_organizer() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let message_id = insert_request_invite_body(&state).await;
    let account_id = state.default_account_id_opt().unwrap();
    let older = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("calendar-organizer-change")
        .subject("Planning session suspicious")
        .from_address("Other Organizer", "other@example.com")
        .to_address(Some("User"), "user@example.com")
        .has_attachments(true)
        .build();
    state.store.upsert_envelope(&older).await.unwrap();
    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: older.id.clone(),
            text_plain: Some("Suspicious planning session invite".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                calendar: Some(mxr_core::types::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Planning session suspicious".into()),
                    uid: Some("planning-uid@example.com".into()),
                    sequence: Some(1),
                    organizer: Some(mxr_core::types::CalendarPerson {
                        email: "other@example.com".into(),
                        name: Some("Other Organizer".into()),
                        uri: Some("mailto:other@example.com".into()),
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
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 22,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id,
                action: mxr_protocol::CalendarInviteActionData::Accept,
                dry_run: true,
            }),
        },
    )
    .await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::InviteResponsePreview { preview },
        }) => assert!(preview
            .warnings
            .iter()
            .any(|warning| warning.contains("different organizer"))),
        other => panic!("Expected organizer warning preview, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_respond_invite_blocks_fatal_parser_warning() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("calendar-bad-parse")
        .subject("Broken invite")
        .from_address("Organizer", "organizer@example.com")
        .to_address(Some("User"), "user@example.com")
        .has_attachments(true)
        .build();
    let message_id = envelope.id.clone();
    state.store.upsert_envelope(&envelope).await.unwrap();
    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: message_id.clone(),
            text_plain: Some("Broken invite".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                calendar: Some(mxr_core::types::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Broken invite".into()),
                    uid: Some("broken-uid@example.com".into()),
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
                    warnings: vec!["calendar invite could not be parsed as RFC 5545".into()],
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 21,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::RespondInvite {
                message_id,
                action: mxr_protocol::CalendarInviteActionData::Accept,
                dry_run: false,
            }),
        },
    )
    .await;

    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("fatal parser warnings"));
        }
        other => panic!("Expected fatal parser warning error, got {:?}", other),
    }
    assert!(fake.sent_drafts().is_empty());
}

#[tokio::test]
async fn dispatch_get_body_preserves_exact_sources_and_inline_metadata() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let attachment_id = mxr_core::AttachmentId::new();

    let stored = mxr_core::types::MessageBody {
        message_id: id.clone(),
        text_plain: Some("Hello team, \n> exact quote\n".into()),
        text_html: Some("<p>Hello <img src=\"cid:logo@example.com\"></p>".into()),
        attachments: vec![mxr_core::types::AttachmentMeta {
            id: attachment_id.clone(),
            message_id: id.clone(),
            filename: "logo.png".into(),
            mime_type: "image/png".into(),
            disposition: mxr_core::types::AttachmentDisposition::Inline,
            content_id: Some("logo@example.com".into()),
            content_location: Some("https://example.com/logo.png".into()),
            size_bytes: 2048,
            local_path: None,
            provider_id: "att-inline".into(),
        }],
        fetched_at: chrono::Utc::now(),
        metadata: mxr_core::types::MessageMetadata {
            text_plain_format: Some(mxr_core::types::TextPlainFormat::Flowed { delsp: true }),
            text_plain_source: Some(mxr_core::types::BodyPartSource::Exact),
            text_html_source: Some(mxr_core::types::BodyPartSource::Exact),
            ..Default::default()
        },
    };

    state.store.insert_body(&stored).await.unwrap();

    let msg = IpcMessage {
        id: 18,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Body { body },
        }) => {
            assert_eq!(body.text_plain, stored.text_plain);
            assert_eq!(body.text_html, stored.text_html);
            assert_eq!(
                body.metadata.text_plain_format,
                stored.metadata.text_plain_format
            );
            assert_eq!(
                body.metadata.text_plain_source,
                stored.metadata.text_plain_source
            );
            assert_eq!(
                body.metadata.text_html_source,
                stored.metadata.text_html_source
            );
            assert_eq!(body.attachments.len(), 1);
            assert_eq!(body.attachments[0].id, attachment_id);
            assert_eq!(
                body.attachments[0].content_id.as_deref(),
                Some("logo@example.com")
            );
            assert_eq!(
                body.attachments[0].content_location.as_deref(),
                Some("https://example.com/logo.png")
            );
        }
        other => panic!("Expected Body, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_get_html_image_assets_resolves_inline_and_blocks_remote() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let attachment_id = mxr_core::AttachmentId::new();
    let temp_dir = tempfile::tempdir().unwrap();
    let inline_path = temp_dir.path().join("logo.png");
    std::fs::write(&inline_path, tiny_png_bytes()).unwrap();

    let stored = mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: None,
            text_html: Some(concat!(
                "<img alt=\"Logo\" src=\"cid:logo@example.com\">",
                "<img alt=\"Badge\" src=\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO9xw1QAAAAASUVORK5CYII=\">",
                "<img alt=\"Hero\" src=\"https://example.com/hero.png\">"
            ).into()),
            attachments: vec![mxr_core::types::AttachmentMeta {
                id: attachment_id.clone(),
                message_id: id.clone(),
                filename: "logo.png".into(),
                mime_type: "image/png".into(),
                disposition: mxr_core::types::AttachmentDisposition::Inline,
                content_id: Some("logo@example.com".into()),
                content_location: None,
                size_bytes: 67,
                local_path: Some(inline_path.clone()),
                provider_id: "att-inline".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                text_html_source: Some(mxr_core::types::BodyPartSource::Exact),
                ..Default::default()
            },
        };
    state.store.insert_body(&stored).await.unwrap();

    let msg = IpcMessage {
        id: 16,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetHtmlImageAssets {
            message_id: id.clone(),
            allow_remote: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::HtmlImageAssets { assets, .. },
        }) => {
            assert_eq!(assets.len(), 3);

            let inline = assets
                .iter()
                .find(|asset| asset.source.starts_with("cid:"))
                .expect("cid asset");
            assert_eq!(inline.status, mxr_core::types::HtmlImageAssetStatus::Ready);
            assert_eq!(inline.path.as_deref(), Some(inline_path.as_path()));

            let embedded = assets
                .iter()
                .find(|asset| asset.source.starts_with("data:"))
                .expect("data asset");
            assert_eq!(
                embedded.status,
                mxr_core::types::HtmlImageAssetStatus::Ready,
                "embedded asset: {:?}",
                embedded
            );
            assert!(embedded.path.as_ref().is_some_and(|path| path.exists()));

            let remote = assets
                .iter()
                .find(|asset| asset.source.starts_with("https://"))
                .expect("remote asset");
            assert_eq!(
                remote.status,
                mxr_core::types::HtmlImageAssetStatus::Blocked
            );
            assert!(remote.path.is_none());
        }
        other => panic!("Expected HtmlImageAssets, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_get_html_image_assets_fetches_remote_when_enabled() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "image/png")
                .set_body_bytes(tiny_png_bytes()),
        )
        .mount(&server)
        .await;

    let stored = mxr_core::types::MessageBody {
        message_id: id.clone(),
        text_plain: None,
        text_html: Some(format!(
            r#"<img alt="Hero" src="{}/hero.png">"#,
            server.uri()
        )),
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: mxr_core::types::MessageMetadata {
            text_html_source: Some(mxr_core::types::BodyPartSource::Exact),
            ..Default::default()
        },
    };
    state.store.insert_body(&stored).await.unwrap();

    let msg = IpcMessage {
        id: 17,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetHtmlImageAssets {
            message_id: id.clone(),
            allow_remote: true,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::HtmlImageAssets { assets, .. },
        }) => {
            assert_eq!(assets.len(), 1);
            assert_eq!(
                assets[0].status,
                mxr_core::types::HtmlImageAssetStatus::Ready
            );
            let path = assets[0].path.as_ref().expect("cached path");
            assert!(path.exists());
            assert_eq!(std::fs::read(path).unwrap(), tiny_png_bytes());
        }
        other => panic!("Expected HtmlImageAssets, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_download_attachment_persists_local_path() {
    let state = AppState::in_memory().await.unwrap();
    state.set_attachment_dir_for_tests(
        std::env::temp_dir().join(format!("mxr-attachments-test-{}", uuid::Uuid::new_v4())),
    );
    let state = Arc::new(state);

    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let list_msg = IpcMessage {
        id: 14,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 200,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    let envelope = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => envelopes
            .into_iter()
            .find(|envelope| envelope.has_attachments)
            .expect("fixture should include an attachment"),
        other => panic!("Expected Envelopes, got {:?}", other),
    };

    let body_msg = IpcMessage {
        id: 15,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: envelope.id.clone(),
        }),
    };
    let resp = handle_request(&state, &body_msg).await;
    let attachment_id = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Body { body },
        }) => body.attachments[0].id.clone(),
        other => panic!("Expected Body, got {:?}", other),
    };

    let download_msg = IpcMessage {
        id: 16,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::DownloadAttachment {
            message_id: envelope.id.clone(),
            attachment_id: attachment_id.clone(),
            destination: None,
        }),
    };
    let resp = handle_request(&state, &download_msg).await;
    let path = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::AttachmentFile { file },
        }) => std::path::PathBuf::from(file.path),
        other => panic!("Expected AttachmentFile, got {:?}", other),
    };

    assert!(path.exists(), "downloaded attachment should exist on disk");

    let body = state
        .store
        .get_body(&envelope.id)
        .await
        .unwrap()
        .expect("body should remain cached");
    let attachment = body
        .attachments
        .iter()
        .find(|attachment| attachment.id == attachment_id)
        .expect("attachment should still exist");
    assert_eq!(attachment.local_path.as_ref(), Some(&path));

    let _ = std::fs::remove_dir_all(state.attachment_dir());
}

#[tokio::test]
async fn dispatch_set_reply_later_persists_flag_visible_in_queue() {
    // Behavior: marking a message reply-later via IPC persists the flag,
    // and subsequent `ListReplyQueue` requests return the envelope.
    // Clearing the flag removes it from the queue.
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    // Initially the queue is empty.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 200,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ListReplyQueue),
        },
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyQueue { messages },
        }) => assert!(messages.is_empty(), "fresh queue is empty"),
        other => panic!("expected ReplyQueue, got {other:?}"),
    }

    // Set the flag.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 201,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetReplyLater {
                message_id: id.clone(),
                flag: true,
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // Queue now contains the flagged envelope.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 202,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ListReplyQueue),
        },
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyQueue { messages },
        }) => {
            assert_eq!(messages.len(), 1, "one flagged message");
            assert_eq!(messages[0].id, id);
        }
        other => panic!("expected ReplyQueue, got {other:?}"),
    }
    let ast = mxr_search::parse_query("is:reply-later").unwrap();
    let schema = mxr_search::MxrSchema::build();
    let query = mxr_search::QueryBuilder::new(&schema).build(&ast);
    let search_page = state
        .search
        .search_ast(query, 10, 0, mxr_core::types::SortOrder::DateDesc)
        .await
        .unwrap();
    assert_eq!(search_page.results.len(), 1, "search sees reply-later");
    assert_eq!(search_page.results[0].message_id, id.as_str());

    // Clear the flag.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 203,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetReplyLater {
                message_id: id.clone(),
                flag: false,
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // Queue is empty again.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 204,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ListReplyQueue),
        },
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyQueue { messages },
        }) => assert!(messages.is_empty(), "queue empty after clear"),
        other => panic!("expected ReplyQueue, got {other:?}"),
    }
    let ast = mxr_search::parse_query("is:reply-later").unwrap();
    let schema = mxr_search::MxrSchema::build();
    let query = mxr_search::QueryBuilder::new(&schema).build(&ast);
    let search_page = state
        .search
        .search_ast(query, 10, 0, mxr_core::types::SortOrder::DateDesc)
        .await
        .unwrap();
    assert!(search_page.results.is_empty(), "search updates after clear");
}

/// Phase 2.1: dismissing a reply-later flag is a pure metadata
/// operation. It removes the message from the queue, but it must
/// not generate a draft, hand a message to the outbound pipeline,
/// or otherwise pretend the user replied. The user is saying
/// "never mind, I'm not going to reply" — the daemon must take that
/// at face value.
#[tokio::test]
async fn dispatch_clearing_reply_later_does_not_send_reply() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;
    let account_id = state.default_account_id();

    // Flag it first so we have something to dismiss.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 250,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetReplyLater {
                message_id: id.clone(),
                flag: true,
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // Capture a baseline of state that would change if a reply were
    // queued or sent.
    let drafts_before = state.store.list_drafts(&account_id).await.unwrap().len();
    let mut events = state.event_tx.subscribe();

    // Dismiss the flag — this is the "I'm not going to reply"
    // outcome the user signals by clearing the queue entry.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 251,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetReplyLater {
                message_id: id.clone(),
                flag: false,
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // No draft created, none consumed — the drafts table is exactly
    // where it was before the dismiss.
    let drafts_after = state.store.list_drafts(&account_id).await.unwrap().len();
    assert_eq!(
        drafts_after, drafts_before,
        "dismissing reply-later must not touch the drafts table"
    );

    // No daemon event was emitted by the dismiss. The flag clear
    // is a pure metadata edit; anything published here means the
    // path is doing more than the user asked for.
    match events.try_recv() {
        Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {} // expected
        Ok(received) => panic!(
            "dismissing reply-later must not emit any daemon event; got {:?}",
            received.payload
        ),
        Err(err) => panic!("unexpected event channel state: {err:?}"),
    }
}

#[tokio::test]
async fn dispatch_set_auto_reminder_persists_and_loop_fires_when_due() {
    // End-to-end: setting a reminder via IPC persists it; the
    // background-loop function fires it once `now >= remind_at` and
    // emits a `ReminderTriggered` event.
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;
    let mut events = state.event_tx.subscribe();

    // Set the reminder for "1 hour ago" so it's already due.
    let remind_at = chrono::Utc::now() - chrono::Duration::hours(1);
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 300,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetAutoReminder {
                sent_message_id: id.clone(),
                remind_at,
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // Run one tick of the loop with `now` past the reminder.
    let fired = crate::loops::process_due_reminders(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired, 1, "one due reminder fires");

    // Expect a ReminderTriggered event for the right message.
    let received = events.try_recv().expect("event published");
    match received.payload {
        IpcPayload::Event(DaemonEvent::ReminderTriggered { sent_message_id }) => {
            assert_eq!(sent_message_id, id);
        }
        other => panic!("expected ReminderTriggered event, got {other:?}"),
    }

    let queue = handle_request(
        &state,
        &IpcMessage {
            id: 302,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ListReplyQueue),
        },
    )
    .await;
    match queue.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyQueue { messages },
        }) => {
            assert!(
                messages.iter().any(|message| message.id == id),
                "due reminders must be visible in the reply-later queue"
            );
        }
        other => panic!("expected ReplyQueue response, got {other:?}"),
    }

    // Second tick: nothing fires (already-triggered reminders are
    // excluded).
    let fired_again = crate::loops::process_due_reminders(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired_again, 0, "fired reminders are not re-fired");
}

#[tokio::test]
async fn dispatch_cancel_auto_reminder_prevents_firing() {
    // Setting then cancelling a reminder leaves no due rows for
    // the loop to fire.
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let id = sync_and_get_first_id(&state).await;

    let remind_at = chrono::Utc::now() - chrono::Duration::hours(1);
    let _ = handle_request(
        &state,
        &IpcMessage {
            id: 310,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetAutoReminder {
                sent_message_id: id.clone(),
                remind_at,
            }),
        },
    )
    .await;
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 311,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::CancelAutoReminder {
                sent_message_id: id.clone(),
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    let fired = crate::loops::process_due_reminders(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired, 0, "cancelled reminders never fire");
}

#[tokio::test]
async fn dispatch_schedule_send_persists_and_loop_flushes_when_due() {
    // End-to-end: schedule an existing draft for a past send_at,
    // run one tick of the loop, expect the send pipeline to fire
    // and the draft's status to advance past 'draft'.
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let _ = sync_and_get_first_id(&state).await;

    // Insert a draft for the synthetic account.
    let account = state
        .store
        .list_accounts()
        .await
        .unwrap()
        .first()
        .unwrap()
        .clone();
    let draft = mxr_core::types::Draft {
        id: mxr_core::id::DraftId::new(),
        account_id: account.id.clone(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "you@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "scheduled".into(),
        body_markdown: "Body".into(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();

    // Schedule for "1 hour ago" — already due.
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 400,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ScheduleSend {
                draft_id: draft.id.clone(),
                send_at: chrono::Utc::now() - chrono::Duration::hours(1),
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));
    assert_eq!(
        state
            .store
            .get_scheduled_send(&draft.id)
            .await
            .unwrap()
            .is_some(),
        true,
        "send_at persisted"
    );

    // Run a tick of the flusher.
    let fired = crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired, 1);

    // Draft no longer needs sending: either advanced past `draft`
    // status (FakeProvider may delete on success) or is gone entirely.
    let status = state.store.get_draft_status(&draft.id).await.unwrap();
    assert!(
        !matches!(status, Some(mxr_core::types::DraftStatus::Draft)),
        "draft no longer in 'draft' status: {status:?}"
    );

    // The schedule entry is cleared (the row may be gone too) so a
    // second tick won't try to re-flush it.
    assert!(state
        .store
        .get_scheduled_send(&draft.id)
        .await
        .unwrap()
        .is_none());
    let fired_again = crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired_again, 0);
}

#[tokio::test]
async fn dispatch_cancel_scheduled_send_prevents_flush() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let _ = sync_and_get_first_id(&state).await;

    let account = state
        .store
        .list_accounts()
        .await
        .unwrap()
        .first()
        .unwrap()
        .clone();
    let draft = mxr_core::types::Draft {
        id: mxr_core::id::DraftId::new(),
        account_id: account.id.clone(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "you@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "scheduled-then-cancelled".into(),
        body_markdown: "Body".into(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();

    let _ = handle_request(
        &state,
        &IpcMessage {
            id: 410,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ScheduleSend {
                draft_id: draft.id.clone(),
                send_at: chrono::Utc::now() - chrono::Duration::hours(1),
            }),
        },
    )
    .await;
    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 411,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::CancelScheduledSend {
                draft_id: draft.id.clone(),
            }),
        },
    )
    .await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    let fired = crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fired, 0);

    // Draft remains in 'draft' status — never sent.
    assert_eq!(
        state.store.get_draft_status(&draft.id).await.unwrap(),
        Some(mxr_core::types::DraftStatus::Draft)
    );
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

#[tokio::test]
async fn dispatch_mutation_star() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
            message_ids: vec![id.clone()],
            starred: true,
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);

    // Verify flag is set
    let get_msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
    };
    let resp = handle_request(&state, &get_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelope { envelope },
        }) => {
            assert!(
                envelope
                    .flags
                    .contains(mxr_core::types::MessageFlags::STARRED),
                "Expected STARRED flag to be set, got {:?}",
                envelope.flags
            );
        }
        other => panic!("Expected Envelope, got {:?}", other),
    }
}

#[tokio::test]
async fn modify_labels_on_folder_provider_does_not_leave_one_message_in_two_folders() {
    let state = folder_copy_state().await;
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::ModifyLabels {
            message_ids: vec![id],
            add: vec!["Archive".to_string()],
            remove: vec![],
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);

    let envelopes = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 20, 0)
        .await
        .unwrap();
    assert_eq!(
        envelopes.len(),
        2,
        "expected exactly one inbox copy and one archive copy after folder add: {envelopes:?}"
    );
    assert!(
            !envelopes.iter().any(|envelope| {
                envelope
                    .label_provider_ids
                    .iter()
                    .any(|provider_id| provider_id == "INBOX")
                    && envelope
                        .label_provider_ids
                        .iter()
                        .any(|provider_id| provider_id == "Archive")
            }),
            "folder-based providers should not be flattened into one message with two folders: {envelopes:?}"
        );
    assert!(
        envelopes
            .iter()
            .any(|envelope| envelope.label_provider_ids == vec!["INBOX".to_string()]),
        "expected inbox copy after folder add"
    );
    assert!(
        envelopes
            .iter()
            .any(|envelope| envelope.label_provider_ids == vec!["Archive".to_string()]),
        "expected archive copy after folder add"
    );
}

#[tokio::test]
async fn snooze_on_folder_provider_reanchors_to_reconciled_message_copy() {
    let state = folder_copy_state().await;
    let original_id = sync_and_get_first_id(&state).await;

    let snooze = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Snooze {
            message_id: original_id.clone(),
            wake_at: chrono::Utc::now() + chrono::Duration::hours(4),
        }),
    };
    match handle_request(&state, &snooze).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack for Snooze, got {:?}", other),
    }

    let snoozed = state.store.list_snoozed().await.unwrap();
    assert_eq!(snoozed.len(), 1, "expected one snoozed message");
    assert_ne!(
        snoozed[0].message_id, original_id,
        "folder-backed snooze should track the reconciled message copy"
    );

    let archived = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 20, 0)
        .await
        .unwrap();
    assert_eq!(
        archived.len(),
        1,
        "expected exactly one archived copy after snooze: {archived:?}"
    );
    assert!(
        archived
            .iter()
            .all(|envelope| envelope.label_provider_ids == vec!["Archive".to_string()]),
        "expected only archived copy after snooze: {archived:?}"
    );

    let unsnooze = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsnooze {
            message_id: snoozed[0].message_id.clone(),
        }),
    };
    match handle_request(&state, &unsnooze).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack for Unsnooze, got {:?}", other),
    }

    let inbox = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 20, 0)
        .await
        .unwrap();
    assert_eq!(
        inbox.len(),
        1,
        "expected exactly one inbox copy after unsnooze: {inbox:?}"
    );
    assert!(
        inbox
            .iter()
            .all(|envelope| envelope.label_provider_ids == vec!["INBOX".to_string()]),
        "expected only inbox copy after unsnooze: {inbox:?}"
    );
    assert!(
        state.store.list_snoozed().await.unwrap().is_empty(),
        "expected snooze row to be cleared after unsnooze"
    );
}

#[tokio::test]
async fn snooze_on_folder_provider_errors_when_reconciled_copy_is_missing() {
    let state = folder_copy_state_with_mode(FolderCopyReanchorMode::MissingAfterArchive).await;
    let original_id = sync_and_get_first_id(&state).await;

    let snooze = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Snooze {
            message_id: original_id,
            wake_at: chrono::Utc::now() + chrono::Duration::hours(4),
        }),
    };
    match handle_request(&state, &snooze).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.contains("Reconciled message not found"),
                "expected missing reanchor error, got: {message}"
            );
        }
        other => panic!(
            "Expected Error for missing reconciled snooze copy, got {:?}",
            other
        ),
    }

    assert!(
        state.store.list_snoozed().await.unwrap().is_empty(),
        "expected no snooze row after failed reanchor"
    );
    assert!(
        state
            .store
            .list_envelopes_by_account(&state.default_account_id(), 20, 0)
            .await
            .unwrap()
            .is_empty(),
        "expected provider sync to reflect the missing reconciled copy"
    );
}

#[tokio::test]
async fn dispatch_mutation_set_read() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::SetRead {
            message_ids: vec![id.clone()],
            read: true,
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);

    let get_msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
    };
    let resp = handle_request(&state, &get_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelope { envelope },
        }) => {
            assert!(
                envelope.flags.contains(mxr_core::types::MessageFlags::READ),
                "Expected READ flag to be set, got {:?}",
                envelope.flags
            );
        }
        other => panic!("Expected Envelope, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_mutation_archive() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
            message_ids: vec![id.clone()],
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);

    let events = state
        .store
        .list_events(10, None, Some("mutation"))
        .await
        .unwrap();
    let id_str = id.as_str();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].message_id.as_deref(), Some(id_str.as_str()));
    assert!(events[0].summary.contains("Archived"));
}

/// Phase 1.4 / Behaviors 1+2+3+8: archive a message, observe the new
/// `mutation_id` in the response, undo it within the window, and
/// verify the message is back under the INBOX label both locally and
/// on the (fake) provider. Proves the snapshot capture, write,
/// reverse-op dispatch, and local restoration all line up.
#[tokio::test]
async fn undo_archive_restores_inbox_label() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    // Pre-condition: the message has the INBOX label.
    let pre = state.store.get_envelope(&id).await.unwrap().unwrap();
    assert!(
        pre.label_provider_ids.iter().any(|l| l == "INBOX"),
        "fixture must start in INBOX; got {:?}",
        pre.label_provider_ids
    );

    // Archive — captures snapshot, writes undo entry, returns mutation_id.
    let archive = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
            message_ids: vec![id.clone()],
        })),
    };
    let result = assert_mutation_succeeded(handle_request(&state, &archive).await.payload);
    let mutation_id = result
        .mutation_id
        .clone()
        .expect("Archive must return a mutation_id");

    let post_archive = state.store.get_envelope(&id).await.unwrap().unwrap();
    assert!(
        !post_archive.label_provider_ids.iter().any(|l| l == "INBOX"),
        "INBOX must be removed by Archive; got {:?}",
        post_archive.label_provider_ids
    );

    // Undo — restores INBOX both locally and via the fake provider.
    let undo = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UndoMutation {
            mutation_id: mutation_id.clone(),
        }),
    };
    let resp = handle_request(&state, &undo).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("expected Ack from UndoMutation; got {other:?}"),
    }

    let restored = state.store.get_envelope(&id).await.unwrap().unwrap();
    assert!(
        restored.label_provider_ids.iter().any(|l| l == "INBOX"),
        "INBOX must be restored after Undo; got {:?}",
        restored.label_provider_ids
    );

    // The undo entry is consumed — replaying the same id is now a no-op
    // (regression test for "user mashes u and double-undoes").
    let replay = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UndoMutation { mutation_id }),
    };
    match handle_request(&state, &replay).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.to_lowercase().contains("not found"),
                "second undo must return not-found; got {message}"
            );
        }
        other => panic!("expected Error on replay; got {other:?}"),
    }
}

/// Phase 1.4 / Behavior 4: Undo for an unknown id returns Error
/// with "not found" so the TUI can render the right message instead
/// of silently succeeding or panicking.
#[tokio::test]
async fn undo_unknown_mutation_id_returns_not_found() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UndoMutation {
            mutation_id: "01HVTOTALLYBOGUSID0000000".into(),
        }),
    };
    match handle_request(&state, &msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.to_lowercase().contains("not found"),
                "expected not-found error; got {message}"
            );
        }
        other => panic!("expected Error; got {other:?}"),
    }
}

/// Phase 1.4 / Behavior 6: a bulk Archive of multiple messages
/// produces a single mutation_id and a single Undo restores all of
/// them. Catches regressions where snapshots are dropped or only the
/// first envelope is restored.
#[tokio::test]
async fn undo_bulk_archive_restores_all_messages() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    // Sync first to populate the fixture.
    let _ = sync_and_get_first_id(&state).await;

    // Pull three INBOX-tagged messages by listing envelopes.
    let list_msg = IpcMessage {
        id: 100,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 3,
            offset: 0,
        }),
    };
    let envelopes = match handle_request(&state, &list_msg).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => envelopes,
        other => panic!("expected Envelopes; got {other:?}"),
    };
    let ids: Vec<mxr_core::MessageId> = envelopes.iter().take(3).map(|e| e.id.clone()).collect();
    assert!(ids.len() >= 2, "fixture must contain >=2 messages");

    let archive = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
            message_ids: ids.clone(),
        })),
    };
    let result = assert_mutation_succeeded(handle_request(&state, &archive).await.payload);
    let mutation_id = result.mutation_id.clone().expect("mutation_id required");
    assert_eq!(result.succeeded, ids.len() as u32);

    let undo = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UndoMutation { mutation_id }),
    };
    match handle_request(&state, &undo).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("expected Ack; got {other:?}"),
    }

    // Every archived message should now have INBOX again.
    for id in &ids {
        let env = state.store.get_envelope(id).await.unwrap().unwrap();
        assert!(
            env.label_provider_ids.iter().any(|l| l == "INBOX"),
            "{id} must have INBOX restored; got {:?}",
            env.label_provider_ids
        );
    }
}

/// Phase 1.4: Star is not undoable — the response carries no
/// mutation_id so clients know not to render the undo affordance.
#[tokio::test]
async fn star_mutation_omits_mutation_id() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
            message_ids: vec![id],
            starred: true,
        })),
    };
    let result = assert_mutation_succeeded(handle_request(&state, &msg).await.payload);
    assert!(
        result.mutation_id.is_none(),
        "Star must not return a mutation_id; got {:?}",
        result.mutation_id
    );
}

#[tokio::test]
async fn mutation_archives_healthy_account_when_other_account_provider_fails() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let healthy_id = sync_and_get_first_id(&state).await;
    let failing_calls = Arc::new(AtomicUsize::new(0));
    add_failing_sync_account(&state, failing_calls.clone()).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
            message_ids: vec![healthy_id],
        })),
    };
    let result = assert_mutation_succeeded(handle_request(&state, &msg).await.payload);

    assert_eq!(result.requested, 1);
    assert_eq!(result.succeeded, 1);
    assert_eq!(result.skipped, 0);
    assert_eq!(failing_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn mixed_account_mutation_returns_partial_success() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let healthy_id = sync_and_get_first_id(&state).await;
    let failing_calls = Arc::new(AtomicUsize::new(0));
    let (bad_account_id, bad_id) = add_failing_sync_account(&state, failing_calls.clone()).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
            message_ids: vec![healthy_id, bad_id],
        })),
    };
    let result = assert_mutation_succeeded(handle_request(&state, &msg).await.payload);

    assert_eq!(result.requested, 2);
    assert_eq!(result.succeeded, 1);
    assert_eq!(result.skipped, 1);
    assert_eq!(failing_calls.load(Ordering::SeqCst), 1);
    let bad_account = result
        .accounts
        .iter()
        .find(|account| account.account_id == bad_account_id)
        .expect("bad account result");
    assert_eq!(bad_account.succeeded, 0);
    assert_eq!(bad_account.skipped, 1);
    assert!(bad_account
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("keychain"));
}

#[tokio::test]
async fn dispatch_mutation_read_and_archive() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::ReadAndArchive {
            message_ids: vec![id.clone()],
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);

    let envelope = state
        .store
        .get_envelope(&id)
        .await
        .unwrap()
        .expect("message should still exist");
    assert!(envelope.flags.contains(mxr_core::types::MessageFlags::READ));

    let label_ids = state.store.get_message_label_ids(&id).await.unwrap();
    assert!(!label_ids
        .iter()
        .any(|label_id| label_id.as_str() == "INBOX"));

    let events = state
        .store
        .list_events(10, None, Some("mutation"))
        .await
        .unwrap();
    assert!(events[0].summary.contains("read and archived"));
}

#[tokio::test]
async fn dispatch_mutation_trash() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Trash {
            message_ids: vec![id],
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);
}

#[tokio::test]
async fn dispatch_prepare_reply() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let expected_subject = state
        .store
        .get_envelope(&id)
        .await
        .unwrap()
        .unwrap()
        .subject;

    // Fetch body first so it's cached
    let body_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    handle_request(&state, &body_msg).await;

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::PrepareReply {
            message_id: id,
            reply_all: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyContext { context },
        }) => {
            assert!(context.reply_to.contains('@'));
            assert_eq!(context.subject, expected_subject);
        }
        other => panic!("Expected ReplyContext, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_prepare_reply_all() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let expected_subject = state
        .store
        .get_envelope(&id)
        .await
        .unwrap()
        .unwrap()
        .subject;

    // Fetch body first
    let body_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    handle_request(&state, &body_msg).await;

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::PrepareReply {
            message_id: id,
            reply_all: true,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyContext { context },
        }) => {
            assert!(context.reply_to.contains('@'));
            assert_eq!(context.subject, expected_subject);
            // cc may or may not be empty depending on the message, but the field should exist
        }
        other => panic!("Expected ReplyContext, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_prepare_reply_renders_html_context() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: None,
            text_html: Some("<p>Hello <b>world</b></p>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        })
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::PrepareReply {
            message_id: id,
            reply_all: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyContext { context },
        }) => {
            assert!(context.thread_context.contains("Hello world"));
            assert!(!context.thread_context.contains("<p>"));
        }
        other => panic!("Expected ReplyContext, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_prepare_forward() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let expected_subject = state
        .store
        .get_envelope(&id)
        .await
        .unwrap()
        .unwrap()
        .subject;

    // Fetch body first
    let body_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    handle_request(&state, &body_msg).await;

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::PrepareForward { message_id: id }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ForwardContext { context },
        }) => {
            assert_eq!(context.subject, expected_subject);
            assert!(
                !context.forwarded_content.is_empty(),
                "forwarded_content should be non-empty"
            );
        }
        other => panic!("Expected ForwardContext, got {:?}", other),
    }
}

#[tokio::test]
async fn modify_labels_persists_to_store_immediately() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let create = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Follow Up".into(),
            color: None,
            account_id: None,
        }),
    };
    let label = match handle_request(&state, &create).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Label { label },
        }) => label,
        other => panic!("Expected Label response, got {:?}", other),
    };

    let modify = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::ModifyLabels {
            message_ids: vec![id.clone()],
            add: vec![label.name.clone()],
            remove: vec![],
        })),
    };
    assert_mutation_succeeded(handle_request(&state, &modify).await.payload);

    let label_ids = state.store.get_message_label_ids(&id).await.unwrap();
    assert!(label_ids.iter().any(|label_id| label_id == &label.id));
}

#[tokio::test]
async fn get_thread_includes_message_label_provider_ids() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let envelope = state.store.get_envelope(&id).await.unwrap().unwrap();

    let create = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Recruiters".into(),
            color: None,
            account_id: None,
        }),
    };
    let label = match handle_request(&state, &create).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Label { label },
        }) => label,
        other => panic!("Expected Label response, got {:?}", other),
    };

    state
        .store
        .add_message_label(&id, &label.id, mxr_core::EventSource::User)
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetThread {
            thread_id: envelope.thread_id,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Thread { messages, .. },
        }) => {
            let message = messages
                .into_iter()
                .find(|message| message.id == id)
                .unwrap();
            assert!(message
                .label_provider_ids
                .iter()
                .any(|provider_id| provider_id == &label.provider_id));
        }
        other => panic!("Expected Thread response, got {:?}", other),
    }
}

#[tokio::test]
async fn list_envelopes_includes_message_label_provider_ids() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let create = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Recruiters".into(),
            color: None,
            account_id: None,
        }),
    };
    let label = match handle_request(&state, &create).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Label { label },
        }) => label,
        other => panic!("Expected Label response, got {:?}", other),
    };

    state
        .store
        .add_message_label(&id, &label.id, mxr_core::EventSource::User)
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 200,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => {
            let envelope = envelopes
                .into_iter()
                .find(|envelope| envelope.id == id)
                .unwrap();
            assert!(envelope
                .label_provider_ids
                .iter()
                .any(|provider_id| provider_id == &label.provider_id));
        }
        other => panic!("Expected Envelopes response, got {:?}", other),
    }
}

#[tokio::test]
async fn list_accounts_surfaces_runtime_accounts_without_config_entries() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListAccounts),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Accounts { accounts },
        }) => {
            assert_eq!(accounts.len(), 1);
            assert_eq!(accounts[0].email, "user@example.com");
            assert_eq!(accounts[0].source, AccountSourceData::Runtime);
            assert_eq!(accounts[0].editable, AccountEditModeData::RuntimeOnly);
            assert!(accounts[0].is_default);
        }
        other => panic!("Expected Accounts response, got {:?}", other),
    }
}

#[tokio::test]
async fn get_llm_status_reports_noop_provider_by_default() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetLlmStatus),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::LlmStatus { snapshot },
        }) => {
            assert!(!snapshot.enabled);
            assert_eq!(snapshot.provider, "noop");
            assert_eq!(snapshot.model, "noop");
            assert_eq!(snapshot.configured_model, "qwen2.5:3b-instruct");
            assert_eq!(snapshot.base_url, None);
            assert_eq!(snapshot.context_window, 0);
        }
        other => panic!("Expected LlmStatus response, got {:?}", other),
    }
}

#[tokio::test]
async fn config_reload_rebuilds_llm_provider_for_status() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.llm.enabled = true;
    config.llm.model = "local-test-model".to_string();
    config.llm.base_url = "http://127.0.0.1:11434/v1".to_string();
    config.llm.context_window = 4096;
    config.llm.request_timeout_secs = 30;
    state.set_config_for_test(config).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetLlmStatus),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::LlmStatus { snapshot },
        }) => {
            assert!(snapshot.enabled);
            assert_eq!(snapshot.provider, "openai_compatible");
            assert_eq!(snapshot.model, "local-test-model");
            assert_eq!(snapshot.configured_model, "local-test-model");
            assert_eq!(
                snapshot.base_url.as_deref(),
                Some("http://127.0.0.1:11434/v1")
            );
            assert_eq!(snapshot.context_window, 4096);
            assert_eq!(snapshot.request_timeout_secs, 30);
        }
        other => panic!("Expected LlmStatus response, got {:?}", other),
    }
}

#[test]
fn update_llm_config_persists_and_rebuilds_provider_status() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let config_dir = temp_dir.path().join("config");
    let data_dir = temp_dir.path().join("data");
    let socket_path = temp_dir.path().join("mxr.sock");
    std::fs::create_dir_all(&config_dir).expect("config dir");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    temp_env::with_vars(
        [
            ("MXR_CONFIG_DIR", Some(config_dir)),
            ("MXR_DATA_DIR", Some(data_dir)),
            ("MXR_SOCKET_PATH", Some(socket_path)),
        ],
        || {
            runtime.block_on(async {
                mxr_config::save_config(&mxr_config::MxrConfig::default())
                    .expect("save default config");
                let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
                let msg = IpcMessage {
                    id: 1,
                    source: ::mxr_protocol::ClientKind::default(),
                    payload: IpcPayload::Request(Request::UpdateLlmConfig {
                        config: Box::new(mxr_protocol::LlmConfigData {
                            enabled: true,
                            base_url: "http://127.0.0.1:11434/v1".into(),
                            model: "local-test-model".into(),
                            api_key_env: "MXR_TEST_LLM_KEY".into(),
                            context_window: 4096,
                            request_timeout_secs: 30,
                            allow_cloud_relationship_data: true,
                            overrides: None,
                        }),
                    }),
                };

                let resp = handle_request(&state, &msg).await;
                match resp.payload {
                    IpcPayload::Response(Response::Ok {
                        data: ResponseData::LlmConfig { config },
                    }) => {
                        assert!(config.enabled);
                        assert_eq!(config.model, "local-test-model");
                        assert!(config.allow_cloud_relationship_data);
                    }
                    other => panic!("Expected LlmConfig response, got {other:?}"),
                }

                let saved = mxr_config::load_config().expect("load saved config");
                assert!(saved.llm.enabled);
                assert_eq!(saved.llm.model, "local-test-model");
                assert_eq!(saved.llm.api_key_env, "MXR_TEST_LLM_KEY");
                assert!(saved.llm.allow_cloud_relationship_data);

                let status_msg = IpcMessage {
                    id: 2,
                    source: ::mxr_protocol::ClientKind::default(),
                    payload: IpcPayload::Request(Request::GetLlmStatus),
                };
                let status_resp = handle_request(&state, &status_msg).await;
                match status_resp.payload {
                    IpcPayload::Response(Response::Ok {
                        data: ResponseData::LlmStatus { snapshot },
                    }) => {
                        assert!(snapshot.enabled);
                        assert_eq!(snapshot.provider, "openai_compatible");
                        assert_eq!(snapshot.model, "local-test-model");
                        assert_eq!(snapshot.context_window, 4096);
                        assert_eq!(snapshot.request_timeout_secs, 30);
                    }
                    other => panic!("Expected LlmStatus response, got {other:?}"),
                }
            });
        },
    );
}

#[tokio::test]
async fn update_llm_config_rejects_blank_model() {
    let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UpdateLlmConfig {
            config: Box::new(mxr_protocol::LlmConfigData {
                enabled: true,
                base_url: "http://127.0.0.1:11434/v1".into(),
                model: "  ".into(),
                api_key_env: String::new(),
                context_window: 4096,
                request_timeout_secs: 30,
                allow_cloud_relationship_data: false,
                overrides: None,
            }),
        }),
    };

    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("llm.model must not be empty"));
        }
        other => panic!("Expected error response, got {other:?}"),
    }
    assert_eq!(
        state.config_snapshot().llm.model,
        mxr_config::LlmConfig::default().model
    );
}

#[tokio::test]
async fn dispatch_send_draft() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: state.default_account_id(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Test subject".to_string(),
        body_markdown: "Test body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendDraft {
            draft,
            override_safety_token: None,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SendReceipt { .. },
        }) => {}
        other => panic!("Expected SendReceipt, got {:?}", other),
    }
}

#[tokio::test]
async fn draft_only_safety_policy_blocks_send_but_allows_local_draft() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.general.safety_policy = mxr_config::SafetyPolicy::DraftOnly;
    state.set_config_for_test(config).await;

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: state.default_account_id(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Draft-only policy".to_string(),
        body_markdown: "Test body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let send = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendDraft {
            draft: draft.clone(),
            override_safety_token: None,
        }),
    };
    match handle_request(&state, &send).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("draft-only safety policy"));
        }
        other => panic!("Expected safety policy error, got {:?}", other),
    }

    let save = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SaveDraft { draft }),
    };
    match handle_request(&state, &save).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected SaveDraft Ack, got {:?}", other),
    }
}

#[tokio::test]
async fn read_only_safety_policy_blocks_mutations_but_allows_search() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.general.safety_policy = mxr_config::SafetyPolicy::ReadOnly;
    state.set_config_for_test(config).await;

    let mutation = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
            message_ids: vec![mxr_core::MessageId::new()],
            starred: true,
        })),
    };
    match handle_request(&state, &mutation).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("read-only safety policy"));
        }
        other => panic!("Expected safety policy error, got {:?}", other),
    }

    let search = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "hello".into(),
            limit: 10,
            offset: 0,
            mode: None,
            sort: None,
            explain: false,
        }),
    };
    match handle_request(&state, &search).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SearchResults { .. },
        }) => {}
        other => panic!("Expected SearchResults, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_send_draft_preserves_keychain_repair_error() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let sync_provider = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = Arc::new(FailingSendProvider {
            message: "Keyring error: Password for mxr/consulting-smtp/hello@bhekani.com requires interactive macOS keychain approval. Re-save that account password once with `mxr accounts repair`.",
        });
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Test subject".to_string(),
        body_markdown: "Test body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendDraft {
            draft,
            override_safety_token: None,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("consulting-smtp"));
            assert!(message.contains("mxr accounts repair"));
        }
        other => panic!("Expected send error, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_snooze_and_list() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    // Snooze
    let wake_at = chrono::Utc::now() + chrono::Duration::hours(24);
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Snooze {
            message_id: id.clone(),
            wake_at,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack for Snooze, got {:?}", other),
    }

    // List snoozed - should have 1
    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSnoozed),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SnoozedMessages { snoozed },
        }) => {
            assert_eq!(snoozed.len(), 1, "Expected 1 snoozed message");
        }
        other => panic!("Expected SnoozedMessages, got {:?}", other),
    }

    // Unsnooze
    let msg = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsnooze { message_id: id }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack for Unsnooze, got {:?}", other),
    }

    // List snoozed - should have 0
    let msg = IpcMessage {
        id: 4,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSnoozed),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SnoozedMessages { snoozed },
        }) => {
            assert_eq!(
                snoozed.len(),
                0,
                "Expected 0 snoozed messages after unsnooze"
            );
        }
        other => panic!("Expected SnoozedMessages, got {:?}", other),
    }
}

#[tokio::test]
async fn snooze_removes_inbox_and_unsnooze_restores_it() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let envelope = state.store.get_envelope(&id).await.unwrap().unwrap();
    let inbox = state
        .store
        .list_labels_by_account(&envelope.account_id)
        .await
        .unwrap()
        .into_iter()
        .find(|label| label.provider_id == "INBOX")
        .unwrap();

    let before = state.store.get_message_label_ids(&id).await.unwrap();
    assert!(before.iter().any(|label_id| label_id == &inbox.id));

    let snooze = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Snooze {
            message_id: id.clone(),
            wake_at: chrono::Utc::now() + chrono::Duration::hours(4),
        }),
    };
    match handle_request(&state, &snooze).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {:?}", other),
    }

    let snoozed_labels = state.store.get_message_label_ids(&id).await.unwrap();
    assert!(!snoozed_labels.iter().any(|label_id| label_id == &inbox.id));

    let unsnooze = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsnooze {
            message_id: id.clone(),
        }),
    };
    match handle_request(&state, &unsnooze).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {:?}", other),
    }

    let restored_labels = state.store.get_message_label_ids(&id).await.unwrap();
    assert!(restored_labels.iter().any(|label_id| label_id == &inbox.id));
}

#[tokio::test]
async fn dispatch_set_flags() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    use mxr_core::types::MessageFlags;
    let flags = MessageFlags::READ | MessageFlags::STARRED;
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SetFlags {
            message_id: id.clone(),
            flags,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {:?}", other),
    }

    // Verify flags
    let get_msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
    };
    let resp = handle_request(&state, &get_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelope { envelope },
        }) => {
            assert_eq!(
                envelope.flags, flags,
                "Expected flags {:?}, got {:?}",
                flags, envelope.flags
            );
        }
        other => panic!("Expected Envelope, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_unsubscribe_no_method() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    // The first envelope from FakeProvider fixtures uses UnsubscribeMethod::None
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsubscribe { message_id: id }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.contains("unsubscribe"),
                "Expected error about unsubscribe, got: {}",
                message
            );
        }
        other => panic!("Expected Error for no unsubscribe method, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_unsubscribe_mailto_sends_via_provider() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let mailto_id = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 200, 0)
        .await
        .unwrap()
        .into_iter()
        .find(|envelope| matches!(envelope.unsubscribe, UnsubscribeMethod::Mailto { .. }))
        .map(|envelope| envelope.id)
        .expect("mailto fixture");

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsubscribe {
            message_id: mailto_id,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack for mailto unsubscribe, got {:?}", other),
    }

    let sent = fake.sent_drafts();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to[0].email, "unsub@changelog.com");
    assert_eq!(sent[0].subject, "unsubscribe");
}

/// Phase 2.6: `mxr unsubscribe <id>` is idempotent. A second call
/// against the same message must NOT re-send the mailto / re-POST
/// the one-click URL — the user's intent on the second call is "I
/// already unsubscribed, stop bugging me." Without this guard, a
/// shell retry / agent loop would spam the list operator's inbox.
/// Phase 1.5: saved-search unread counts return one entry per
/// configured saved search. Counts reflect the saved query
/// ANDed with `is:unread`. The tab strip uses this to render
/// `(N)` on each tab.
#[tokio::test]
async fn dispatch_list_saved_search_unread_counts_returns_id_to_count_map() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    // Register two saved searches with predictable shapes:
    //   "All Mail"  = "" (empty query) — matches everything;
    //                  unread count == number of unread messages
    //   "Nonexistent" = "from:nobody@nope.example" — zero matches
    let now = chrono::Utc::now();
    let search_all = mxr_core::types::SavedSearch {
        id: mxr_core::id::SavedSearchId::new(),
        account_id: None,
        name: "All Mail".to_string(),
        query: String::new(),
        search_mode: mxr_core::SearchMode::Lexical,
        sort: mxr_core::SortOrder::DateDesc,
        icon: None,
        position: 0,
        created_at: now,
    };
    let search_none = mxr_core::types::SavedSearch {
        id: mxr_core::id::SavedSearchId::new(),
        account_id: None,
        name: "Nonexistent".to_string(),
        query: "from:nobody@nope.example".to_string(),
        search_mode: mxr_core::SearchMode::Lexical,
        sort: mxr_core::SortOrder::DateDesc,
        icon: None,
        position: 1,
        created_at: now,
    };
    state.store.insert_saved_search(&search_all).await.unwrap();
    state.store.insert_saved_search(&search_none).await.unwrap();

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ListSavedSearchUnreadCounts),
        },
    )
    .await;
    let counts = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearchUnreadCounts { counts },
        }) => counts,
        other => panic!("expected SavedSearchUnreadCounts, got {other:?}"),
    };

    // Both saved searches appear in the response (even the
    // zero-match one — the tab strip needs to know it exists).
    assert!(
        counts.contains_key(&search_all.id),
        "every registered saved search must be present in the count map; missing All Mail"
    );
    assert!(
        counts.contains_key(&search_none.id),
        "every registered saved search must be present in the count map; missing Nonexistent"
    );
    assert_eq!(
        counts[&search_none.id], 0,
        "the never-matching saved search reports zero unread"
    );
    // We don't assert an exact number for All Mail because the
    // FakeProvider fixture set evolves; we just assert it's
    // non-negative (always true for u32) and the response shape
    // is correct.
}

#[tokio::test]
async fn dispatch_unsubscribe_is_idempotent_via_event_log() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let mailto_id = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 200, 0)
        .await
        .unwrap()
        .into_iter()
        .find(|envelope| matches!(envelope.unsubscribe, UnsubscribeMethod::Mailto { .. }))
        .map(|envelope| envelope.id)
        .expect("mailto fixture");

    let request = || IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsubscribe {
            message_id: mailto_id.clone(),
        }),
    };

    // First call: succeeds and emits the outbound message.
    let resp = handle_request(&state, &request()).await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));
    assert_eq!(
        fake.sent_drafts().len(),
        1,
        "first call sends the unsubscribe mail"
    );

    // Second call: also returns Ack but MUST NOT re-send.
    let resp = handle_request(&state, &request()).await;
    assert!(
        matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ),
        "repeated unsubscribe should still ack so scripts and agents don't see a spurious failure"
    );
    assert_eq!(
        fake.sent_drafts().len(),
        1,
        "second call must not produce a second outbound — that's the entire point of idempotency"
    );
}

/// Phase 2.6: when the unsubscribe URL fails (network error,
/// non-2xx), the handler must surface an Error response. Quietly
/// returning Ack would mislead the user into thinking they were
/// removed from the list when nothing happened. Equally critically,
/// no `_unsubscribed` event must be logged on a failed attempt —
/// otherwise the idempotency check would block a future retry.
#[tokio::test]
async fn dispatch_unsubscribe_oneclick_failure_returns_error_and_does_not_log() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    // Replace the fixture envelope's unsubscribe method with a
    // OneClick URL pointing at a port nothing's listening on — the
    // POST will reliably fail.
    let mut envelope = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 200, 0)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("at least one fixture envelope");
    envelope.unsubscribe = UnsubscribeMethod::OneClick {
        // 127.0.0.1:1 — RFC 6890 / "definitely-no-listener" port.
        url: "http://127.0.0.1:1/unsubscribe".into(),
    };
    state
        .store
        .upsert_envelope_with_direction(&envelope, mxr_core::types::MessageDirection::Inbound)
        .await
        .unwrap();

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::Unsubscribe {
                message_id: envelope.id.clone(),
            }),
        },
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Error { .. }) => {} // expected
        other => panic!("expected Error for failed one-click POST, got {other:?}"),
    }

    // No success event was logged. If it were, a retry would be
    // blocked by the idempotency short-circuit.
    let logged = state
        .store
        .has_event_for_message_with_summary(&envelope.id.as_str(), "mutation", "unsubscrib")
        .await
        .unwrap();
    assert!(
            !logged,
            "a failed unsubscribe must not write a success event — otherwise retries are silently blocked"
        );
}

#[tokio::test]
async fn dispatch_mutation_nonexistent_message() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let fake_id = mxr_core::MessageId::new();
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
            message_ids: vec![fake_id],
            starred: true,
        })),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.contains("not found") || message.contains("Not found"),
                "Expected 'not found' error, got: {}",
                message
            );
        }
        other => panic!("Expected Error, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_list_drafts_empty() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListDrafts),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Drafts { drafts },
        }) => {
            assert!(drafts.is_empty(), "Expected empty drafts list");
        }
        other => panic!("Expected Drafts, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_list_drafts_includes_all_accounts() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let default_account_id = state.default_account_id();
    let other_account_id = mxr_core::AccountId::new();
    let other_account = crate::test_fixtures::test_account_with_id(other_account_id.clone());
    state.store.insert_account(&other_account).await.unwrap();

    let old_draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: default_account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "Default account draft".to_string(),
        body_markdown: "older".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now() - chrono::Duration::minutes(5),
        updated_at: chrono::Utc::now() - chrono::Duration::minutes(5),
    };
    let new_draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: other_account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "Other account draft".to_string(),
        body_markdown: "newer".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&old_draft).await.unwrap();
    state.store.insert_draft(&new_draft).await.unwrap();

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListDrafts),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Drafts { drafts },
        }) => {
            assert_eq!(drafts.len(), 2);
            assert_eq!(drafts[0].id, new_draft.id);
            assert_eq!(drafts[1].id, old_draft.id);
        }
        other => panic!("Expected Drafts, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_save_and_send_stored_draft() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Stored draft".to_string(),
        body_markdown: "Test body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let save_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SaveDraft {
            draft: draft.clone(),
        }),
    };
    let save_resp = handle_request(&state, &save_msg).await;
    assert!(matches!(
        save_resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    let send_msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
            draft_id: draft.id.clone(),
            override_safety_token: None,
        }),
    };
    let send_resp = handle_request(&state, &send_msg).await;
    assert!(
        matches!(
            send_resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SendReceipt { .. }
            })
        ),
        "send_stored_draft should return SendReceipt, got {:?}",
        send_resp.payload
    );

    assert_eq!(fake.sent_drafts().len(), 1);
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_none());
}

/// Slice 1.3: when CheckDraftSafety returns Blocked, the daemon
/// mints a single-use override token and stamps it onto each
/// blocker issue. The next SendStoredDraft with that token must
/// succeed (and FakeProvider must actually be invoked exactly once),
/// while a second send attempt with the same token must fail with
/// the token already-used error.
#[tokio::test]
async fn override_token_unblocks_send_exactly_once() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    // PEM private key in the body → Blocker.
    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: account_id.clone(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "alice@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "key transfer".to_string(),
        body_markdown: "Here is the key:\n-----BEGIN RSA PRIVATE KEY-----\n...\n".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    // Save the draft so SendStoredDraft can locate it.
    let save = handle_request(
        &state,
        &IpcMessage {
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SaveDraft {
                draft: draft.clone(),
            }),
        },
    )
    .await;
    assert!(matches!(
        save.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // 1. Check returns Blocked + a token on the blocker issue.
    let check = handle_request(
        &state,
        &IpcMessage {
            id: 2,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::CheckDraftSafety {
                draft: draft.clone(),
                context: Default::default(),
            }),
        },
    )
    .await;
    let token = match check.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::DraftSafetyReportResponse { report },
        }) => {
            assert!(matches!(
                report.verdict,
                mxr_core::DraftSafetyVerdict::Blocked
            ));
            let blocker = report
                .issues
                .iter()
                .find(|i| i.severity == mxr_core::DraftSafetySeverity::Blocker)
                .expect("at least one blocker");
            blocker
                .override_token
                .clone()
                .expect("blocker should carry override token")
        }
        other => panic!("expected DraftSafetyReportResponse, got {other:?}"),
    };

    // 2. SendStoredDraft WITHOUT the token: refused, FakeProvider untouched.
    assert_eq!(fake.sent_drafts().len(), 0);
    let blocked = handle_request(
        &state,
        &IpcMessage {
            id: 3,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
                draft_id: draft.id.clone(),
                override_safety_token: None,
            }),
        },
    )
    .await;
    match blocked.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("blocked"), "{message}");
        }
        other => panic!("expected Error, got {other:?}"),
    }
    assert_eq!(
        fake.sent_drafts().len(),
        0,
        "provider must NOT be called when blocked"
    );
    // Draft must still be in `Draft` status (no CAS to Sending).
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_some());

    // 3. SendStoredDraft WITH token: succeeds.
    let ok = handle_request(
        &state,
        &IpcMessage {
            id: 4,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
                draft_id: draft.id.clone(),
                override_safety_token: Some(token.clone()),
            }),
        },
    )
    .await;
    assert!(
        matches!(
            ok.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SendReceipt { .. }
            })
        ),
        "expected SendReceipt with override, got {:?}",
        ok.payload
    );
    assert_eq!(fake.sent_drafts().len(), 1);

    // 4. Reusing the same token after the draft is gone — token is
    // single-use; consume must fail. We test by minting a fresh
    // override against a new draft, sending once, then trying the
    // SAME token a second time to assert single-use.
    let draft2 = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: account_id.clone(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "bob@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "again".into(),
        body_markdown: "-----BEGIN RSA PRIVATE KEY-----\nzz\n".into(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let _ = handle_request(
        &state,
        &IpcMessage {
            id: 5,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SaveDraft {
                draft: draft2.clone(),
            }),
        },
    )
    .await;
    let check2 = handle_request(
        &state,
        &IpcMessage {
            id: 6,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::CheckDraftSafety {
                draft: draft2.clone(),
                context: Default::default(),
            }),
        },
    )
    .await;
    let token2 = match check2.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::DraftSafetyReportResponse { report },
        }) => report
            .issues
            .iter()
            .find(|i| i.severity == mxr_core::DraftSafetySeverity::Blocker)
            .and_then(|i| i.override_token.clone())
            .expect("blocker token"),
        other => panic!("unexpected: {other:?}"),
    };
    // First use succeeds.
    let first = handle_request(
        &state,
        &IpcMessage {
            id: 7,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
                draft_id: draft2.id.clone(),
                override_safety_token: Some(token2.clone()),
            }),
        },
    )
    .await;
    assert!(matches!(
        first.payload,
        IpcPayload::Response(Response::Ok { .. })
    ));
    // Second use with the SAME token must fail (token consumed). We
    // can't re-send the same draft (already gone after send), so we
    // make a third draft and try to use the spent token.
    let draft3 = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        ..draft2
    };
    let _ = handle_request(
        &state,
        &IpcMessage {
            id: 8,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SaveDraft {
                draft: draft3.clone(),
            }),
        },
    )
    .await;
    let reuse = handle_request(
        &state,
        &IpcMessage {
            id: 9,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
                draft_id: draft3.id.clone(),
                override_safety_token: Some(token2),
            }),
        },
    )
    .await;
    match reuse.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.contains("override token unknown or already used")
                    || message.contains("does not cover blocker"),
                "got {message}"
            );
        }
        other => panic!("expected error on token reuse, got {other:?}"),
    }
}

/// The live send pipeline must touch `last_heartbeat_at` once it has
/// CAS'd a draft into `Sending`. Otherwise, a long-running send (large
/// attachment, slow OAuth refresh) could be misidentified as orphaned
/// by the 1h startup recovery cutoff. We verify this by exercising the
/// failure path: with no send provider configured, `send_stored_draft`
/// CAS's into `Sending`, touches the heartbeat, then reverts to
/// `Draft` when provider lookup fails — leaving a fresh heartbeat we
/// can read back.
#[tokio::test]
async fn send_stored_draft_touches_heartbeat_after_cas() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    // No send provider — `send_provider_for_account` will fail.
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, None)
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Heartbeat probe".to_string(),
        body_markdown: "Body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();
    // Pre-condition: a brand-new draft has no heartbeat.
    assert_eq!(
        state.store.get_draft_heartbeat(&draft.id).await.unwrap(),
        None,
        "fresh draft must have NULL last_heartbeat_at"
    );

    let before = chrono::Utc::now();
    let send_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
            draft_id: draft.id.clone(),
            override_safety_token: None,
        }),
    };
    let send_resp = handle_request(&state, &send_msg).await;
    assert!(
        matches!(
            send_resp.payload,
            IpcPayload::Response(Response::Error { .. })
        ),
        "send_stored_draft without a send provider must error, got {:?}",
        send_resp.payload
    );

    // Post-condition: heartbeat was set during the CAS-to-Sending phase
    // and survives the revert-to-Draft on provider-lookup failure.
    let heartbeat = state
        .store
        .get_draft_heartbeat(&draft.id)
        .await
        .unwrap()
        .expect("send_stored_draft must touch the heartbeat after CAS");
    let after = chrono::Utc::now();
    assert!(
        heartbeat >= before - chrono::Duration::seconds(1),
        "heartbeat {heartbeat} must not predate test start {before}"
    );
    assert!(
        heartbeat <= after + chrono::Duration::seconds(1),
        "heartbeat {heartbeat} must not postdate test end {after}"
    );
}

#[tokio::test]
async fn send_stored_draft_blocks_empty_recipient_before_sending_state() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "No recipients".to_string(),
        body_markdown: "Body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();

    let send_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
            draft_id: draft.id.clone(),
            override_safety_token: None,
        }),
    };
    match handle_request(&state, &send_msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("draft safety"));
            assert!(message.contains("recipient"));
        }
        other => panic!("Expected draft safety error, got {other:?}"),
    }

    assert_eq!(
        state.store.get_draft_status(&draft.id).await.unwrap(),
        Some(mxr_core::DraftStatus::Draft)
    );
    assert_eq!(
        state.store.get_draft_heartbeat(&draft.id).await.unwrap(),
        None
    );
    assert_eq!(fake.sent_drafts().len(), 0);
}

#[tokio::test]
async fn send_draft_blocks_invalid_recipient_before_provider_send() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "not an address".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Invalid recipient".to_string(),
        body_markdown: "Body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let send_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendDraft {
            draft,
            override_safety_token: None,
        }),
    };
    match handle_request(&state, &send_msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("draft safety"));
            assert!(message.contains("invalid recipient"));
        }
        other => panic!("Expected draft safety error, got {other:?}"),
    }

    assert_eq!(fake.sent_drafts().len(), 0);
}

#[tokio::test]
async fn send_stored_reply_all_blocks_missing_original_recipient_before_sending_state() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let account_email = account.email.clone();
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let mut parent = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .provider_id("reply-all-parent")
        .message_id_header(Some("<reply-all-parent@example.com>".to_string()))
        .build();
    parent.from = mxr_core::types::Address {
        name: None,
        email: "alice@example.com".to_string(),
    };
    parent.to = vec![
        mxr_core::types::Address {
            name: None,
            email: account_email,
        },
        mxr_core::types::Address {
            name: None,
            email: "bob@example.com".to_string(),
        },
    ];
    parent.cc = vec![mxr_core::types::Address {
        name: None,
        email: "carol@example.com".to_string(),
    }];
    state.store.upsert_envelope(&parent).await.unwrap();

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: Some(mxr_core::ReplyHeaders {
            in_reply_to: "<reply-all-parent@example.com>".to_string(),
            references: vec!["<reply-all-parent@example.com>".to_string()],
            thread_id: None,
        }),
        intent: mxr_core::DraftIntent::ReplyAll,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "alice@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Re: parent".to_string(),
        body_markdown: "reply".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();

    let send_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
            draft_id: draft.id.clone(),
            override_safety_token: None,
        }),
    };
    match handle_request(&state, &send_msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("reply-all is missing recipient"));
            assert!(message.contains("bob@example.com"));
        }
        other => panic!("Expected draft safety error, got {other:?}"),
    }

    assert_eq!(
        state.store.get_draft_status(&draft.id).await.unwrap(),
        Some(mxr_core::DraftStatus::Draft)
    );
    assert_eq!(
        state.store.get_draft_heartbeat(&draft.id).await.unwrap(),
        None
    );
    assert_eq!(fake.sent_drafts().len(), 0);
}

#[tokio::test]
async fn dispatch_send_draft_preserves_parent_thread_for_synthetic_sent() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );
    let parent_thread_id = mxr_core::ThreadId::new();
    let parent = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .thread_id(parent_thread_id.clone())
        .provider_id("parent")
        .message_id_header(Some("<parent@example.com>".to_string()))
        .build();
    state.store.upsert_envelope(&parent).await.unwrap();
    state
        .store
        .set_reply_later(&parent.id, chrono::Utc::now())
        .await
        .unwrap();

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: Some(mxr_core::ReplyHeaders {
            in_reply_to: "<parent@example.com>".to_string(),
            references: vec!["<parent@example.com>".to_string()],
            thread_id: None,
        }),
        intent: mxr_core::DraftIntent::Reply,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Re: parent".to_string(),
        body_markdown: "reply".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendDraft {
            draft,
            override_safety_token: None,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    let local_message_id = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SendReceipt {
                local_message_id, ..
            },
        }) => local_message_id,
        other => panic!("Expected SendReceipt, got {:?}", other),
    };
    let sent = state
        .store
        .get_envelope(&local_message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sent.thread_id, parent_thread_id);
    assert!(
        !state.store.is_reply_later(&parent.id).await.unwrap(),
        "sending a reply clears the parent reply-later flag"
    );
}

#[tokio::test]
async fn dispatch_save_draft_to_server_falls_back_to_local_draft() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake;
    let send_provider: Arc<dyn mxr_core::MailSendProvider> =
        Arc::new(UnsupportedServerDraftProvider);
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Local fallback".to_string(),
        body_markdown: "body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SaveDraftToServer {
            draft: draft.clone(),
        }),
    };
    let resp = handle_request(&state, &msg).await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_some());
}

#[tokio::test]
async fn dispatch_saved_search_delete() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    // Create a saved search
    let create_msg = IpcMessage {
        id: 20,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateSavedSearch {
            name: "ToDelete".to_string(),
            query: "is:unread".to_string(),
            search_mode: mxr_core::SearchMode::Lexical,
        }),
    };
    let resp = handle_request(&state, &create_msg).await;
    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearchData { search },
        }) => {
            assert_eq!(search.name, "ToDelete");
        }
        other => panic!("Expected SavedSearchData, got {:?}", other),
    }

    // Verify it's in the list
    let list_msg = IpcMessage {
        id: 21,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSavedSearches),
    };
    let resp = handle_request(&state, &list_msg).await;
    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearches { searches },
        }) => {
            assert_eq!(searches.len(), 1);
            assert_eq!(searches[0].name, "ToDelete");
        }
        other => panic!("Expected SavedSearches with 1 item, got {:?}", other),
    }

    // Delete it
    let delete_msg = IpcMessage {
        id: 22,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::DeleteSavedSearch {
            name: "ToDelete".to_string(),
        }),
    };
    let resp = handle_request(&state, &delete_msg).await;
    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {:?}", other),
    }

    // Verify it's gone
    let list_msg2 = IpcMessage {
        id: 23,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSavedSearches),
    };
    let resp = handle_request(&state, &list_msg2).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearches { searches },
        }) => {
            assert!(
                searches.is_empty(),
                "Saved searches should be empty after delete"
            );
        }
        other => panic!("Expected empty SavedSearches, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_export_thread_markdown() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    // Sync to get messages
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    // Get an envelope to find its thread_id
    let list_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 1,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    let thread_id = match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => envelopes[0].thread_id.clone(),
        other => panic!("Expected Envelopes, got {:?}", other),
    };

    // Export the thread as markdown
    let export_msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ExportThread {
            thread_id,
            format: mxr_core::types::ExportFormat::Markdown,
        }),
    };
    let resp = handle_request(&state, &export_msg).await;
    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ExportResult { content },
        }) => {
            assert!(
                content.starts_with("# Thread:"),
                "Should be markdown: {}",
                content
            );
            assert!(content.contains("Exported from mxr"));
        }
        other => panic!("Expected ExportResult, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_sync_now_acknowledges() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 300,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SyncNow { account_id: None }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_export_thread_json_is_valid() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let list_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 1,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    let thread_id = match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => envelopes[0].thread_id.clone(),
        other => panic!("Expected Envelopes, got {:?}", other),
    };

    let export_msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ExportThread {
            thread_id,
            format: mxr_core::types::ExportFormat::Json,
        }),
    };
    let resp = handle_request(&state, &export_msg).await;
    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ExportResult { content },
        }) => {
            let parsed: serde_json::Value =
                serde_json::from_str(content).expect("Export JSON should be valid");
            assert!(parsed["message_count"].as_u64().unwrap() >= 1);
            assert!(parsed["subject"].is_string());
        }
        other => panic!("Expected ExportResult, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_get_headers_includes_standards_metadata() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let mut body = state.store.get_body(&id).await.unwrap().unwrap();
    body.metadata.list_id = Some("fixtures.example.com".into());
    body.metadata.auth_results = vec!["mx.example.net; dkim=pass".into()];
    body.metadata.content_language = vec!["en".into(), "fr".into()];
    state.store.insert_body(&body).await.unwrap();

    let msg = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetHeaders {
            message_id: id.clone(),
        }),
    };
    let resp = handle_request(&state, &msg).await;

    let headers = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Headers { headers },
        }) => headers,
        other => panic!("Expected Headers, got {:?}", other),
    };

    assert!(headers.iter().any(|(name, _)| name == "From"));
    assert!(headers.iter().any(|(name, _)| name == "Subject"));
    assert!(headers
        .iter()
        .any(|(name, value)| name == "List-Id" && value == "fixtures.example.com"));
    assert!(headers.iter().any(|(name, value)| {
        name == "Authentication-Results" && value == "mx.example.net; dkim=pass"
    }));
    assert!(headers
        .iter()
        .any(|(name, value)| { name == "Content-Language" && value == "en, fr" }));
}

#[tokio::test]
async fn dispatch_export_search_json_is_valid() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 4,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ExportSearch {
            query: "deployment".into(),
            format: mxr_core::types::ExportFormat::Json,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ExportResult { content },
        }) => {
            let parsed: serde_json::Value =
                serde_json::from_str(content).expect("Export JSON should be valid");
            let messages = parsed["messages"]
                .as_array()
                .expect("export search should include messages");
            assert!(messages.len() >= 1, "export search should return results");
            assert!(messages[0].as_object().is_some());
        }
        other => panic!("Expected ExportResult, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_save_draft_to_server() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: state.default_account_id(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: Some("Recipient".into()),
            email: "recipient@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Saved draft".into(),
        body_markdown: "Body".into(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let msg = IpcMessage {
        id: 5,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SaveDraftToServer { draft }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {:?}", other),
    }
}
