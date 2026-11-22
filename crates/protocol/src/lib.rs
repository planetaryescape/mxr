//! Wire types for daemon IPC.
//!
//! `mxr-protocol` is the contract between the mxr daemon and its clients
//! (CLI, TUI, web bridge, MCP server). It depends only on `mxr-core`;
//! provider-specific adaptation happens below this layer and client-specific
//! shaping happens above it, so nothing here should describe a screen or a
//! provider.
//!
//! # Framing
//!
//! Messages travel over the daemon's Unix socket as length-delimited JSON:
//! a 4-byte length prefix followed by one serialized [`IpcMessage`], encoded
//! and decoded by [`IpcCodec`] (frames are capped at 16 MiB). Each
//! [`IpcMessage`] carries a client-chosen correlation `id`, the originating
//! [`ClientKind`], and an [`IpcPayload`] — a [`Request`] going in, and a
//! [`Response`] or an unsolicited [`DaemonEvent`] coming back.
//!
//! # Compatibility
//!
//! [`IPC_PROTOCOL_VERSION`] identifies the wire revision. The daemon
//! advertises it on status surfaces (for example the web bridge's status
//! endpoint) so clients can detect a mismatched daemon. Field additions use
//! `#[serde(default)]` / `skip_serializing_if` so older peers keep decoding.
//! Adding enum variants requires a bump because older peers reject unknown
//! variants. The exception is [`DaemonEvent`]: from protocol 29 it has an
//! `Unknown` catch-all, so a client at 29 or later skips events added after it
//! was built. Clients older than 29 still fail to decode an event they have no
//! variant for, which is why 29 itself is a bump.
//!
//! Protocol 32 requires `DeclareClient` before ordinary requests or events.
//! Token transports authenticate first. Declaration selects one cooperative
//! client kind for the connection lifetime; it does not authenticate a process.

mod archive;
mod codec;
mod remote;
mod search_degraded;
mod selection;
mod types;

pub use archive::*;
pub use codec::IpcCodec;
pub use remote::*;
pub use search_degraded::*;
pub use selection::*;
pub use types::*;

/// Wire revision of the IPC protocol. Bumped when a client needs a daemon
/// capability that older protocol versions cannot provide; additive fields
/// stay decodable across versions via serde defaults. New enum variants require
/// a bump because older peers reject unknown variants.
pub const IPC_PROTOCOL_VERSION: u32 = 32;

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests use panic and unwrap for direct fixture failures"
    )]

    use super::*;
    use bytes::BytesMut;

    /// The request shape before `limit_from_config` existed. A daemon built
    /// from that code decodes a new client's search with this type.
    #[derive(serde::Deserialize)]
    #[serde(tag = "cmd")]
    enum PreConfigLimitRequest {
        Search {
            query: String,
            limit: u32,
            #[serde(default)]
            offset: u32,
            #[serde(default)]
            account_id: Option<AccountId>,
            mode: Option<SearchMode>,
            #[serde(default)]
            sort: Option<SortOrder>,
            explain: bool,
        },
    }

    #[test]
    fn search_with_limit_from_config_decodes_on_a_daemon_that_predates_the_field() {
        let on = serde_json::to_value(Request::Search {
            query: "inbox".into(),
            limit: 50,
            offset: 0,
            account_id: None,
            mode: None,
            sort: None,
            explain: false,
            limit_from_config: true,
        })
        .unwrap();

        let legacy: PreConfigLimitRequest = serde_json::from_value(on).unwrap();
        match legacy {
            PreConfigLimitRequest::Search {
                query,
                limit,
                offset,
                account_id,
                mode,
                sort,
                explain,
            } => {
                assert_eq!(query, "inbox");
                assert_eq!(limit, 50);
                assert_eq!(offset, 0);
                assert!(account_id.is_none());
                assert!(mode.is_none());
                assert_eq!(sort, None);
                assert!(!explain);
            }
        }
    }

    /// The requests and job shape before selection jobs (protocol 27).
    #[derive(serde::Deserialize)]
    #[serde(tag = "cmd")]
    #[allow(dead_code, reason = "fields are read through the decode only")]
    enum PreSelectionRequest {
        GetJob { job_id: String },
    }

    #[derive(serde::Deserialize)]
    #[allow(dead_code, reason = "fields are read through the decode only")]
    struct PreSelectionJob {
        job_id: String,
        kind: String,
        status: JobStatusData,
        progress: JobProgressData,
        #[serde(default)]
        undo_ids: Vec<String>,
        error: Option<String>,
        started_at: i64,
        finished_at: Option<i64>,
        result: Option<MutationResultData>,
    }

    fn selection_job() -> JobData {
        JobData {
            job_id: "job-1".into(),
            kind: "mutation.archive".into(),
            status: JobStatusData::Failed,
            progress: JobProgressData {
                total: 10,
                completed: 4,
                succeeded: 3,
                skipped: 0,
                failed: 1,
            },
            undo_ids: Vec::new(),
            error: Some("stopped".into()),
            started_at: 1,
            finished_at: Some(2),
            result: None,
            selection: Some(SelectionData::Search {
                query: "from:x".into(),
                account_id: None,
            }),
            failures: vec![JobFailureData {
                message_id: MessageId::new(),
                error: "provider said no".into(),
            }],
            paused_until: Some(3),
            action: None,
            undo_id_count: 0,
            failure_count: 0,
        }
    }

    #[test]
    fn an_older_daemon_rejects_the_selection_requests() {
        let mutation = MutationCommand::Archive {
            message_ids: Vec::new(),
        };
        let selection = SelectionData::Search {
            query: "from:x".into(),
            account_id: None,
        };
        let requests = [
            Request::PreviewSelection {
                selection: selection.clone(),
                mutation: mutation.clone(),
                sample_size: None,
            },
            Request::StartSelectionJob {
                job_id: "j".into(),
                preview_token: "t".into(),
                selection,
                mutation,
                client_correlation_id: None,
            },
        ];
        for request in requests {
            let wire = serde_json::to_value(&request).unwrap();
            serde_json::from_value::<Request>(wire.clone()).unwrap();
            assert!(
                serde_json::from_value::<PreSelectionRequest>(wire).is_err(),
                "{request:?}"
            );
        }
    }

    #[test]
    fn job_fields_added_for_selection_jobs_are_optional_both_ways() {
        let job = selection_job();
        let wire = serde_json::to_value(&job).unwrap();
        let old: PreSelectionJob = serde_json::from_value(wire).unwrap();
        assert_eq!(old.progress.failed, 1);

        let mut plain = selection_job();
        plain.selection = None;
        plain.failures.clear();
        plain.paused_until = None;
        let plain_wire = serde_json::to_value(&plain).unwrap();
        for key in [
            "selection",
            "failures",
            "paused_until",
            "action",
            "undo_id_count",
        ] {
            assert!(plain_wire.get(key).is_none(), "{key} should be omitted");
        }
        let decoded: JobData = serde_json::from_value(plain_wire).unwrap();
        assert_eq!(decoded, plain);
    }

    /// The request shapes before `label_role` / `all_accounts` existed.
    #[derive(serde::Deserialize)]
    #[serde(tag = "cmd")]
    #[allow(dead_code, reason = "fields are read through the decode only")]
    #[allow(
        clippy::enum_variant_names,
        reason = "variant names are the wire commands"
    )]
    enum PreLabelRoleRequest {
        ListEnvelopes {
            label_id: Option<LabelId>,
            account_id: Option<AccountId>,
            limit: u32,
            offset: u32,
        },
        ListThreads {
            account_id: Option<AccountId>,
            label_id: Option<LabelId>,
            limit: u32,
            offset: u32,
            #[serde(default)]
            sort: Option<SortOrder>,
        },
        ListLabels {
            account_id: Option<AccountId>,
        },
    }

    #[test]
    fn label_role_stays_off_the_wire_unless_set_and_old_clients_still_decode() {
        let list = |label_role: Option<&str>| Request::ListEnvelopes {
            label_id: Some(LabelId::new()),
            account_id: None,
            limit: 10,
            offset: 0,
            label_role: label_role.map(str::to_string),
        };
        let off = serde_json::to_value(list(None)).unwrap();
        assert!(!off.to_string().contains("label_role"), "{off}");
        let decoded: Request = serde_json::from_value(off).unwrap();
        assert!(matches!(
            decoded,
            Request::ListEnvelopes {
                label_role: None,
                ..
            }
        ));

        let on = serde_json::to_value(list(Some("inbox"))).unwrap();
        let decoded: Request = serde_json::from_value(on.clone()).unwrap();
        assert!(matches!(
            &decoded,
            Request::ListEnvelopes { label_role: Some(role), .. } if role == "inbox"
        ));
        let legacy: PreLabelRoleRequest = serde_json::from_value(on).unwrap();
        assert!(matches!(
            legacy,
            PreLabelRoleRequest::ListEnvelopes {
                label_id: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn a_provider_key_in_a_request_never_shows_in_debug_output_and_roundtrips() {
        let request = Request::SetLlmKey {
            provider: LlmProviderData::Anthropic,
            key: LlmKeySecret::new("sk-ant-do-not-log".into()),
            dry_run: false,
        };
        assert!(!format!("{request:?}").contains("sk-ant"));
        let json = serde_json::to_string(&request).unwrap();
        match serde_json::from_str::<Request>(&json).unwrap() {
            Request::SetLlmKey { provider, key, .. } => {
                assert_eq!(provider, LlmProviderData::Anthropic);
                assert_eq!(key.expose(), "sk-ant-do-not-log");
            }
            other => panic!("wrong variant: {other:?}"),
        }
        assert!(!remote::remote_read_allowed(&request));
        assert!(!remote::remote_send_allowed(&request));
    }

    #[test]
    fn legacy_unsubscribe_decoder_rejects_preview_commit_instead_of_purging() {
        #[derive(serde::Deserialize)]
        #[serde(tag = "cmd")]
        enum LegacySenderRequest {
            UnsubscribePurge {},
        }
        let legacy = serde_json::json!({"cmd":"UnsubscribePurge", "address":"news@example.com", "archive":false});
        assert!(serde_json::from_value::<LegacySenderRequest>(legacy).is_ok());
        for archive in [false, true] {
            let commit = Request::CommitUnsubscribePreview {
                address: "news@example.com".into(),
                account_id: None,
                preview_token: "tok".into(),
                archive,
                archive_on_no_method: false,
            };
            let error = serde_json::from_value::<LegacySenderRequest>(
                serde_json::to_value(commit).unwrap(),
            )
            .err()
            .unwrap();
            assert!(error
                .to_string()
                .contains("unknown variant `CommitUnsubscribePreview`"));
        }
    }

    use mxr_core::id::*;
    use mxr_core::{
        Address, Draft, DraftContent, DraftIntent, ExportFormat, SearchMode, SemanticProfile,
        SortOrder,
    };
    use tokio_util::codec::{Decoder, Encoder};

    fn sample_account_config() -> AccountConfigData {
        AccountConfigData {
            color: None,
            tag: None,
            key: "work".into(),
            name: "Work".into(),
            email: "work@example.com".into(),
            enabled: true,
            sync: None,
            send: None,
            is_default: true,
        }
    }

    #[test]
    fn restored_profile_actions_are_versioned_and_owner_bound() {
        let workspace = ArchiveWorkspaceData {
            workspace_id: "workspace".into(),
            recovery_generation: "generation".into(),
        };
        let activate = Request::ActivateRecovery {
            expected_workspace: workspace.clone(),
            source_owner_stopped: true,
            dry_run: false,
        };
        assert!(activate.requires_workspace_owner());
        let encoded = serde_json::to_value(&activate).unwrap();
        assert_eq!(encoded["cmd"], "ActivateRecovery");
        assert_eq!(
            encoded["expected_workspace"]["recovery_generation"],
            "generation"
        );

        let preview = Request::PrepareRecoveredDraft {
            draft_id: DraftId::new(),
            expected_revision: 7,
            expected_workspace: workspace,
            dry_run: true,
        };
        assert!(preview.requires_workspace_owner());
        assert_eq!(preview.category(), IpcCategory::CoreMail);
    }

    fn sample_draft() -> Draft {
        let now = chrono::Utc::now();
        Draft {
            revision: Some(1),
            id: DraftId::new(),
            account_id: AccountId::new(),
            from: None,
            intent: DraftIntent::New,
            reply_headers: None,
            to: vec![Address {
                name: Some("Alice".into()),
                email: "alice@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "hello".into(),
            content: DraftContent::markdown("body"),
            managed_attachments: None,
            attachments: Vec::new(),
            inline_assets: Vec::new(),
            inline_calendar_reply: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn request_category_cases() -> Vec<(Request, IpcCategory)> {
        let now = chrono::Utc::now();
        vec![
            (
                Request::ListEnvelopes {
                    label_id: None,
                    account_id: None,
                    limit: 10,
                    offset: 0,
                    label_role: None,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::ListEnvelopesByIds {
                    message_ids: vec![MessageId::new()],
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::GetEnvelope {
                    message_id: MessageId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::GetBody {
                    message_id: MessageId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::GetHtmlImageAssets {
                    message_id: MessageId::new(),
                    allow_remote: false,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::DownloadAttachment {
                    message_id: MessageId::new(),
                    attachment_id: AttachmentId::new(),
                    destination: None,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::OpenAttachment {
                    message_id: MessageId::new(),
                    attachment_id: AttachmentId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::ListBodies {
                    message_ids: vec![MessageId::new()],
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::GetThread {
                    thread_id: ThreadId::new(),
                    window: None,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::ListLabels {
                    account_id: Some(AccountId::new()),
                    all_accounts: false,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::SystemMailboxCounts { account_id: None },
                IpcCategory::CoreMail,
            ),
            (
                Request::CreateLabel {
                    name: "todo".into(),
                    color: Some("#fff".into()),
                    account_id: None,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::DeleteLabel {
                    name: "todo".into(),
                    account_id: None,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::RenameLabel {
                    old: "a".into(),
                    new: "b".into(),
                    account_id: None,
                },
                IpcCategory::CoreMail,
            ),
            (Request::ListAccounts, IpcCategory::MxrPlatform),
            (Request::ListAccountsConfig, IpcCategory::MxrPlatform),
            (
                Request::AuthorizeAccountConfig {
                    account: sample_account_config(),
                    reauthorize: false,
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::UpsertAccountConfig {
                    account: sample_account_config(),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::SetAccountIdentity {
                    key: "work".into(),
                    color: Some(mxr_core::AccountColor::Amber),
                    tag: Some("W".into()),
                    dry_run: true,
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::SetDefaultAccount { key: "work".into() },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::TestAccountConfig {
                    account: sample_account_config(),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::DisableAccountConfig { key: "work".into() },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::RemoveAccountConfig {
                    key: "work".into(),
                    purge_local_data: false,
                    dry_run: true,
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::RepairAccountConfig {
                    account: sample_account_config(),
                },
                IpcCategory::MxrPlatform,
            ),
            (Request::ListRules, IpcCategory::MxrPlatform),
            (
                Request::GetRule {
                    rule: "rule-1".into(),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::GetRuleForm {
                    rule: "rule-1".into(),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::UpsertRule {
                    rule: serde_json::json!({"id":"r1"}),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::UpsertRuleForm {
                    account_id: None,
                    existing_rule: None,
                    name: "rule".into(),
                    condition: "from contains a".into(),
                    action: "archive".into(),
                    priority: 1,
                    enabled: true,
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::DeleteRule {
                    rule: "rule-1".into(),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::DryRunRules {
                    rule: None,
                    all: true,
                    after: None,
                },
                IpcCategory::MxrPlatform,
            ),
            (Request::ListSavedSearches, IpcCategory::MxrPlatform),
            (
                Request::ListSubscriptions {
                    account_id: None,
                    limit: 10,
                    all_accounts: false,
                },
                IpcCategory::MxrPlatform,
            ),
            (Request::GetLlmStatus, IpcCategory::MxrPlatform),
            (Request::GetLlmConfig, IpcCategory::MxrPlatform),
            (
                Request::UpdateLlmConfig {
                    config: Box::new(LlmConfigData {
                        enabled: true,
                        base_url: "http://localhost:11434/v1".into(),
                        model: "qwen2.5:3b-instruct".into(),
                        compose_model: None,
                        models: Some(vec![]),
                        api_key_env: String::new(),
                        context_window: 8192,
                        request_timeout_secs: 120,
                        allow_cloud_relationship_data: false,
                        overrides: Some(LlmOverridesData::default()),
                    }),
                },
                IpcCategory::MxrPlatform,
            ),
            (Request::ReloadLlmConfig, IpcCategory::MxrPlatform),
            (
                Request::SetLlmKey {
                    provider: LlmProviderData::Anthropic,
                    key: LlmKeySecret::new("sk-test".into()),
                    dry_run: true,
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::RemoveLlmKey {
                    provider: LlmProviderData::Openrouter,
                    dry_run: true,
                },
                IpcCategory::MxrPlatform,
            ),
            (Request::GetNotificationChimes, IpcCategory::MxrPlatform),
            (
                Request::UpdateNotificationChimes {
                    config: Box::new(NotificationChimesData::default()),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::PatchNotificationChimes {
                    patch: Box::new(NotificationChimesPatchData::default()),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::PreviewNotificationChime {
                    event: NotificationChimeEventData::Archived,
                },
                IpcCategory::MxrPlatform,
            ),
            (Request::GetSemanticStatus, IpcCategory::MxrPlatform),
            (
                Request::EnableSemantic { enabled: true },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::InstallSemanticProfile {
                    profile: SemanticProfile::BgeSmallEnV15,
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::UseSemanticProfile {
                    profile: SemanticProfile::BgeSmallEnV15,
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::ReindexSemantic { force: false },
                IpcCategory::MxrPlatform,
            ),
            (Request::BackfillSemantic, IpcCategory::MxrPlatform),
            (
                Request::CreateSavedSearch {
                    name: "Unread".into(),
                    query: "is:unread".into(),
                    account_id: None,
                    search_mode: SearchMode::Lexical,
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::DeleteSavedSearch {
                    name: "Unread".into(),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::RunSavedSearch {
                    name: "Unread".into(),
                    limit: 10,
                    account_id: None,
                },
                IpcCategory::MxrPlatform,
            ),
            (
                Request::ListEvents {
                    limit: 10,
                    level: None,
                    category: None,
                    since: None,
                    until: None,
                    search: None,
                    category_prefix: None,
                    offset: 0,
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                Request::GetLogs {
                    limit: 10,
                    level: None,
                    search: None,
                },
                IpcCategory::AdminMaintenance,
            ),
            (Request::GetDoctorReport, IpcCategory::AdminMaintenance),
            (
                Request::GenerateBugReport {
                    verbose: false,
                    full_logs: false,
                    since: None,
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                Request::Search {
                    query: "inbox".into(),
                    limit: 10,
                    offset: 0,
                    account_id: None,
                    mode: Some(SearchMode::Lexical),
                    sort: Some(SortOrder::DateDesc),
                    explain: false,
                    limit_from_config: false,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::SyncNow {
                    account_id: None,
                    background: false,
                },
                IpcCategory::CoreMail,
            ),
            (Request::PreviewSyncAll, IpcCategory::CoreMail),
            (Request::SyncAll, IpcCategory::CoreMail),
            (
                Request::GetSyncStatus {
                    account_id: AccountId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::GetFreshness {
                    account_id: None,
                    limit: Some(5),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::SetFlags {
                    message_id: MessageId::new(),
                    flags: mxr_core::types::MessageFlags::READ,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::Count {
                    query: "inbox".into(),
                    account_id: None,
                    mode: Some(SearchMode::Lexical),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::SearchAggregation {
                    query: "inbox".into(),
                    account_id: None,
                    mode: Some(SearchMode::Lexical),
                    group_by: SearchAggregationGroupBy::From,
                    limit: Some(25),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::GetHeaders {
                    message_id: MessageId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::ListRuleHistory {
                    rule: None,
                    limit: 10,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::mutation(MutationCommand::Archive {
                    message_ids: vec![MessageId::new()],
                }),
                IpcCategory::CoreMail,
            ),
            (
                Request::start_mutation_job(MutationCommand::Archive {
                    message_ids: vec![MessageId::new()],
                }),
                IpcCategory::CoreMail,
            ),
            (Request::ListJobs, IpcCategory::CoreMail),
            (
                Request::GetJob {
                    job_id: "job-1".into(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::Unsubscribe {
                    message_id: MessageId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::Snooze {
                    message_id: MessageId::new(),
                    wake_at: now,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::Unsnooze {
                    message_id: MessageId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (Request::ListSnoozed, IpcCategory::CoreMail),
            (
                Request::PrepareReply {
                    message_id: MessageId::new(),
                    reply_all: true,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::EnsureReplyDraft {
                    message_id: MessageId::new(),
                    reply_all: true,
                    dry_run: true,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::SetReplyDraftAudience {
                    draft_id: DraftId::new(),
                    reply_all: false,
                    expected_revision: 7,
                    dry_run: false,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::PrepareForward {
                    message_id: MessageId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::SendDraft {
                    send_identity: Some(mxr_core::SendIdentity::for_draft(&sample_draft())),
                    draft: sample_draft(),
                    override_safety_token: None,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::SaveDraft {
                    draft: sample_draft(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::SendStoredDraft {
                    managed_attachments_reviewed: true,
                    send_identity: Some(mxr_core::SendIdentity::for_revision(&DraftId::new(), 1)),
                    draft_id: DraftId::new(),
                    override_safety_token: None,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::DeleteDraft {
                    expected_revision: Some(1),
                    draft_id: DraftId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::GetDraft {
                    draft_id: DraftId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::UpdateDraft {
                    draft: sample_draft(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::SaveDraftToServer {
                    draft: sample_draft(),
                },
                IpcCategory::CoreMail,
            ),
            (Request::ListDrafts, IpcCategory::CoreMail),
            (
                Request::ListScheduledSends { account_id: None },
                IpcCategory::CoreMail,
            ),
            (
                Request::ExportThread {
                    thread_id: ThreadId::new(),
                    format: ExportFormat::Markdown,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::ExportSearch {
                    query: "inbox".into(),
                    account_id: None,
                    format: ExportFormat::Markdown,
                },
                IpcCategory::CoreMail,
            ),
            (Request::GetStatus, IpcCategory::AdminMaintenance),
            (Request::Ping, IpcCategory::AdminMaintenance),
            (Request::Shutdown, IpcCategory::AdminMaintenance),
        ]
    }

    /// A daemon newer than this build can push an event this build has no
    /// variant for. It must decode as `Unknown` (to be skipped), not fail the
    /// frame, or the client drops its connection.
    #[test]
    fn event_this_build_does_not_know_decodes_as_unknown() {
        let frame = serde_json::json!({
            "id": 0,
            "payload": {
                "type": "Event",
                "event": "SomeEventAddedLater",
                "account_id": "a",
                "extra": [1, 2]
            }
        });

        let parsed: IpcMessage = serde_json::from_value(frame).unwrap();

        assert!(matches!(
            parsed.payload,
            IpcPayload::Event(DaemonEvent::Unknown)
        ));
    }

    #[test]
    fn codec_multiple_messages() {
        let mut codec = IpcCodec::new();
        let mut buf = BytesMut::new();

        for i in 0..3 {
            let msg = IpcMessage {
                expected_workspace: None,
                id: i,
                source: crate::ClientKind::default(),
                payload: IpcPayload::Request(Request::Ping),
            };
            codec.encode(msg, &mut buf).unwrap();
        }

        for i in 0..3 {
            let decoded = codec.decode(&mut buf).unwrap().unwrap();
            assert_eq!(decoded.id, i);
        }

        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn request_categories_cover_every_case() {
        for (request, expected) in request_category_cases() {
            assert_eq!(request.category(), expected, "{request:?}");
        }
    }

    #[test]
    fn legacy_status_response_defaults_new_fields() {
        let json = serde_json::json!({
            "id": 7,
            "payload": {
                "type": "Response",
                "status": "Ok",
                "data": {
                    "kind": "Status",
                    "uptime_secs": 42,
                    "accounts": ["personal"],
                    "total_messages": 123,
                    "daemon_pid": 999
                }
            }
        });

        let parsed: IpcMessage = serde_json::from_value(json).unwrap();
        match parsed.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::Status {
                        sync_statuses,
                        protocol_version,
                        daemon_version,
                        daemon_build_id,
                        repair_required,
                        ..
                    },
            }) => {
                assert!(sync_statuses.is_empty());
                assert_eq!(protocol_version, 0);
                assert!(daemon_version.is_none());
                assert!(daemon_build_id.is_none());
                assert!(!repair_required);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn account_scoped_requests_deserialize_legacy_payloads() {
        let count: Request = serde_json::from_value(serde_json::json!({
            "cmd": "Count",
            "query": "inbox",
            "mode": null
        }))
        .unwrap();
        assert!(matches!(
            count,
            Request::Count {
                account_id: None,
                ..
            }
        ));

        let create: Request = serde_json::from_value(serde_json::json!({
            "cmd": "CreateSavedSearch",
            "name": "Inbox",
            "query": "inbox",
            "search_mode": "lexical"
        }))
        .unwrap();
        assert!(matches!(
            create,
            Request::CreateSavedSearch {
                account_id: None,
                ..
            }
        ));

        let run: Request = serde_json::from_value(serde_json::json!({
            "cmd": "RunSavedSearch",
            "name": "Inbox",
            "limit": 10
        }))
        .unwrap();
        assert!(matches!(
            run,
            Request::RunSavedSearch {
                account_id: None,
                ..
            }
        ));

        let export: Request = serde_json::from_value(serde_json::json!({
            "cmd": "ExportSearch",
            "query": "inbox",
            "format": "Markdown"
        }))
        .unwrap();
        assert!(matches!(
            export,
            Request::ExportSearch {
                account_id: None,
                ..
            }
        ));
    }

    #[test]
    fn account_summary_without_identity_still_decodes() {
        let older = serde_json::json!({
            "account_id": AccountId::new(),
            "key": "work",
            "name": "Work",
            "email": "bk@work.example",
            "provider_kind": "fake",
            "sync_kind": null,
            "send_kind": null,
            "enabled": true,
            "is_default": false,
            "source": "config",
            "editable": "full",
            "sync": null,
            "send": null,
        });
        let decoded: AccountSummaryData = serde_json::from_value(older).unwrap();
        assert!(decoded.identity.is_none());
        let encoded = serde_json::to_value(&decoded).unwrap();
        assert!(encoded.get("identity").is_none());
    }

    /// New clients name the replay policy explicitly; old clients still decode.
    #[test]
    fn replay_policy_is_not_silently_ignored_by_older_daemons() {
        let old: Request = serde_json::from_str(r#"{"cmd":"DraftEval","limit":10}"#).unwrap();
        assert!(matches!(old, Request::DraftEval { rewrite: false, .. }));
        #[derive(serde::Deserialize)]
        #[serde(tag = "cmd")]
        #[expect(dead_code, reason = "only decode compatibility is checked")]
        enum LegacyRequest {
            DraftEval { limit: u32 },
        }
        for rewrite in [false, true] {
            let request = Request::DraftEval {
                account_id: None,
                limit: 10,
                rewrite,
            };
            let wire = serde_json::to_value(&request).unwrap();
            assert_eq!(wire["cmd"], "DraftEvalWithPolicy");
            assert!(serde_json::from_value::<LegacyRequest>(wire.clone()).is_err());
            assert!(
                matches!(serde_json::from_value::<Request>(wire).unwrap(), Request::DraftEval { rewrite: actual, .. } if actual == rewrite)
            );
        }
    }

    /// Writing help is a command of its own: a daemon that predates it fails
    /// to decode it, so it never drafts over what the user typed.
    #[test]
    fn compose_assist_is_a_command_older_daemons_reject() {
        let wire = serde_json::to_value(Request::ComposeAssist {
            draft: DraftTargetData::Reply {
                message_id: MessageId::new(),
                all: false,
            },
            text: "Thursday works.".into(),
            selection: None,
            action: None,
            instruction: None,
            adjust: AssistAdjustData::default(),
            model: None,
            revise: None,
        })
        .unwrap();
        assert_eq!(wire["cmd"], "ComposeAssist");
        assert!(wire.get("revise").is_none(), "absent stays off the wire");
        assert_eq!(wire["draft"]["kind"], "reply");
        #[derive(serde::Deserialize)]
        #[serde(tag = "cmd")]
        #[expect(dead_code, reason = "only that the command fails to decode is checked")]
        enum PreAssistRequest {
            DraftCompose { instruction: String },
        }
        assert!(serde_json::from_value::<PreAssistRequest>(wire).is_err());
    }

    #[test]
    fn unsubscribe_preview_commit_requires_token_and_choice() {
        for json in [
            serde_json::json!({"cmd":"CommitUnsubscribePreview", "address":"news@example.com", "archive":false}),
            serde_json::json!({"cmd":"CommitUnsubscribePreview", "address":"news@example.com", "preview_token":"tok"}),
        ] {
            assert!(serde_json::from_value::<Request>(json).is_err());
        }
        let request: Request = serde_json::from_value(serde_json::json!({"cmd":"CommitUnsubscribePreview", "address":"news@example.com", "archive":false, "preview_token":"tok"})).unwrap();
        assert!(
            matches!(request, Request::CommitUnsubscribePreview { archive:false, preview_token, .. } if preview_token == "tok")
        );
    }
}
