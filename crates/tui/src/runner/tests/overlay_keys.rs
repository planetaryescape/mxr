//! Overlays read keys in their own handlers, as the tool screens do, so
//! this extends `screen_keys_are_what_the_definition_says` to them: every
//! key the definition lists for an overlay does what it says, and every key
//! it doesn't list does nothing. Most overlay keys act in place (`inline:`),
//! so the probe watches what the key changes: the drawn screen, the status
//! line and the requests waiting to go out.

use super::screen_keys::screen_probe_keys;
use super::*;
use crate::app::{
    AccountFormMode, ConfirmChoice, DraftOptionsField, ErrorModalState, ExportStage, MoveMenu,
    PendingBulkConfirm, PendingUnsubscribeConfirm, ReplyLaterPromptState, SavedSearchFormField,
    SavedSearchFormState, TodoPromptKind, TodoPromptState,
};
use mxr_protocol::SenderKindData;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::collections::BTreeMap;

/// An overlay, the states that show it, and whether letters type in it.
struct OverlayProbe {
    context: &'static str,
    setups: &'static [fn(&mut App)],
    types: bool,
}

const fn probe(context: &'static str, setups: &'static [fn(&mut App)]) -> OverlayProbe {
    OverlayProbe {
        context,
        setups,
        types: false,
    }
}

const fn typing(context: &'static str, setups: &'static [fn(&mut App)]) -> OverlayProbe {
    OverlayProbe {
        context,
        setups,
        types: true,
    }
}

const OVERLAYS: &[OverlayProbe] = &[
    typing("help", &[help]),
    typing("palette", &[palette]),
    typing("label-picker", &[label_picker]),
    probe("links", &[links]),
    probe("unsubscribe", &[unsubscribe]),
    probe("move-picker", &[move_picker]),
    probe("move-picker-sender", &[move_picker_sender]),
    probe("new-sender-picker", &[new_sender_picker]),
    probe("sender-kind-picker", &[sender_kind_picker]),
    probe("not-now", &[not_now]),
    typing("not-now-custom", &[not_now_custom]),
    probe("correct-todo", &[correct_todo]),
    probe("delete-confirm", &[delete_confirm]),
    probe("bulk-confirm", &[bulk_confirm]),
    probe("sweep-confirm", &[sweep_confirm]),
    typing("reply-later", &[reply_later]),
    probe("categories", &[categories]),
    probe("send-review", &[send_review, blocked_send_review]),
    typing("send-time", &[send_time]),
    probe("error", &[error]),
    probe("sheet", &[sheet]),
    probe("platform", &[platform]),
    probe("welcome", &[welcome]),
    probe("snippets", &[snippets]),
    probe("sender-profile", &[sender_profile, sender_profile_messages]),
    probe("reply-queue", &[reply_queue]),
    probe("drafts", &[drafts]),
    typing("compose-picker", &[compose_picker]),
    probe("model-picker", &[model_picker]),
    probe("voice-source", &[voice_source]),
    probe("attachments", &[attachments]),
    typing("save-attachment", &[save_attachment]),
    probe("export", &[export]),
    probe("export-destination", &[export_destination]),
    typing("export-path", &[export_path]),
    typing("filter-bar", &[filter_bar]),
    typing("analytics-filter", &[analytics_filter]),
    typing("saved-search-form", &[saved_search_form]),
    probe("messages-filter", &[messages_filter]),
    probe("draft-options", &[draft_options]),
    typing("records-prompt", &[records_prompt]),
    probe("pass-menu", &[pass_menu]),
    typing("todo-prompt", &[todo_prompt]),
    probe("arrivals-list", &[arrivals_list]),
    probe("resume-draft", &[resume_draft]),
    probe("account-mode-switch", &[account_mode_switch]),
    probe("identity-editor", &[identity_editor]),
    probe("auth-session", &[auth_session]),
    probe("assistant-history", &[assistant_history]),
];

/// Overlays the probe leaves out, and why. Every other overlay with TUI
/// keys must be in `OVERLAYS`.
const NOT_PROBED: &[&str] = &[
    // Every key but its own passes through to the place underneath, which
    // that place's probe covers; `runner::tests::moves` covers y and n.
    "sender-ask",
    // The same: any other key works as usual. `app::promises` covers it.
    "promise-prompt",
    // With a proof or a question waiting, any other key works as it does
    // in the assistant pane, which the screen probe covers;
    // `runner::tests::assistant` covers their own keys.
    "assistant-approval",
    "assistant-question",
];

fn inbox() -> App {
    let mut app = App::new();
    app.screen = Screen::Mailbox;
    app.mailbox.mailbox_view = MailboxView::Messages;
    app.mailbox.active_pane = ActivePane::MailList;
    app.mailbox.envelopes = make_test_envelopes(3);
    app
}

fn press(app: &mut App, code: KeyCode) {
    let modifiers = match code {
        KeyCode::Char(c) if c.is_ascii_uppercase() => KeyModifiers::SHIFT,
        _ => KeyModifiers::NONE,
    };
    if let Some(action) = app.handle_key(KeyEvent::new(code, modifiers)) {
        app.apply(action);
    }
}

fn help(app: &mut App) {
    *app = inbox();
    app.modals.help_open = true;
    app.modals.help_scroll_offset = 4;
}

fn palette(app: &mut App) {
    *app = inbox();
    app.apply(Action::OpenCommandPalette);
    app.command_palette.palette.select_next();
}

fn label_picker(app: &mut App) {
    *app = inbox();
    let account_id = AccountId::new();
    app.mailbox.labels = ["Work", "Home", "Bills"]
        .into_iter()
        .map(|name| Label {
            id: LabelId::new(),
            account_id: account_id.clone(),
            provider_id: name.into(),
            name: name.into(),
            kind: LabelKind::User,
            color: None,
            unread_count: 0,
            total_count: 1,
            role: None,
        })
        .collect();
    app.apply(Action::ApplyLabel);
    app.modals.label_picker.select_next();
}

fn links(app: &mut App) {
    *app = inbox();
    app.mailbox.url_modal = Some(crate::ui::url_modal::UrlModalState::new(
        [
            "https://a.example",
            "https://b.example",
            "https://c.example",
        ]
        .into_iter()
        .map(|url| crate::ui::url_modal::UrlEntry {
            label: url.into(),
            url: url.into(),
        })
        .collect(),
    ));
    app.mailbox.url_modal.as_mut().unwrap().next();
}

fn unsubscribe(app: &mut App) {
    *app = inbox();
    app.modals.pending_unsubscribe_confirm = Some(PendingUnsubscribeConfirm {
        message_id: MessageId::new(),
        account_id: AccountId::new(),
        sender_email: "news@example.com".into(),
        method_label: "one-click".into(),
        archive_message_ids: vec![MessageId::new()],
    });
}

fn move_menu(app: &mut App, sender_only: bool, new_sender: bool) {
    *app = inbox();
    app.mailbox.trust.move_menu = Some(MoveMenu {
        message_id: MessageId::new(),
        display: "Maya".into(),
        sender_only,
        not_sure: false,
        current: None,
        new_sender: new_sender.then(|| mxr_protocol::ScreenerQuestionData {
            account_id: AccountId::new(),
            sender_email: "maya@example.com".into(),
            question: "New sender. Keep in Messages?".into(),
            choices: [
                (SenderKindData::People, "Messages"),
                (SenderKindData::PaperTrail, "Updates"),
                (SenderKindData::Reading, "Reading"),
                (SenderKindData::ScreenedOut, "Block"),
            ]
            .into_iter()
            .map(|(kind, label)| mxr_protocol::ScreenerChoiceData {
                kind,
                label: label.into(),
            })
            .collect(),
        }),
        selected: 1,
    });
}

fn move_picker(app: &mut App) {
    move_menu(app, false, false);
}

fn move_picker_sender(app: &mut App) {
    move_menu(app, true, false);
}

fn new_sender_picker(app: &mut App) {
    move_menu(app, false, true);
}

fn sender_kind_picker(app: &mut App) {
    *app = inbox();
    app.mailbox.sender_kind_menu = Some(crate::app::SenderKindMenu {
        account_id: AccountId::new(),
        sender_email: "news@example.com".into(),
        display: "News".into(),
        current: mxr_protocol::MailKindData {
            kind: SenderKindData::Reading,
            rule: mxr_protocol::KindRuleData::ListUnsubscribe,
            reason: "a list".into(),
            corrected: false,
        },
        selected: 1,
    });
}

fn not_now(app: &mut App) {
    *app = inbox();
    app.apply(Action::Snooze);
    app.modals.snooze_panel.selected_index = 1;
}

/// "fri 3" reads two ways, so Tab has another reading to show.
const TWO_READINGS: &str = "fri 3";

fn not_now_custom(app: &mut App) {
    not_now(app);
    app.modals.snooze_panel.custom_input = Some(TWO_READINGS.into());
    let config = app.modals.snooze_config.clone();
    app.modals
        .snooze_panel
        .custom_time
        .update(TWO_READINGS, &config);
}

fn correct_todo(app: &mut App) {
    *app = inbox();
    app.mailbox.mailbox_view = MailboxView::Todo;
    let mut prompt = TodoPromptState::new(
        TodoPromptKind::Correct {
            todo_id: "todo_1".into(),
        },
        String::new(),
    );
    prompt.selected = 1;
    app.mailbox.todo_page.prompt = Some(prompt);
}

/// The saved-search delete; every other destroying confirm answers through
/// the same `ConfirmChoice::key`.
fn delete_confirm(app: &mut App) {
    *app = inbox();
    app.modals.pending_saved_search_delete_confirm = Some("Receipts".into());
    app.modals.saved_search_delete_choice = ConfirmChoice::default();
}

fn bulk_confirm(app: &mut App) {
    *app = inbox();
    let ids: Vec<MessageId> = app.mailbox.envelopes.iter().map(|e| e.id.clone()).collect();
    app.modals.pending_bulk_confirm = Some(PendingBulkConfirm {
        title: "Archive 3 conversations?".into(),
        detail: "From the inbox".into(),
        request: Request::Mutation {
            mutation: MutationCommand::Archive {
                message_ids: ids.clone(),
            },
            client_correlation_id: None,
        },
        effect: MutationEffect::StatusOnly("Archived".into()),
        optimistic_effect: None,
        status_message: "Archiving...".into(),
        preview: crate::app::BulkPreview {
            rows: (0..40)
                .map(|i| crate::app::BulkPreviewRow {
                    message_id: MessageId::new(),
                    sender: format!("sender{i}@example.com"),
                    subject: format!("Subject {i}"),
                })
                .collect(),
            not_in_view: 0,
        },
        preview_scroll: 2,
    });
}

fn sweep_confirm(app: &mut App) {
    *app = super::places::whole_place_preview();
}

fn reply_later(app: &mut App) {
    *app = inbox();
    let mut prompt = ReplyLaterPromptState::new(ThreadId::new(), MessageId::new(), false);
    prompt.input = TWO_READINGS.into();
    prompt.time.update(TWO_READINGS, &app.modals.snooze_config);
    app.modals.reply_later_prompt = Some(prompt);
}

fn categories(app: &mut App) {
    *app = inbox();
    let picker = &mut app.modals.personal_categories;
    picker.open(
        crate::ui::personal_category_picker::CategorySurface::Catalog,
        vec![MessageId::new()],
    );
    picker.set_categories(
        ["Travel", "Family", "Bills"]
            .into_iter()
            .map(|name| PersonalCategory {
                id: CategoryId::new(),
                name: name.into(),
                unread_count: 0,
                total_count: 2,
            })
            .collect(),
    );
    picker.selected = 1;
}

fn pending_send() -> PendingSend {
    PendingSend {
        draft_revision: None,
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "recipient@example.com".into(),
            from: "sender@example.com".into(),
            subject: "Plans".into(),
            ..Default::default()
        },
        body: "Hello".into(),
        draft_path: std::path::PathBuf::from("/tmp/mxr-overlay-probe-draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
        resolved_from: None,
        stored_draft: None,
    }
}

fn send_review(app: &mut App) {
    *app = inbox();
    app.compose.pending_send_confirm = Some(pending_send());
}

/// Blocked, so `O` overrides; nothing else sends.
fn blocked_send_review(app: &mut App) {
    send_review(app);
    let pending = app.compose.pending_send_confirm.as_mut().unwrap();
    pending.safety_report = Some(DraftSafetyReport {
        allowed: false,
        issues: Vec::new(),
        verdict: DraftSafetyVerdict::Blocked,
        checked_at: None,
    });
    pending.override_token = Some("token".into());
    pending.suggested_collaborators = vec![mxr_protocol::SuggestedRecipientData {
        email: "ana@example.com".into(),
        display_name: None,
        reason: "often copied".into(),
        confidence: "high".into(),
        evidence_msg_ids: Vec::new(),
    }];
}

fn send_time(app: &mut App) {
    send_review(app);
    app.compose.pending_send_at_input = Some(TWO_READINGS.into());
    let config = app.modals.snooze_config.clone();
    app.compose.pending_time.update(TWO_READINGS, &config);
}

fn error(app: &mut App) {
    *app = inbox();
    let detail = (0..80).map(|i| format!("line {i}\n")).collect::<String>();
    let mut state = ErrorModalState::new("Sync failed", detail);
    state.scroll_offset = 10;
    app.modals.error = Some(state);
}

/// The summary; the briefing, whois and expert sheets close on the same keys.
fn sheet(app: &mut App) {
    *app = inbox();
    app.modals.summary.visible = true;
}

fn platform(app: &mut App) {
    *app = inbox();
    app.modals
        .platform
        .open_loading("Voice profile", "Loading voice profile...");
}

/// First run: no account yet, so `q` quits.
fn welcome(app: &mut App) {
    *app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.onboarding_required = true;
    app.accounts.page.onboarding_modal_open = true;
}

fn snippets(app: &mut App) {
    *app = inbox();
    app.apply(Action::OpenSnippets);
}

fn sender_profile(app: &mut App) {
    *app = inbox();
    app.modals.sender_profile.visible = true;
}

fn sender_profile_messages(app: &mut App) {
    sender_profile(app);
    app.modals
        .sender_profile
        .select_tab(crate::app::SenderProfileTab::Messages);
}

fn reply_queue(app: &mut App) {
    *app = inbox();
    app.apply(Action::OpenReplyQueue);
}

fn drafts(app: &mut App) {
    *app = inbox();
    app.apply(Action::OpenStoredDrafts);
}

fn compose_picker(app: &mut App) {
    *app = inbox();
    app.mailbox.all_envelopes = make_test_envelopes(3);
    app.apply(Action::Compose);
    app.compose.compose_picker.select_next();
}

fn model_picker(app: &mut App) {
    *app = inbox();
    let choice = |id: &str| mxr_protocol::ModelChoiceData {
        id: id.into(),
        label: id.into(),
        cloud: true,
        sees: "this conversation only".into(),
        available: true,
        unavailable_reason: None,
    };
    app.compose.model_picker = Some(crate::app::ComposeModelPicker {
        choices: mxr_protocol::AssistantModelsData {
            current: None,
            choices: vec![choice("luna"), choice("sol")],
        },
        selected_index: 1,
        pending: Box::new(pending_send()),
        purpose: crate::app::ComposeModelPickerPurpose::InitialDraft,
    });
}

fn voice_source(app: &mut App) {
    *app = inbox();
    app.compose.viewing_voice_source = true;
    app.mailbox.message_scroll_offset = 20;
}

fn attachments(app: &mut App) {
    *app = inbox();
    let message_id = app.mailbox.envelopes[0].id.clone();
    let panel = &mut app.mailbox.attachment_panel;
    panel.visible = true;
    panel.message_id = Some(message_id.clone());
    panel.attachments = ["a.pdf", "b.pdf", "c.pdf"]
        .into_iter()
        .map(|name| AttachmentMeta {
            id: AttachmentId::new(),
            message_id: message_id.clone(),
            filename: name.into(),
            mime_type: "application/pdf".into(),
            disposition: AttachmentDisposition::Attachment,
            content_id: None,
            content_location: None,
            size_bytes: 40,
            local_path: None,
            provider_id: name.into(),
        })
        .collect();
    panel.selected_index = 1;
}

fn save_attachment(app: &mut App) {
    attachments(app);
    press(app, KeyCode::Char('d'));
    app.modals.save_attachment.input = "/elsewhere/b.pdf".into();
}

fn export(app: &mut App) {
    *app = inbox();
    app.modals.export.open(ThreadId::new());
    app.modals.export.selected = 1;
}

fn export_destination(app: &mut App) {
    export(app);
    app.modals.export.stage = ExportStage::Destination;
    app.modals.export.format = Some(mxr_core::ExportFormat::Markdown);
}

fn export_path(app: &mut App) {
    export(app);
    app.modals.export.stage = ExportStage::Path;
    app.modals.export.input = "/tmp/thread.md".into();
}

fn filter_bar(app: &mut App) {
    *app = inbox();
    app.apply(Action::OpenMailboxFilter);
}

fn analytics_filter(app: &mut App) {
    *app = inbox();
    app.screen = Screen::Analytics;
    let field = |label: &str, value: &str, options: &[&str]| crate::app::AnalyticsFilterField {
        label: label.into(),
        value: value.into(),
        options: options.iter().map(|option| (*option).into()).collect(),
    };
    app.modals.analytics_filter = Some(crate::app::AnalyticsFilterModalState {
        view: crate::app::AnalyticsView::Storage,
        active_field: 1,
        fields: vec![
            field("Account", "", &[]),
            field("Group by", "sender", &["sender", "domain", "label"]),
            field("Limit", "20", &[]),
        ],
        validation_error: None,
    });
}

fn saved_search_form(app: &mut App) {
    *app = inbox();
    app.modals.saved_search_form = Some(SavedSearchFormState {
        existing_name: None,
        name: "Receipts".into(),
        query: "from:shop".into(),
        search_mode: SearchMode::Lexical,
        active_field: SavedSearchFormField::Mode,
        validation_error: None,
    });
}

fn messages_filter(app: &mut App) {
    *app = inbox();
    app.mailbox.mailbox_view = MailboxView::People;
    app.modals.messages_filter = Some(crate::app::MessagesFilterPicker { cursor: 2 });
}

fn draft_options(app: &mut App) {
    *app = inbox();
    app.modals.draft_options.open(ThreadId::new());
    app.modals.draft_options.active = DraftOptionsField::Register;
    app.modals.draft_options.register_idx = 1;
}

fn records_prompt(app: &mut App) {
    *app = inbox();
    app.mailbox.mailbox_view = MailboxView::ArchiveMode;
    app.mailbox.records_page.prompt = Some(crate::app::RecordFixPrompt {
        record_id: "rec_1".into(),
        input: "amount=12".into(),
        previewed: None,
        preview: None,
        error: None,
    });
}

fn pass_menu(app: &mut App) {
    *app = inbox();
    app.mailbox.records_page.pass_menu = Some(crate::app::PassMenu {
        message_id: MessageId::new(),
        preview: None,
    });
}

fn todo_prompt(app: &mut App) {
    *app = inbox();
    app.mailbox.todo_page.prompt = Some(TodoPromptState::new(
        TodoPromptKind::Create {
            message_id: MessageId::new(),
        },
        "Reply to Maya".into(),
    ));
}

fn arrivals_list(app: &mut App) {
    *app = inbox();
    app.mailbox.mailbox_view = MailboxView::Now;
    // A fixed day, not the clock: "2m ago" turning into "3m ago" during
    // the probe's two minutes would read as every later key acting.
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-05T09:00:00Z")
        .expect("a valid time")
        .with_timezone(&chrono::Utc);
    let item = |subject: &str| mxr_protocol::ArrivalItemData {
        account_id: AccountId::new(),
        message_id: MessageId::new(),
        thread_id: ThreadId::new(),
        sender_email: "maya@example.com".into(),
        sender_name: None,
        subject: subject.into(),
        date: now,
        first_seen_at: now,
        arrived_in: mxr_protocol::ArrivalBucketData::Messages,
        bucket: mxr_protocol::ArrivalBucketData::Messages,
        reason: None,
        chip: "→ Messages · from a person".into(),
        moved: false,
        not_sure: false,
        unread: true,
        also_todo: false,
        also_archive: false,
    };
    app.mailbox.trust.arrivals_list = Some(crate::app::ArrivalsListState {
        since: now,
        until: now,
        buckets: vec![mxr_protocol::ArrivalBucketData::Messages],
        bucket: None,
        list: Some(mxr_protocol::ArrivalListData {
            since: now,
            until: now,
            bucket: None,
            total: 3,
            items: vec![item("One"), item("Two"), item("Three")],
        }),
        selected: 1,
    });
}

fn accounts_screen(app: &mut App) {
    *app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.onboarding_required = false;
}

fn resume_draft(app: &mut App) {
    accounts_screen(app);
    app.accounts.page.new_account_draft = Some(crate::app::AccountFormState {
        is_new_account: true,
        ..Default::default()
    });
    app.apply(Action::OpenAccountFormNew);
}

fn account_mode_switch(app: &mut App) {
    accounts_screen(app);
    app.accounts.page.form.visible = true;
    app.accounts.page.form.pending_mode_switch = Some(AccountFormMode::Gmail);
}

fn identity_editor(app: &mut App) {
    accounts_screen(app);
    app.accounts.page.identity_editor = Some(crate::app::AccountIdentityEditor {
        account_key: "work".into(),
        account_name: "Work".into(),
        color: mxr_core::AccountColor::Sky,
        tag: "WK".into(),
        field: crate::app::IdentityField::Colour,
        previewed: false,
        in_flight_dry_run: None,
        message: None,
    });
}

fn auth_session(app: &mut App) {
    accounts_screen(app);
    app.accounts.page.active_auth_session = Some(mxr_protocol::AuthSessionData {
        session_id: mxr_protocol::AuthSessionId("probe".into()),
        state: mxr_protocol::AuthSessionStateData::WaitingForUser,
        flow: mxr_protocol::AuthFlowData::Device,
        account_key: "work".into(),
        auth_url: None,
        user_code: Some("ABC-DEF".into()),
        verification_uri: None,
        expires_at_unix: None,
        poll_interval_secs: Some(1),
        message: None,
        error: None,
    });
}

fn assistant_history(app: &mut App) {
    let summary = |id: &str, title: &str| mxr_protocol::AssistantConversationSummaryData {
        id: id.into(),
        title: title.into(),
        status: mxr_protocol::AssistantStatusData::Idle,
        last_outcome: None,
        created_at: 0,
        updated_at: 0,
        message_count: 2,
        pending_count: 0,
    };
    app.screen = Screen::Mailbox;
    app.assistant.open = true;
    app.assistant.focus = crate::app::AssistantFocus::Pane;
    app.assistant.list = Some(crate::app::AssistantListState {
        loading: false,
        // The middle row lit, so both j and k move.
        conversations: vec![
            summary("c1", "Receipts"),
            summary("c2", "Old trip"),
            summary("c3", "Invoices"),
        ],
        selected: 1,
    });
}

/// What a key can change while an overlay is open: the drawn screen, the
/// status line, and requests waiting to go out.
fn fingerprint(app: &mut App) -> String {
    use std::hash::{Hash, Hasher};
    let mut terminal = Terminal::new(TestBackend::new(110, 34)).expect("test terminal");
    terminal
        .draw(|frame| app.draw(frame))
        .expect("the app draws");
    // A highlight is a style, so each cell's colours and modifiers count.
    let mut screen = std::collections::hash_map::DefaultHasher::new();
    for cell in terminal.backend().buffer().content() {
        (cell.symbol(), cell.fg, cell.bg, cell.modifier).hash(&mut screen);
    }
    // State a key changes that the screen behind an overlay may not draw.
    format!(
        "{} {:?} {} {} {} {} {:?} {:?} {} {} {:?} {:?} {} {:?} {}",
        screen.finish(),
        app.status_message,
        app.pending_mutation_queue.len(),
        app.pending_category_requests.len(),
        app.modals.pending_platform_dispatch.len(),
        app.compose.pending_compose.is_some(),
        app.mailbox.pending_attachment_action.is_some(),
        app.accounts.pending_identity.is_some(),
        app.compose.viewing_voice_source,
        app.mailbox.message_scroll_offset,
        app.mailbox.records_page.pending_fix_preview.is_some(),
        app.mailbox.records_page.pending_file_preview.is_some(),
        app.modals.personal_categories.filtering,
        app.modals.sender_profile.active_tab,
        app.assistant.outbound.len(),
    )
}

/// Key → definition name for an overlay.
fn defined(overlay: &str) -> BTreeMap<String, String> {
    let mut keys = BTreeMap::new();
    for binding in mxr_keymap::defaults().actions.iter().flat_map(|a| &a.tui) {
        if binding.contexts.includes(overlay) {
            for key in &binding.keys {
                keys.insert(key.clone(), binding.name.clone());
            }
        }
    }
    keys
}

/// A key a text field takes as typing.
fn types(event: &KeyEvent) -> bool {
    match event.code {
        KeyCode::Char(_) => !event
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT),
        KeyCode::Backspace => true,
        _ => false,
    }
}

struct Pressed {
    action: Option<Action>,
    changed: bool,
    after: String,
}

/// Press `event` in a fresh app set up by `setup`, whose fingerprint is
/// `before`.
fn press_in(setup: fn(&mut App), before: &str, event: KeyEvent) -> Pressed {
    let mut app = App::new();
    setup(&mut app);
    let action = app.handle_key(event);
    let after = fingerprint(&mut app);
    Pressed {
        action,
        changed: before != after,
        after,
    }
}

/// The probe draws the screen for every key in every overlay, so it is split
/// into this many tests for a runner to run side by side. Each takes every
/// `PARTS`th overlay, so a new overlay lands in one without being assigned.
const PARTS: usize = 4;

#[test]
fn overlay_keys_are_what_the_definition_says_part_1() {
    overlay_keys_are_what_the_definition_says(0);
}

#[test]
fn overlay_keys_are_what_the_definition_says_part_2() {
    overlay_keys_are_what_the_definition_says(1);
}

#[test]
fn overlay_keys_are_what_the_definition_says_part_3() {
    overlay_keys_are_what_the_definition_says(2);
}

#[test]
fn overlay_keys_are_what_the_definition_says_part_4() {
    overlay_keys_are_what_the_definition_says(3);
}

fn overlay_keys_are_what_the_definition_says(part: usize) {
    let keymap = mxr_keymap::defaults();
    let keys = screen_probe_keys();
    let mut wrong = Vec::new();
    for overlay in OVERLAYS.iter().skip(part).step_by(PARTS) {
        let context = overlay.context;
        let defined = defined(context);
        for key in defined.keys() {
            if !keys.iter().any(|(_, name)| name == key) {
                wrong.push(format!("{context} `{key}`: not a key the probe presses"));
            }
        }
        let mut befores = Vec::new();
        for setup in overlay.setups {
            let mut first = App::new();
            setup(&mut first);
            let once = fingerprint(&mut first);
            if once != fingerprint(&mut first) {
                wrong.push(format!("{context}: draws differently each time"));
            }
            befores.push(once);
        }
        for (event, name) in &keys {
            let pressed: Vec<Pressed> = overlay
                .setups
                .iter()
                .zip(&befores)
                .map(|(setup, before)| press_in(*setup, before, *event))
                .collect();
            let acted = |p: &Pressed| p.action.is_some() || p.changed;
            match defined.get(name).map(String::as_str) {
                Some(inline) if inline.starts_with("inline:") => {
                    if let Some(action) = pressed.iter().find_map(|p| p.action.as_ref()) {
                        wrong.push(format!(
                            "{context} `{name}`: listed as {inline} but runs {action:?}"
                        ));
                    } else if !pressed.iter().any(|p| p.changed) {
                        wrong.push(format!(
                            "{context} `{name}`: listed as {inline} but changes nothing"
                        ));
                    }
                }
                Some(lit) if lit.starts_with("lit:") => {
                    if !pressed.iter().any(acted) {
                        wrong.push(format!(
                            "{context} `{name}`: listed as {lit} but does nothing"
                        ));
                    }
                }
                Some(listed) => {
                    let got: Vec<String> = pressed
                        .iter()
                        .filter_map(|p| p.action.as_ref().map(|a| format!("{a:?}")))
                        .collect();
                    if got.is_empty() || got.iter().any(|action| action != listed) {
                        wrong.push(format!(
                            "{context} `{name}`: listed as {listed} but runs {got:?}"
                        ));
                    }
                }
                None if overlay.types && types(event) => {}
                None if keymap.moved_from(context, name).is_some() => {}
                None => {
                    // Handlers that ignore Ctrl or Alt make `Ctrl+j` an echo of `j`.
                    let echo = event
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && overlay.setups.iter().zip(&befores).zip(&pressed).all(
                            |((setup, before), p)| {
                                let plain = press_in(
                                    *setup,
                                    before,
                                    KeyEvent::new(event.code, KeyModifiers::NONE),
                                );
                                plain.action == p.action && plain.after == p.after
                            },
                        );
                    if !echo && pressed.iter().any(acted) {
                        let action = pressed.iter().find_map(|p| p.action.as_ref());
                        wrong.push(format!(
                            "{context} `{name}`: does something ({action:?}) but the definition doesn't list it"
                        ));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_overlay_with_keys_is_probed() {
    let keymap = mxr_keymap::defaults();
    let unprobed: Vec<&str> = mxr_keymap::OVERLAY_CONTEXTS
        .into_iter()
        .filter(|place| {
            keymap
                .actions
                .iter()
                .flat_map(|a| &a.tui)
                .any(|b| !b.keys.is_empty() && b.contexts.includes(place))
        })
        .filter(|place| {
            !OVERLAYS.iter().any(|overlay| overlay.context == *place) && !NOT_PROBED.contains(place)
        })
        .collect();
    assert!(unprobed.is_empty(), "add {unprobed:?} to OVERLAYS");
}
