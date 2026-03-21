use chrono::{TimeZone, Utc};
use mxr_compose::frontmatter::ComposeFrontmatter;
use mxr_core::id::{AccountId, MessageId, SavedSearchId, ThreadId};
use mxr_core::types::{Address, Envelope, Label, LabelKind, MessageFlags, UnsubscribeMethod};
use mxr_core::types::{SavedSearch, SearchMode, SortOrder};
use mxr_protocol::{
    AccountEditModeData, AccountSourceData, AccountSummaryData, DaemonHealthClass, DoctorDataStats,
    DoctorReport, EventLogEntry, MutationCommand, Request,
};
use mxr_test_support::render_to_string;
use mxr_tui::app::{
    AccountFormState, AccountsPageState, ActivePane, AttachmentPanelState, BodySource,
    BodyViewState, DiagnosticsPageState, MailListMode, MailListRow, MutationEffect,
    PendingBulkConfirm, PendingSend, Screen, SearchPageState,
};
use mxr_tui::ui::attachment_modal::draw as draw_attachment_modal;
use mxr_tui::ui::bulk_confirm_modal::draw as draw_bulk_confirm_modal;
use mxr_tui::ui::command_palette::{draw as draw_command_palette, CommandPalette};
use mxr_tui::ui::compose_picker::{draw as draw_compose_picker, ComposePicker, Contact};
use mxr_tui::ui::help_modal::{draw as draw_help_modal, HelpModalState};
use mxr_tui::ui::label_picker::{draw as draw_label_picker, LabelPicker, LabelPickerMode};
use mxr_tui::ui::message_view::{draw as draw_message_view, ThreadMessageBlock};
use mxr_tui::ui::search_bar::{draw as draw_search_bar, SearchBar};
use mxr_tui::ui::search_page::draw as draw_search_page;
use mxr_tui::ui::send_confirm_modal::draw as draw_send_confirm;
use mxr_tui::ui::sidebar::{draw as draw_sidebar, SidebarView};
use mxr_tui::ui::status_bar::{draw as draw_status_bar, StatusBarState};
use mxr_tui::ui::unsubscribe_modal::draw as draw_unsubscribe_modal;
use mxr_tui::ui::{accounts_page, diagnostics_page};
use ratatui::layout::Rect;

fn sample_envelope() -> Envelope {
    Envelope {
        id: MessageId::new(),
        account_id: AccountId::new(),
        provider_id: "msg-1".into(),
        thread_id: ThreadId::new(),
        message_id_header: Some("<msg-1@example.com>".into()),
        in_reply_to: None,
        references: vec![],
        from: Address {
            name: Some("Alice Example".into()),
            email: "alice@example.com".into(),
        },
        to: vec![Address {
            name: Some("Bob Example".into()),
            email: "bob@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Snapshot fixture".into(),
        date: Utc.with_ymd_and_hms(2024, 3, 15, 9, 30, 0).unwrap(),
        flags: MessageFlags::READ | MessageFlags::STARRED,
        snippet: "fixture snippet".into(),
        has_attachments: true,
        size_bytes: 1024,
        unsubscribe: UnsubscribeMethod::HttpLink {
            url: "https://example.com/unsubscribe".into(),
        },
        label_provider_ids: vec!["INBOX".into(), "STARRED".into()],
    }
}

fn sample_label(name: &str, kind: LabelKind, unread_count: u32, total_count: u32) -> Label {
    Label {
        id: mxr_core::id::LabelId::from_provider_id("test", name),
        account_id: AccountId::new(),
        name: name.into(),
        kind,
        color: None,
        provider_id: name.into(),
        unread_count,
        total_count,
    }
}

fn sample_mail_row() -> MailListRow {
    MailListRow {
        thread_id: ThreadId::new(),
        representative: sample_envelope(),
        message_count: 3,
        unread_count: 1,
    }
}

#[test]
fn message_view_snapshot() {
    let block = ThreadMessageBlock {
        envelope: sample_envelope(),
        body_state: BodyViewState::Ready {
            raw: "Hello\n> quoted\n> lines\n-- \nSig".into(),
            rendered: "Hello\n> quoted\n> lines\n-- \nSig".into(),
            source: BodySource::Plain,
        },
        labels: vec!["INBOX".into(), "STARRED".into(), "UNSUBSCRIBE".into()],
        attachments: vec![mxr_tui::app::AttachmentSummary {
            filename: "report.pdf".into(),
            size_bytes: 10,
        }],
        selected: true,
        has_unsubscribe: true,
        signature_expanded: false,
    };

    let snapshot = render_to_string(70, 20, |frame| {
        draw_message_view(
            frame,
            Rect::new(0, 0, 70, 20),
            &[block],
            0,
            &ActivePane::MessageView,
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("message_view_snapshot", snapshot);
}

#[test]
fn command_palette_snapshot() {
    let mut palette = CommandPalette::default();
    palette.toggle();
    palette.on_char('u');
    palette.on_char('n');

    let snapshot = render_to_string(70, 20, |frame| {
        draw_command_palette(
            frame,
            Rect::new(0, 0, 70, 20),
            &palette,
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("command_palette_snapshot", snapshot);
}

#[test]
fn label_picker_snapshot() {
    let mut picker = LabelPicker::default();
    picker.open(
        vec![
            Label {
                id: mxr_core::id::LabelId::from_provider_id("test", "INBOX"),
                account_id: AccountId::new(),
                name: "INBOX".into(),
                kind: LabelKind::System,
                color: None,
                provider_id: "INBOX".into(),
                unread_count: 4,
                total_count: 10,
            },
            Label {
                id: mxr_core::id::LabelId::from_provider_id("test", "Projects"),
                account_id: AccountId::new(),
                name: "Projects".into(),
                kind: LabelKind::User,
                color: None,
                provider_id: "Projects".into(),
                unread_count: 0,
                total_count: 2,
            },
        ],
        LabelPickerMode::Move,
    );
    picker.on_char('p');

    let snapshot = render_to_string(70, 20, |frame| {
        draw_label_picker(
            frame,
            Rect::new(0, 0, 70, 20),
            &picker,
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("label_picker_snapshot", snapshot);
}

#[test]
fn send_confirm_snapshot() {
    let pending = PendingSend {
        fm: ComposeFrontmatter {
            to: "bob@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Snapshot draft".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            references: vec![],
            attach: vec![],
        },
        body: "Hello".into(),
        draft_path: "/tmp/draft.md".into(),
        allow_send: true,
    };

    let snapshot = render_to_string(70, 20, |frame| {
        draw_send_confirm(
            frame,
            Rect::new(0, 0, 70, 20),
            Some(&pending),
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("send_confirm_snapshot", snapshot);
}

#[test]
fn sidebar_snapshot() {
    let labels = vec![
        sample_label("INBOX", LabelKind::System, 4, 10),
        sample_label("STARRED", LabelKind::System, 0, 2),
        sample_label("Projects", LabelKind::User, 1, 5),
    ];
    let searches = vec![SavedSearch {
        id: SavedSearchId::new(),
        account_id: None,
        name: "Unread".into(),
        query: "is:unread".into(),
        search_mode: SearchMode::Lexical,
        sort: SortOrder::DateDesc,
        icon: None,
        position: 0,
        created_at: Utc.with_ymd_and_hms(2024, 3, 15, 9, 30, 0).unwrap(),
    }];

    let snapshot = render_to_string(40, 18, |frame| {
        draw_sidebar(
            frame,
            Rect::new(0, 0, 40, 18),
            &SidebarView {
                labels: &labels,
                active_pane: &ActivePane::Sidebar,
                saved_searches: &searches,
                sidebar_selected: 0,
                all_mail_active: false,
                subscriptions_active: false,
                subscription_count: 2,
                system_expanded: true,
                user_expanded: true,
                saved_searches_expanded: true,
                active_label: Some(&labels[0].id),
            },
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("sidebar_snapshot", snapshot);
}

#[test]
fn search_page_snapshot() {
    let state = SearchPageState {
        query: "deployment".into(),
        editing: false,
        results: vec![sample_envelope()],
        scores: std::collections::HashMap::new(),
        selected_index: 0,
        scroll_offset: 0,
    };
    let rows = vec![sample_mail_row()];
    let preview = vec![ThreadMessageBlock {
        envelope: sample_envelope(),
        body_state: BodyViewState::Ready {
            raw: "Preview body".into(),
            rendered: "Preview body".into(),
            source: BodySource::Plain,
        },
        labels: vec!["INBOX".into()],
        attachments: vec![],
        selected: true,
        has_unsubscribe: true,
        signature_expanded: false,
    }];

    let snapshot = render_to_string(90, 24, |frame| {
        draw_search_page(
            frame,
            Rect::new(0, 0, 90, 24),
            &state,
            &rows,
            MailListMode::Threads,
            &preview,
            0,
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("search_page_snapshot", snapshot);
}

#[test]
fn accounts_page_snapshot() {
    let state = AccountsPageState {
        accounts: vec![AccountSummaryData {
            account_id: AccountId::new(),
            key: Some("work".into()),
            name: "Work".into(),
            email: "me@example.com".into(),
            provider_kind: "imap".into(),
            sync_kind: Some("imap".into()),
            send_kind: Some("smtp".into()),
            enabled: true,
            is_default: true,
            source: AccountSourceData::Config,
            editable: AccountEditModeData::Full,
            sync: None,
            send: None,
        }],
        selected_index: 0,
        status: Some("n:new  Enter:edit  t:test  d:set default".into()),
        last_result: None,
        refresh_pending: false,
        onboarding_required: false,
        onboarding_modal_open: false,
        form: AccountFormState::default(),
    };

    let snapshot = render_to_string(100, 20, |frame| {
        accounts_page::draw(
            frame,
            Rect::new(0, 0, 100, 20),
            &state,
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("accounts_page_snapshot", snapshot);
}

#[test]
fn accounts_form_gmail_snapshot() {
    let state = AccountsPageState {
        form: AccountFormState {
            visible: true,
            mode: mxr_tui::app::AccountFormMode::Gmail,
            key: "work".into(),
            name: "Work".into(),
            email: "me@example.com".into(),
            gmail_token_ref: "mxr/work-gmail".into(),
            ..AccountFormState::default()
        },
        ..AccountsPageState::default()
    };

    let snapshot = render_to_string(100, 20, |frame| {
        accounts_page::draw(
            frame,
            Rect::new(0, 0, 100, 20),
            &state,
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("accounts_form_gmail_snapshot", snapshot);
}

#[test]
fn accounts_form_gmail_editing_snapshot() {
    let state = AccountsPageState {
        form: AccountFormState {
            visible: true,
            mode: mxr_tui::app::AccountFormMode::Gmail,
            key: "work".into(),
            name: "Work".into(),
            email: "me@example.com".into(),
            gmail_token_ref: "mxr/work-gmail".into(),
            active_field: 1,
            editing_field: true,
            field_cursor: 4,
            ..AccountFormState::default()
        },
        ..AccountsPageState::default()
    };

    let snapshot = render_to_string(100, 20, |frame| {
        accounts_page::draw(
            frame,
            Rect::new(0, 0, 100, 20),
            &state,
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("accounts_form_gmail_editing_snapshot", snapshot);
}

#[test]
fn accounts_page_onboarding_snapshot() {
    let state = AccountsPageState {
        onboarding_required: true,
        onboarding_modal_open: true,
        ..AccountsPageState::default()
    };

    let snapshot = render_to_string(100, 20, |frame| {
        accounts_page::draw(
            frame,
            Rect::new(0, 0, 100, 20),
            &state,
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("accounts_page_onboarding_snapshot", snapshot);
}

#[test]
fn diagnostics_page_snapshot() {
    let state = DiagnosticsPageState {
        uptime_secs: Some(3600),
        daemon_pid: Some(4242),
        accounts: vec!["me@example.com".into()],
        total_messages: Some(42),
        sync_statuses: vec![mxr_protocol::AccountSyncStatus {
            account_id: AccountId::new(),
            account_name: "me@example.com".into(),
            last_attempt_at: Some("2026-03-20T10:58:00+00:00".into()),
            last_success_at: Some("2026-03-20T10:59:00+00:00".into()),
            last_error: None,
            failure_class: None,
            consecutive_failures: 0,
            backoff_until: None,
            sync_in_progress: false,
            current_cursor_summary: Some("gmail history_id=4242".into()),
            last_synced_count: 12,
            healthy: true,
        }],
        doctor: Some(DoctorReport {
            healthy: true,
            health_class: DaemonHealthClass::Healthy,
            lexical_index_freshness: mxr_protocol::IndexFreshness::Current,
            last_successful_sync_at: Some("2026-03-20T10:59:00+00:00".into()),
            lexical_last_rebuilt_at: Some("2026-03-20T10:55:00+00:00".into()),
            semantic_enabled: true,
            semantic_active_profile: Some("bge-small-en-v1.5".into()),
            semantic_index_freshness: mxr_protocol::IndexFreshness::Current,
            semantic_last_indexed_at: Some("2026-03-20T10:57:00+00:00".into()),
            data_stats: DoctorDataStats {
                accounts: 1,
                labels: 12,
                messages: 42,
                unread_messages: 7,
                starred_messages: 3,
                messages_with_attachments: 5,
                message_labels: 61,
                bodies: 40,
                attachments: 8,
                drafts: 2,
                snoozed: 1,
                saved_searches: 3,
                rules: 4,
                rule_logs: 9,
                sync_log: 14,
                sync_runtime_statuses: 1,
                event_log: 22,
                semantic_profiles: 1,
                semantic_chunks: 120,
                semantic_embeddings: 120,
            },
            data_dir_exists: true,
            database_exists: true,
            index_exists: true,
            socket_exists: true,
            socket_reachable: true,
            stale_socket: false,
            daemon_running: true,
            daemon_pid: Some(4242),
            daemon_protocol_version: 1,
            daemon_version: Some("0.4.3".into()),
            daemon_build_id: Some("0.4.3:/tmp/mxr:123:456".into()),
            index_lock_held: false,
            index_lock_error: None,
            restart_required: false,
            repair_required: false,
            database_path: "/tmp/mxr.db".into(),
            database_size_bytes: 1024,
            index_path: "/tmp/index".into(),
            index_size_bytes: 2048,
            log_path: "/tmp/mxr.log".into(),
            log_size_bytes: 512,
            sync_statuses: vec![],
            recent_sync_events: vec![],
            recent_error_logs: vec![],
            recommended_next_steps: vec!["mxr status".into()],
        }),
        events: vec![EventLogEntry {
            timestamp: 1710495000,
            level: "INFO".into(),
            category: "sync".into(),
            account_id: None,
            message_id: None,
            rule_id: None,
            summary: "Sync completed".into(),
            details: None,
        }],
        logs: vec!["daemon started".into(), "sync complete".into()],
        status: None,
        refresh_pending: false,
        pending_requests: 0,
    };

    let snapshot = render_to_string(100, 24, |frame| {
        diagnostics_page::draw(
            frame,
            Rect::new(0, 0, 100, 24),
            &state,
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("diagnostics_page_snapshot", snapshot);
}

#[test]
fn help_modal_snapshot() {
    let snapshot = render_to_string(100, 28, |frame| {
        draw_help_modal(
            frame,
            Rect::new(0, 0, 100, 28),
            HelpModalState {
                open: true,
                screen: Screen::Mailbox,
                active_pane: &ActivePane::MailList,
                selected_count: 2,
                scroll_offset: 0,
            },
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("help_modal_snapshot", snapshot);
}

#[test]
fn attachment_modal_snapshot() {
    let panel = AttachmentPanelState {
        visible: true,
        message_id: Some(MessageId::new()),
        attachments: vec![mxr_core::AttachmentMeta {
            id: mxr_core::AttachmentId::new(),
            message_id: MessageId::new(),
            filename: "report.pdf".into(),
            mime_type: "application/pdf".into(),
            size_bytes: 10,
            local_path: Some("/tmp/report.pdf".into()),
            provider_id: "att-1".into(),
        }],
        selected_index: 0,
        status: Some("Enter/o open  d download  j/k move  Esc close".into()),
    };

    let snapshot = render_to_string(100, 24, |frame| {
        draw_attachment_modal(
            frame,
            Rect::new(0, 0, 100, 24),
            &panel,
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("attachment_modal_snapshot", snapshot);
}

#[test]
fn compose_picker_snapshot() {
    let mut picker = ComposePicker::default();
    picker.open(vec![
        Contact {
            name: "Alice Example".into(),
            email: "alice@example.com".into(),
        },
        Contact {
            name: "Bob Example".into(),
            email: "bob@example.com".into(),
        },
    ]);
    picker.on_char('a');

    let snapshot = render_to_string(100, 20, |frame| {
        draw_compose_picker(
            frame,
            Rect::new(0, 0, 100, 20),
            &picker,
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("compose_picker_snapshot", snapshot);
}

#[test]
fn bulk_confirm_snapshot() {
    let pending = PendingBulkConfirm {
        title: "Archive messages".into(),
        detail: "Archive 3 selected messages?".into(),
        request: Request::Mutation(MutationCommand::Archive {
            message_ids: vec![MessageId::new()],
        }),
        effect: MutationEffect::RefreshList,
        status_message: "Archiving...".into(),
    };

    let snapshot = render_to_string(100, 20, |frame| {
        draw_bulk_confirm_modal(
            frame,
            Rect::new(0, 0, 100, 20),
            Some(&pending),
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("bulk_confirm_snapshot", snapshot);
}

#[test]
fn unsubscribe_modal_snapshot() {
    let pending = mxr_tui::app::PendingUnsubscribeConfirm {
        message_id: MessageId::new(),
        account_id: AccountId::new(),
        sender_email: "news@example.com".into(),
        method_label: "one-click".into(),
        archive_message_ids: vec![MessageId::new(), MessageId::new()],
    };

    let snapshot = render_to_string(100, 20, |frame| {
        draw_unsubscribe_modal(
            frame,
            Rect::new(0, 0, 100, 20),
            Some(&pending),
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("unsubscribe_modal_snapshot", snapshot);
}

#[test]
fn bars_snapshot() {
    let mut search = SearchBar::default();
    search.activate();
    search.on_char('d');
    search.on_char('e');

    let snapshot = render_to_string(80, 8, |frame| {
        draw_search_bar(
            frame,
            Rect::new(0, 0, 80, 8),
            &search,
            &mxr_tui::theme::Theme::default(),
        );
        draw_status_bar(
            frame,
            Rect::new(0, 6, 80, 1),
            &StatusBarState {
                mailbox_name: "INBOX".into(),
                total_count: 6_421,
                unread_count: 1_833,
                starred_count: 96,
                sync_status: Some("synced just now".into()),
                status_message: None,
            },
            &mxr_tui::theme::Theme::default(),
        );
    });
    insta::assert_snapshot!("bars_snapshot", snapshot);
}
