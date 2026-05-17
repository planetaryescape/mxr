mod codec;
mod types;

pub use codec::IpcCodec;
pub use types::*;

pub const IPC_PROTOCOL_VERSION: u32 = 3;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use bytes::BytesMut;
    use mxr_core::id::*;
    use mxr_core::{
        Address, Draft, DraftIntent, ExportFormat, SavedSearch, SearchMode, SemanticProfile,
        SemanticRuntimeMetrics, SemanticStatusSnapshot, SortOrder,
    };
    use proptest::prelude::*;
    use tokio_util::codec::{Decoder, Encoder};

    fn sample_account_config() -> AccountConfigData {
        AccountConfigData {
            key: "work".into(),
            name: "Work".into(),
            email: "work@example.com".into(),
            enabled: true,
            sync: None,
            send: None,
            is_default: true,
        }
    }

    fn sample_draft() -> Draft {
        let now = chrono::Utc::now();
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            intent: DraftIntent::New,
            reply_headers: None,
            to: vec![Address {
                name: Some("Alice".into()),
                email: "alice@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "hello".into(),
            body_markdown: "body".into(),
            attachments: Vec::new(),
            inline_calendar_reply: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn sample_address() -> Address {
        Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        }
    }

    fn sample_envelope() -> mxr_core::Envelope {
        mxr_core::Envelope {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: "provider-1".into(),
            thread_id: ThreadId::new(),
            message_id_header: Some("<id@example.com>".into()),
            in_reply_to: None,
            references: Vec::new(),
            from: sample_address(),
            to: vec![sample_address()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "hello".into(),
            date: chrono::Utc::now(),
            flags: mxr_core::types::MessageFlags::READ,
            snippet: "snippet".into(),
            has_attachments: false,
            size_bytes: 42,
            unsubscribe: mxr_core::types::UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: Vec::new(),
        }
    }

    fn sample_body() -> mxr_core::MessageBody {
        mxr_core::MessageBody {
            message_id: MessageId::new(),
            text_plain: Some("plain".into()),
            text_html: Some("<p>html</p>".into()),
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata::default(),
        }
    }

    fn sample_label() -> mxr_core::Label {
        mxr_core::Label {
            id: LabelId::new(),
            account_id: AccountId::new(),
            name: "Inbox".into(),
            kind: mxr_core::types::LabelKind::System,
            color: None,
            provider_id: "INBOX".into(),
            unread_count: 1,
            total_count: 2,
            role: None,
        }
    }

    fn sample_thread() -> mxr_core::Thread {
        mxr_core::Thread {
            id: ThreadId::new(),
            account_id: AccountId::new(),
            subject: "hello".into(),
            participants: vec![sample_address()],
            message_count: 1,
            unread_count: 0,
            latest_date: chrono::Utc::now(),
            snippet: "snippet".into(),
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
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::ListLabels {
                    account_id: Some(AccountId::new()),
                },
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
                },
                IpcCategory::MxrPlatform,
            ),
            (Request::GetLlmStatus, IpcCategory::MxrPlatform),
            (Request::GetLlmConfig, IpcCategory::MxrPlatform),
            (
                Request::UpdateLlmConfig {
                    config: LlmConfigData {
                        enabled: true,
                        base_url: "http://localhost:11434/v1".into(),
                        model: "qwen2.5:3b-instruct".into(),
                        api_key_env: String::new(),
                        context_window: 8192,
                        request_timeout_secs: 120,
                        allow_cloud_relationship_data: false,
                        overrides: Some(LlmOverridesData::default()),
                    },
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
            (Request::ReindexSemantic, IpcCategory::MxrPlatform),
            (Request::BackfillSemantic, IpcCategory::MxrPlatform),
            (
                Request::CreateSavedSearch {
                    name: "Unread".into(),
                    query: "is:unread".into(),
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
                    mode: Some(SearchMode::Lexical),
                    sort: Some(SortOrder::DateDesc),
                    explain: false,
                },
                IpcCategory::CoreMail,
            ),
            (Request::SyncNow { account_id: None }, IpcCategory::CoreMail),
            (
                Request::GetSyncStatus {
                    account_id: AccountId::new(),
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
                    mode: Some(SearchMode::Lexical),
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
                Request::PrepareForward {
                    message_id: MessageId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::SendDraft {
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
                    draft_id: DraftId::new(),
                    override_safety_token: None,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::DeleteDraft {
                    draft_id: DraftId::new(),
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
                Request::ExportThread {
                    thread_id: ThreadId::new(),
                    format: ExportFormat::Markdown,
                },
                IpcCategory::CoreMail,
            ),
            (
                Request::ExportSearch {
                    query: "inbox".into(),
                    format: ExportFormat::Markdown,
                },
                IpcCategory::CoreMail,
            ),
            (Request::GetStatus, IpcCategory::AdminMaintenance),
            (Request::Ping, IpcCategory::AdminMaintenance),
            (Request::Shutdown, IpcCategory::AdminMaintenance),
        ]
    }

    fn response_category_cases() -> Vec<(ResponseData, IpcCategory)> {
        vec![
            (ResponseData::Pong, IpcCategory::AdminMaintenance),
            (ResponseData::Ack, IpcCategory::AdminMaintenance),
            (
                ResponseData::Envelopes {
                    envelopes: vec![sample_envelope()],
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::Envelope {
                    envelope: sample_envelope(),
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::Body {
                    body: sample_body(),
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::HtmlImageAssets {
                    message_id: MessageId::new(),
                    assets: Vec::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::AttachmentFile {
                    file: AttachmentFile {
                        attachment_id: AttachmentId::new(),
                        filename: "a.txt".into(),
                        path: "/tmp/a.txt".into(),
                    },
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::Bodies {
                    bodies: vec![sample_body()],
                    failures: Vec::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::Thread {
                    thread: sample_thread(),
                    messages: vec![sample_envelope()],
                    summary: None,
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::Labels {
                    labels: vec![sample_label()],
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::Label {
                    label: sample_label(),
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::SavedSearches {
                    searches: Vec::new(),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::SearchResults {
                    results: Vec::new(),
                    total: 0,
                    has_more: false,
                    next_offset: None,
                    explain: None,
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::Status {
                    uptime_secs: 0,
                    accounts: Vec::new(),
                    total_messages: 0,
                    daemon_pid: None,
                    sync_statuses: Vec::new(),
                    protocol_version: 1,
                    daemon_version: None,
                    daemon_build_id: None,
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                ResponseData::SemanticStatus {
                    snapshot: SemanticStatusSnapshot {
                        enabled: true,
                        active_profile: SemanticProfile::BgeSmallEnV15,
                        profiles: Vec::new(),
                        runtime: SemanticRuntimeMetrics::default(),
                    },
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::LlmStatus {
                    snapshot: LlmStatusSnapshot {
                        enabled: false,
                        provider: "noop".into(),
                        model: "noop".into(),
                        configured_model: "qwen2.5:3b-instruct".into(),
                        base_url: None,
                        api_key_env: None,
                        api_key_present: false,
                        context_window: 0,
                        supports_streaming: false,
                        request_timeout_secs: 120,
                    },
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::LlmConfig {
                    config: LlmConfigData {
                        enabled: false,
                        base_url: "http://localhost:11434/v1".into(),
                        model: "qwen2.5:3b-instruct".into(),
                        api_key_env: String::new(),
                        context_window: 8192,
                        request_timeout_secs: 120,
                        allow_cloud_relationship_data: false,
                        overrides: Some(LlmOverridesData::default()),
                    },
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::Drafts { drafts: Vec::new() },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::SnoozedMessages {
                    snoozed: Vec::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::MutationResult {
                    result: MutationResultData {
                        requested: 1,
                        succeeded: 1,
                        skipped: 0,
                        failed: 0,
                        accounts: vec![AccountMutationResultData {
                            account_id: AccountId::new(),
                            account_name: "Work".into(),
                            succeeded: 1,
                            skipped: 0,
                            failed: 0,
                            error: None,
                        }],
                        mutation_id: None,
                    },
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::EventLogEntries {
                    entries: Vec::new(),
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                ResponseData::LogLines { lines: Vec::new() },
                IpcCategory::AdminMaintenance,
            ),
            (
                ResponseData::Rules { rules: Vec::new() },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::RuleData {
                    rule: serde_json::json!({}),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::Accounts {
                    accounts: Vec::new(),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::AccountsConfig {
                    accounts: Vec::new(),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::AccountOperation {
                    result: AccountOperationResult {
                        ok: true,
                        summary: "ok".into(),
                        save: None,
                        auth: None,
                        sync: None,
                        send: None,
                        device_code_url: None,
                        device_code_user_code: None,
                    },
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::RuleFormData {
                    form: RuleFormData {
                        id: None,
                        name: "rule".into(),
                        condition: "from contains a".into(),
                        action: "archive".into(),
                        priority: 1,
                        enabled: true,
                    },
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::RuleDryRun {
                    results: Vec::new(),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::DoctorReport {
                    report: DoctorReport {
                        healthy: true,
                        health_class: DaemonHealthClass::Healthy,
                        lexical_index_freshness: IndexFreshness::Current,
                        last_successful_sync_at: None,
                        lexical_last_rebuilt_at: None,
                        semantic_enabled: false,
                        semantic_active_profile: None,
                        semantic_index_freshness: IndexFreshness::Disabled,
                        semantic_last_indexed_at: None,
                        feature_health: None,
                        data_stats: DoctorDataStats::default(),
                        data_dir_exists: true,
                        database_exists: true,
                        index_exists: true,
                        socket_exists: true,
                        socket_reachable: true,
                        stale_socket: false,
                        daemon_running: true,
                        daemon_pid: None,
                        daemon_protocol_version: 1,
                        daemon_version: None,
                        daemon_build_id: None,
                        index_lock_held: false,
                        index_lock_error: None,
                        restart_required: false,
                        repair_required: false,
                        database_path: "/tmp/db".into(),
                        database_size_bytes: 0,
                        index_path: "/tmp/index".into(),
                        index_size_bytes: 0,
                        log_path: "/tmp/log".into(),
                        log_size_bytes: 0,
                        sync_statuses: Vec::new(),
                        recent_sync_events: Vec::new(),
                        recent_error_logs: Vec::new(),
                        recommended_next_steps: Vec::new(),
                        findings: Vec::new(),
                    },
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                ResponseData::BugReport {
                    content: "report".into(),
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                ResponseData::RuleHistory {
                    entries: Vec::new(),
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                ResponseData::SyncStatus {
                    sync: AccountSyncStatus {
                        account_id: AccountId::new(),
                        account_name: "work".into(),
                        last_attempt_at: None,
                        last_success_at: None,
                        last_error: None,
                        failure_class: None,
                        consecutive_failures: 0,
                        backoff_until: None,
                        sync_in_progress: false,
                        current_cursor_summary: None,
                        last_synced_count: 0,
                        healthy: true,
                    },
                },
                IpcCategory::CoreMail,
            ),
            (ResponseData::Count { count: 1 }, IpcCategory::CoreMail),
            (
                ResponseData::Headers {
                    headers: vec![("Subject".into(), "x".into())],
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::Subscriptions {
                    subscriptions: Vec::new(),
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::SavedSearchData {
                    search: SavedSearch {
                        id: SavedSearchId::new(),
                        account_id: None,
                        name: "Unread".into(),
                        query: "is:unread".into(),
                        search_mode: SearchMode::Lexical,
                        sort: SortOrder::DateDesc,
                        icon: None,
                        position: 0,
                        created_at: chrono::Utc::now(),
                    },
                },
                IpcCategory::MxrPlatform,
            ),
            (
                ResponseData::ReplyContext {
                    context: ReplyContext {
                        account_id: AccountId::new(),
                        in_reply_to: "id".into(),
                        references: Vec::new(),
                        reply_to: "a@example.com".into(),
                        cc: String::new(),
                        subject: "Re: x".into(),
                        from: "a@example.com".into(),
                        thread_context: "ctx".into(),
                        thread_id: None,
                    },
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::ForwardContext {
                    context: ForwardContext {
                        account_id: AccountId::new(),
                        subject: "Fwd: x".into(),
                        from: "a@example.com".into(),
                        forwarded_content: "body".into(),
                    },
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::ExportResult {
                    content: "body".into(),
                },
                IpcCategory::CoreMail,
            ),
        ]
    }

    #[test]
    fn request_serde_roundtrip() {
        let variants: Vec<Request> = vec![
            Request::Ping,
            Request::Shutdown,
            Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 50,
                offset: 0,
            },
            Request::GetEnvelope {
                message_id: MessageId::new(),
            },
            Request::Search {
                query: "test".to_string(),
                limit: 10,
                offset: 0,
                mode: None,
                sort: None,
                explain: false,
            },
        ];

        for req in variants {
            let msg = IpcMessage {
                id: 1,
                source: crate::ClientKind::default(),
                payload: IpcPayload::Request(req),
            };
            let json = serde_json::to_string(&msg).unwrap();
            let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.id, 1);
        }
    }

    #[test]
    fn response_serde_roundtrip() {
        let ok = Response::Ok {
            data: ResponseData::Pong,
        };
        let err = Response::error("something failed");

        for resp in [ok, err] {
            let msg = IpcMessage {
                id: 2,
                source: crate::ClientKind::default(),
                payload: IpcPayload::Response(resp),
            };
            let json = serde_json::to_string(&msg).unwrap();
            let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.id, 2);
        }
    }

    #[test]
    fn daemon_event_roundtrip() {
        let events: Vec<DaemonEvent> = vec![
            DaemonEvent::SyncCompleted {
                account_id: AccountId::new(),
                messages_synced: 10,
            },
            DaemonEvent::SyncError {
                account_id: AccountId::new(),
                error: "timeout".to_string(),
            },
            DaemonEvent::MessageUnsnoozed {
                message_id: MessageId::new(),
            },
        ];

        for event in events {
            let msg = IpcMessage {
                id: 0,
                source: crate::ClientKind::default(),
                payload: IpcPayload::Event(event),
            };
            let json = serde_json::to_string(&msg).unwrap();
            let _parsed: IpcMessage = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn codec_encode_decode() {
        let mut codec = IpcCodec::new();
        let msg = IpcMessage {
            id: 42,
            source: crate::ClientKind::default(),
            payload: IpcPayload::Request(Request::Ping),
        };

        let mut buf = BytesMut::new();
        codec.encode(msg, &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.id, 42);
    }

    #[test]
    fn codec_multiple_messages() {
        let mut codec = IpcCodec::new();
        let mut buf = BytesMut::new();

        for i in 0..3 {
            let msg = IpcMessage {
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

        // No more messages
        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn request_categories_cover_core_mail_variants() {
        for (request, expected) in request_category_cases()
            .into_iter()
            .filter(|(_, category)| *category == IpcCategory::CoreMail)
        {
            assert_eq!(request.category(), expected, "{request:?}");
        }
    }

    #[test]
    fn request_categories_cover_mxr_platform_variants() {
        for (request, expected) in request_category_cases()
            .into_iter()
            .filter(|(_, category)| *category == IpcCategory::MxrPlatform)
        {
            assert_eq!(request.category(), expected, "{request:?}");
        }
    }

    #[test]
    fn request_categories_cover_admin_variants() {
        for (request, expected) in request_category_cases()
            .into_iter()
            .filter(|(_, category)| *category == IpcCategory::AdminMaintenance)
        {
            assert_eq!(request.category(), expected, "{request:?}");
        }
    }

    #[test]
    fn response_data_categories_cover_core_mail_variants() {
        for (data, expected) in response_category_cases()
            .into_iter()
            .filter(|(_, category)| *category == IpcCategory::CoreMail)
        {
            assert_eq!(data.category(), expected, "{data:?}");
        }
    }

    #[test]
    fn response_data_categories_cover_mxr_platform_variants() {
        for (data, expected) in response_category_cases()
            .into_iter()
            .filter(|(_, category)| *category == IpcCategory::MxrPlatform)
        {
            assert_eq!(data.category(), expected, "{data:?}");
        }
    }

    #[test]
    fn response_data_categories_cover_admin_variants() {
        for (data, expected) in response_category_cases()
            .into_iter()
            .filter(|(_, category)| *category == IpcCategory::AdminMaintenance)
        {
            assert_eq!(data.category(), expected, "{data:?}");
        }
    }

    #[test]
    fn daemon_event_categories_cover_every_variant() {
        let aid = AccountId::new();
        let event_categories = vec![
            (
                DaemonEvent::SyncCompleted {
                    account_id: aid.clone(),
                    messages_synced: 1,
                },
                IpcCategory::CoreMail,
            ),
            (
                DaemonEvent::SyncError {
                    account_id: aid.clone(),
                    error: "boom".into(),
                },
                IpcCategory::CoreMail,
            ),
            (
                DaemonEvent::NewMessages {
                    envelopes: Vec::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                DaemonEvent::MessageUnsnoozed {
                    message_id: MessageId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                DaemonEvent::ReminderTriggered {
                    sent_message_id: MessageId::new(),
                },
                IpcCategory::CoreMail,
            ),
            (
                DaemonEvent::LabelCountsUpdated { counts: Vec::new() },
                IpcCategory::CoreMail,
            ),
            (
                DaemonEvent::MutationReconciliationFailed {
                    client_correlation_id: "1".into(),
                    error_summary: "x".into(),
                },
                IpcCategory::CoreMail,
            ),
            (
                DaemonEvent::OperationStarted {
                    operation_id: "op".into(),
                    operation: "task".into(),
                    account_id: Some(aid.clone()),
                    message: "m".into(),
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                DaemonEvent::OperationProgress {
                    operation_id: "op".into(),
                    operation: "task".into(),
                    account_id: Some(aid.clone()),
                    current: 1,
                    total: Some(2),
                    message: "m".into(),
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                DaemonEvent::OperationCompleted {
                    operation_id: "op".into(),
                    operation: "task".into(),
                    account_id: Some(aid.clone()),
                    message: "m".into(),
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                DaemonEvent::OperationFailed {
                    operation_id: "op".into(),
                    operation: "task".into(),
                    account_id: Some(aid.clone()),
                    error: "e".into(),
                    retryable: false,
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                DaemonEvent::OperationCancelled {
                    operation_id: "op".into(),
                    operation: "task".into(),
                    account_id: Some(aid),
                    message: "m".into(),
                },
                IpcCategory::AdminMaintenance,
            ),
        ];

        for (event, expected) in event_categories {
            assert_eq!(event.category(), expected, "{event:?}");
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
    fn check_draft_safety_request_roundtrip() {
        use mxr_core::types::{
            Address, CitationRef, Draft, DraftIntent, DraftSafetyIssue, DraftSafetyIssueCode,
            DraftSafetyReport, DraftSafetySeverity, DraftSafetyVerdict,
        };
        use std::path::PathBuf;

        let draft = Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            reply_headers: None,
            intent: DraftIntent::Reply,
            to: vec![Address {
                email: "alice@example.com".into(),
                name: None,
            }],
            cc: vec![],
            bcc: vec![],
            subject: "see attached".into(),
            body_markdown: "yo".into(),
            attachments: Vec::<PathBuf>::new(),
            inline_calendar_reply: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let req = IpcMessage {
            id: 1,
            source: crate::ClientKind::default(),
            payload: IpcPayload::Request(Request::CheckDraftSafety {
                draft: draft.clone(),
                context: DraftSafetyContextData {
                    mode: DraftSafetyModeData::Check,
                    reply_all: true,
                    original_message_id: None,
                    thread_id: None,
                    allow_llm: false,
                    proposed_send_at: None,
                },
            }),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
        match parsed.payload {
            IpcPayload::Request(Request::CheckDraftSafety { context, .. }) => {
                assert_eq!(context.mode, DraftSafetyModeData::Check);
                assert!(context.reply_all);
                assert!(!context.allow_llm);
            }
            other => panic!("unexpected payload: {other:?}"),
        }

        // Response shape with citations + override token round-trips losslessly.
        let report = DraftSafetyReport {
            allowed: false,
            verdict: DraftSafetyVerdict::Blocked,
            checked_at: Some(chrono::Utc::now()),
            issues: vec![DraftSafetyIssue::new(
                DraftSafetyIssueCode::PiiSecret,
                DraftSafetySeverity::Blocker,
                "PEM private key detected",
            )
            .with_detail("redacted")
            .with_citations(vec![CitationRef {
                message_id: Some("msg-1".into()),
                thread_id: None,
                field: "body".into(),
                quote: "redacted".into(),
            }])],
        };
        let resp = IpcMessage {
            id: 1,
            source: crate::ClientKind::default(),
            payload: IpcPayload::Response(Response::Ok {
                data: ResponseData::DraftSafetyReportResponse {
                    report: report.clone(),
                },
            }),
        };
        let json = serde_json::to_string(&resp).unwrap();
        // PII redacted preview was set; raw secret bytes never appeared
        // in this test fixture, so guard against accidental future
        // regression that surfaces them.
        assert!(!json.contains("BEGIN PRIVATE KEY"));
        let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
        match parsed.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::DraftSafetyReportResponse { report: r2 },
            }) => {
                assert_eq!(r2.allowed, false);
                assert!(matches!(r2.verdict, DraftSafetyVerdict::Blocked));
                assert_eq!(r2.issues.len(), 1);
                assert_eq!(r2.issues[0].citations.len(), 1);
                assert_eq!(
                    r2.issues[0].citations[0].message_id.as_deref(),
                    Some("msg-1")
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    proptest! {
        #[test]
        fn search_ipc_message_serde_roundtrip(
            id in any::<u64>(),
            limit in 0u32..128,
            offset in 0u32..64,
            explain in any::<bool>(),
            query in "[ -~]{0,64}",
        ) {
            let msg = IpcMessage {
                id,
                source: crate::ClientKind::default(),
                payload: IpcPayload::Request(Request::Search {
                    query: query.clone(),
                    limit,
                    offset,
                    mode: Some(SearchMode::Lexical),
                    sort: Some(SortOrder::DateDesc),
                    explain,
                }),
            };

            let json = serde_json::to_string(&msg)?;
            let parsed: IpcMessage = serde_json::from_str(&json)?;
            prop_assert_eq!(parsed.id, id);

            match parsed.payload {
                IpcPayload::Request(Request::Search {
                    query: parsed_query,
                    limit: parsed_limit,
                    offset: parsed_offset,
                    explain: parsed_explain,
                    ..
                }) => {
                    prop_assert_eq!(parsed_query, query);
                    prop_assert_eq!(parsed_limit, limit);
                    prop_assert_eq!(parsed_offset, offset);
                    prop_assert_eq!(parsed_explain, explain);
                }
                other => prop_assert!(false, "unexpected payload: {other:?}"),
            }
        }

        // -----------------------------------------------------------
        // AI-email IPC variants — one proptest per Request variant.
        // Each test varies at least one field and round-trips via JSON.
        // -----------------------------------------------------------

        #[test]
        fn check_draft_safety_roundtrip(
            reply_all in any::<bool>(),
            allow_llm in any::<bool>(),
        ) {
            let req = Request::CheckDraftSafety {
                draft: ai_email_test::sample_draft("hi"),
                context: DraftSafetyContextData {
                    mode: DraftSafetyModeData::Check,
                    reply_all,
                    original_message_id: None,
                    thread_id: None,
                    allow_llm,
                    proposed_send_at: None,
                },
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::CheckDraftSafety { context, .. } => {
                    prop_assert_eq!(context.reply_all, reply_all);
                    prop_assert_eq!(context.allow_llm, allow_llm);
                }
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn extract_draft_commitments_roundtrip(body in "[a-zA-Z ]{1,64}") {
            let req = Request::ExtractDraftCommitments {
                draft: ai_email_test::sample_draft(&body),
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::ExtractDraftCommitments { draft } => {
                    prop_assert_eq!(draft.body_markdown, body);
                }
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn list_owed_replies_roundtrip(
            since in proptest::option::of(1u32..365),
            within in proptest::option::of(1u32..365),
            limit in 1u32..200,
        ) {
            let account_id = AccountId::new();
            let req = Request::ListOwedReplies {
                account_id: account_id.clone(),
                older_than_days: since,
                within_days: within,
                limit,
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::ListOwedReplies { account_id: a, older_than_days, within_days, limit: l } => {
                    prop_assert_eq!(a, account_id);
                    prop_assert_eq!(older_than_days, since);
                    prop_assert_eq!(within_days, within);
                    prop_assert_eq!(l, limit);
                }
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn archive_ask_roundtrip(
            question in "[a-zA-Z ?]{1,64}",
            limit in 1u32..50,
        ) {
            let req = Request::ArchiveAsk {
                question: question.clone(),
                filters: ArchiveAskFiltersData {
                    account_id: Some(AccountId::new()),
                    from: Some("alice@example.com".into()),
                    to: None,
                    after: None,
                    before: None,
                    mode: ArchiveAskMode::Hybrid,
                },
                limit,
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::ArchiveAsk { question: q, limit: l, filters } => {
                    prop_assert_eq!(q, question);
                    prop_assert_eq!(l, limit);
                    prop_assert_eq!(filters.mode, ArchiveAskMode::Hybrid);
                }
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn list_decision_log_roundtrip(
            topic in proptest::option::of("[a-z]{1,16}"),
            since_days in proptest::option::of(1u32..730),
            limit in 1u32..200,
        ) {
            let account_id = AccountId::new();
            let req = Request::ListDecisionLog {
                account_id: account_id.clone(),
                topic: topic.clone(),
                since_days,
                limit,
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::ListDecisionLog { account_id: a, topic: t, since_days: s, limit: l } => {
                    prop_assert_eq!(a, account_id);
                    prop_assert_eq!(t, topic);
                    prop_assert_eq!(s, since_days);
                    prop_assert_eq!(l, limit);
                }
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn send_time_recommendation_roundtrip(recipient in "[a-z]{1,16}@example.com") {
            let account_id = AccountId::new();
            let req = Request::SendTimeRecommendation {
                account_id: account_id.clone(),
                recipients: vec![recipient.clone()],
                proposed_at: None,
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::SendTimeRecommendation { account_id: a, recipients: r, proposed_at: at } => {
                    prop_assert_eq!(a, account_id);
                    prop_assert_eq!(r, vec![recipient]);
                    prop_assert!(at.is_none(), "default None roundtrips");
                }
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn watch_cadence_roundtrip(
            email in "[a-z]{1,16}@example.com",
            // Use a roundtrip-safe f64 strategy: integer halves only
            // (0.5, 1.0, 1.5, ... 365.0). JSON loses precision on
            // arbitrary f64s; we only need to verify the variant
            // shape, not that JSON preserves IEEE-754 exactly.
            expected_halves in proptest::option::of(1u32..730),
            allow_list_sender in any::<bool>(),
        ) {
            let expected = expected_halves.map(|n| n as f64 / 2.0);
            let req = Request::WatchCadence {
                account_id: AccountId::new(),
                email: email.clone(),
                expected_days: expected,
                note: None,
                allow_list_sender,
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::WatchCadence { email: e, expected_days, allow_list_sender: a, .. } => {
                    prop_assert_eq!(e, email);
                    prop_assert_eq!(expected_days, expected);
                    prop_assert_eq!(a, allow_list_sender);
                }
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn unwatch_cadence_roundtrip(email in "[a-z]{1,16}@example.com") {
            let req = Request::UnwatchCadence {
                account_id: AccountId::new(),
                email: email.clone(),
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::UnwatchCadence { email: e, .. } => prop_assert_eq!(e, email),
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn list_cadence_watch_roundtrip(seed in any::<u64>()) {
            let _ = seed;
            let account_id = AccountId::new();
            let req = Request::ListCadenceWatch { account_id: account_id.clone() };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::ListCadenceWatch { account_id: a } => prop_assert_eq!(a, account_id),
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn list_cadence_drift_roundtrip(seed in any::<u64>()) {
            let _ = seed;
            let account_id = AccountId::new();
            let req = Request::ListCadenceDrift { account_id: account_id.clone() };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::ListCadenceDrift { account_id: a } => prop_assert_eq!(a, account_id),
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn get_thread_briefing_roundtrip(refresh in any::<bool>()) {
            let thread_id = ThreadId::new();
            let req = Request::GetThreadBriefing {
                thread_id: thread_id.clone(),
                refresh,
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::GetThreadBriefing { thread_id: t, refresh: r } => {
                    prop_assert_eq!(t, thread_id);
                    prop_assert_eq!(r, refresh);
                }
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn get_recipient_briefing_roundtrip(
            email in "[a-z]{1,16}@example.com",
            refresh in any::<bool>(),
        ) {
            let req = Request::GetRecipientBriefing {
                account_id: AccountId::new(),
                email: email.clone(),
                refresh,
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::GetRecipientBriefing { email: e, refresh: r, .. } => {
                    prop_assert_eq!(e, email);
                    prop_assert_eq!(r, refresh);
                }
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn suggest_collaborators_roundtrip(limit in 1u32..50) {
            let req = Request::SuggestCollaborators {
                draft: ai_email_test::sample_draft("hi"),
                limit,
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::SuggestCollaborators { limit: l, .. } => prop_assert_eq!(l, limit),
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn find_expert_roundtrip(
            query in "[a-zA-Z ?]{1,64}",
            include_self in any::<bool>(),
            limit in 1u32..50,
        ) {
            let req = Request::FindExpert {
                account_id: AccountId::new(),
                query: query.clone(),
                include_self,
                limit,
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::FindExpert { query: q, include_self: i, limit: l, .. } => {
                    prop_assert_eq!(q, query);
                    prop_assert_eq!(i, include_self);
                    prop_assert_eq!(l, limit);
                }
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }

        #[test]
        fn explain_entity_roundtrip(
            query in "[a-zA-Z @.]{1,64}",
            limit in 1u32..50,
        ) {
            let req = Request::ExplainEntity {
                account_id: AccountId::new(),
                query: query.clone(),
                limit,
            };
            let json = serde_json::to_string(&req)?;
            let parsed: Request = serde_json::from_str(&json)?;
            match parsed {
                Request::ExplainEntity { query: q, limit: l, .. } => {
                    prop_assert_eq!(q, query);
                    prop_assert_eq!(l, limit);
                }
                other => prop_assert!(false, "wrong variant: {other:?}"),
            }
        }
    }

    /// Helpers shared across the AI-email roundtrip proptests.
    mod ai_email_test {
        use super::*;

        pub(super) fn sample_draft(body: &str) -> Draft {
            Draft {
                id: DraftId::new(),
                account_id: AccountId::new(),
                reply_headers: None,
                intent: DraftIntent::New,
                to: vec![Address {
                    name: None,
                    email: "alice@example.com".into(),
                }],
                cc: vec![],
                bcc: vec![],
                subject: "test".into(),
                body_markdown: body.into(),
                attachments: vec![],
                inline_calendar_reply: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }
        }
    }
}
