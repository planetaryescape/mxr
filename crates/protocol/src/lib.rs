mod codec;
mod types;

pub use codec::IpcCodec;
pub use types::*;

pub const IPC_PROTOCOL_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use bytes::BytesMut;
    use mxr_core::id::*;
    use mxr_core::{
        Address, Draft, ExportFormat, SavedSearch, SearchMode, SemanticProfile,
        SemanticRuntimeMetrics, SemanticStatusSnapshot, SortOrder,
    };
    use proptest::prelude::*;
    use tokio_util::codec::{Decoder, Encoder};

    fn sample_account_config() -> AccountConfigData {
        AccountConfigData {
            key: "work".into(),
            name: "Work".into(),
            email: "work@example.com".into(),
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
                },
                IpcCategory::AdminMaintenance,
            ),
            (
                Request::GetLogs {
                    limit: 10,
                    level: None,
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
                Request::Mutation(MutationCommand::Archive {
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
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::Thread {
                    thread: sample_thread(),
                    messages: vec![sample_envelope()],
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
                    has_more: false,
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
                        in_reply_to: "id".into(),
                        references: Vec::new(),
                        reply_to: "a@example.com".into(),
                        cc: String::new(),
                        subject: "Re: x".into(),
                        from: "a@example.com".into(),
                        thread_context: "ctx".into(),
                    },
                },
                IpcCategory::CoreMail,
            ),
            (
                ResponseData::ForwardContext {
                    context: ForwardContext {
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
        let err = Response::Error {
            message: "something failed".to_string(),
        };

        for resp in [ok, err] {
            let msg = IpcMessage {
                id: 2,
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
        let event_categories = vec![
            (
                DaemonEvent::SyncCompleted {
                    account_id: AccountId::new(),
                    messages_synced: 1,
                },
                IpcCategory::CoreMail,
            ),
            (
                DaemonEvent::SyncError {
                    account_id: AccountId::new(),
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
                DaemonEvent::LabelCountsUpdated { counts: Vec::new() },
                IpcCategory::CoreMail,
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
    }
}
