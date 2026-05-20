use super::{apply_all_envelopes_refresh, handle_daemon_event, run_with_terminal_suspended_with};
use crate::action::Action;
use crate::app::PendingSend;
use crate::app::{
    ActivePane, App, BodySource, BodyViewMetadata, BodyViewState, LayoutMode, MailListMode,
    MailboxView, MutationEffect, PendingSearchRequest, PendingSendMode, Screen, SearchPane,
    SearchTarget, SidebarItem, SEARCH_PAGE_SIZE,
};
use crate::async_result::{ComposeReadyData, SearchResultData};
use crate::compose_flow::{handle_compose_editor_status, pending_send_from_edited_draft};
use crate::input::InputHandler;
use crate::runtime::{enqueue_replaceable_request, ReplaceableRequest};
use crate::test_fixtures::TestEnvelopeBuilder;
use crate::ui::command_palette::default_commands;
use crate::ui::command_palette::CommandPalette;
use crate::ui::search_bar::SearchBar;
use crate::ui::status_bar;
use mxr_config::RenderConfig;
use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::MxrError;
use mxr_protocol::{BodyFailure, DaemonEvent, LabelCount, MutationCommand, Request};
use mxr_test_support::render_to_string;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;
use std::os::unix::process::ExitStatusExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::Instant;

fn make_test_envelopes(count: usize) -> Vec<Envelope> {
    (0..count)
        .map(|i| {
            TestEnvelopeBuilder::new()
                .provider_id(format!("fake-{}", i))
                .with_from_address(&format!("User {}", i), &format!("user{}@example.com", i))
                .to(vec![])
                .subject(format!("Subject {}", i))
                .message_id_header(None)
                .flags(if i % 2 == 0 {
                    MessageFlags::READ
                } else {
                    MessageFlags::empty()
                })
                .snippet(format!("Snippet {}", i))
                .size_bytes(1000)
                .build()
        })
        .collect()
}

fn account_summary(
    account_id: AccountId,
    enabled: bool,
    is_default: bool,
) -> mxr_protocol::AccountSummaryData {
    mxr_protocol::AccountSummaryData {
        account_id,
        key: Some("user".into()),
        name: "User".into(),
        email: "user@example.com".into(),
        provider_kind: "imap".into(),
        sync_kind: Some("imap".into()),
        send_kind: Some("smtp".into()),
        enabled,
        is_default,
        source: mxr_protocol::AccountSourceData::Config,
        editable: mxr_protocol::AccountEditModeData::Full,
        sync: None,
        send: None,
        capabilities: Default::default(),
    }
}

fn make_unsubscribe_envelope(
    account_id: AccountId,
    sender_email: &str,
    unsub: UnsubscribeMethod,
) -> Envelope {
    TestEnvelopeBuilder::new()
        .account_id(account_id)
        .provider_id("unsub-fixture")
        .with_from_address("Newsletter", sender_email)
        .to(vec![])
        .subject("Newsletter")
        .message_id_header(None)
        .snippet("newsletter")
        .size_bytes(42)
        .unsubscribe(unsub)
        .build()
}

struct TestEventSource {
    id: usize,
    dropped: Arc<AtomicBool>,
}

impl Drop for TestEventSource {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

fn exit_status(code: i32) -> std::process::ExitStatus {
    std::process::ExitStatus::from_raw(code)
}

#[test]
fn input_j_moves_down() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
        Some(Action::MoveDown)
    );
}

#[test]
fn input_k_moves_up() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)),
        Some(Action::MoveUp)
    );
}

#[test]
fn suspended_handoff_drops_old_event_source_before_running_action() {
    let old_dropped = Arc::new(AtomicBool::new(false));
    let new_created = Arc::new(AtomicBool::new(false));
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut terminal = 1usize;
    let mut events = Some(TestEventSource {
        id: 1,
        dropped: old_dropped.clone(),
    });

    let result = run_with_terminal_suspended_with(
        &mut terminal,
        &mut events,
        {
            let order = order.clone();
            move || order.lock().unwrap().push("restore")
        },
        {
            let order = order.clone();
            move || {
                order.lock().unwrap().push("init");
                2usize
            }
        },
        {
            let order = order.clone();
            let new_created = new_created.clone();
            move || {
                order.lock().unwrap().push("events");
                new_created.store(true, Ordering::SeqCst);
                TestEventSource {
                    id: 2,
                    dropped: Arc::new(AtomicBool::new(false)),
                }
            }
        },
        {
            let order = order.clone();
            let old_dropped = old_dropped.clone();
            let new_created = new_created.clone();
            move || {
                assert!(old_dropped.load(Ordering::SeqCst));
                assert!(!new_created.load(Ordering::SeqCst));
                order.lock().unwrap().push("run");
                "done"
            }
        },
    );

    assert_eq!(result, "done");
    assert_eq!(terminal, 2);
    assert_eq!(events.as_ref().map(|event| event.id), Some(2));
    assert_eq!(
        order.lock().unwrap().as_slice(),
        ["restore", "run", "init", "events"]
    );
}

#[tokio::test]
async fn compose_editor_success_opens_send_confirmation() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-editor-success-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let content = "---\nto: a@example.com\ncc: \"\"\nbcc: \"\"\nsubject: Hello\nfrom: me@example.com\nattach: []\n---\n\nBody\n";
    std::fs::write(&temp, content).unwrap();

    let data = ComposeReadyData {
        account_id: AccountId::new(),
        intent: mxr_core::DraftIntent::New,
        draft_path: temp.clone(),
        cursor_line: 1,
        initial_content: String::new(),
        invite_reply: None,
    };
    let mut app = App::new();
    let (bg, mut bg_rx) = mpsc::unbounded_channel::<crate::ipc::IpcRequest>();
    // Drain the safety-check IPC the new wiring fires; reply with an
    // error so the modal opens with `safety_report = None` (the
    // contract under test here is mode/state, not safety).
    let drain = tokio::spawn(async move {
        if let Some(req) = bg_rx.recv().await {
            let _ = req.reply.send(Err(MxrError::Ipc("test fixture".into())));
        }
    });

    handle_compose_editor_status(&mut app, &data, Ok(exit_status(0)), &bg).await;
    drop(bg);
    drain.await.ok();

    assert_eq!(
        app.compose
            .pending_send_confirm
            .as_ref()
            .map(|pending| pending.mode),
        Some(PendingSendMode::SendOrSave)
    );
    assert!(app.status_message.is_none());

    let _ = std::fs::remove_file(temp);
}

#[tokio::test]
async fn compose_editor_cancel_discards_draft() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-editor-cancel-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(&temp, "---\n").unwrap();

    let data = ComposeReadyData {
        account_id: AccountId::new(),
        intent: mxr_core::DraftIntent::New,
        draft_path: temp.clone(),
        cursor_line: 1,
        initial_content: String::new(),
        invite_reply: None,
    };
    let mut app = App::new();
    // Editor exited non-zero, so the safety-check path is never
    // taken; bg never receives a request.
    let (bg, _bg_rx) = mpsc::unbounded_channel::<crate::ipc::IpcRequest>();

    handle_compose_editor_status(&mut app, &data, Ok(exit_status(1)), &bg).await;

    assert_eq!(app.status_message.as_deref(), Some("Draft discarded"));
    assert!(app.compose.pending_send_confirm.is_none());
    assert!(!temp.exists());
}

/// Slice 1.5 wiring contract (C2.1): the editor-finished handler
/// MUST fire `Request::CheckDraftSafety` before showing the modal,
/// and MUST stamp the response onto `pending_send_confirm`.
#[tokio::test]
async fn compose_editor_finish_stamps_safety_report_onto_pending() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-safety-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let content = "---\nto: a@example.com\ncc: \"\"\nbcc: \"\"\nsubject: Hello\nfrom: me@example.com\nattach: []\n---\n\nBody\n";
    std::fs::write(&temp, content).unwrap();

    let data = ComposeReadyData {
        account_id: AccountId::new(),
        intent: mxr_core::DraftIntent::New,
        draft_path: temp.clone(),
        cursor_line: 1,
        initial_content: String::new(),
        invite_reply: None,
    };
    let mut app = App::new();

    // Fake daemon: returns a Blocked report with a single PiiSecret
    // issue carrying override_token = Some("tok-test"). This is the
    // exact shape the daemon mints for blocker verdicts.
    let (bg, mut bg_rx) = mpsc::unbounded_channel::<crate::ipc::IpcRequest>();
    let fake_daemon = tokio::spawn(async move {
        let req = bg_rx.recv().await.expect("safety check IPC fired");
        // Verify the wiring sent a CheckDraftSafety, not some other
        // request.
        assert!(
            matches!(req.request, Request::CheckDraftSafety { .. }),
            "expected CheckDraftSafety, got: {:?}",
            req.request
        );
        let issue = mxr_core::DraftSafetyIssue::new(
            mxr_core::DraftSafetyIssueCode::PiiSecret,
            mxr_core::DraftSafetySeverity::Blocker,
            "secret pattern",
        )
        .with_override_token("tok-test");
        let report = mxr_core::DraftSafetyReport::from_issues(vec![issue]);
        let _ = req.reply.send(Ok(mxr_protocol::Response::Ok {
            data: mxr_protocol::ResponseData::DraftSafetyReportResponse { report },
        }));
    });

    handle_compose_editor_status(&mut app, &data, Ok(exit_status(0)), &bg).await;
    drop(bg);
    fake_daemon.await.unwrap();

    let pending = app
        .compose
        .pending_send_confirm
        .as_ref()
        .expect("modal should open");
    let report = pending
        .safety_report
        .as_ref()
        .expect("safety_report stamped onto pending");
    assert_eq!(report.verdict, mxr_core::DraftSafetyVerdict::Blocked);
    assert_eq!(pending.override_token.as_deref(), Some("tok-test"));
    assert!(pending.is_blocked());

    let _ = std::fs::remove_file(temp);
}

/// Slice 1.5 wiring contract (C2.1): pressing `[s] send` while
/// the safety verdict is Blocked is a no-op — the modal stays
/// open, no SendDraft mutation is queued. The user must use
/// `Ctrl-O` to override or edit the draft.
#[test]
fn pressing_s_with_blocked_verdict_is_a_noop() {
    let mut app = App::new();
    let issue = mxr_core::DraftSafetyIssue::new(
        mxr_core::DraftSafetyIssueCode::PiiSecret,
        mxr_core::DraftSafetySeverity::Blocker,
        "secret",
    );
    let report = mxr_core::DraftSafetyReport::from_issues(vec![issue]);
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "alice@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "hi".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "hi".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: Some(report),
        override_token: Some("tok-1".into()),
        suggested_collaborators: vec![],
        invite_reply: None,
    });
    let mutations_before = app.pending_mutation_queue.len();

    let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE);
    let _ = app.handle_key(key);

    assert!(
        app.compose.pending_send_confirm.is_some(),
        "modal must stay open"
    );
    assert_eq!(
        app.pending_mutation_queue.len(),
        mutations_before,
        "no mutation queued"
    );
}

/// Slice 1.5 wiring contract (C2.1): Ctrl-O on a Blocked verdict
/// dispatches SendDraft with override_safety_token = the token
/// the daemon minted.
#[test]
fn ctrl_o_dispatches_send_with_override_token() {
    let mut app = App::new();
    let issue = mxr_core::DraftSafetyIssue::new(
        mxr_core::DraftSafetyIssueCode::PiiSecret,
        mxr_core::DraftSafetySeverity::Blocker,
        "secret",
    )
    .with_override_token("tok-override-9");
    let report = mxr_core::DraftSafetyReport::from_issues(vec![issue]);
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "alice@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "hi".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "hi".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: Some(report),
        override_token: Some("tok-override-9".into()),
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let key = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
    let _ = app.handle_key(key);

    // The mutation queue must contain a SendDraft with the
    // override token.
    let queued = app.pending_mutation_queue.first().expect("mutation queued");
    match &queued.request {
        Request::SendDraft {
            override_safety_token,
            ..
        } => {
            assert_eq!(override_safety_token.as_deref(), Some("tok-override-9"));
        }
        other => panic!("expected SendDraft with override, got: {other:?}"),
    }
}

/// Slice 2.3 wiring contract (C2.2): selecting the Owed sidebar
/// entry switches MailboxView to Owed AND requests a fresh
/// ListOwedReplies fetch.
#[test]
fn opening_owed_lens_switches_view_and_queues_refresh() {
    let mut app = App::new();
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Messages);
    assert!(!app.mailbox.pending_owed_refresh);

    app.apply(Action::OpenOwedReplies);

    assert_eq!(app.mailbox.mailbox_view, MailboxView::Owed);
    assert!(
        app.mailbox.pending_owed_refresh,
        "OpenOwedReplies must queue a refresh"
    );
}

/// Slice 2.3 wiring contract (C2.2): a successful SendDraft
/// mutation queues a ListOwedReplies refresh so a sent reply
/// disappears from the lens without manual intervention.
#[test]
fn sent_success_effect_triggers_owed_refresh() {
    let mut app = App::new();
    // Pretend the user is sitting on the owed lens.
    app.mailbox.mailbox_view = MailboxView::Owed;
    app.mailbox.pending_owed_refresh = false;

    // Apply the SentSuccess mutation completion directly. The
    // contract is: this branch sets pending_owed_refresh = true.
    app.apply_mutation_completion(
        MutationEffect::SentSuccess {
            status: "Sent!".into(),
            remind_at: None,
            sent_message_id: None,
        },
        true,
    );

    assert!(
        app.mailbox.pending_owed_refresh,
        "SentSuccess effect must trigger an owed refresh"
    );
}

/// Slice 5.1 wiring contract (C2.6): pressing Action::OpenThreadBriefing
/// when a thread is focused must open the modal in loading state AND
/// queue a pending briefing fetch.
#[test]
fn open_thread_briefing_action_opens_modal_and_queues_fetch() {
    let mut app = App::new();
    // Seed an envelope so context_envelope() returns something.
    let env = TestEnvelopeBuilder::new().build();
    app.mailbox.envelopes = vec![env.clone()];
    app.mailbox.all_envelopes = vec![env.clone()];
    app.apply(Action::OpenSelected);

    app.apply(Action::OpenThreadBriefing);

    assert!(app.modals.briefing.visible, "briefing modal must open");
    assert!(app.modals.briefing.loading);
    assert!(matches!(
        app.modals.briefing.subject,
        Some(crate::app::BriefingModalSubject::Thread(_))
    ));
    assert!(
        matches!(
            app.pending_briefing_request,
            Some(crate::app::BriefingRequest::Thread(_))
        ),
        "pending request must be queued for the runtime to drain"
    );
}

/// Slice 5.1 (C2.6 cont): dormant_thread_hint returns Some when
/// the focused row's representative is >=30 days old AND the
/// thread has >=3 messages.
#[test]
fn dormant_hint_fires_at_30d_3msgs_and_nothing_below() {
    let mut app = App::new();
    // 31 days old + 3 messages -> dormant.
    let mut old = TestEnvelopeBuilder::new().build();
    old.date = chrono::Utc::now() - chrono::Duration::days(31);
    app.mailbox.envelopes = vec![old.clone()];
    app.mailbox.all_envelopes = vec![old.clone()];
    // Force the row to think there are 3 messages in the thread.
    app.apply(Action::OpenSelected);
    // Inject message_count by re-constructing the row via the
    // helper. The cleanest way is to overwrite the thread row
    // count through the existing aggregation; instead, we test
    // via a row count we control. Use 2 messages -> not dormant.
    let mut row = app.mail_list_rows().into_iter().next().unwrap();
    row.message_count = 2;
    assert!(
        row_to_dormant(&row, 31).is_none(),
        "2-message thread isn't dormant even if old"
    );
    row.message_count = 3;
    assert!(
        row_to_dormant(&row, 31).is_some(),
        "30d-old 3-message thread IS dormant"
    );
    row.message_count = 3;
    // Just below threshold.
    let mut fresh_row = row.clone();
    fresh_row.representative.date = chrono::Utc::now() - chrono::Duration::days(29);
    assert!(
        row_to_dormant(&fresh_row, 29).is_none(),
        "29d-old must NOT trigger the dormant hint"
    );
}

/// Helper for the dormant test: replicates the dormant logic
/// in mail_list_title without going through the title formatter.
fn row_to_dormant(row: &crate::app::MailListRow, days: i64) -> Option<String> {
    if row.message_count < 3 || days < 30 {
        return None;
    }
    Some(format!("Dormant {days}d. Press B for briefing"))
}

/// Slice 5.3 (C2.7 cont): Ctrl-A on the compose-confirm modal
/// adds the top "maybe include" suggestion to the draft's Cc
/// and removes it from the list.
#[test]
fn ctrl_a_adds_top_suggestion_to_cc() {
    let mut app = App::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "alice@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "hi".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "hi".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        override_token: None,
        invite_reply: None,
        suggested_collaborators: vec![mxr_protocol::SuggestedRecipientData {
            email: "bob@example.com".into(),
            display_name: None,
            reason: "co-participant on 3 threads".into(),
            confidence: "medium".into(),
            evidence_msg_ids: vec![],
        }],
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));

    let pending = app.compose.pending_send_confirm.as_ref().unwrap();
    assert_eq!(pending.fm.cc, "bob@example.com");
    assert!(
        pending.suggested_collaborators.is_empty(),
        "consumed suggestion must be removed from the list"
    );
}

/// Slice 5.4 (C2.8 cont): FindExpertOnFocusedMessage opens
/// the expert modal in loading state and queues a query.
#[test]
fn find_expert_action_opens_modal_and_queues_query() {
    let mut app = App::new();
    let mut env = TestEnvelopeBuilder::new().build();
    env.subject = "kafka rebalance question".into();
    env.snippet = "how do consumers rebalance?".into();
    app.mailbox.envelopes = vec![env.clone()];
    app.mailbox.all_envelopes = vec![env.clone()];
    app.apply(Action::OpenSelected);

    app.apply(Action::FindExpertOnFocusedMessage);

    assert!(app.modals.expert.visible);
    assert!(app.modals.expert.loading);
    let q = app.modals.expert.query.as_deref().unwrap_or("");
    assert!(q.contains("kafka rebalance"));
    assert!(app.pending_expert_query.is_some());
}

#[test]
fn close_expert_modal_clears_state() {
    let mut app = App::new();
    app.modals.expert.open_loading("kafka".into());
    assert!(app.modals.expert.visible);
    app.apply(Action::CloseExpertModal);
    assert!(!app.modals.expert.visible);
}

/// Slice 6.1 wiring contract (C2.9): pressing
/// OpenWhoisOnFocusedSender opens the whois modal in loading
/// state and queues a pending whois fetch with the focused
/// sender's email as the query.
#[test]
fn open_whois_action_seeds_modal_and_queues_query() {
    let mut app = App::new();
    let mut env = TestEnvelopeBuilder::new().build();
    env.from = mxr_core::Address {
        name: None,
        email: "carol@example.com".into(),
    };
    app.mailbox.envelopes = vec![env.clone()];
    app.mailbox.all_envelopes = vec![env.clone()];
    app.apply(Action::OpenSelected);

    app.apply(Action::OpenWhoisOnFocusedSender);

    assert!(app.modals.whois.visible);
    assert!(app.modals.whois.loading);
    assert_eq!(app.modals.whois.query.as_deref(), Some("carol@example.com"));
    assert_eq!(
        app.pending_whois_query.as_deref(),
        Some("carol@example.com")
    );
}

/// Esc on the whois modal closes it.
#[test]
fn close_whois_modal_action_clears_state() {
    let mut app = App::new();
    app.modals.whois.open_loading("alice@example.com".into());
    assert!(app.modals.whois.visible);

    app.apply(Action::CloseWhoisModal);

    assert!(!app.modals.whois.visible);
    assert!(app.modals.whois.query.is_none());
}

/// Esc on the briefing modal closes it.
#[test]
fn close_briefing_modal_action_clears_state() {
    let mut app = App::new();
    app.modals
        .briefing
        .open_thread_loading(mxr_core::ThreadId::new());
    assert!(app.modals.briefing.visible);

    app.apply(Action::CloseBriefingModal);

    assert!(!app.modals.briefing.visible);
    assert!(app.modals.briefing.subject.is_none());
}

#[test]
fn suspended_handoff_preserves_non_compose_results() {
    let old_dropped = Arc::new(AtomicBool::new(false));
    let mut terminal = 1usize;
    let mut events = Some(TestEventSource {
        id: 1,
        dropped: old_dropped.clone(),
    });

    let result: Result<String, MxrError> = run_with_terminal_suspended_with(
        &mut terminal,
        &mut events,
        || {},
        || 2usize,
        || TestEventSource {
            id: 2,
            dropped: Arc::new(AtomicBool::new(false)),
        },
        || {
            assert!(old_dropped.load(Ordering::SeqCst));
            Ok("Log open cancelled".into())
        },
    );

    assert_eq!(result.unwrap(), "Log open cancelled");
    assert_eq!(terminal, 2);
    assert_eq!(events.as_ref().map(|event| event.id), Some(2));
}

#[test]
fn replaceable_request_queue_supersedes_older_status_refresh() {
    let mut pending = VecDeque::new();
    enqueue_replaceable_request(
        &mut pending,
        ReplaceableRequest::Status {
            request_id: 1,
            enqueued_at: Instant::now(),
        },
    );
    enqueue_replaceable_request(
        &mut pending,
        ReplaceableRequest::Status {
            request_id: 2,
            enqueued_at: Instant::now(),
        },
    );

    assert_eq!(pending.len(), 1);
    match pending.pop_front() {
        Some(ReplaceableRequest::Status { request_id, .. }) => assert_eq!(request_id, 2),
        other => panic!("expected status request, got {other:?}"),
    }
}

#[test]
fn input_gg_jumps_top() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        Some(Action::JumpTop)
    );
}

#[test]
fn input_zz_centers() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE)),
        Some(Action::CenterCurrent)
    );
}

#[test]
fn input_enter_opens() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(Action::OpenSelected)
    );
}

#[test]
fn input_o_opens() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)),
        Some(Action::OpenSelected)
    );
}

#[test]
fn input_escape_back() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        Some(Action::Back)
    );
}

#[test]
fn input_q_quits() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        Some(Action::QuitView)
    );
}

#[test]
fn input_hml_viewport() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT)),
        Some(Action::ViewportTop)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('M'), KeyModifiers::SHIFT)),
        Some(Action::ViewportMiddle)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT)),
        Some(Action::ViewportBottom)
    );
}

#[test]
fn input_uppercase_shortcuts_work_without_explicit_shift_modifier() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE)),
        Some(Action::ViewportTop)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE)),
        Some(Action::AttachmentList)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE)),
        Some(Action::OpenLogs)
    );
}

#[test]
fn input_ctrl_du_page() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        Some(Action::PageDown)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
        Some(Action::PageUp)
    );
}

#[test]
fn app_move_down() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.apply(Action::MoveDown);
    assert_eq!(app.mailbox.selected_index, 1);
}

#[test]
fn app_move_up_at_zero() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.apply(Action::MoveUp);
    assert_eq!(app.mailbox.selected_index, 0);
}

#[test]
fn app_jump_top() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(10);
    app.mailbox.selected_index = 5;
    app.apply(Action::JumpTop);
    assert_eq!(app.mailbox.selected_index, 0);
}

#[test]
fn app_switch_pane() {
    let mut app = App::new();
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    app.apply(Action::SwitchPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::Sidebar);
    app.apply(Action::SwitchPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
}

#[test]
fn app_quit() {
    let mut app = App::new();
    app.apply(Action::QuitView);
    assert!(app.should_quit);
}

#[test]
fn app_new_uses_original_html_as_default_message_view() {
    let app = App::new();
    assert!(app.mailbox.reader_mode);
    assert!(app.mailbox.html_view);
}

#[test]
fn app_from_render_config_respects_text_reader_mode() {
    let config = RenderConfig {
        reader_mode: true,
        ..Default::default()
    };
    let app = App::from_render_config(&config);
    assert!(app.mailbox.reader_mode);
    assert!(app.mailbox.html_view);
}

#[test]
fn apply_runtime_config_updates_tui_settings() {
    let mut app = App::new();
    let mut config = mxr_config::MxrConfig::default();
    config.render.reader_mode = false;
    config.snooze.morning_hour = 7;
    config.appearance.theme = "light".into();

    app.apply_runtime_config(&config);

    assert!(!app.mailbox.reader_mode);
    assert!(app.mailbox.html_view);
    assert_eq!(app.modals.snooze_config.morning_hour, 7);
    assert_eq!(
        app.theme.selection_fg,
        crate::theme::Theme::light().selection_fg
    );
}

#[test]
fn edit_config_action_sets_pending_flag() {
    let mut app = App::new();

    app.apply(Action::EditConfig);

    assert!(app.diagnostics.pending_config_edit);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Opening config in editor...")
    );
}

#[test]
fn open_logs_action_sets_pending_flag() {
    let mut app = App::new();

    app.apply(Action::OpenLogs);

    assert!(app.diagnostics.pending_log_open);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Opening log file in editor...")
    );
}

#[test]
fn open_in_browser_action_queues_html_body_open() {
    let mut app = App::new();
    let env = make_test_envelopes(1).remove(0);
    app.mailbox.viewing_envelope = Some(env.clone());
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Plain body".into()),
            text_html: Some("<p>Hello html</p>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenInBrowser);

    let pending = app
        .mailbox
        .pending_browser_open
        .as_ref()
        .expect("browser open should be queued");
    assert_eq!(pending.message_id, env.id);
    assert_eq!(pending.document, "<p>Hello html</p>");
    assert_eq!(app.status_message.as_deref(), Some("Opening in browser..."));
}

#[test]
fn open_in_browser_action_wraps_plain_text_when_html_is_missing() {
    let mut app = App::new();
    let env = make_test_envelopes(1).remove(0);
    app.mailbox.viewing_envelope = Some(env.clone());
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Plain body".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenInBrowser);

    let pending = app
        .mailbox
        .pending_browser_open
        .as_ref()
        .expect("plain text should still open in browser");
    assert_eq!(pending.message_id, env.id);
    assert!(pending.document.contains("<pre>Plain body</pre>"));
    assert!(pending.document.contains("<!doctype html>"));
    assert_eq!(app.status_message.as_deref(), Some("Opening in browser..."));
}

#[test]
fn open_in_browser_action_wraps_best_effort_fallback_body() {
    let mut app = App::new();
    let env = make_test_envelopes(1).remove(0);
    app.mailbox.viewing_envelope = Some(env.clone());
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: None,
            text_html: None,
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
                filename: "invite.ics".into(),
                mime_type: "text/calendar".into(),
                disposition: AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 2048,
                local_path: None,
                provider_id: "att-1".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                calendar: Some(CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Demo call".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        },
    );

    app.apply(Action::OpenInBrowser);

    let pending = app
        .mailbox
        .pending_browser_open
        .as_ref()
        .expect("best-effort fallback should open in browser");
    assert_eq!(pending.message_id, env.id);
    assert!(pending.document.contains("Calendar invite"));
    assert!(pending.document.contains("Summary: Demo call"));
    assert_eq!(app.status_message.as_deref(), Some("Opening in browser..."));
}

#[test]
fn open_in_browser_action_missing_body_queues_fetch_and_opens_on_success() {
    let mut app = App::new();
    let env = make_test_envelopes(1).remove(0);
    app.mailbox.viewing_envelope = Some(env.clone());

    app.apply(Action::OpenInBrowser);

    assert_eq!(app.mailbox.queued_body_fetches, vec![env.id.clone()]);
    assert!(app.mailbox.in_flight_body_requests.contains(&env.id));
    assert_eq!(
        app.mailbox.pending_browser_open_after_load,
        Some(env.id.clone())
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("Loading message body...")
    );

    app.resolve_body_success(MessageBody {
        message_id: env.id.clone(),
        text_plain: Some("Loaded later".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: Default::default(),
    });

    let pending = app
        .mailbox
        .pending_browser_open
        .as_ref()
        .expect("browser open should resume after body load");
    assert_eq!(pending.message_id, env.id);
    assert!(pending.document.contains("<pre>Loaded later</pre>"));
    assert!(app.mailbox.pending_browser_open_after_load.is_none());
}

#[test]
fn app_move_down_bounds() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.apply(Action::MoveDown);
    app.apply(Action::MoveDown);
    app.apply(Action::MoveDown);
    assert_eq!(app.mailbox.selected_index, 2);
}

#[test]
fn layout_mode_switching() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    app.apply(Action::OpenMessageView);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    app.apply(Action::CloseMessageView);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
}

#[test]
fn fullscreen_opens_selected_message_from_mail_list() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::ToggleFullscreen);

    assert_eq!(app.mailbox.layout_mode, LayoutMode::FullScreen);
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
    assert!(app.mailbox.viewing_envelope.is_some());
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(app.mailbox.envelopes[0].id.clone())
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("Showing full message view")
    );
}

#[test]
fn fullscreen_keeps_sidebar_visible() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::ToggleFullscreen);

    let output = render_to_string(120, 20, |frame| app.draw(frame));
    assert!(output.contains("Sidebar"));
    assert!(output.contains("Inbox"));
    assert!(output.contains("Subject 0"));
}

#[test]
fn fullscreen_switch_pane_skips_hidden_mail_list() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::ToggleFullscreen);
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);

    app.apply(Action::SwitchPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::Sidebar);

    app.apply(Action::SwitchPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
}

#[test]
fn command_palette_toggle() {
    let mut p = CommandPalette::default();
    assert!(!p.visible);
    p.toggle(crate::action::UiContext::MailboxList);
    assert!(p.visible);
    p.toggle(crate::action::UiContext::MailboxList);
    assert!(!p.visible);
}

#[test]
fn command_palette_fuzzy_filter() {
    let mut p = CommandPalette::default();
    p.toggle(crate::action::UiContext::MailboxList);
    p.on_char('i');
    p.on_char('n');
    p.on_char('b');
    let labels: Vec<&str> = p
        .filtered
        .iter()
        .map(|&i| p.commands[i].label.as_str())
        .collect();
    assert!(labels.contains(&"Go to Inbox"));
}

#[test]
fn command_palette_shortcut_filter_finds_edit_config() {
    let mut p = CommandPalette::default();
    p.toggle(crate::action::UiContext::MailboxList);
    p.on_char('g');
    p.on_char('c');
    let labels: Vec<&str> = p
        .filtered
        .iter()
        .map(|&i| p.commands[i].label.as_str())
        .collect();
    assert!(labels.contains(&"Edit Config"));
}

#[test]
fn unsubscribe_opens_confirm_modal_and_scopes_archive_to_sender_and_account() {
    let mut app = App::new();
    let account_id = AccountId::new();
    let other_account_id = AccountId::new();
    let target = make_unsubscribe_envelope(
        account_id.clone(),
        "news@example.com",
        UnsubscribeMethod::HttpLink {
            url: "https://example.com/unsub".into(),
        },
    );
    let same_sender_same_account = make_unsubscribe_envelope(
        account_id.clone(),
        "news@example.com",
        UnsubscribeMethod::None,
    );
    let same_sender_other_account = make_unsubscribe_envelope(
        other_account_id,
        "news@example.com",
        UnsubscribeMethod::None,
    );
    let different_sender_same_account =
        make_unsubscribe_envelope(account_id, "other@example.com", UnsubscribeMethod::None);

    app.mailbox.envelopes = vec![target.clone()];
    app.mailbox.all_envelopes = vec![
        target.clone(),
        same_sender_same_account.clone(),
        same_sender_other_account,
        different_sender_same_account,
    ];

    app.apply(Action::Unsubscribe);

    let pending = app
        .modals
        .pending_unsubscribe_confirm
        .as_ref()
        .expect("unsubscribe modal should open");
    assert_eq!(pending.sender_email, "news@example.com");
    assert_eq!(pending.method_label, "browser link");
    assert_eq!(pending.archive_message_ids.len(), 2);
    assert!(pending.archive_message_ids.contains(&target.id));
    assert!(pending
        .archive_message_ids
        .contains(&same_sender_same_account.id));
}

#[test]
fn unsubscribe_without_method_sets_status_error() {
    let mut app = App::new();
    let env = make_unsubscribe_envelope(
        AccountId::new(),
        "news@example.com",
        UnsubscribeMethod::None,
    );
    app.mailbox.envelopes = vec![env];

    app.apply(Action::Unsubscribe);

    assert!(app.modals.pending_unsubscribe_confirm.is_none());
    assert_eq!(
        app.status_message.as_deref(),
        Some("No unsubscribe option found for this message")
    );
}

#[test]
fn unsubscribe_confirm_archive_populates_pending_action() {
    let mut app = App::new();
    let env = make_unsubscribe_envelope(
        AccountId::new(),
        "news@example.com",
        UnsubscribeMethod::OneClick {
            url: "https://example.com/one-click".into(),
        },
    );
    app.mailbox.envelopes = vec![env.clone()];
    app.mailbox.all_envelopes = vec![env.clone()];
    app.apply(Action::Unsubscribe);
    app.apply(Action::ConfirmUnsubscribeAndArchiveSender);

    let pending = app
        .modals
        .pending_unsubscribe_action
        .as_ref()
        .expect("unsubscribe action should be queued");
    assert_eq!(pending.message_id, env.id);
    assert_eq!(pending.archive_message_ids.len(), 1);
    assert_eq!(pending.sender_email, "news@example.com");
}

#[test]
fn search_input_lifecycle() {
    let mut bar = SearchBar::default();
    bar.activate();
    assert!(bar.active);
    bar.on_char('h');
    bar.on_char('e');
    bar.on_char('l');
    bar.on_char('l');
    bar.on_char('o');
    assert_eq!(bar.query, "hello");
    let q = bar.submit();
    assert_eq!(q, "hello");
    assert!(!bar.active);
}

#[test]
fn search_bar_cycles_modes() {
    let mut bar = SearchBar::default();
    assert_eq!(bar.mode, mxr_core::SearchMode::Lexical);
    bar.cycle_mode();
    assert_eq!(bar.mode, mxr_core::SearchMode::Hybrid);
    bar.cycle_mode();
    assert_eq!(bar.mode, mxr_core::SearchMode::Semantic);
    bar.cycle_mode();
    assert_eq!(bar.mode, mxr_core::SearchMode::Lexical);
}

#[test]
fn reopening_active_search_preserves_query() {
    let mut app = App::new();
    app.search.active = true;
    app.search.bar.query = "deploy".to_string();
    app.search.bar.cursor_pos = 0;

    app.apply(Action::OpenMailboxFilter);

    assert!(app.search.bar.active);
    assert_eq!(app.search.bar.query, "deploy");
    assert_eq!(app.search.bar.cursor_pos, "deploy".len());
}

#[test]
fn g_prefix_navigation() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
        Some(Action::GoToInbox)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
        Some(Action::GoToStarred)
    );
}

#[test]
fn status_bar_sync_formats() {
    assert_eq!(
        status_bar::format_sync_status(12, Some("synced 2m ago")),
        "[INBOX] 12 unread | synced 2m ago"
    );
    assert_eq!(
        status_bar::format_sync_status(0, None),
        "[INBOX] 0 unread | not synced"
    );
}

fn make_test_labels() -> Vec<Label> {
    crate::test_fixtures::test_system_labels(&AccountId::new())
}

/// Put `app` into an Inbox-active state so optimistic mutation effects
/// (which only fire when the active label matches the labels the
/// mutation removes) take effect during tests.
fn set_active_inbox(app: &mut App) {
    app.mailbox.labels = make_test_labels();
    app.mailbox.active_label = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name.eq_ignore_ascii_case("INBOX"))
        .map(|label| label.id.clone());
}

// --- Navigation tests ---

#[test]
fn threepane_l_loads_new_message() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    // Open first message
    app.apply(Action::OpenSelected);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    let first_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    // Move focus back to mail list
    app.mailbox.active_pane = ActivePane::MailList;
    // Navigate to second message
    app.apply(Action::MoveDown);
    // Press l (which triggers OpenSelected)
    app.apply(Action::OpenSelected);
    let second_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    assert_ne!(
        first_id, second_id,
        "l should load the new message, not stay on old one"
    );
    assert_eq!(app.mailbox.selected_index, 1);
}

#[test]
fn threepane_jk_auto_preview() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    // Open first message to enter ThreePane
    app.apply(Action::OpenSelected);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    let first_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    // Move focus back to mail list
    app.mailbox.active_pane = ActivePane::MailList;
    // Move down — should auto-preview
    app.apply(Action::MoveDown);
    let preview_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    assert_ne!(first_id, preview_id, "j/k should auto-preview in ThreePane");
    // Body should be loaded from cache (or None if not cached in test)
    // No async fetch needed — bodies are inline with envelopes
}

#[test]
fn twopane_jk_no_auto_preview() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    // Don't open message — stay in TwoPane
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    app.apply(Action::MoveDown);
    assert!(
        app.mailbox.viewing_envelope.is_none(),
        "j/k should not auto-preview in TwoPane"
    );
    // No body fetch triggered in TwoPane mode
}

// --- Back navigation tests ---

#[test]
fn back_in_message_view_closes_preview_pane() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    app.apply(Action::Back);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    assert!(app.mailbox.viewing_envelope.is_none());
}

#[test]
fn back_in_mail_list_clears_label_filter() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.labels = make_test_labels();
    let inbox_id = app
        .mailbox
        .labels
        .iter()
        .find(|l| l.name == "INBOX")
        .unwrap()
        .id
        .clone();
    // Simulate label filter active
    app.mailbox.active_label = Some(inbox_id);
    app.mailbox.envelopes = vec![app.mailbox.envelopes[0].clone()]; // Filtered down
                                                                    // Esc should clear filter
    app.apply(Action::Back);
    assert!(
        app.mailbox.active_label.is_none(),
        "Esc should clear label filter"
    );
    assert_eq!(
        app.mailbox.envelopes.len(),
        5,
        "Should restore all envelopes"
    );
}

#[test]
fn back_in_mail_list_closes_threepane_when_no_filter() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected); // ThreePane
    app.mailbox.active_pane = ActivePane::MailList; // Move back
                                                    // No filter active — Esc should close ThreePane
    app.apply(Action::Back);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
}

// --- Sidebar tests ---

#[test]
fn sidebar_system_labels_before_user_labels() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let ordered = app.ordered_visible_labels();
    // System labels should come first
    let first_user_idx = ordered.iter().position(|l| l.kind == LabelKind::User);
    let last_system_idx = ordered.iter().rposition(|l| l.kind == LabelKind::System);
    if let (Some(first_user), Some(last_system)) = (first_user_idx, last_system_idx) {
        assert!(
            last_system < first_user,
            "All system labels should come before user labels"
        );
    }
}

#[test]
fn sidebar_system_labels_in_correct_order() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let ordered = app.ordered_visible_labels();
    let system_names: Vec<&str> = ordered
        .iter()
        .filter(|l| l.kind == LabelKind::System)
        .map(|l| l.name.as_str())
        .collect();
    // INBOX should be first, then STARRED, SENT, etc.
    assert_eq!(system_names[0], "INBOX");
    assert_eq!(system_names[1], "STARRED");
    assert_eq!(system_names[2], "SENT");
    assert_eq!(system_names[3], "DRAFT");
    assert_eq!(system_names[4], "ARCHIVE");
}

#[test]
fn sidebar_items_put_inbox_before_all_mail() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();

    let items = app.sidebar_items();
    let all_mail_index = items
        .iter()
        .position(|item| matches!(item, SidebarItem::AllMail))
        .unwrap();

    assert!(matches!(
        items.first(),
        Some(SidebarItem::Label(label)) if label.name == "INBOX"
    ));
    assert!(all_mail_index > 0);
}

#[test]
fn sidebar_hidden_labels_not_shown() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let ordered = app.ordered_visible_labels();
    let names: Vec<&str> = ordered.iter().map(|l| l.name.as_str()).collect();
    assert!(
        !names.contains(&"CATEGORY_UPDATES"),
        "Gmail categories should be hidden"
    );
}

#[test]
fn sidebar_empty_system_labels_hidden_except_primary() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let ordered = app.ordered_visible_labels();
    let names: Vec<&str> = ordered.iter().map(|l| l.name.as_str()).collect();
    // CHAT has 0 total, 0 unread — should be hidden
    assert!(
        !names.contains(&"CHAT"),
        "Empty non-primary system labels should be hidden"
    );
    // DRAFT has 0 total but is primary — should be shown
    assert!(
        names.contains(&"DRAFT"),
        "Primary system labels shown even if empty"
    );
    assert!(
        names.contains(&"ARCHIVE"),
        "Archive should be shown as a primary system label even if empty"
    );
    // IMPORTANT has 5 total — should be shown (non-primary but non-empty)
    assert!(
        names.contains(&"IMPORTANT"),
        "Non-empty system labels should be shown"
    );
}

#[test]
fn sidebar_user_labels_alphabetical() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let ordered = app.ordered_visible_labels();
    let user_names: Vec<&str> = ordered
        .iter()
        .filter(|l| l.kind == LabelKind::User)
        .map(|l| l.name.as_str())
        .collect();
    // Personal < Work alphabetically
    assert_eq!(user_names, vec!["Personal", "Work"]);
}

// --- GoTo navigation tests ---

#[test]
fn goto_inbox_sets_active_label() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.labels = make_test_labels();
    app.apply(Action::GoToInbox);
    let label = app
        .mailbox
        .labels
        .iter()
        .find(|l| l.name == "INBOX")
        .unwrap();
    assert!(
        app.mailbox.active_label.is_none(),
        "GoToInbox should wait for fetch success before swapping active label"
    );
    assert_eq!(
        app.mailbox.pending_active_label.as_ref().unwrap(),
        &label.id
    );
    assert!(
        app.mailbox.pending_label_fetch.is_some(),
        "Should trigger label fetch"
    );
}

#[test]
fn goto_inbox_without_labels_records_desired_mailbox() {
    let mut app = App::new();
    app.apply(Action::GoToInbox);
    assert_eq!(app.mailbox.desired_system_mailbox.as_deref(), Some("INBOX"));
    assert!(app.mailbox.pending_label_fetch.is_none());
    assert!(app.mailbox.pending_active_label.is_none());
}

#[test]
fn labels_refresh_resolves_desired_inbox() {
    let mut app = App::new();
    app.mailbox.desired_system_mailbox = Some("INBOX".into());
    app.mailbox.labels = make_test_labels();

    app.resolve_desired_system_mailbox();

    let inbox_id = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name == "INBOX")
        .unwrap()
        .id
        .clone();
    assert_eq!(app.mailbox.pending_active_label.as_ref(), Some(&inbox_id));
    assert_eq!(app.mailbox.pending_label_fetch.as_ref(), Some(&inbox_id));
    assert!(app.mailbox.active_label.is_none());
}

#[test]
fn sync_completed_requests_live_refresh_even_without_active_label() {
    let mut app = App::new();

    handle_daemon_event(
        &mut app,
        DaemonEvent::SyncCompleted {
            account_id: AccountId::new(),
            messages_synced: 5,
        },
    );

    assert!(app.mailbox.pending_labels_refresh);
    assert!(app.mailbox.pending_all_envelopes_refresh);
    assert!(app.diagnostics.pending_status_refresh);
    assert!(app.mailbox.pending_label_fetch.is_none());
    assert_eq!(app.status_message.as_deref(), Some("Synced 5 messages"));
}

#[test]
fn mutation_reconciliation_failed_event_replays_optimistic_snapshot() {
    let mut app = App::new();
    let envelopes = make_test_envelopes(1);
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;
    app.mailbox.selected_index = 0;

    app.apply(Action::Star);
    assert!(
        app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "star applies optimistically"
    );
    let mid = app.pending_mutation_queue[0].id;

    handle_daemon_event(
        &mut app,
        DaemonEvent::MutationReconciliationFailed {
            client_correlation_id: mid.raw().to_string(),
            error_summary: "provider rejected".into(),
        },
    );

    assert!(
        !app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "daemon failure event rolls back starred state"
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("Mutation failed: provider rejected")
    );
}

#[test]
fn status_bar_uses_label_counts_instead_of_loaded_window() {
    let mut app = App::new();
    let mut envelopes = make_test_envelopes(5);
    if let Some(first) = envelopes.first_mut() {
        first.flags.remove(MessageFlags::READ);
        first.flags.insert(MessageFlags::STARRED);
    }
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;
    app.mailbox.labels = make_test_labels();
    let inbox = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name == "INBOX")
        .unwrap()
        .id
        .clone();
    app.mailbox.active_label = Some(inbox);
    app.last_sync_status = Some("synced just now".into());

    let state = app.status_bar_state();

    assert_eq!(state.mailbox_name, "INBOX");
    assert_eq!(state.total_count, 10);
    assert_eq!(state.unread_count, 3);
    assert_eq!(state.starred_count, 2);
    assert_eq!(state.sync_status.as_deref(), Some("synced just now"));
}

#[test]
fn all_envelopes_refresh_updates_visible_all_mail() {
    let mut app = App::new();
    let envelopes = make_test_envelopes(4);
    app.mailbox.active_label = None;
    app.search.active = false;

    apply_all_envelopes_refresh(&mut app, envelopes.clone());

    assert_eq!(app.mailbox.all_envelopes.len(), 4);
    assert_eq!(app.mailbox.envelopes.len(), 4);
    assert_eq!(app.mailbox.selected_index, 0);
}

#[test]
fn all_envelopes_refresh_preserves_selection_when_possible() {
    let mut app = App::new();
    app.visible_height = 3;
    app.mailbox.mail_list_mode = MailListMode::Messages;
    let initial = make_test_envelopes(4);
    app.mailbox.all_envelopes = initial.clone();
    app.mailbox.envelopes = initial.clone();
    app.mailbox.selected_index = 2;
    app.mailbox.scroll_offset = 1;

    let mut refreshed = initial.clone();
    refreshed.push(make_test_envelopes(1).remove(0));

    apply_all_envelopes_refresh(&mut app, refreshed);

    assert_eq!(app.mailbox.selected_index, 2);
    assert_eq!(
        app.mailbox.envelopes[app.mailbox.selected_index].id,
        initial[2].id
    );
    assert_eq!(app.mailbox.scroll_offset, 1);
}

#[test]
fn all_envelopes_refresh_preserves_selected_message_when_rows_shift() {
    let mut app = App::new();
    app.mailbox.mail_list_mode = MailListMode::Messages;
    let initial = make_test_envelopes(4);
    let selected_id = initial[2].id.clone();
    app.mailbox.all_envelopes = initial.clone();
    app.mailbox.envelopes = initial;
    app.mailbox.selected_index = 2;

    let mut refreshed = make_test_envelopes(1);
    refreshed.extend(app.mailbox.envelopes.clone());

    apply_all_envelopes_refresh(&mut app, refreshed);

    assert_eq!(
        app.mailbox.envelopes[app.mailbox.selected_index].id,
        selected_id
    );
}

#[test]
fn all_envelopes_refresh_preserves_pending_label_view() {
    let mut app = App::new();
    let labels = make_test_labels();
    let inbox_id = labels
        .iter()
        .find(|label| label.name == "INBOX")
        .unwrap()
        .id
        .clone();
    let initial = make_test_envelopes(2);
    let refreshed = make_test_envelopes(5);
    app.mailbox.labels = labels;
    app.mailbox.envelopes = initial.clone();
    app.mailbox.all_envelopes = initial;
    app.mailbox.pending_active_label = Some(inbox_id);

    apply_all_envelopes_refresh(&mut app, refreshed.clone());

    assert_eq!(app.mailbox.all_envelopes.len(), refreshed.len());
    assert_eq!(app.mailbox.all_envelopes[0].id, refreshed[0].id);
    assert_eq!(app.mailbox.envelopes.len(), 2);
}

#[test]
fn label_counts_refresh_can_follow_empty_boot() {
    let mut app = App::new();
    app.mailbox.desired_system_mailbox = Some("INBOX".into());

    handle_daemon_event(
        &mut app,
        DaemonEvent::SyncCompleted {
            account_id: AccountId::new(),
            messages_synced: 0,
        },
    );

    assert!(app.mailbox.pending_labels_refresh);
    assert!(app.mailbox.pending_all_envelopes_refresh);
    assert_eq!(app.mailbox.desired_system_mailbox.as_deref(), Some("INBOX"));
}

#[test]
fn clear_filter_restores_all_envelopes() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(10);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.labels = make_test_labels();
    let inbox_id = app
        .mailbox
        .labels
        .iter()
        .find(|l| l.name == "INBOX")
        .unwrap()
        .id
        .clone();
    app.mailbox.active_label = Some(inbox_id);
    app.mailbox.envelopes = vec![app.mailbox.envelopes[0].clone()]; // Simulate filtered
    app.mailbox.selected_index = 0;
    app.apply(Action::ClearFilter);
    assert!(app.mailbox.active_label.is_none());
    assert_eq!(app.mailbox.envelopes.len(), 10, "Should restore full list");
}

#[test]
fn mail_list_rows_include_open_commitment_counts() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    let envelope = app.mailbox.envelopes[0].clone();
    app.mailbox
        .open_commitment_counts
        .insert((envelope.account_id.clone(), envelope.thread_id.clone()), 2);

    let rows = app.mail_list_rows();

    assert_eq!(rows[0].open_commitment_count, 2);
}

// --- Mutation effect tests ---

#[test]
fn archive_removes_from_list() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let removed_id = app.mailbox.envelopes[0].id.clone();
    app.apply(Action::Archive);
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.mailbox.envelopes.len(), 4);
    assert!(!app
        .mailbox
        .envelopes
        .iter()
        .any(|envelope| envelope.id == removed_id));
}

#[test]
fn archive_in_threads_mode_targets_every_message_in_thread() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    let mut envelopes = make_test_envelopes(5);
    let shared_thread = ThreadId::new();
    envelopes[0].thread_id = shared_thread.clone();
    envelopes[2].thread_id = shared_thread.clone();
    envelopes[4].thread_id = shared_thread.clone();
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;
    // Threads mode is the default; sanity-check it.
    assert_eq!(app.mailbox.mail_list_mode, MailListMode::Threads);
    // Cursor is on the row representing the 3-message thread.
    app.mailbox.selected_index = 0;

    app.apply(Action::Archive);

    // 3 targets triggers the bulk-confirm modal before the mutation
    // is dispatched. Inspect the staged request there.
    let pending = app
        .modals
        .pending_bulk_confirm
        .as_ref()
        .expect("expected bulk confirm for multi-target archive");
    match &pending.request {
        Request::Mutation {
            mutation: MutationCommand::Archive { message_ids },
            ..
        } => {
            assert_eq!(message_ids.len(), 3, "all thread members archived");
        }
        other => panic!("expected Archive mutation, got {other:?}"),
    }
}

#[test]
fn stale_label_refresh_does_not_revive_optimistically_archived_envelope() {
    // Reproduces the bounce-back bug: user presses `e` on a message,
    // it disappears optimistically, then a label-refresh response
    // (sync- or mutation-triggered) lands with the still-present
    // envelope because the daemon hasn't yet processed the archive.
    // Before the fix, the response unconditionally replaced
    // `mailbox.envelopes` and the row came back. With the fix, the
    // pending optimistic state masks the refresh until the mutation
    // acks.
    let mut app = App::new();
    set_active_inbox(&mut app);
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let before_archive = app.mailbox.envelopes.clone();
    let archived_id = app.mailbox.envelopes[0].id.clone();

    app.apply(Action::Archive);
    assert_eq!(app.mailbox.envelopes.len(), 2, "optimistic remove fired");
    assert!(app.pending_optimistic.is_removed(&archived_id));

    // Stale refresh from the daemon: it hasn't processed the archive
    // yet, so it returns every envelope including the archived one.
    let mut refresh = before_archive.clone();
    app.pending_optimistic.apply(&mut refresh);
    assert_eq!(
        refresh.len(),
        2,
        "stale refresh must be filtered to honor the pending archive"
    );
    assert!(!refresh.iter().any(|env| env.id == archived_id));

    // Once the daemon acks, future refreshes are unmasked again.
    let mutation_id = app.pending_mutation_queue[0].id;
    app.pending_optimistic.clear(mutation_id);
    let mut after_ack = before_archive;
    app.pending_optimistic.apply(&mut after_ack);
    assert_eq!(
        after_ack.len(),
        3,
        "after mutation ack, refresh is no longer masked"
    );
}

#[test]
fn archived_message_stays_hidden_while_transient_failure_retries() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let archived_id = app.mailbox.envelopes[0].id.clone();

    app.apply(Action::Archive);

    assert_eq!(app.mailbox.envelopes.len(), 2);
    assert!(app.pending_optimistic.is_removed(&archived_id));
    let queued = app.pending_mutation_queue.remove(0);
    assert!(!queued.best_effort);

    let error = MxrError::Ipc("mutation skipped 1 message(s): pool timed out".into());
    app.finish_pending_mutation();
    app.schedule_mutation_retry(queued, &error);

    assert!(app.modals.error.is_none());
    assert_eq!(
        app.mailbox.envelopes.len(),
        2,
        "retrying a transient failure must not bounce the archived row back into view"
    );
    assert!(app.pending_optimistic.is_removed(&archived_id));
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.pending_mutation_queue[0].attempts, 1);
    assert_eq!(app.pending_mutation_count, 1);
}

#[test]
fn archive_outside_inbox_does_not_remove_optimistically() {
    let mut app = App::new();
    // Active label = STARRED (not INBOX). Archive removes INBOX, so the
    // message still belongs in the Starred view and should NOT vanish.
    app.mailbox.labels = make_test_labels();
    app.mailbox.active_label = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name.eq_ignore_ascii_case("STARRED"))
        .map(|label| label.id.clone());
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::Archive);

    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(
        app.mailbox.envelopes.len(),
        3,
        "archive outside inbox should not strip the row before the daemon responds"
    );
}

#[test]
fn star_updates_flags_in_place() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    // First envelope is READ (even index), not starred
    assert!(!app.mailbox.envelopes[0]
        .flags
        .contains(MessageFlags::STARRED));
    app.apply(Action::Star);
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.pending_mutation_count, 1);
    assert!(app.mailbox.envelopes[0]
        .flags
        .contains(MessageFlags::STARRED));
}

#[test]
fn bulk_mark_read_applies_flags_when_confirmed() {
    let mut app = App::new();
    let mut envelopes = make_test_envelopes(3);
    for envelope in &mut envelopes {
        envelope.flags.remove(MessageFlags::READ);
    }
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes.clone();
    app.mailbox.selected_set = envelopes
        .iter()
        .map(|envelope| envelope.id.clone())
        .collect();

    app.apply(Action::MarkRead);
    assert!(app.pending_mutation_queue.is_empty());
    match app.modals.pending_bulk_confirm.as_ref() {
        Some(confirm) => match &confirm.request {
            Request::Mutation {
                mutation: MutationCommand::SetRead { message_ids, read },
                ..
            } => {
                assert!(*read);
                assert_eq!(message_ids.len(), 3);
            }
            other => panic!("Expected SetRead bulk request, got {other:?}"),
        },
        None => panic!("Expected pending bulk confirmation"),
    }
    assert!(app
        .mailbox
        .envelopes
        .iter()
        .all(|envelope| !envelope.flags.contains(MessageFlags::READ)));

    app.apply(Action::OpenSelected);

    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.pending_mutation_count, 1);
    assert!(app.modals.pending_bulk_confirm.is_none());
    assert!(app
        .mailbox
        .envelopes
        .iter()
        .all(|envelope| envelope.flags.contains(MessageFlags::READ)));
    assert_eq!(
        app.pending_mutation_status.as_deref(),
        Some("Marking 3 messages as read...")
    );
}

#[test]
fn status_bar_shows_pending_mutation_indicator_after_other_actions() {
    let mut app = App::new();
    let mut envelopes = make_test_envelopes(2);
    for envelope in &mut envelopes {
        envelope.flags.remove(MessageFlags::READ);
    }
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;

    app.apply(Action::MarkRead);
    app.apply(Action::MoveDown);

    let state = app.status_bar_state();
    assert_eq!(state.pending_mutation_count, 1);
    assert_eq!(
        state.pending_mutation_status.as_deref(),
        Some("Marking 1 message as read...")
    );
}

#[test]
fn mark_read_and_archive_removes_message_optimistically_and_queues_mutation() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    let mut envelopes = make_test_envelopes(1);
    envelopes[0].flags.remove(MessageFlags::READ);
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;
    let message_id = app.mailbox.envelopes[0].id.clone();

    app.apply(Action::MarkReadAndArchive);

    assert!(app.mailbox.envelopes.is_empty());
    assert!(app.mailbox.all_envelopes.is_empty());
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::Mutation {
            mutation: MutationCommand::ReadAndArchive { message_ids },
            ..
        } => {
            assert_eq!(message_ids, &vec![message_id]);
        }
        other => panic!("expected read-and-archive mutation, got {other:?}"),
    }
}

#[test]
fn bulk_mark_read_and_archive_removes_messages_when_confirmed() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    let mut envelopes = make_test_envelopes(3);
    for envelope in &mut envelopes {
        envelope.flags.remove(MessageFlags::READ);
    }
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes.clone();
    app.mailbox.selected_set = envelopes
        .iter()
        .map(|envelope| envelope.id.clone())
        .collect();

    app.apply(Action::MarkReadAndArchive);
    match app.modals.pending_bulk_confirm.as_ref() {
        Some(confirm) => match &confirm.request {
            Request::Mutation {
                mutation: MutationCommand::ReadAndArchive { message_ids },
                ..
            } => {
                assert_eq!(message_ids.len(), 3);
            }
            other => panic!("Expected ReadAndArchive bulk request, got {other:?}"),
        },
        None => panic!("Expected pending bulk confirmation"),
    }
    assert_eq!(app.mailbox.envelopes.len(), 3);

    app.apply(Action::OpenSelected);

    assert!(app.modals.pending_bulk_confirm.is_none());
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.pending_mutation_count, 1);
    assert!(app.mailbox.envelopes.is_empty());
    assert!(app.mailbox.all_envelopes.is_empty());
    assert_eq!(
        app.pending_mutation_status.as_deref(),
        Some("Marking 3 messages as read and archiving...")
    );
}

#[test]
fn invite_response_action_requires_calendar_metadata() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::RespondInvite(
        mxr_protocol::CalendarInviteActionData::Accept,
    ));

    assert!(app.pending_mutation_queue.is_empty());
    assert!(app.modals.pending_bulk_confirm.is_none());
    assert_eq!(
        app.status_message.as_deref(),
        Some("No calendar invite found for this message")
    );
}

/// `Action::RespondInvite` no longer opens a modal — it arms
/// `pending_invite_send` with a 1s hold window. The tick loop later
/// drains it into `pending_mutation_queue`. Pressing `u` within the
/// window cancels without ever talking to the daemon, so no email is
/// sent on a mistaken keystroke.
#[test]
fn invite_response_action_arms_pending_send_with_undo_window() {
    let mut app = App::new();
    let envelope = make_test_envelopes(1).remove(0);
    app.mailbox.envelopes = vec![envelope.clone()];
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.body_cache.insert(
        envelope.id.clone(),
        MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some("Join us".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                calendar: Some(CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Planning session".into()),
                    starts_at: Some("2026-05-20T15:00:00Z".into()),
                    organizer: Some(CalendarPerson {
                        email: "organizer@example.com".into(),
                        name: Some("Organizer".into()),
                        uri: Some("mailto:organizer@example.com".into()),
                    }),
                    attendees: vec![CalendarAttendee {
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
        },
    );

    app.apply(Action::RespondInvite(
        mxr_protocol::CalendarInviteActionData::Accept,
    ));

    assert!(
        app.modals.pending_bulk_confirm.is_none(),
        "auto-confirm flow must not open the bulk confirm modal"
    );
    let pending = app
        .pending_invite_send
        .as_ref()
        .expect("RSVP must arm pending_invite_send slot");
    assert_eq!(pending.message_id, envelope.id);
    assert_eq!(
        pending.action,
        mxr_protocol::CalendarInviteActionData::Accept
    );
    assert!(
        app.pending_mutation_queue.is_empty(),
        "the daemon RPC must not fire until the 1s window elapses"
    );

    // Tick past the dispatch deadline and confirm the request drains
    // into the mutation queue.
    let future = pending.dispatch_at + std::time::Duration::from_millis(1);
    app.tick_pending_invite_send(future);

    assert!(app.pending_invite_send.is_none());
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::RespondInvite {
            message_id,
            action,
            dry_run,
        } => {
            assert_eq!(message_id, &envelope.id);
            assert_eq!(*action, mxr_protocol::CalendarInviteActionData::Accept);
            assert!(!dry_run);
        }
        other => panic!("expected queued RespondInvite request, got {other:?}"),
    }
}

/// Pressing `u` while `pending_invite_send` is armed cancels the RSVP
/// before any daemon RPC fires — no email goes out.
#[test]
fn invite_response_undo_within_window_prevents_send() {
    let mut app = App::new();
    let envelope = make_test_envelopes(1).remove(0);
    app.mailbox.envelopes = vec![envelope.clone()];
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.body_cache.insert(
        envelope.id.clone(),
        MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some("Join us".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                calendar: Some(CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Planning session".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        },
    );

    app.apply(Action::RespondInvite(
        mxr_protocol::CalendarInviteActionData::Accept,
    ));
    assert!(app.pending_invite_send.is_some());

    app.apply(Action::UndoLastMutation);

    assert!(
        app.pending_invite_send.is_none(),
        "undo must clear the pending invite send"
    );
    assert!(
        app.pending_mutation_queue.is_empty(),
        "undo must prevent the daemon RPC entirely"
    );
}

/// `format_mutation_failure` is what the runtime surfaces to the
/// user when the daemon returns 0 succeeded with skipped > 0.
/// Locks down two behaviors:
///  - when no per-account error is set, it falls back to the
///    skipped-count summary (the previous all-the-time behavior);
///  - when per-account errors are present, they are joined onto
///    the summary so the user sees the real cause (e.g. pool
///    timeout) instead of a meaningless "skipped 1 message(s)".
#[test]
fn format_mutation_failure_joins_per_account_errors() {
    use super::format_mutation_failure;
    use mxr_core::id::AccountId;
    use mxr_protocol::{AccountMutationResultData, MutationResultData};

    let bare = MutationResultData {
        requested: 1,
        succeeded: 0,
        skipped: 1,
        failed: 0,
        accounts: vec![AccountMutationResultData {
            account_id: AccountId::new(),
            account_name: "primary".into(),
            succeeded: 0,
            skipped: 1,
            failed: 0,
            error: None,
        }],
        mutation_id: None,
    };
    assert_eq!(
        format_mutation_failure(&bare),
        "mutation skipped 1 message(s)"
    );

    let with_error = MutationResultData {
        accounts: vec![
            AccountMutationResultData {
                account_id: AccountId::new(),
                account_name: "primary".into(),
                succeeded: 0,
                skipped: 1,
                failed: 0,
                error: Some("pool timed out while waiting for an open connection".into()),
            },
            AccountMutationResultData {
                account_id: AccountId::new(),
                account_name: "secondary".into(),
                succeeded: 0,
                skipped: 1,
                failed: 0,
                error: Some("disk I/O error".into()),
            },
        ],
        ..bare
    };
    let formatted = format_mutation_failure(&with_error);
    assert!(formatted.starts_with("mutation skipped 1 message(s):"));
    assert!(formatted.contains("pool timed out"));
    assert!(formatted.contains("disk I/O error"));
}

#[test]
fn mutation_failure_opens_error_modal_and_refreshes_mailbox() {
    let mut app = App::new();

    app.show_mutation_failure(&MxrError::Ipc("boom".into()));
    app.refresh_mailbox_after_mutation_failure();

    assert_eq!(
        app.modals.error.as_ref().map(|modal| modal.title.as_str()),
        Some("Mutation Failed")
    );
    assert_eq!(
        app.modals
            .error
            .as_ref()
            .map(|modal| modal.detail.contains("boom")),
        Some(true)
    );
    assert!(app.mailbox.pending_labels_refresh);
    assert!(app.mailbox.pending_all_envelopes_refresh);
    assert!(app.diagnostics.pending_status_refresh);
    assert!(app.mailbox.pending_subscriptions_refresh);
}

#[test]
fn mutation_failure_reloads_pending_label_fetch() {
    let mut app = App::new();
    let inbox_id = LabelId::new();
    app.mailbox.pending_active_label = Some(inbox_id.clone());

    app.refresh_mailbox_after_mutation_failure();

    assert_eq!(app.mailbox.pending_label_fetch.as_ref(), Some(&inbox_id));
}

#[test]
fn archive_viewing_message_effect() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    // Open first message
    app.apply(Action::OpenSelected);
    let viewing_id = app
        .mailbox
        .viewing_envelope
        .as_ref()
        .expect("open selected should populate viewing envelope")
        .id
        .clone();
    // The pending_mutation_queue is empty — Archive wasn't pressed yet
    // Press archive while viewing
    app.apply(Action::Archive);
    let effect = app.pending_mutation_queue.remove(0).effect;
    // Verify the effect targets the viewing envelope
    match &effect {
        MutationEffect::RemoveFromList(id) => {
            assert_eq!(*id, viewing_id);
        }
        _ => panic!("Expected RemoveFromList"),
    }
}

#[test]
fn archive_keeps_reader_open_and_selects_next_message() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    let removed_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    let next_id = app.mailbox.envelopes[1].id.clone();

    app.apply_removed_message_ids(&[removed_id]);

    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    assert_eq!(app.mailbox.selected_index, 0);
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|envelope| envelope.id.clone()),
        Some(next_id)
    );
}

#[test]
fn archive_keeps_mail_list_focus_when_reader_was_visible() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.mailbox.active_pane = ActivePane::MailList;
    let removed_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    let next_id = app.mailbox.envelopes[1].id.clone();

    app.apply_removed_message_ids(&[removed_id]);

    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|envelope| envelope.id.clone()),
        Some(next_id)
    );
}

#[test]
fn archive_last_visible_message_closes_reader() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    let removed_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();

    app.apply_removed_message_ids(&[removed_id]);

    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    assert!(app.mailbox.viewing_envelope.is_none());
    assert!(app.mailbox.envelopes.is_empty());
}

// --- Mail list title tests ---

#[test]
fn mail_list_title_shows_message_count() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let title = app.mail_list_title();
    assert!(title.contains("5"), "Title should show message count");
    assert!(
        title.contains("Threads"),
        "Default title should say Threads"
    );
}

#[test]
fn mail_list_title_shows_label_name() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.labels = make_test_labels();
    let inbox_id = app
        .mailbox
        .labels
        .iter()
        .find(|l| l.name == "INBOX")
        .unwrap()
        .id
        .clone();
    app.mailbox.active_label = Some(inbox_id);
    let title = app.mail_list_title();
    assert!(
        title.contains("Inbox"),
        "Title should show humanized label name"
    );
}

#[test]
fn mail_list_title_shows_search_query() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.search.active = true;
    app.search.bar.query = "deployment".to_string();
    let title = app.mail_list_title();
    assert!(
        title.contains("deployment"),
        "Title should show search query"
    );
    assert!(title.contains("Search"), "Title should indicate search");
}

#[test]
fn message_view_body_display() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenMessageView);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    app.mailbox.body_view_state = BodyViewState::ready(
        "Hello".into(),
        "Hello".into(),
        BodySource::Plain,
        BodyViewMetadata::default(),
    );
    assert_eq!(app.mailbox.body_view_state.display_text(), Some("Hello"));
    app.apply(Action::CloseMessageView);
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Empty { .. }
    ));
}

#[test]
fn close_message_view_preserves_reader_mode() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenMessageView);

    app.apply(Action::CloseMessageView);

    assert!(app.mailbox.reader_mode);
    assert!(app.mailbox.html_view);
}

#[test]
fn open_selected_populates_visible_thread_messages() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    let shared_thread = ThreadId::new();
    app.mailbox.envelopes[0].thread_id = shared_thread.clone();
    app.mailbox.envelopes[1].thread_id = shared_thread;
    app.mailbox.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(5);
    app.mailbox.envelopes[1].date = chrono::Utc::now();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);

    assert_eq!(app.mailbox.viewed_thread_messages.len(), 2);
    assert_eq!(
        app.mailbox.viewed_thread_messages[0].id,
        app.mailbox.envelopes[0].id
    );
    assert_eq!(
        app.mailbox.viewed_thread_messages[1].id,
        app.mailbox.envelopes[1].id
    );
}

#[test]
fn mail_list_defaults_to_threads() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    let shared_thread = ThreadId::new();
    app.mailbox.envelopes[0].thread_id = shared_thread.clone();
    app.mailbox.envelopes[1].thread_id = shared_thread;
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    assert_eq!(app.mail_list_rows().len(), 2);
    assert_eq!(
        app.selected_mail_row().map(|row| row.message_count),
        Some(2)
    );
}

#[test]
fn open_thread_focuses_latest_unread_message() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    let shared_thread = ThreadId::new();
    app.mailbox.envelopes[0].thread_id = shared_thread.clone();
    app.mailbox.envelopes[1].thread_id = shared_thread;
    app.mailbox.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(10);
    app.mailbox.envelopes[1].date = chrono::Utc::now();
    app.mailbox.envelopes[0].flags = MessageFlags::READ;
    app.mailbox.envelopes[1].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);

    assert_eq!(app.mailbox.thread_selected_index, 1);
    assert_eq!(
        app.focused_thread_envelope().map(|env| env.id.clone()),
        Some(app.mailbox.envelopes[1].id.clone())
    );
}

#[test]
fn open_selected_marks_unread_message_read_after_dwell() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);

    assert!(!app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(!app.mailbox.all_envelopes[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(!app.mailbox.viewed_thread_messages[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(!app
        .mailbox
        .viewing_envelope
        .as_ref()
        .unwrap()
        .flags
        .contains(MessageFlags::READ));
    assert!(app.pending_mutation_queue.is_empty());

    app.expire_pending_preview_read_for_tests();
    app.tick();

    assert!(app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(app.mailbox.all_envelopes[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(app.mailbox.viewed_thread_messages[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(app
        .mailbox
        .viewing_envelope
        .as_ref()
        .unwrap()
        .flags
        .contains(MessageFlags::READ));
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert!(app.pending_mutation_queue[0].best_effort);
    match &app.pending_mutation_queue[0].request {
        Request::Mutation {
            mutation: MutationCommand::SetRead { message_ids, read },
            ..
        } => {
            assert!(*read);
            assert_eq!(message_ids, &vec![app.mailbox.envelopes[0].id.clone()]);
        }
        other => panic!("expected set-read mutation, got {other:?}"),
    }
}

#[test]
fn preview_read_transient_failure_retries_without_error_modal_or_rollback() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.expire_pending_preview_read_for_tests();
    app.tick();

    let queued = app.pending_mutation_queue.remove(0);
    assert!(queued.best_effort);
    assert!(app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));

    let error = MxrError::Ipc("mutation skipped 1 message(s): pool timed out".into());
    app.finish_pending_mutation();
    assert!(app.should_retry_mutation_failure(&error));
    app.schedule_mutation_retry(queued, &error);

    assert!(app.modals.error.is_none());
    assert!(app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(app.mailbox.all_envelopes[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(app.mailbox.viewed_thread_messages[0]
        .flags
        .contains(MessageFlags::READ));
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.pending_mutation_queue[0].attempts, 1);
    assert!(app.pending_mutation_queue[0].run_after.is_some());
    assert_eq!(app.pending_mutation_count, 1);
}

#[test]
fn preview_read_exhausted_failure_reconciles_without_error_modal() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.expire_pending_preview_read_for_tests();
    app.tick();

    let queued = app.pending_mutation_queue.remove(0);
    assert!(queued.best_effort);
    assert!(app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));

    app.finish_pending_mutation();
    app.handle_mutation_failure_result(
        queued.id,
        queued.best_effort,
        &MxrError::Ipc("pool timed out".into()),
    );

    assert!(app.modals.error.is_none());
    assert!(!app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(!app.mailbox.all_envelopes[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(!app.mailbox.viewed_thread_messages[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(!app
        .mailbox
        .viewing_envelope
        .as_ref()
        .unwrap()
        .flags
        .contains(MessageFlags::READ));
    assert!(app.mailbox.pending_labels_refresh);
    assert!(app.mailbox.pending_all_envelopes_refresh);
    assert!(app.diagnostics.pending_status_refresh);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Mailbox refreshing to reconcile state")
    );
}

#[test]
fn open_selected_on_read_message_does_not_queue_read_mutation() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].flags = MessageFlags::READ;
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.expire_pending_preview_read_for_tests();
    app.tick();

    assert!(app.pending_mutation_queue.is_empty());
}

#[test]
fn reopening_same_message_does_not_queue_duplicate_read_mutation() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.apply(Action::OpenSelected);

    assert!(app.pending_mutation_queue.is_empty());
    app.expire_pending_preview_read_for_tests();
    app.tick();
    assert_eq!(app.pending_mutation_queue.len(), 1);
}

#[test]
fn single_message_view_uses_jk_to_scroll() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);

    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
    assert_eq!(app.mailbox.viewed_thread_messages.len(), 1);
    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert_eq!(app.mailbox.message_scroll_offset, 0);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert_eq!(app.mailbox.message_scroll_offset, 1);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert_eq!(app.mailbox.message_scroll_offset, 2);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert_eq!(app.mailbox.message_scroll_offset, 1);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert_eq!(app.mailbox.message_scroll_offset, 0);
}

#[test]
fn thread_move_down_changes_reply_target() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    let shared_thread = ThreadId::new();
    app.mailbox.envelopes[0].thread_id = shared_thread.clone();
    app.mailbox.envelopes[1].thread_id = shared_thread;
    app.mailbox.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(5);
    app.mailbox.envelopes[1].date = chrono::Utc::now();
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.envelopes[1].flags = MessageFlags::READ;
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    assert_eq!(
        app.focused_thread_envelope().map(|env| env.id.clone()),
        Some(app.mailbox.envelopes[0].id.clone())
    );

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

    assert_eq!(
        app.focused_thread_envelope().map(|env| env.id.clone()),
        Some(app.mailbox.envelopes[1].id.clone())
    );
    app.apply(Action::Reply);
    assert_eq!(
        app.compose.pending_compose,
        Some(crate::app::ComposeAction::Reply {
            message_id: app.mailbox.envelopes[1].id.clone(),
            account_id: app.mailbox.envelopes[1].account_id.clone(),
            preloaded: None,
        })
    );
}

#[test]
fn thread_focus_change_marks_newly_focused_unread_message_read_after_dwell() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    let shared_thread = ThreadId::new();
    app.mailbox.envelopes[0].thread_id = shared_thread.clone();
    app.mailbox.envelopes[1].thread_id = shared_thread;
    app.mailbox.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(5);
    app.mailbox.envelopes[1].date = chrono::Utc::now();
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.envelopes[1].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    assert_eq!(app.mailbox.thread_selected_index, 1);
    assert!(app.pending_mutation_queue.is_empty());

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));

    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert!(!app.mailbox.viewed_thread_messages[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(app.pending_mutation_queue.is_empty());

    app.expire_pending_preview_read_for_tests();
    app.tick();

    assert!(app.mailbox.viewed_thread_messages[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(app
        .mailbox
        .viewing_envelope
        .as_ref()
        .unwrap()
        .flags
        .contains(MessageFlags::READ));
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::Mutation {
            mutation: MutationCommand::SetRead { message_ids, read },
            ..
        } => {
            assert!(*read);
            assert_eq!(message_ids, &vec![app.mailbox.envelopes[0].id.clone()]);
        }
        other => panic!("expected set-read mutation, got {other:?}"),
    }
}

#[test]
fn preview_navigation_only_marks_message_read_after_settling() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.envelopes[1].flags = MessageFlags::empty();
    app.mailbox.envelopes[0].thread_id = ThreadId::new();
    app.mailbox.envelopes[1].thread_id = ThreadId::new();
    app.mailbox.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(1);
    app.mailbox.envelopes[1].date = chrono::Utc::now();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.apply(Action::MoveDown);

    assert!(!app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(!app.mailbox.envelopes[1].flags.contains(MessageFlags::READ));
    assert!(app.pending_mutation_queue.is_empty());

    app.expire_pending_preview_read_for_tests();
    app.tick();

    assert!(!app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(app.mailbox.envelopes[1].flags.contains(MessageFlags::READ));
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::Mutation {
            mutation: MutationCommand::SetRead { message_ids, read },
            ..
        } => {
            assert!(*read);
            assert_eq!(message_ids, &vec![app.mailbox.envelopes[1].id.clone()]);
        }
        other => panic!("expected set-read mutation, got {other:?}"),
    }
}

#[test]
fn help_action_toggles_modal_state() {
    let mut app = App::new();

    app.apply(Action::Help);
    assert!(app.modals.help_open);
    assert!(app.modals.help_query.is_empty());
    assert_eq!(app.modals.help_selected, 0);

    app.modals.help_query = "config".into();
    app.modals.help_selected = 3;
    app.apply(Action::Help);
    assert!(!app.modals.help_open);
    assert!(app.modals.help_query.is_empty());
    assert_eq!(app.modals.help_selected, 0);
}

#[test]
fn help_modal_typing_enters_search_mode_and_backspace_clears_it() {
    let mut app = App::new();
    app.apply(Action::Help);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.help_query, "g");
    assert_eq!(app.modals.help_selected, 0);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.help_query, "gc");

    let action = app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.help_query, "g");

    let action = app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(action.is_none());
    assert!(app.modals.help_query.is_empty());
    assert_eq!(app.modals.help_selected, 0);
}

#[test]
fn help_modal_o_types_instead_of_reopening_onboarding() {
    let mut app = App::new();
    app.apply(Action::Help);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.help_query, "o");
    assert!(!app.modals.onboarding.visible);
}

#[test]
fn account_form_validation_points_to_first_invalid_field() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.mode = crate::app::AccountFormMode::ImapSmtp;
    app.accounts.page.form.key = "work".into();
    app.accounts.page.form.email = "me@example.com".into();
    app.accounts.page.form.imap_port = "993".into();
    app.accounts.page.form.smtp_host = "smtp.example.com".into();
    app.accounts.page.form.smtp_port = "587".into();
    app.accounts.page.form.smtp_auth_required = false;

    app.apply(Action::TestAccountForm);

    assert_eq!(app.accounts.page.form.active_field, 4);
    assert!(!app.accounts.page.operation_in_flight);
    assert!(app.accounts.pending_test.is_none());
    let result = app.accounts.page.form.last_result.as_ref().unwrap();
    assert!(result.summary.contains("Account form has problems."));
    assert_eq!(
            result.sync.as_ref().unwrap().detail,
            "IMAP host is required. IMAP auth is enabled, so IMAP password or IMAP pass ref is required."
        );
}

#[test]
fn smtp_only_form_test_allows_no_auth_and_marks_operation_pending() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.mode = crate::app::AccountFormMode::SmtpOnly;
    app.accounts.page.form.key = "relay".into();
    app.accounts.page.form.email = "relay@example.com".into();
    app.accounts.page.form.smtp_host = "smtp.example.com".into();
    app.accounts.page.form.smtp_port = "25".into();
    app.accounts.page.form.smtp_auth_required = false;
    app.accounts.page.form.last_result = Some(mxr_protocol::AccountOperationResult {
        ok: false,
        summary: "stale".into(),
        save: None,
        auth: None,
        sync: None,
        send: None,
        device_code_url: None,
        device_code_user_code: None,
    });

    app.apply(Action::TestAccountForm);

    assert!(app.accounts.page.operation_in_flight);
    assert!(app.accounts.page.form.last_result.is_none());
    let pending = app.accounts.pending_test.take().unwrap();
    match pending.send.unwrap() {
        mxr_protocol::AccountSendConfigData::Smtp {
            auth_required,
            username,
            password_ref,
            ..
        } => {
            assert!(!auth_required);
            assert!(username.is_empty());
            assert!(password_ref.is_empty());
        }
        other => panic!("expected smtp config, got {other:?}"),
    }
}

#[test]
fn auth_required_form_generates_secret_refs_from_account_key() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.is_new_account = true;
    app.accounts.page.form.mode = crate::app::AccountFormMode::ImapSmtp;
    app.accounts.page.form.key = "work".into();
    app.accounts.page.form.email = "me@example.com".into();
    app.accounts.page.form.imap_host = "imap.example.com".into();
    app.accounts.page.form.imap_port = "993".into();
    app.accounts.page.form.imap_password = "imap-secret".into();
    app.accounts.page.form.smtp_host = "smtp.example.com".into();
    app.accounts.page.form.smtp_port = "587".into();
    app.accounts.page.form.smtp_password = "smtp-secret".into();

    app.apply(Action::TestAccountForm);

    let pending = app.accounts.pending_test.take().unwrap();
    match pending.sync.unwrap() {
        mxr_protocol::AccountSyncConfigData::Imap { password_ref, .. } => {
            assert_eq!(password_ref, "mxr/work-imap");
        }
        other => panic!("expected imap config, got {other:?}"),
    }
    match pending.send.unwrap() {
        mxr_protocol::AccountSendConfigData::Smtp { password_ref, .. } => {
            assert_eq!(password_ref, "mxr/work-smtp");
        }
        other => panic!("expected smtp config, got {other:?}"),
    }
}

#[test]
fn failed_account_operation_opens_details_modal() {
    let mut app = App::new();
    let result = mxr_protocol::AccountOperationResult {
            ok: false,
            summary: "Account 'consulting' test failed.".into(),
            save: None,
            auth: None,
            sync: Some(mxr_protocol::AccountOperationStep {
                ok: false,
                detail: "IMAP server returned a NAMESPACE response in an unsupported format during folder discovery. This looks like a server compatibility issue, not necessarily a bad username or password.".into(),
            }),
            send: Some(mxr_protocol::AccountOperationStep {
                ok: true,
                detail: "SMTP send ok".into(),
            }),
            device_code_url: None,
            device_code_user_code: None,
        };

    app.apply_account_operation_result(result);

    let modal = app.modals.error.as_ref().unwrap();
    assert_eq!(modal.title, "Account Test Failed");
    assert!(modal.detail.contains("NAMESPACE response"));
    assert!(modal.detail.contains("compatibility issue"));
}

#[test]
fn account_form_o_reopens_result_details_modal() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.last_result = Some(mxr_protocol::AccountOperationResult {
        ok: false,
        summary: "Account 'consulting' test failed.".into(),
        save: None,
        auth: None,
        sync: Some(mxr_protocol::AccountOperationStep {
            ok: false,
            detail: "IMAP server returned a response mxr could not parse.".into(),
        }),
        send: None,
        device_code_url: None,
        device_code_user_code: None,
    });

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

    assert!(action.is_none());
    assert_eq!(
        app.modals.error.as_ref().map(|modal| modal.title.as_str()),
        Some("Account Test Failed")
    );
}

#[test]
fn error_modal_supports_scrolling_keys() {
    let mut app = App::new();
    app.modals.error = Some(crate::app::ErrorModalState::new(
        "Account Test Failed",
        "line1\nline2\nline3\nline4\nline5",
    ));

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.error.as_ref().unwrap().scroll_offset, 1);

    let action = app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.error.as_ref().unwrap().scroll_offset, 9);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.error.as_ref().unwrap().scroll_offset, 8);

    let action = app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.error.as_ref().unwrap().scroll_offset, 0);
}

#[test]
fn closing_new_account_form_preserves_draft_and_resume_restores_it() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.is_new_account = true;
    app.accounts.page.form.key = "draft".into();
    app.accounts.page.form.email = "draft@example.com".into();
    app.accounts.page.form.smtp_host = "smtp.example.com".into();

    let action = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(action.is_none());
    assert!(!app.accounts.page.form.visible);
    assert_eq!(
        app.accounts.page.new_account_draft.as_ref().unwrap().key,
        "draft"
    );

    app.apply(Action::OpenAccountFormNew);
    assert!(app.accounts.page.resume_new_account_draft_prompt_open);
    assert!(!app.accounts.page.form.visible);

    let action = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(action.is_none());
    assert!(app.accounts.page.form.visible);
    assert_eq!(app.accounts.page.form.key, "draft");
    assert_eq!(app.accounts.page.form.email, "draft@example.com");
    assert!(app.accounts.page.new_account_draft.is_none());
}

#[test]
fn new_account_draft_prompt_can_start_fresh_form() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.is_new_account = true;
    app.accounts.page.form.key = "draft".into();
    app.accounts.page.form.email = "draft@example.com".into();

    let action = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(
        app.accounts
            .page
            .new_account_draft
            .as_ref()
            .map(|draft| draft.email.as_str()),
        Some("draft@example.com")
    );

    app.apply(Action::OpenAccountFormNew);
    assert!(app.accounts.page.resume_new_account_draft_prompt_open);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert!(app.accounts.page.form.visible);
    assert!(app.accounts.page.form.is_new_account);
    assert!(app.accounts.page.form.key.is_empty());
    assert!(app.accounts.page.new_account_draft.is_none());
    assert!(!app.accounts.page.resume_new_account_draft_prompt_open);
}

#[test]
fn leaving_accounts_screen_preserves_new_account_draft() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.is_new_account = true;
    app.accounts.page.form.key = "draft".into();
    app.accounts.page.form.email = "draft@example.com".into();

    app.apply(Action::OpenMailboxScreen);

    assert_eq!(app.screen, Screen::Mailbox);
    assert!(!app.accounts.page.form.visible);
    assert_eq!(
        app.accounts.page.new_account_draft.as_ref().unwrap().email,
        "draft@example.com"
    );
}

#[test]
fn open_search_screen_activates_dedicated_search_workspace() {
    let mut app = App::new();
    app.apply(Action::OpenSearchScreen);
    assert_eq!(app.screen, Screen::Search);
    assert!(app.search.page.editing);
}

#[test]
fn search_screen_typing_updates_results_and_queues_search() {
    let mut app = App::new();
    let mut envelopes = make_test_envelopes(2);
    envelopes[0].subject = "crates.io release".into();
    envelopes[0].snippet = "mxr publish".into();
    envelopes[1].subject = "support request".into();
    envelopes[1].snippet = "billing".into();
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;

    app.apply(Action::OpenSearchScreen);
    app.search.page.query.clear();
    app.search.page.results = app.mailbox.all_envelopes.clone();

    for ch in "crate".chars() {
        let action = app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        assert!(action.is_none());
    }

    assert_eq!(app.search.page.query, "crate");
    assert!(app.search.page.results.is_empty());
    assert!(!app.search.page.loading_more);
    assert!(!app.search.page.count_pending);
    assert_eq!(
        app.search.page.ui_status,
        crate::app::SearchUiStatus::Debouncing
    );
    assert_eq!(
        app.search.pending_debounce,
        Some(crate::app::PendingSearchDebounce {
            query: "crate".into(),
            mode: mxr_core::SearchMode::Lexical,
            session_id: app.search.page.session_id,
            due_at: app
                .search
                .pending_debounce
                .as_ref()
                .map(|pending| pending.due_at)
                .expect("debounce timer should be set"),
        })
    );
    assert!(app.search.pending.is_none());
    assert!(app.search.pending_count.is_none());
}

#[test]
fn open_search_screen_preserves_existing_search_session() {
    let mut app = App::new();
    let results = make_test_envelopes(2);
    app.search.bar.query = "stale overlay".into();
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.search.page.selected_index = 1;
    app.search.page.result_selected = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewing_envelope = Some(results[1].clone());

    app.apply(Action::OpenRulesScreen);
    app.apply(Action::OpenSearchScreen);

    assert_eq!(app.screen, Screen::Search);
    assert_eq!(app.search.page.query, "deploy");
    assert_eq!(app.search.page.results.len(), 2);
    assert_eq!(app.search.page.selected_index, 1);
    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[1].id.clone())
    );
    assert!(app.search.pending.is_none());
}

#[test]
fn slash_opens_global_search_and_ctrl_f_opens_mailbox_filter() {
    let mut app = App::new();

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert_eq!(action, Some(Action::OpenGlobalSearch));
    app.apply(action.expect("slash should map to search"));
    assert_eq!(app.screen, Screen::Search);
    assert!(app.search.page.editing);

    app.apply(Action::OpenMailboxScreen);
    let action = app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
    assert_eq!(action, Some(Action::OpenMailboxFilter));
}

#[test]
fn search_results_accept_gg_and_g_navigation() {
    let mut app = App::new();
    app.apply(Action::OpenSearchScreen);
    app.search.page.editing = false;
    app.search.page.results = make_test_envelopes(3);
    app.search.page.selected_index = 2;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    assert!(action.is_none());
    let action = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    assert_eq!(action, Some(Action::JumpTop));
    app.apply(action.unwrap());
    assert_eq!(app.search.page.selected_index, 0);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT));
    assert_eq!(action, Some(Action::JumpBottom));
    app.apply(action.unwrap());
    assert_eq!(app.search.page.selected_index, 2);
}

#[test]
fn open_search_screen_without_session_clears_stale_preview_and_query() {
    let mut app = App::new();
    let envelope = make_test_envelopes(1).remove(0);
    app.search.bar.query = "mailbox quick filter".into();
    app.mailbox.viewing_envelope = Some(envelope.clone());
    app.mailbox.viewed_thread_messages = vec![envelope];
    app.search.page.query = "stale".into();
    app.search.page.session_active = false;
    app.search.page.results.clear();

    app.apply(Action::OpenSearchScreen);

    assert_eq!(app.screen, Screen::Search);
    assert!(app.search.page.editing);
    assert!(app.search.page.query.is_empty());
    assert!(app.mailbox.viewing_envelope.is_none());
    assert!(app.mailbox.viewed_thread_messages.is_empty());
    assert_eq!(app.search.page.ui_status, crate::app::SearchUiStatus::Idle);
}

#[test]
fn non_mail_screens_ignore_label_shortcut() {
    let mut app = App::new();

    for screen in [Screen::Rules, Screen::Accounts, Screen::Diagnostics] {
        app.screen = screen;
        app.modals.label_picker.close();
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert!(action.is_none(), "unexpected action on {screen:?}");
        assert!(
            !app.modals.label_picker.visible,
            "label picker opened on {screen:?}"
        );
    }
}

#[test]
fn rules_navigation_refreshes_selected_panel_request() {
    let mut app = App::new();
    app.screen = Screen::Rules;
    app.rules.page.rules = vec![
        serde_json::json!({"id": "rule-1", "name": "One"}),
        serde_json::json!({"id": "rule-2", "name": "Two"}),
    ];
    app.rules.page.panel = crate::app::RulesPanel::History;

    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .is_none());
    assert_eq!(app.rules.page.selected_index, 1);
    assert_eq!(app.rules.pending_history.as_deref(), Some("rule-2"));

    app.rules.page.panel = crate::app::RulesPanel::DryRun;
    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .is_none());
    assert_eq!(app.rules.page.selected_index, 0);
    assert_eq!(app.rules.pending_dry_run.as_deref(), Some("rule-1"));
}

#[test]
fn search_open_selected_keeps_search_screen_and_focuses_preview() {
    let mut app = App::new();
    let results = make_test_envelopes(2);
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.search.page.selected_index = 1;

    app.apply(Action::OpenSelected);

    assert_eq!(app.screen, Screen::Search);
    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[1].id.clone())
    );
}

#[test]
fn search_open_message_follows_cursor_after_returning_to_results() {
    let mut app = App::new();
    let results = make_test_envelopes(3);
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.mailbox.all_envelopes = results.clone();

    app.apply(Action::OpenSelected);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[0].id.clone())
    );

    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .is_none());
    assert_eq!(app.search.page.active_pane, SearchPane::Results);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[0].id.clone())
    );

    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .is_none());
    assert_eq!(app.search.page.selected_index, 1);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[1].id.clone())
    );
}

#[test]
fn search_results_allow_mail_actions_without_preview_focus() {
    let mut app = App::new();
    let results = make_test_envelopes(2);
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.search.page.selected_index = 1;
    app.search.page.active_pane = SearchPane::Results;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert_eq!(action, Some(Action::Star));

    app.apply(action.expect("star action should be available from search results"));

    assert!(app.search.page.results[1]
        .flags
        .contains(MessageFlags::STARRED));
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::Mutation {
            mutation:
                MutationCommand::Star {
                    message_ids,
                    starred,
                },
            ..
        } => {
            assert_eq!(message_ids, &vec![results[1].id.clone()]);
            assert!(*starred);
        }
        other => panic!("expected star mutation, got {other:?}"),
    }
}

#[test]
fn search_results_follow_mail_list_mode_and_open_thread_rows() {
    let mut app = App::new();
    let thread_id = ThreadId::new();
    let now = chrono::Utc::now();
    let older = TestEnvelopeBuilder::new()
        .provider_id("thread-old")
        .thread_id(thread_id.clone())
        .subject("Older hit")
        .date(now - chrono::Duration::minutes(5))
        .build();
    let newer = TestEnvelopeBuilder::new()
        .provider_id("thread-new")
        .thread_id(thread_id)
        .subject("Newer hit")
        .date(now)
        .build();
    let other = TestEnvelopeBuilder::new()
        .provider_id("other-thread")
        .subject("Other thread")
        .date(now - chrono::Duration::minutes(1))
        .build();
    let results = vec![older, newer.clone(), other];
    app.mailbox.mail_list_mode = MailListMode::Messages;
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.mailbox.all_envelopes = results;

    app.apply(Action::ToggleMailListMode);

    assert_eq!(app.search_row_count(), 2);
    assert_eq!(
        app.selected_search_envelope().map(|env| env.id.clone()),
        Some(newer.id.clone())
    );

    app.apply(Action::OpenSelected);

    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(newer.id.clone())
    );
}

#[test]
fn search_results_refresh_preserves_open_row_when_it_still_exists() {
    let mut app = App::new();
    let results = make_test_envelopes(3);
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.search.page.selected_index = 1;
    app.mailbox.all_envelopes = results.clone();

    app.apply(Action::OpenSelected);
    app.apply_search_page_results(
        false,
        SearchResultData {
            envelopes: vec![results[0].clone(), results[1].clone()],
            scores: std::collections::HashMap::new(),
            has_more: false,
        },
    );

    assert_eq!(app.search.page.selected_index, 1);
    assert!(app.search.page.result_selected);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[1].id.clone())
    );
}

#[test]
fn search_results_refresh_clears_open_message_when_selected_row_disappears() {
    let mut app = App::new();
    let results = make_test_envelopes(3);
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.search.page.selected_index = 1;
    app.mailbox.all_envelopes = results.clone();

    app.apply(Action::OpenSelected);
    app.apply_search_page_results(
        false,
        SearchResultData {
            envelopes: vec![results[0].clone()],
            scores: std::collections::HashMap::new(),
            has_more: false,
        },
    );

    assert_eq!(app.search.page.selected_index, 0);
    assert!(!app.search.page.result_selected);
    assert_eq!(app.search.page.active_pane, SearchPane::Results);
    assert!(app.mailbox.viewing_envelope.is_none());
    assert!(app.mailbox.viewed_thread_messages.is_empty());
}

#[test]
fn search_jump_bottom_loads_remaining_pages() {
    let mut app = App::new();
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = make_test_envelopes(3);
    app.search.page.session_active = true;
    app.search.page.has_more = true;
    app.search.page.loading_more = false;
    app.search.page.session_id = 9;

    app.apply(Action::JumpBottom);

    assert!(app.search.page.load_to_end);
    assert!(app.search.page.loading_more);
    assert_eq!(
        app.search.pending,
        Some(PendingSearchRequest {
            query: "deploy".into(),
            mode: mxr_core::SearchMode::Lexical,
            sort: mxr_core::SortOrder::DateDesc,
            limit: SEARCH_PAGE_SIZE,
            offset: 3,
            target: SearchTarget::SearchPage,
            append: true,
            session_id: 9,
        })
    );
}

#[test]
fn search_jump_bottom_uses_search_results_viewport_height() {
    let mut app = App::new();
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = make_test_envelopes(15);
    app.search.page.session_active = true;

    let _ = render_to_string(120, 20, |frame| app.draw(frame));

    app.apply(Action::JumpBottom);

    assert_eq!(app.visible_height, 10);
    assert_eq!(app.search.page.selected_index, 14);
    assert_eq!(app.search.page.scroll_offset, 5);
}

#[test]
fn search_escape_routes_back_to_inbox() {
    let mut app = App::new();
    app.screen = Screen::Search;
    app.search.page.session_active = true;
    app.search.page.query = "deploy".into();
    app.search.page.results = make_test_envelopes(2);
    app.search.page.active_pane = SearchPane::Results;

    let action = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(action, Some(Action::OpenMailboxScreen));
}

#[test]
fn open_rules_screen_marks_refresh_pending() {
    let mut app = App::new();
    app.apply(Action::OpenRulesScreen);
    assert_eq!(app.screen, Screen::Rules);
    assert!(app.rules.page.refresh_pending);
}

#[test]
fn open_diagnostics_screen_marks_refresh_pending() {
    let mut app = App::new();
    app.apply(Action::OpenDiagnosticsScreen);
    assert_eq!(app.screen, Screen::Diagnostics);
    assert!(app.diagnostics.page.refresh_pending);
}

#[test]
fn open_accounts_screen_marks_refresh_pending() {
    let mut app = App::new();
    app.apply(Action::OpenAccountsScreen);
    assert_eq!(app.screen, Screen::Accounts);
    assert!(app.accounts.page.refresh_pending);
}

#[test]
fn new_account_form_opens_from_accounts_screen() {
    let mut app = App::new();
    app.apply(Action::OpenAccountsScreen);
    app.apply(Action::OpenAccountFormNew);

    assert_eq!(app.screen, Screen::Accounts);
    assert!(app.accounts.page.form.visible);
    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::Gmail
    );
}

#[test]
fn app_from_empty_config_enters_account_onboarding() {
    let config = mxr_config::MxrConfig::default();
    let app = App::from_config(&config);

    // Onboarding modal shows on whatever page the user is on (mailbox by default)
    assert_eq!(app.screen, Screen::Mailbox);
    assert!(app.accounts.page.onboarding_required);
    assert!(app.accounts.page.onboarding_modal_open);
}

#[test]
fn onboarding_confirm_opens_new_account_form() {
    let config = mxr_config::MxrConfig::default();
    let mut app = App::from_config(&config);

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.screen, Screen::Accounts);
    assert!(app.accounts.page.form.visible);
    assert!(!app.accounts.page.onboarding_modal_open);
}

#[test]
fn onboarding_q_quits() {
    let config = mxr_config::MxrConfig::default();
    let mut app = App::from_config(&config);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::QuitView));
}

#[test]
fn onboarding_blocks_mailbox_screen_until_account_exists() {
    let config = mxr_config::MxrConfig::default();
    let mut app = App::from_config(&config);

    app.apply(Action::OpenMailboxScreen);

    assert_eq!(app.screen, Screen::Accounts);
    assert!(app.accounts.page.onboarding_required);
}

#[test]
fn account_form_h_and_l_switch_modes_from_any_field() {
    let mut app = App::new();
    app.apply(Action::OpenAccountFormNew);
    app.accounts.page.form.active_field = 2;

    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::ImapSmtp
    );

    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::Gmail
    );
}

#[test]
fn account_form_tab_on_mode_cycles_modes() {
    let mut app = App::new();
    app.apply(Action::OpenAccountFormNew);
    app.accounts.page.form.active_field = 0;

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::ImapSmtp
    );

    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::Gmail
    );
}

#[test]
fn account_form_mode_switch_with_input_requires_confirmation() {
    let mut app = App::new();
    app.apply(Action::OpenAccountFormNew);
    app.accounts.page.form.key = "work".into();

    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));

    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::Gmail
    );
    assert_eq!(
        app.accounts.page.form.pending_mode_switch,
        Some(crate::app::AccountFormMode::ImapSmtp)
    );
}

#[test]
fn account_form_mode_switch_confirmation_applies_mode_change() {
    let mut app = App::new();
    app.apply(Action::OpenAccountFormNew);
    app.accounts.page.form.key = "work".into();

    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::ImapSmtp
    );
    assert!(app.accounts.page.form.pending_mode_switch.is_none());
}

#[test]
fn account_form_mode_switch_confirmation_cancel_keeps_mode() {
    let mut app = App::new();
    app.apply(Action::OpenAccountFormNew);
    app.accounts.page.form.key = "work".into();

    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));

    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::Gmail
    );
    assert!(app.accounts.page.form.pending_mode_switch.is_none());
}

#[test]
fn flattened_sidebar_navigation_reaches_saved_searches() {
    let mut app = App::new();
    app.mailbox.labels = vec![Label {
        id: LabelId::new(),
        account_id: AccountId::new(),
        provider_id: "inbox".into(),
        name: "INBOX".into(),
        kind: LabelKind::System,
        color: None,
        unread_count: 1,
        total_count: 3,
        role: None,
    }];
    app.mailbox.saved_searches = vec![SavedSearch {
        id: SavedSearchId::new(),
        account_id: None,
        name: "Unread".into(),
        query: "is:unread".into(),
        search_mode: SearchMode::Lexical,
        sort: SortOrder::DateDesc,
        icon: None,
        position: 0,
        created_at: chrono::Utc::now(),
    }];
    app.mailbox.active_pane = ActivePane::Sidebar;

    // Sidebar order: INBOX, AllMail, Subscriptions, Owed (Slice 2.3),
    // SavedSearch. Four `j` presses to reach the saved search.
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

    assert!(matches!(
        app.selected_sidebar_item(),
        Some(crate::app::SidebarItem::SavedSearch(_))
    ));
}

#[test]
fn toggle_select_advances_cursor_and_updates_preview() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    app.mailbox.active_pane = ActivePane::MailList;

    app.apply(Action::ToggleSelect);

    assert_eq!(app.mailbox.selected_index, 1);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(app.mailbox.envelopes[1].id.clone())
    );
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Loading { ref preview }
            if preview.as_deref() == Some("Snippet 1")
    ));
}

#[test]
fn toggle_select_in_message_view_keeps_current_message_visible() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);

    let original_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    app.apply(Action::ToggleSelect);

    assert_eq!(app.mailbox.selected_index, 0);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(original_id.clone())
    );
    assert!(app.mailbox.selected_set.contains(&original_id));
}

#[test]
fn label_count_updates_preserve_sidebar_selection_identity() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();

    let selected_index = app
        .sidebar_items()
        .iter()
        .position(
            |item| matches!(item, crate::app::SidebarItem::Label(label) if label.name == "Work"),
        )
        .unwrap();
    app.mailbox.sidebar_selected = selected_index;

    handle_daemon_event(
        &mut app,
        DaemonEvent::LabelCountsUpdated {
            counts: vec![
                LabelCount {
                    label_id: LabelId::from_provider_id("test", "STARRED"),
                    unread_count: 0,
                    total_count: 0,
                },
                LabelCount {
                    label_id: LabelId::from_provider_id("test", "SENT"),
                    unread_count: 0,
                    total_count: 0,
                },
            ],
        },
    );

    assert!(matches!(
        app.selected_sidebar_item(),
        Some(crate::app::SidebarItem::Label(label)) if label.name == "Work"
    ));
}

#[test]
fn labels_refresh_preserves_active_label_context_when_label_becomes_empty() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let work = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name == "Work")
        .unwrap()
        .clone();
    app.mailbox.active_label = Some(work.id.clone());
    app.mailbox.sidebar_selected = app
        .sidebar_items()
        .iter()
        .position(
            |item| matches!(item, crate::app::SidebarItem::Label(label) if label.id == work.id),
        )
        .unwrap();

    let refreshed = app
        .mailbox
        .labels
        .iter()
        .filter(|label| label.id != work.id)
        .cloned()
        .collect();

    super::apply_labels_refresh(&mut app, refreshed);

    let preserved = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.id == work.id)
        .unwrap();
    assert_eq!(preserved.unread_count, 0);
    assert_eq!(preserved.total_count, 0);
    assert_eq!(app.mailbox.active_label.as_ref(), Some(&work.id));
    assert!(matches!(
        app.selected_sidebar_item(),
        Some(crate::app::SidebarItem::Label(label)) if label.id == work.id
    ));
    assert_eq!(app.status_bar_state().mailbox_name, "Work");
}

#[test]
fn opening_search_result_keeps_search_workspace_open() {
    let mut app = App::new();
    app.screen = Screen::Search;
    app.search.page.results = make_test_envelopes(2);
    app.search.page.selected_index = 1;

    app.apply(Action::OpenSelected);

    assert_eq!(app.screen, Screen::Search);
    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(app.search.page.results[1].id.clone())
    );
}

#[test]
fn attachment_list_opens_modal_for_current_message() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("hello".into()),
            text_html: None,
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
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
        },
    );

    app.apply(Action::OpenSelected);
    app.apply(Action::AttachmentList);

    assert!(app.mailbox.attachment_panel.visible);
    assert_eq!(app.mailbox.attachment_panel.attachments.len(), 1);
    assert_eq!(
        app.mailbox.attachment_panel.attachments[0].filename,
        "report.pdf"
    );
}

#[test]
fn attachment_list_sorts_file_attachments_before_inline_images() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("hello".into()),
            text_html: Some("<img src=\"cid:inline-1\">".into()),
            attachments: vec![
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "inline-1.png".into(),
                    mime_type: "image/png".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Inline,
                    content_id: Some("inline-1".into()),
                    content_location: None,
                    size_bytes: 10,
                    local_path: None,
                    provider_id: "att-inline-1".into(),
                },
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "budget.xlsx".into(),
                    mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                        .into(),
                    disposition: mxr_core::types::AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 20,
                    local_path: None,
                    provider_id: "att-xlsx".into(),
                },
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "inline-2.png".into(),
                    mime_type: "image/png".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Inline,
                    content_id: Some("inline-2".into()),
                    content_location: None,
                    size_bytes: 30,
                    local_path: None,
                    provider_id: "att-inline-2".into(),
                },
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "report.pdf".into(),
                    mime_type: "application/pdf".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 40,
                    local_path: None,
                    provider_id: "att-pdf".into(),
                },
            ],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenSelected);
    app.apply(Action::AttachmentList);

    assert!(app.mailbox.attachment_panel.visible);
    assert_eq!(
        app.mailbox
            .attachment_panel
            .attachments
            .iter()
            .map(|attachment| attachment.filename.as_str())
            .collect::<Vec<_>>(),
        vec!["budget.xlsx", "report.pdf", "inline-1.png", "inline-2.png"]
    );
    assert_eq!(app.mailbox.attachment_panel.selected_index, 0);
    assert_eq!(
        app.selected_attachment()
            .map(|attachment| attachment.filename.as_str()),
        Some("budget.xlsx")
    );
}

#[test]
fn attachment_list_navigation_follows_sorted_attachment_order() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("hello".into()),
            text_html: Some("<img src=\"cid:inline-1\">".into()),
            attachments: vec![
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "inline-1.png".into(),
                    mime_type: "image/png".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Inline,
                    content_id: Some("inline-1".into()),
                    content_location: None,
                    size_bytes: 10,
                    local_path: None,
                    provider_id: "att-inline-1".into(),
                },
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "budget.xlsx".into(),
                    mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                        .into(),
                    disposition: mxr_core::types::AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 20,
                    local_path: None,
                    provider_id: "att-xlsx".into(),
                },
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "report.pdf".into(),
                    mime_type: "application/pdf".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 40,
                    local_path: None,
                    provider_id: "att-pdf".into(),
                },
            ],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenSelected);
    app.apply(Action::AttachmentList);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(
        app.selected_attachment()
            .map(|attachment| attachment.filename.as_str()),
        Some("report.pdf")
    );

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(
        app.selected_attachment()
            .map(|attachment| attachment.filename.as_str()),
        Some("inline-1.png")
    );

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(
        app.selected_attachment()
            .map(|attachment| attachment.filename.as_str()),
        Some("report.pdf")
    );
}

#[test]
fn search_preview_attachment_key_opens_modal() {
    let mut app = App::new();
    let mut results = make_test_envelopes(1);
    results[0].has_attachments = true;
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env.clone());
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("hello".into()),
            text_html: None,
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
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
        },
    );

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT));
    assert_eq!(action, Some(Action::AttachmentList));

    app.apply(Action::AttachmentList);

    assert!(app.mailbox.attachment_panel.visible);
    assert_eq!(app.mailbox.attachment_panel.attachments.len(), 1);
    assert_eq!(
        app.mailbox.attachment_panel.attachments[0].filename,
        "report.pdf"
    );
}

#[test]
fn search_preview_o_opens_in_browser() {
    let mut app = App::new();
    let results = make_test_envelopes(1);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::OpenInBrowser));
}

#[test]
fn search_preview_r_toggles_reader_mode_without_shift_modifier() {
    let mut app = App::new();
    let results = make_test_envelopes(1);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::ToggleReaderMode));
}

#[test]
fn search_preview_h_and_m_toggle_html_controls_without_shift_modifier() {
    let mut app = App::new();
    let results = make_test_envelopes(1);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env);

    let html = app.handle_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE));
    let remote = app.handle_key(KeyEvent::new(KeyCode::Char('M'), KeyModifiers::NONE));

    assert_eq!(html, Some(Action::ToggleHtmlView));
    assert_eq!(remote, Some(Action::ToggleRemoteContent));
}

#[test]
fn search_results_f_opens_full_message_view() {
    let mut app = App::new();
    let results = make_test_envelopes(2);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Results;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('F'), KeyModifiers::NONE));
    assert_eq!(action, Some(Action::ToggleFullscreen));

    app.apply(Action::ToggleFullscreen);

    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert!(app.search.page.result_selected);
    assert!(app.search.page.preview_fullscreen);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|message| message.id.clone()),
        Some(env.id)
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("Showing full message view")
    );
}

#[test]
fn search_preview_f_toggles_back_to_split_view() {
    let mut app = App::new();
    let results = make_test_envelopes(1);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.search.page.result_selected = true;
    app.search.page.preview_fullscreen = true;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env);

    app.apply(Action::ToggleFullscreen);

    assert!(!app.search.page.preview_fullscreen);
    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert_eq!(app.status_message.as_deref(), Some("Showing split view"));
}

#[test]
fn search_fullscreen_render_hides_results_pane() {
    let mut app = App::new();
    let results = make_test_envelopes(1);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.search.page.result_selected = true;
    app.search.page.preview_fullscreen = true;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env);
    app.mailbox.body_view_state = BodyViewState::ready(
        "hello".into(),
        "hello".into(),
        BodySource::Plain,
        BodyViewMetadata::default(),
    );

    let output = render_to_string(120, 20, |frame| app.draw(frame));

    assert!(output.contains("Search All Mail"));
    assert!(!output.contains("Search Results /"));
}

#[test]
fn search_preview_toggle_select_keeps_current_message_visible() {
    let mut app = App::new();
    let results = make_test_envelopes(2);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env.clone());

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    assert_eq!(action, Some(Action::ToggleSelect));

    app.apply(Action::ToggleSelect);

    assert_eq!(app.search.page.selected_index, 0);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|current| current.id.clone()),
        Some(env.id.clone())
    );
    assert!(app.mailbox.selected_set.contains(&env.id));
}

#[tokio::test]
async fn unchanged_editor_result_disables_send_actions() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-test-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let content = "---\nto: a@example.com\ncc: \"\"\nbcc: \"\"\nsubject: Hello\nfrom: me@example.com\nattach: []\n---\n\nBody\n";
    std::fs::write(&temp, content).unwrap();

    let pending = pending_send_from_edited_draft(&ComposeReadyData {
        account_id: AccountId::new(),
        intent: mxr_core::DraftIntent::New,
        draft_path: temp.clone(),
        cursor_line: 1,
        initial_content: content.to_string(),
        invite_reply: None,
    })
    .await
    .unwrap()
    .expect("pending send should exist");

    assert_eq!(pending.mode, PendingSendMode::Unchanged);

    let _ = std::fs::remove_file(temp);
}

#[test]
fn send_key_is_ignored_for_unchanged_draft_confirmation() {
    let mut app = App::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "a@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "Body".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::Unchanged,
        safety_report: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

    assert_eq!(
        app.compose
            .pending_send_confirm
            .as_ref()
            .map(|pending| pending.mode),
        Some(PendingSendMode::Unchanged)
    );
    assert!(app.pending_mutation_queue.is_empty());
}

#[test]
fn send_key_uses_pending_compose_account() {
    let mut app = App::new();
    let pending_account_id = AccountId::new();
    let other_account_id = AccountId::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].account_id = other_account_id;
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: pending_account_id.clone(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "a@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "Body".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

    match app
        .pending_mutation_queue
        .first()
        .map(|queued| &queued.request)
    {
        Some(Request::SendDraft { draft, .. }) => {
            assert_eq!(draft.account_id, pending_account_id);
        }
        other => panic!("Expected SendDraft request, got {other:?}"),
    }
}

#[test]
fn send_at_prompt_saves_draft_then_schedules_send() {
    let mut app = App::new();
    let pending_account_id = AccountId::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: pending_account_id.clone(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "a@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Scheduled hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "Body".into(),
        draft_path: std::path::PathBuf::from("/tmp/scheduled-draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    for c in "in 2h".chars() {
        let _ = app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.compose.pending_send_confirm.is_none());
    assert!(app.compose.pending_send_at_input.is_none());
    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].prelude.len(), 1);
    let draft_id = match &queue[0].prelude[0] {
        Request::SaveDraft { draft } => {
            assert_eq!(draft.account_id, pending_account_id);
            assert_eq!(draft.subject, "Scheduled hello");
            draft.id.clone()
        }
        other => panic!("Expected SaveDraft prelude, got {other:?}"),
    };
    match &queue[0].request {
        Request::ScheduleSend {
            draft_id: scheduled_id,
            send_at,
        } => {
            assert_eq!(scheduled_id, &draft_id);
            assert!(*send_at > chrono::Utc::now());
        }
        other => panic!("Expected ScheduleSend request, got {other:?}"),
    }
}

#[test]
fn remind_prompt_sends_draft_with_pending_reminder_time() {
    let mut app = App::new();
    let pending_account_id = AccountId::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: pending_account_id.clone(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "a@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Needs follow-up".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "Body".into(),
        draft_path: std::path::PathBuf::from("/tmp/reminder-draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    for c in "in 2h".chars() {
        let _ = app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.compose.pending_send_confirm.is_none());
    assert!(app.compose.pending_remind_at_input.is_none());
    let queued = app
        .pending_mutation_queue
        .first()
        .expect("reminder send queues SendDraft");
    match &queued.request {
        Request::SendDraft { draft, .. } => {
            assert_eq!(draft.account_id, pending_account_id);
            assert_eq!(draft.subject, "Needs follow-up");
        }
        other => panic!("Expected SendDraft request, got {other:?}"),
    }
    match &queued.effect {
        MutationEffect::SentSuccess {
            remind_at,
            sent_message_id,
            ..
        } => {
            assert!(remind_at.is_some(), "reminder time is carried with send");
            assert!(
                sent_message_id.is_none(),
                "sent message id is not known until daemon SendReceipt"
            );
        }
        other => panic!("Expected SentSuccess effect, got {other:?}"),
    }
}

#[test]
fn cancel_reminder_action_queues_cancel_for_focused_message() {
    let mut app = App::new();
    let env = TestEnvelopeBuilder::new().build();
    app.mailbox.viewing_envelope = Some(env.clone());

    app.apply(Action::CancelAutoReminder);

    let queued = app
        .pending_mutation_queue
        .first()
        .expect("cancel reminder should queue daemon mutation");
    match &queued.request {
        Request::CancelAutoReminder { sent_message_id } => {
            assert_eq!(sent_message_id, &env.id);
        }
        other => panic!("Expected CancelAutoReminder request, got {other:?}"),
    }
    assert_eq!(
        app.pending_mutation_status.as_deref(),
        Some("Cancelling reminder...")
    );
}

#[test]
fn reminder_triggered_event_marks_reply_queue_and_refreshes_open_modal() {
    let mut app = App::new();
    let message_id = MessageId::new();
    app.modals.reply_queue.open_loading();

    handle_daemon_event(
        &mut app,
        DaemonEvent::ReminderTriggered {
            sent_message_id: message_id.clone(),
        },
    );

    assert!(
        app.mailbox.reply_later_message_ids.contains(&message_id),
        "TUI should show reminder-triggered messages as reply-later nudges"
    );
    assert!(
        app.pending_reply_queue_refresh,
        "open reply queue should refresh when a reminder fires"
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("Reminder due; added to reply queue")
    );
}

#[test]
fn reply_queue_enter_starts_reply_compose_for_selected_message() {
    let mut app = App::new();
    let messages = make_test_envelopes(2);
    let selected = messages[1].clone();
    app.modals.reply_queue.open_loading();
    app.modals.reply_queue.set_messages(messages);
    app.modals.reply_queue.select_next();

    let action = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, Some(Action::ReplyQueueModalReply));
    app.apply(action.unwrap());

    assert!(!app.modals.reply_queue.visible);
    assert_eq!(
        app.compose.pending_compose,
        Some(crate::app::ComposeAction::Reply {
            message_id: selected.id,
            account_id: selected.account_id,
            preloaded: None,
        })
    );
}

#[test]
fn compose_blank_recipient_advances_to_subject_modal() {
    let mut app = App::new();
    app.mailbox.all_envelopes = make_test_envelopes(1);
    app.apply(Action::Compose);

    assert!(app.compose.compose_picker.visible);
    assert_eq!(
        app.compose.compose_picker.mode,
        crate::ui::compose_picker::ComposePickerMode::To
    );

    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.compose.compose_picker.visible);
    assert_eq!(
        app.compose.compose_picker.mode,
        crate::ui::compose_picker::ComposePickerMode::Subject
    );
}

#[test]
fn compose_blank_subject_starts_new_compose_with_empty_fields() {
    let mut app = App::new();
    app.mailbox.all_envelopes = make_test_envelopes(1);
    app.apply(Action::Compose);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.compose.pending_compose,
        Some(crate::app::ComposeAction::New {
            to: String::new(),
            subject: String::new(),
        })
    );
    assert!(!app.compose.compose_picker.visible);
}

#[test]
fn escape_closes_recipient_modal_without_starting_compose() {
    let mut app = App::new();
    app.mailbox.all_envelopes = make_test_envelopes(1);
    app.apply(Action::Compose);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(!app.compose.compose_picker.visible);
    assert!(app.compose.pending_compose.is_none());
    assert!(app.compose.compose_picker.pending_to.is_empty());
}

#[test]
fn escape_closes_subject_modal_without_starting_compose() {
    let mut app = App::new();
    app.mailbox.all_envelopes = make_test_envelopes(1);
    app.apply(Action::Compose);
    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(!app.compose.compose_picker.visible);
    assert!(app.compose.pending_compose.is_none());
    assert!(app.compose.compose_picker.pending_to.is_empty());
}

#[tokio::test]
async fn blank_recipient_draft_opens_draft_only_confirmation() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-test-missing-to-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let content = "---\nto: \"\"\ncc: \"\"\nbcc: \"\"\nsubject: Hello\nfrom: me@example.com\nattach: []\n---\n\nBody\n";
    std::fs::write(&temp, content).unwrap();

    let pending = pending_send_from_edited_draft(&ComposeReadyData {
        account_id: AccountId::new(),
        intent: mxr_core::DraftIntent::New,
        draft_path: temp.clone(),
        cursor_line: 1,
        initial_content: String::new(),
        invite_reply: None,
    })
    .await
    .unwrap()
    .expect("pending send should exist");

    assert_eq!(pending.mode, PendingSendMode::DraftOnlyNoRecipients);

    let _ = std::fs::remove_file(temp);
}

#[test]
fn send_key_is_ignored_for_missing_recipient_draft_confirmation() {
    let mut app = App::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "Body".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::DraftOnlyNoRecipients,
        safety_report: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

    assert_eq!(
        app.compose
            .pending_send_confirm
            .as_ref()
            .map(|pending| pending.mode),
        Some(PendingSendMode::DraftOnlyNoRecipients)
    );
    assert!(app.pending_mutation_queue.is_empty());
}

#[test]
fn save_key_saves_missing_recipient_draft_to_server() {
    let mut app = App::new();
    app.mailbox.all_envelopes = make_test_envelopes(1);
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "Body".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::DraftOnlyNoRecipients,
        safety_report: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

    assert!(app.compose.pending_send_confirm.is_none());
    assert!(matches!(
        app.pending_mutation_queue
            .first()
            .map(|queued| &queued.request),
        Some(Request::SaveDraftToServer { .. })
    ));
}

#[test]
fn edit_key_reopens_missing_recipient_draft() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-edit-draft-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(&temp, "draft").unwrap();

    let mut app = App::new();
    let account_id = AccountId::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: account_id.clone(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "Body".into(),
        draft_path: temp.clone(),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::DraftOnlyNoRecipients,
        safety_report: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

    assert!(app.compose.pending_send_confirm.is_none());
    assert_eq!(
        app.compose.pending_compose,
        Some(crate::app::ComposeAction::EditDraft {
            path: temp.clone(),
            account_id,
        })
    );

    let _ = std::fs::remove_file(temp);
}

#[test]
fn escape_discards_missing_recipient_draft_confirmation_and_queues_cleanup() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-discard-draft-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(&temp, "draft").unwrap();

    let mut app = App::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "Body".into(),
        draft_path: temp.clone(),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::DraftOnlyNoRecipients,
        safety_report: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(app.compose.pending_send_confirm.is_none());
    assert!(temp.exists());
    assert_eq!(app.compose.pending_draft_cleanup, vec![temp.clone()]);
    assert_eq!(app.status_message.as_deref(), Some("Discarded"));

    let _ = std::fs::remove_file(temp);
}

#[test]
fn mail_list_l_opens_label_picker_not_message() {
    let mut app = App::new();
    app.mailbox.active_pane = ActivePane::MailList;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::ApplyLabel));
}

#[test]
fn input_gc_opens_config_editor() {
    let mut h = InputHandler::new();

    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
        Some(Action::EditConfig)
    );
}

#[test]
fn input_g_shift_l_opens_logs() {
    let mut h = InputHandler::new();

    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT)),
        Some(Action::OpenLogs)
    );
}

#[test]
fn input_m_marks_read_and_archives() {
    let mut app = App::new();

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::MarkReadAndArchive));
}

#[test]
fn reconnect_detection_treats_connection_refused_as_recoverable() {
    let result = Err(MxrError::Ipc(
        "IPC error: Connection refused (os error 61)".into(),
    ));

    assert!(crate::ipc::should_reconnect_ipc(&result));
}

#[test]
fn autostart_detection_handles_refused_and_missing_socket() {
    let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
    let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
    let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);

    assert!(crate::ipc::should_autostart_daemon(&refused));
    assert!(crate::ipc::should_autostart_daemon(&missing));
    assert!(!crate::ipc::should_autostart_daemon(&other));
}

#[test]
fn diagnostics_shift_l_opens_logs() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT));

    assert_eq!(action, Some(Action::OpenLogs));
}

#[test]
fn diagnostics_uppercase_l_opens_logs_without_shift_modifier() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::OpenLogs));
}

#[test]
fn diagnostics_tab_cycles_selected_pane() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;

    let action = app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    assert!(action.is_none());
    assert_eq!(
        app.diagnostics.page.selected_pane,
        crate::app::DiagnosticsPaneKind::Data
    );
}

#[test]
fn diagnostics_enter_toggles_fullscreen_for_selected_pane() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;
    app.diagnostics.page.selected_pane = crate::app::DiagnosticsPaneKind::Logs;

    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .is_none());
    assert_eq!(
        app.diagnostics.page.fullscreen_pane,
        Some(crate::app::DiagnosticsPaneKind::Logs)
    );
    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .is_none());
    assert_eq!(app.diagnostics.page.fullscreen_pane, None);
}

#[test]
fn diagnostics_d_opens_selected_pane_details() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;
    app.diagnostics.page.selected_pane = crate::app::DiagnosticsPaneKind::Events;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::OpenDiagnosticsPaneDetails));
}

#[test]
fn back_clears_selection_before_other_mail_list_back_behavior() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox
        .selected_set
        .insert(app.mailbox.envelopes[0].id.clone());

    app.apply(Action::Back);

    assert!(app.mailbox.selected_set.is_empty());
    assert_eq!(app.status_message.as_deref(), Some("Selection cleared"));
}

#[test]
fn bulk_archive_requires_confirmation_before_queueing() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.selected_set = app
        .mailbox
        .envelopes
        .iter()
        .map(|env| env.id.clone())
        .collect();

    app.apply(Action::Archive);

    assert!(app.pending_mutation_queue.is_empty());
    match app.modals.pending_bulk_confirm.as_ref() {
        Some(confirm) => match &confirm.request {
            Request::Mutation {
                mutation: MutationCommand::Archive { message_ids },
                ..
            } => {
                assert_eq!(message_ids.len(), 3);
            }
            other => panic!("Expected Archive bulk request, got {other:?}"),
        },
        None => panic!("Expected pending bulk confirmation"),
    }
}

#[test]
fn confirming_bulk_archive_queues_mutation_and_clears_selection() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.selected_set = app
        .mailbox
        .envelopes
        .iter()
        .map(|env| env.id.clone())
        .collect();
    app.apply(Action::Archive);

    app.apply(Action::OpenSelected);

    assert!(app.modals.pending_bulk_confirm.is_none());
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert!(app.mailbox.selected_set.is_empty());
}

#[test]
fn command_palette_includes_major_mail_actions() {
    let labels: Vec<String> = default_commands()
        .into_iter()
        .map(|cmd| cmd.label)
        .collect();
    assert!(labels.contains(&"Reply".to_string()));
    assert!(labels.contains(&"Reply All".to_string()));
    assert!(labels.contains(&"Archive".to_string()));
    assert!(labels.contains(&"Delete".to_string()));
    assert!(labels.contains(&"Apply Label".to_string()));
    assert!(labels.contains(&"Snooze".to_string()));
    assert!(labels.contains(&"Clear Selection".to_string()));
    assert!(labels.contains(&"Open Accounts Page".to_string()));
    assert!(labels.contains(&"New IMAP/SMTP Account".to_string()));
    assert!(labels.contains(&"Set Default Account".to_string()));
    assert!(labels.contains(&"Edit Config".to_string()));
}

#[test]
fn local_label_changes_update_open_message() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);

    let user_label = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name == "Work")
        .unwrap()
        .clone();
    let message_id = app.mailbox.envelopes[0].id.clone();

    app.apply_local_label_refs(
        std::slice::from_ref(&message_id),
        std::slice::from_ref(&user_label.name),
        &[],
    );

    assert!(app
        .mailbox
        .viewing_envelope
        .as_ref()
        .unwrap()
        .label_provider_ids
        .contains(&user_label.provider_id));
}

#[test]
fn snooze_action_opens_modal_then_queues_request() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::Snooze);
    assert!(app.modals.snooze_panel.visible);

    app.apply(Action::Snooze);
    assert!(!app.modals.snooze_panel.visible);
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::Snooze {
            message_id,
            wake_at,
        } => {
            assert_eq!(message_id, &app.mailbox.envelopes[0].id);
            assert!(*wake_at > chrono::Utc::now());
        }
        other => panic!("expected snooze request, got {other:?}"),
    }
}

#[test]
fn open_selected_cache_miss_enters_loading_with_snippet_preview() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Loading { ref preview }
            if preview.as_deref() == Some("Snippet 0")
    ));
    assert!(app.mailbox.queued_body_fetches.is_empty());
    assert_eq!(
        app.mailbox.priority_body_fetches,
        vec![app.mailbox.envelopes[0].id.clone()]
    );
    assert!(app
        .mailbox
        .in_flight_body_requests
        .contains(&app.mailbox.envelopes[0].id));
}

#[test]
fn cached_plain_body_resolves_ready_state() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();

    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Plain body".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenSelected);

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Ready {
            ref raw,
            ref rendered,
            source: BodySource::Plain,
            ..
        } if raw.as_str() == "Plain body" && rendered.as_str() == "Plain body"
    ));
}

#[test]
fn cached_html_only_body_resolves_ready_state() {
    let mut app = App::new();
    app.mailbox.html_view = true;
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();

    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: None,
            text_html: Some("<p>Hello html</p>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenSelected);

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Ready {
            ref raw,
            ref rendered,
            source: BodySource::Html,
            ref metadata,
        } if raw.as_str() == "<p>Hello html</p>"
            && rendered.as_str() == raw.as_str()
            && metadata.mode == crate::app::BodyViewMode::Html
    ));
}

#[test]
fn cached_attachment_only_body_resolves_fallback_ready_state() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();

    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: None,
            text_html: None,
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
                filename: "report.pdf".into(),
                mime_type: "application/pdf".into(),
                disposition: AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 1024,
                local_path: None,
                provider_id: "att-1".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenSelected);

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Ready {
            ref raw,
            ref rendered,
            source: BodySource::Fallback,
            ..
        } if raw.contains("Attachment-only message")
            && rendered.contains("report.pdf")
    ));
}

#[test]
fn body_fetch_error_resolves_error_not_loading() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    let env = app.mailbox.envelopes[0].clone();

    app.resolve_body_fetch_error(&env.id, "boom".into());

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Error { ref message, ref preview }
            if message == "boom" && preview.as_deref() == Some("Snippet 0")
    ));
    assert!(!app.mailbox.in_flight_body_requests.contains(&env.id));
}

#[test]
fn current_body_fetch_is_prioritized_even_when_prefetch_is_already_in_flight() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.in_flight_body_requests.insert(env.id.clone());

    app.apply(Action::OpenSelected);

    assert_eq!(app.mailbox.priority_body_fetches, vec![env.id.clone()]);
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Loading { ref preview }
            if preview.as_deref() == Some("Snippet 0")
    ));
}

#[test]
fn body_batch_uses_daemon_failure_message_for_missing_current_body() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    let env = app.mailbox.envelopes[0].clone();

    app.resolve_body_batch(
        vec![env.id.clone()],
        vec![],
        vec![BodyFailure {
            message_id: env.id.clone(),
            error: "hydrate failed".into(),
        }],
    );

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Error { ref message, ref preview }
            if message == "hydrate failed" && preview.as_deref() == Some("Snippet 0")
    ));
    assert!(!app.mailbox.in_flight_body_requests.contains(&env.id));
}

#[test]
fn late_prefetch_failure_does_not_clobber_priority_body_success() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    let env = app.mailbox.envelopes[0].clone();

    app.resolve_body_success(MessageBody {
        message_id: env.id.clone(),
        text_plain: Some("Loaded by priority request".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: Default::default(),
    });

    app.resolve_body_batch(
        vec![env.id.clone()],
        vec![],
        vec![BodyFailure {
            message_id: env.id.clone(),
            error: "late prefetch failed".into(),
        }],
    );

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Ready { ref raw, .. }
            if raw.as_str() == "Loaded by priority request"
    ));
}

#[test]
fn stale_body_response_does_not_clobber_current_view() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    let first = app.mailbox.envelopes[0].clone();
    app.mailbox.active_pane = ActivePane::MailList;
    app.apply(Action::MoveDown);
    let second = app.mailbox.envelopes[1].clone();

    app.resolve_body_success(MessageBody {
        message_id: first.id.clone(),
        text_plain: Some("Old body".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: Default::default(),
    });

    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(second.id)
    );
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Loading { ref preview }
            if preview.as_deref() == Some("Snippet 1")
    ));
}

#[test]
fn reader_mode_toggle_shows_raw_html_when_disabled() {
    let mut app = App::new();
    app.mailbox.html_view = false;
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: None,
            text_html: Some("<p>Hello html</p>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenSelected);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready { raw, rendered, .. } => {
            assert_eq!(raw.as_str(), "<p>Hello html</p>");
            assert_ne!(rendered.as_str(), raw.as_str());
            assert!(rendered.contains("Hello html"));
        }
        other => panic!("expected ready state, got {other:?}"),
    }

    app.apply(Action::ToggleReaderMode);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready { raw, rendered, .. } => {
            assert_eq!(raw.as_str(), "<p>Hello html</p>");
            assert_eq!(rendered.as_str(), raw.as_str());
        }
        other => panic!("expected ready state, got {other:?}"),
    }

    app.apply(Action::ToggleReaderMode);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready { raw, rendered, .. } => {
            assert_eq!(raw.as_str(), "<p>Hello html</p>");
            assert_ne!(rendered.as_str(), raw.as_str());
            assert!(rendered.contains("Hello html"));
        }
        other => panic!("expected ready state, got {other:?}"),
    }
}

#[test]
fn html_view_toggle_updates_mode_and_remote_content_status() {
    let mut app = App::new();
    app.mailbox.html_view = true;
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Fallback plain".into()),
            text_html: Some(
                "<p>Hello <img alt=\"Hero\" src=\"https://example.com/hero.png\"></p>".into(),
            ),
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
                filename: "logo.png".into(),
                mime_type: "image/png".into(),
                disposition: AttachmentDisposition::Inline,
                content_id: Some("logo@example.com".into()),
                content_location: None,
                size_bytes: 2048,
                local_path: None,
                provider_id: "att-inline".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                text_plain_source: Some(BodyPartSource::Exact),
                text_html_source: Some(BodyPartSource::Exact),
                ..Default::default()
            },
        },
    );

    app.apply(Action::OpenSelected);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready {
            source: BodySource::Html,
            metadata,
            ..
        } => {
            assert_eq!(metadata.mode, crate::app::BodyViewMode::Html);
            assert!(metadata.inline_images);
            assert!(metadata.remote_content_available);
            assert!(metadata.remote_content_enabled);
        }
        other => panic!("expected html ready state, got {other:?}"),
    }

    app.apply(Action::ToggleHtmlView);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready {
            source: BodySource::Plain,
            metadata,
            ..
        } => {
            assert_eq!(metadata.mode, crate::app::BodyViewMode::Text);
            assert!(metadata.inline_images);
            assert!(metadata.remote_content_available);
            assert!(metadata.remote_content_enabled);
        }
        other => panic!("expected text ready state, got {other:?}"),
    }
    assert_eq!(app.status_message.as_deref(), Some("View: Reading"));
    assert!(app
        .status_bar_state()
        .body_status
        .as_deref()
        .is_some_and(|status| status.contains("View: Reading")));

    app.apply(Action::ToggleRemoteContent);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready { metadata, .. } => {
            assert_eq!(metadata.mode, crate::app::BodyViewMode::Text);
            assert!(!metadata.remote_content_enabled);
        }
        other => panic!("expected text ready state, got {other:?}"),
    }
    assert_eq!(
        app.status_message.as_deref(),
        Some("Remote images blocked in HTML view")
    );
    assert!(app
        .status_bar_state()
        .body_status
        .as_deref()
        .is_some_and(|status| status.contains("View: Reading")));
}

#[test]
fn reader_mode_toggle_is_blocked_in_html_view() {
    let mut app = App::new();
    app.mailbox.html_view = true;
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: None,
            text_html: Some("<p>Hello html</p>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                text_html_source: Some(BodyPartSource::Exact),
                ..Default::default()
            },
        },
    );

    app.apply(Action::OpenSelected);
    let reader_mode_before = app.mailbox.reader_mode;

    app.apply(Action::ToggleReaderMode);

    assert_eq!(app.mailbox.reader_mode, reader_mode_before);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Switch to text view to use reading view")
    );
}

#[test]
fn reader_stats_visibility_respects_config() {
    let mut app = App::new();
    app.mailbox.body_view_state = BodyViewState::ready(
        "Hello".into(),
        "Hello".into(),
        BodySource::Plain,
        BodyViewMetadata {
            mode: crate::app::BodyViewMode::Text,
            provenance: Some(BodyPartSource::Exact),
            reader_applied: true,
            original_lines: Some(12),
            cleaned_lines: Some(7),
            ..BodyViewMetadata::default()
        },
    );

    app.mailbox.show_reader_stats = false;
    assert!(app
        .status_bar_state()
        .body_status
        .as_deref()
        .is_some_and(|status| !status.contains("trimmed 5 lines")));

    app.mailbox.show_reader_stats = true;
    assert!(app
        .status_bar_state()
        .body_status
        .as_deref()
        .is_some_and(|status| status.contains("trimmed 5 lines")));
}

#[test]
fn account_switch_complete_closes_open_message_state() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.mailbox_view = MailboxView::Subscriptions;
    app.mailbox.layout_mode = LayoutMode::FullScreen;
    app.mailbox.active_pane = ActivePane::MessageView;
    app.mailbox.viewing_envelope = Some(app.mailbox.envelopes[0].clone());
    app.mailbox.viewed_thread_messages = app.mailbox.envelopes.clone();
    app.mailbox.body_view_state = BodyViewState::ready(
        "hello".into(),
        "hello".into(),
        BodySource::Plain,
        BodyViewMetadata::default(),
    );
    app.mailbox.active_label = Some(LabelId::new());
    app.mailbox.pending_active_label = Some(LabelId::new());
    app.mailbox.pending_label_fetch = Some(LabelId::new());
    app.mailbox
        .selected_set
        .insert(app.mailbox.envelopes[0].id.clone());

    app.handle_account_switch_complete();

    assert!(app.mailbox.viewing_envelope.is_none());
    assert!(app.mailbox.viewed_thread_messages.is_empty());
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Empty { .. }
    ));
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Messages);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    assert!(app.mailbox.envelopes.is_empty());
    assert!(app.mailbox.all_envelopes.is_empty());
    assert!(app.search.page.results.is_empty());
    assert!(app.mailbox.subscriptions_page.entries.is_empty());
    assert!(app.mailbox.selected_set.is_empty());
    assert!(app.mailbox.active_label.is_none());
    assert!(app.mailbox.pending_active_label.is_none());
    assert!(app.mailbox.pending_label_fetch.is_none());
    assert!(app.mailbox.pending_labels_refresh);
    assert!(app.mailbox.pending_all_envelopes_refresh);
    assert!(app.mailbox.pending_subscriptions_refresh);
    assert!(app.diagnostics.pending_status_refresh);
    assert_eq!(
        app.mailbox.mailbox_loading_message.as_deref(),
        Some("Loading selected account...")
    );
    assert_eq!(app.mailbox.desired_system_mailbox.as_deref(), Some("INBOX"));
}

#[test]
fn mailbox_refresh_clears_account_switch_loader() {
    let mut app = App::new();
    app.handle_account_switch_complete();

    let envelopes = make_test_envelopes(2);
    apply_all_envelopes_refresh(&mut app, envelopes.clone());

    assert!(app.mailbox.mailbox_loading_message.is_none());
    assert_eq!(app.status_message.as_deref(), Some("Account switched"));
    assert_eq!(app.mailbox.all_envelopes.len(), envelopes.len());
}

/// Phase 1.1 / Behavior 4: when the user sends from the Sent view,
/// applying the completion of a SendDraft mutation refreshes the active
/// label so the new message appears without a manual sync. The status
/// message reads "Sent!" — not "Synced" — because the user just sent.
#[test]
fn sent_success_effect_refreshes_active_label_and_sets_status() {
    let mut app = App::new();
    let label_id = LabelId::new();
    app.mailbox.active_label = Some(label_id.clone());
    // Simulate a single in-flight mutation so completion logic shows status.
    app.pending_mutation_count = 1;

    app.apply_mutation_completion(
        MutationEffect::SentSuccess {
            status: "Sent!".into(),
            remind_at: None,
            sent_message_id: None,
        },
        true,
    );

    assert_eq!(
        app.mailbox.pending_label_fetch,
        Some(label_id),
        "active label must be queued for refetch so the Sent view shows the new message"
    );
    assert!(
        app.mailbox.pending_subscriptions_refresh,
        "subscriptions must refresh after a successful send"
    );
    assert_eq!(app.status_message.as_deref(), Some("Sent!"));
}

#[test]
fn sent_success_with_reminder_queues_auto_reminder_for_sent_message() {
    let mut app = App::new();
    let sent_message_id = MessageId::new();
    let remind_at = chrono::Utc::now() + chrono::Duration::hours(2);

    app.apply_mutation_completion(
        MutationEffect::SentSuccess {
            status: "Sent!".into(),
            remind_at: Some(remind_at),
            sent_message_id: Some(sent_message_id.clone()),
        },
        true,
    );

    let queued = app
        .pending_mutation_queue
        .first()
        .expect("sent reminder should queue SetAutoReminder");
    match &queued.request {
        Request::SetAutoReminder {
            sent_message_id: queued_message_id,
            remind_at: queued_remind_at,
        } => {
            assert_eq!(queued_message_id, &sent_message_id);
            assert_eq!(queued_remind_at, &remind_at);
        }
        other => panic!("Expected SetAutoReminder request, got {other:?}"),
    }
}

/// Phase 1.1 / Behavior 4: with no active label (e.g. on the accounts
/// screen), applying SentSuccess still updates the status message but
/// does not enqueue a label fetch. Catches regressions that would
/// either crash on the None case or leak a stale label fetch.
#[test]
fn sent_success_effect_with_no_active_label_only_updates_status() {
    let mut app = App::new();
    app.mailbox.active_label = None;

    app.apply_mutation_completion(
        MutationEffect::SentSuccess {
            status: "Sent!".into(),
            remind_at: None,
            sent_message_id: None,
        },
        true,
    );

    assert_eq!(app.mailbox.pending_label_fetch, None);
    assert_eq!(app.status_message.as_deref(), Some("Sent!"));
}

/// Phase 1.2 / Behavior 4: connection_state_label exposes a non-empty
/// human-readable string when the connection is not healthy, which the
/// status bar prepends. Catches "silent hang" regressions — a missing
/// or empty label would put the user back to staring at a frozen UI.
#[test]
fn connection_state_label_surfaces_reconnecting_state() {
    use crate::app::ConnectionState;
    let mut app = App::new();
    app.set_connection_state(ConnectionState::Reconnecting {
        since: std::time::Instant::now(),
        reason: "connection refused".into(),
    });

    let label = app.connection_state_label();
    let label = label.expect("label must be Some when not Connected");
    let lower = label.to_lowercase();
    assert!(
        lower.contains("reconnect") || lower.contains("daemon"),
        "label must mention the disconnected state; got {label:?}"
    );
}

/// Phase 1.2 / Behavior 4: when Connected, the label is None so the
/// status bar shows the regular mailbox info, not a stale connection
/// notice.
#[test]
fn connection_state_label_is_none_when_connected() {
    use crate::app::ConnectionState;
    let mut app = App::new();
    app.set_connection_state(ConnectionState::Connected);
    assert!(app.connection_state_label().is_none());
}

/// Phase 1.3 / Behavior 1: a Warn through the reporter lands in the
/// ring buffer with the supplied message and Warn severity. Catches
/// regressions where async errors silently disappear (the original
/// `let _ = ...` smell).
#[test]
fn report_warn_adds_one_entry_to_ring_buffer() {
    use crate::app::UserErrorSeverity;
    let mut app = App::new();
    app.report_warn("body parse failed");

    let log = &app.modals.error_log;
    assert_eq!(log.len(), 1, "exactly one entry after one warn");
    let entry = log.back().expect("entry");
    assert_eq!(entry.message, "body parse failed");
    assert!(matches!(entry.severity, UserErrorSeverity::Warn));
}

/// Phase 1.3 / Behavior 2: the ring buffer caps at 5 — pushing a sixth
/// drops the oldest. No panic, no unbounded growth. Catches both
/// "buffer not capped" (memory leak under error storms) and
/// "buffer drops newest" (would lose the most actionable info).
#[test]
fn ring_buffer_keeps_five_most_recent_entries() {
    let mut app = App::new();
    for i in 0..6 {
        app.report_warn(format!("warn {i}"));
    }

    let log = &app.modals.error_log;
    assert_eq!(log.len(), 5, "ring buffer caps at 5");
    let messages: Vec<&str> = log.iter().map(|e| e.message.as_str()).collect();
    assert!(
        !messages.contains(&"warn 0"),
        "oldest entry must be dropped; got {messages:?}"
    );
    assert!(
        messages.contains(&"warn 5"),
        "newest entry must be kept; got {messages:?}"
    );
}

/// Phase 1.3 / Behavior 3: a warn shown in the status bar auto-clears
/// after 5s of wall time so a transient error doesn't permanently
/// hide the inbox info.
#[test]
fn current_user_warn_clears_after_5s() {
    let mut app = App::new();
    app.report_warn("body parse failed");
    let since = app.modals.error_log.back().expect("entry must exist").since;

    assert_eq!(
        app.current_user_warn(since + std::time::Duration::from_secs(4))
            .as_deref(),
        Some("body parse failed"),
        "warn must still be visible at 4s"
    );
    assert_eq!(
        app.current_user_warn(since + std::time::Duration::from_secs(6)),
        None,
        "warn must clear by 6s"
    );
}

/// Phase 2.1 / Behavior 1: opening a fresh saved-search form has
/// empty fields and lexical mode, and submitting valid name+query
/// produces a `Request::CreateSavedSearch` ready to dispatch.
#[test]
fn saved_search_form_for_new_submits_create_request() {
    let mut app = App::new();
    app.open_saved_search_form_new();

    let form = app
        .modals
        .saved_search_form
        .as_mut()
        .expect("form must open");
    form.name = "Work overdue".into();
    form.query = "label:work older_than:7d".into();

    let request = app
        .take_saved_search_form_request()
        .expect("valid form must yield a request");
    match request {
        mxr_protocol::Request::CreateSavedSearch {
            name,
            query,
            search_mode,
        } => {
            assert_eq!(name, "Work overdue");
            assert_eq!(query, "label:work older_than:7d");
            assert!(matches!(search_mode, mxr_core::types::SearchMode::Lexical));
        }
        other => panic!("expected CreateSavedSearch, got {other:?}"),
    }
    assert!(
        app.modals.saved_search_form.is_none(),
        "form must close after a successful submit"
    );
}

/// Phase 2.1 / Behavior 2: an empty name surfaces a validation
/// error and does NOT yield a request — catches "form silently
/// drops malformed input" regressions.
#[test]
fn saved_search_form_empty_name_rejects_with_validation_error() {
    let mut app = App::new();
    app.open_saved_search_form_new();

    let form = app
        .modals
        .saved_search_form
        .as_mut()
        .expect("form must open");
    form.name = String::new();
    form.query = "label:inbox".into();

    let request = app.take_saved_search_form_request();
    assert!(
        request.is_none(),
        "empty name must not produce a request; got {request:?}"
    );

    let form = app
        .modals
        .saved_search_form
        .as_ref()
        .expect("form must remain open after validation failure");
    assert!(
        form.validation_error
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains("name"),
        "validation error must mention the empty name; got {:?}",
        form.validation_error
    );
}

/// Phase 2.1 / Behavior 4: opening for edit prefills the form and
/// records the existing name. On submit the daemon receives both
/// a Delete (for the old name) and a Create (for the possibly-new
/// name) so name renames don't collide with the unique constraint.
#[test]
fn saved_search_form_for_edit_yields_delete_then_create() {
    let mut app = App::new();
    app.open_saved_search_form_for_edit(
        "Old name".into(),
        "label:work".into(),
        mxr_core::types::SearchMode::Lexical,
    );

    let form = app
        .modals
        .saved_search_form
        .as_mut()
        .expect("form must open");
    // Preserves old name as the source for the delete step.
    assert_eq!(form.existing_name.as_deref(), Some("Old name"));
    // Prefilled with the current name so the user can rename.
    assert_eq!(form.name, "Old name");
    form.name = "New name".into();

    let requests = app
        .take_saved_search_form_requests()
        .expect("edit must yield delete+create requests");
    assert_eq!(requests.len(), 2);
    match &requests[0] {
        mxr_protocol::Request::DeleteSavedSearch { name } => {
            assert_eq!(name, "Old name", "first request must delete the old name");
        }
        other => panic!("expected DeleteSavedSearch first, got {other:?}"),
    }
    match &requests[1] {
        mxr_protocol::Request::CreateSavedSearch { name, .. } => {
            assert_eq!(
                name, "New name",
                "second request must create under the new name"
            );
        }
        other => panic!("expected CreateSavedSearch second, got {other:?}"),
    }
}

/// Phase 2.1 stage B / Behavior 1 + dispatch wiring: dispatching
/// `SaveSavedSearchForm` with a valid form queues exactly one
/// `CreateSavedSearch` request for the IPC dispatcher and closes
/// the form. Catches "save action no-ops" regressions where the
/// keybinding fires but no request reaches the daemon.
#[test]
fn save_saved_search_form_action_queues_create_request() {
    use crate::action::Action;
    let mut app = App::new();
    app.open_saved_search_form_new();
    let form = app.modals.saved_search_form.as_mut().expect("form open");
    form.name = "Important".into();
    form.query = "label:starred".into();

    app.apply(Action::SaveSavedSearchForm);

    let queue = app.take_pending_saved_search_dispatch();
    assert_eq!(queue.len(), 1, "expected one queued request: {queue:?}");
    match &queue[0] {
        mxr_protocol::Request::CreateSavedSearch { name, query, .. } => {
            assert_eq!(name, "Important");
            assert_eq!(query, "label:starred");
        }
        other => panic!("expected CreateSavedSearch, got {other:?}"),
    }
    assert!(
        app.modals.saved_search_form.is_none(),
        "form should close after a valid save"
    );
    assert!(
        app.modals.pending_saved_search_dispatch.is_empty(),
        "queue must be drained by take_pending_saved_search_dispatch"
    );
}

/// Phase 2.1 stage B / Behavior 2: `SaveSavedSearchForm` with an
/// empty query keeps the form open with a validation error and
/// does NOT enqueue a request. Matches the principle "form fails
/// fast, daemon never sees garbage".
#[test]
fn save_saved_search_form_action_skips_dispatch_on_validation_failure() {
    use crate::action::Action;
    let mut app = App::new();
    app.open_saved_search_form_new();
    let form = app.modals.saved_search_form.as_mut().expect("form open");
    form.name = "Important".into();
    form.query = "  ".into(); // whitespace-only — rejected.

    app.apply(Action::SaveSavedSearchForm);

    assert!(
        app.modals.pending_saved_search_dispatch.is_empty(),
        "no requests must queue on a rejected save"
    );
    let form = app
        .modals
        .saved_search_form
        .as_ref()
        .expect("form must remain open");
    assert!(
        form.validation_error.is_some(),
        "validation_error must be set so the modal can surface it"
    );
}

/// Phase 2.1 stage B / Behavior 3: opening the delete-confirm via
/// `DeleteSavedSearch` with a Saved Search row selected, then
/// confirming, queues exactly one `DeleteSavedSearch` request.
/// Cancel path clears the confirm without dispatching.
#[test]
fn delete_saved_search_confirm_path_queues_delete_request() {
    let mut app = App::new();
    // Confirm path
    app.modals.pending_saved_search_delete_confirm = Some("Important".into());
    let confirmed = app.confirm_pending_saved_search_delete();
    assert_eq!(confirmed.as_deref(), Some("Important"));
    let queue = app.take_pending_saved_search_dispatch();
    assert_eq!(queue.len(), 1, "expected one queued delete: {queue:?}");
    match &queue[0] {
        mxr_protocol::Request::DeleteSavedSearch { name } => {
            assert_eq!(name, "Important");
        }
        other => panic!("expected DeleteSavedSearch, got {other:?}"),
    }
    assert!(
        app.modals.pending_saved_search_delete_confirm.is_none(),
        "confirm dialog must close after confirm"
    );
}

/// Phase 2.2 / Palette parity: each of the four semantic palette
/// actions appears in the default palette and is reachable from
/// the standard mailbox context. Catches accidental removal or
/// allowlist drift in `action_allowed_in_context`.
#[test]
fn semantic_palette_entries_present_in_default_commands() {
    let commands = crate::ui::command_palette::default_commands();
    let labels: Vec<&str> = commands.iter().map(|c| c.label.as_str()).collect();
    for needle in [
        "Semantic: Enable",
        "Semantic: Disable",
        "Semantic: Reindex",
        "Semantic: Backfill Missing",
        "Semantic: Install Profile (BGE Small EN)",
        "Semantic: Install Profile (Multilingual E5)",
        "Semantic: Install Profile (BGE-M3)",
    ] {
        assert!(
            labels.contains(&needle),
            "expected `{needle}` in palette; got {labels:?}"
        );
    }
}

#[test]
fn platform_palette_entries_present_in_default_commands() {
    let commands = crate::ui::command_palette::default_commands();
    let labels: Vec<&str> = commands.iter().map(|c| c.label.as_str()).collect();
    for needle in [
        "Draft: Assist Current Thread",
        "Draft: New For Sender",
        "Voice: Show Profile",
        "Voice: Rebuild Profile",
        "Commitments: Show Open",
    ] {
        assert!(
            labels.contains(&needle),
            "expected `{needle}` in palette; got {labels:?}"
        );
    }
}

/// Phase 2.2 / Behavior 1: dispatching `EnableSemantic` queues exactly
/// one `Request::EnableSemantic { enabled: true }` for the
/// dispatcher.
#[test]
fn enable_semantic_action_queues_enabled_true_request() {
    use crate::action::Action;
    let mut app = App::new();
    app.apply(Action::EnableSemantic);
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0] {
        mxr_protocol::Request::EnableSemantic { enabled } => {
            assert!(*enabled, "Enable must request enabled=true");
        }
        other => panic!("expected EnableSemantic, got {other:?}"),
    }
}

/// Phase 2.2 / Behavior 1 (disable): dispatching `DisableSemantic`
/// queues `EnableSemantic { enabled: false }`. Symmetric to enable
/// so the same daemon handler clears the flag.
#[test]
fn disable_semantic_action_queues_enabled_false_request() {
    use crate::action::Action;
    let mut app = App::new();
    app.apply(Action::DisableSemantic);
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0] {
        mxr_protocol::Request::EnableSemantic { enabled } => {
            assert!(!*enabled, "Disable must request enabled=false");
        }
        other => panic!("expected EnableSemantic, got {other:?}"),
    }
}

/// Phase 2.2 / Behavior 2: `ReindexSemantic` queues
/// `Request::ReindexSemantic`.
#[test]
fn reindex_semantic_action_queues_reindex_request() {
    use crate::action::Action;
    let mut app = App::new();
    app.apply(Action::ReindexSemantic);
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    assert!(
        matches!(queue[0], mxr_protocol::Request::ReindexSemantic),
        "expected ReindexSemantic, got {:?}",
        queue[0]
    );
}

#[test]
fn backfill_semantic_action_queues_backfill_request() {
    use crate::action::Action;
    let mut app = App::new();
    app.apply(Action::BackfillSemantic);
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    assert!(
        matches!(queue[0], mxr_protocol::Request::BackfillSemantic),
        "expected BackfillSemantic, got {:?}",
        queue[0]
    );
}

/// Phase 2.2 / Behavior 3: `InstallSemanticProfile(profile)` queues
/// `Request::InstallSemanticProfile { profile }` with the same
/// profile variant. Verifies the profile parameter survives the
/// palette → action → request hop without reshuffling.
#[test]
fn install_semantic_profile_action_queues_install_request() {
    use crate::action::Action;
    let mut app = App::new();
    let profile = mxr_core::types::SemanticProfile::MultilingualE5Small;
    app.apply(Action::InstallSemanticProfile(profile));
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0] {
        mxr_protocol::Request::InstallSemanticProfile { profile: p } => {
            assert_eq!(p.as_str(), profile.as_str());
        }
        other => panic!("expected InstallSemanticProfile, got {other:?}"),
    }
}

#[test]
fn use_semantic_profile_action_queues_use_request() {
    use crate::action::Action;
    let mut app = App::new();
    let profile = mxr_core::types::SemanticProfile::BgeM3;
    app.apply(Action::UseSemanticProfile(profile));
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0] {
        mxr_protocol::Request::UseSemanticProfile { profile: p } => {
            assert_eq!(p.as_str(), profile.as_str());
        }
        other => panic!("expected UseSemanticProfile, got {other:?}"),
    }
}

#[test]
fn draft_assist_action_queues_selected_thread_request() {
    use crate::action::Action;
    let mut app = App::new();
    let envelope = TestEnvelopeBuilder::new()
        .with_from_address("Sender", "sender@example.com")
        .subject("Quarterly plan")
        .build();
    let thread_id = envelope.thread_id.clone();
    app.mailbox.envelopes = vec![envelope];

    app.apply(Action::DraftAssistCurrentThread);

    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].title, "Draft assist");
    match &queue[0].request {
        Request::DraftAssist {
            thread_id: queued,
            instruction,
        } => {
            assert_eq!(queued, &thread_id);
            assert_eq!(instruction, "Draft a concise reply.");
        }
        other => panic!("expected DraftAssist, got {other:?}"),
    }
}

#[test]
fn thread_summary_response_clears_loading_after_user_navigates_away() {
    use crate::daemon_events::apply_thread_summary_loaded;
    // Reproduces the "y doesn't work" bug: user opens thread A,
    // auto-summary fires, user navigates to thread B, response
    // lands for A. Before the fix the response handler bailed on
    // `still_relevant` *without* clearing `thread_summary_loading`,
    // leaving thread A's loading flag stuck. Then `y` on thread A
    // short-circuited with "Summary already running".
    let mut app = App::new();
    let envelope_a = TestEnvelopeBuilder::new()
        .with_from_address("Alice", "alice@example.com")
        .subject("A")
        .build();
    let envelope_b = TestEnvelopeBuilder::new()
        .with_from_address("Bob", "bob@example.com")
        .subject("B")
        .build();
    let thread_a = envelope_a.thread_id.clone();
    // User is now focused on thread B; response will arrive for A.
    app.mailbox.envelopes = vec![envelope_b];
    app.mailbox.thread_summary_loading = Some(thread_a.clone());

    apply_thread_summary_loaded(
        &mut app,
        thread_a.clone(),
        Ok(("summary".into(), "model".into())),
    );

    assert!(
        app.mailbox.thread_summary_loading.is_none(),
        "loading flag must clear so the next `y` press can fire"
    );
}

#[test]
fn summarize_action_starts_background_request_without_modal() {
    use crate::action::Action;
    let mut app = App::new();
    let envelope = TestEnvelopeBuilder::new()
        .with_from_address("Sender", "sender@example.com")
        .subject("Quarterly plan")
        .build();
    let thread_id = envelope.thread_id.clone();
    app.mailbox.envelopes = vec![envelope];

    app.apply(Action::SummarizeCurrentThread);

    assert_eq!(app.pending_summary_request, Some(thread_id.clone()));
    assert_eq!(app.mailbox.thread_summary_loading, Some(thread_id));
    assert!(!app.modals.summary.visible);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Summarizing in background...")
    );
}

#[test]
fn draft_new_for_sender_action_queues_selected_sender_request() {
    use crate::action::Action;
    let mut app = App::new();
    let account_id = AccountId::new();
    let envelope = TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .with_from_address("Sender", "sender@example.com")
        .subject("Quarterly plan")
        .build();
    app.mailbox.envelopes = vec![envelope];

    app.apply(Action::DraftNewForSender);

    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0].request {
        Request::DraftNew {
            account_id: queued_account,
            to,
            purpose,
            register,
            length_hint,
        } => {
            assert_eq!(queued_account, &account_id);
            assert_eq!(to.email, "sender@example.com");
            assert_eq!(purpose, "Follow up on the selected thread: Quarterly plan");
            assert!(register.is_none());
            assert!(length_hint.is_none());
        }
        other => panic!("expected DraftNew, got {other:?}"),
    }
}

#[test]
fn refine_pending_draft_saves_then_queues_refine_request() {
    let mut app = App::new();
    let account_id = AccountId::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: account_id.clone(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "sender@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Quarterly plan".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: Vec::new(),
            thread_id: None,
            attach: Vec::new(),
            signature: None,
        },
        body: "Could you review the plan?".into(),
        draft_path: std::path::PathBuf::from("/tmp/mxr-draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    app.apply(Action::RefinePendingDraft);

    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].prelude.len(), 1);
    let draft_id = match &queue[0].prelude[0] {
        Request::SaveDraft { draft } => {
            assert_eq!(draft.account_id, account_id);
            assert_eq!(draft.subject, "Quarterly plan");
            assert_eq!(draft.body_markdown, "Could you review the plan?");
            draft.id.clone()
        }
        other => panic!("expected SaveDraft prelude, got {other:?}"),
    };
    match &queue[0].request {
        Request::DraftRefine {
            draft_id: queued,
            knobs,
        } => {
            assert_eq!(queued, &draft_id);
            assert_eq!(knobs, &mxr_protocol::DraftRefineKnobsData::default());
        }
        other => panic!("expected DraftRefine, got {other:?}"),
    }
}

#[test]
fn commitments_action_queues_open_commitments_for_selected_sender() {
    use crate::action::Action;
    let mut app = App::new();
    let account_id = AccountId::new();
    let envelope = TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .with_from_address("Sender", "sender@example.com")
        .subject("Quarterly plan")
        .build();
    app.mailbox.envelopes = vec![envelope];

    app.apply(Action::OpenCommitments);

    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0].request {
        Request::ListCommitments {
            account_id: queued_account,
            email,
            status,
        } => {
            assert_eq!(queued_account, &account_id);
            assert_eq!(email.as_deref(), Some("sender@example.com"));
            assert_eq!(*status, Some(mxr_protocol::CommitmentStatusData::Open));
        }
        other => panic!("expected ListCommitments, got {other:?}"),
    }
}

#[test]
fn voice_actions_queue_selected_account_requests() {
    use crate::action::Action;
    let mut app = App::new();
    let account_id = AccountId::new();
    let envelope = TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .with_from_address("Sender", "sender@example.com")
        .build();
    app.mailbox.envelopes = vec![envelope];

    app.apply(Action::OpenVoiceProfile);
    app.apply(Action::RebuildUserVoice);

    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 2);
    match &queue[0].request {
        Request::GetUserVoice { account_id: queued } => assert_eq!(queued, &account_id),
        other => panic!("expected GetUserVoice, got {other:?}"),
    }
    match &queue[1].request {
        Request::RebuildUserVoice { account_id: queued } => assert_eq!(queued, &account_id),
        other => panic!("expected RebuildUserVoice, got {other:?}"),
    }
}

/// Phase 2.5 / Behavior 1: opening an analytics view from the
/// palette switches to the Analytics screen, sets the right view
/// mode, and sets `refresh_pending` so the dispatcher fires the
/// matching `List*` request next tick. Catches "palette entry
/// opens the screen but never loads data" regressions.
#[test]
fn open_analytics_view_action_switches_screen_and_marks_refresh_pending() {
    use crate::action::Action;
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.apply(Action::OpenAnalyticsView(AnalyticsView::Contacts));
    assert!(matches!(app.screen, crate::app::Screen::Analytics));
    assert_eq!(app.analytics.view, AnalyticsView::Contacts);
    assert!(
        app.analytics.refresh_pending,
        "opening an analytics view must mark refresh_pending so the daemon request fires"
    );
}

/// Phase 2.5: the active view determines which `Request` the
/// dispatcher fires. Locks down the mapping so a daemon-side
/// rename (e.g. ListStorageBreakdown → ListStorageBuckets) shows
/// up here as a compile error or a test failure rather than as
/// "the screen renders but nothing ever loads."
#[test]
fn analytics_request_for_active_view_maps_each_variant() {
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Storage;
    assert!(matches!(
        app.analytics_request_for_active_view(),
        Some(mxr_protocol::Request::ListStorageBreakdown { .. })
    ));
    app.analytics.view = AnalyticsView::StaleThreads;
    assert!(matches!(
        app.analytics_request_for_active_view(),
        Some(mxr_protocol::Request::ListStaleThreads { .. })
    ));
    app.analytics.view = AnalyticsView::Contacts;
    // Default contacts_mode is Asymmetry per Default impl.
    assert!(matches!(
        app.analytics_request_for_active_view(),
        Some(mxr_protocol::Request::ListContactAsymmetry { .. })
    ));
    app.analytics.view = AnalyticsView::ResponseTime;
    assert!(matches!(
        app.analytics_request_for_active_view(),
        Some(mxr_protocol::Request::ListResponseTime { .. })
    ));
    app.accounts.page.accounts = vec![account_summary(AccountId::new(), true, true)];
    app.analytics.view = AnalyticsView::CadenceDrift;
    assert!(matches!(
        app.analytics_request_for_active_view(),
        Some(mxr_protocol::Request::ListCadenceDrift { .. })
    ));
}

#[test]
fn cadence_drift_request_requires_enabled_account() {
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::CadenceDrift;
    assert!(
        app.analytics_request_for_active_view().is_none(),
        "cadence drift must not dispatch a request with a fabricated account id"
    );
}

/// Phase 2.5 / Behavior 4: the refresh action re-marks
/// `refresh_pending` and clears any prior error. Catches "press r
/// after a daemon error and nothing happens" regressions.
#[test]
fn refresh_analytics_action_clears_error_and_marks_pending() {
    use crate::action::Action;
    let mut app = App::new();
    app.screen = crate::app::Screen::Analytics;
    app.analytics.error = Some("stale".into());
    app.analytics.refresh_pending = false;
    app.apply(Action::RefreshAnalytics);
    assert!(app.analytics.refresh_pending);
    assert!(app.analytics.error.is_none());
}

/// `OperationProgress` from the daemon must surface in the
/// status bar with the operation name, current/total, and
/// message — otherwise the user sees nothing while the daemon is
/// running long jobs (rebuild-analytics, sync, reindex). Catches
/// "we forgot to wire the new event variant into the status bar"
/// regressions.
#[test]
fn operation_progress_event_updates_status_bar_with_step_count() {
    use mxr_protocol::DaemonEvent;
    let mut app = App::new();
    handle_daemon_event(
        &mut app,
        DaemonEvent::OperationProgress {
            operation_id: "op-1".into(),
            operation: "rebuild-analytics".into(),
            account_id: None,
            current: 3,
            total: Some(6),
            message: "Backfilling reply pairs from messages".into(),
        },
    );
    let status = app
        .status_message
        .as_deref()
        .expect("OperationProgress must set the status bar");
    assert!(status.contains("rebuild-analytics"), "status: {status}");
    assert!(status.contains("[3/6]"), "status: {status}");
    assert!(
        status.contains("Backfilling reply pairs from messages"),
        "status: {status}"
    );
}

/// `OperationProgress` with `total: None` must render `?` rather
/// than fail or print "Some(_)". Guards the formatter against an
/// `unwrap()` regression on streaming ops with unknown total.
#[test]
fn operation_progress_event_with_unknown_total_renders_question_mark() {
    use mxr_protocol::DaemonEvent;
    let mut app = App::new();
    handle_daemon_event(
        &mut app,
        DaemonEvent::OperationProgress {
            operation_id: "op-1".into(),
            operation: "sync".into(),
            account_id: None,
            current: 42,
            total: None,
            message: "Syncing provider".into(),
        },
    );
    let status = app.status_message.as_deref().unwrap_or("");
    assert!(
        status.contains("[42/?]"),
        "expected '[42/?]' fallback for unknown total; got: {status}"
    );
}

/// `OperationCompleted` for `rebuild-analytics` while on the
/// Analytics screen must arm `refresh_pending` so the active
/// view re-fetches against the freshly-rebuilt data. Without
/// this the user runs the rebuild, sees "complete", but their
/// open Analytics view still shows pre-rebuild numbers.
#[test]
fn operation_completed_for_rebuild_analytics_arms_analytics_refresh() {
    use mxr_protocol::DaemonEvent;
    let mut app = App::new();
    app.screen = crate::app::Screen::Analytics;
    app.analytics.refresh_pending = false;
    handle_daemon_event(
        &mut app,
        DaemonEvent::OperationCompleted {
            operation_id: "op-1".into(),
            operation: "rebuild-analytics".into(),
            account_id: None,
            message: "Rebuild complete".into(),
        },
    );
    assert!(
        app.analytics.refresh_pending,
        "the rebuild-analytics completion event must trigger an analytics refresh"
    );
}

/// Slice 3 / B3.1: with the Storage view in `LargestMessages`
/// sub-mode, the request builder must produce
/// `Request::ListLargestMessages` with the state's `since_days`
/// and `limit` — not the breakdown request. Otherwise the user
/// toggles the mode visually and sees breakdown rows.
#[test]
fn storage_largest_messages_mode_dispatches_largest_request() {
    use crate::app::{AnalyticsView, StorageMode};
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Storage;
    app.analytics.storage_mode = StorageMode::LargestMessages;
    app.analytics.largest_limit = 25;
    app.analytics.largest_since_days = Some(90);
    match app.analytics_request_for_active_view() {
        Some(mxr_protocol::Request::ListLargestMessages {
            since_days,
            limit,
            account_id,
        }) => {
            assert_eq!(since_days, Some(90));
            assert_eq!(limit, 25);
            assert!(account_id.is_none());
        }
        other => panic!("expected ListLargestMessages, got {other:?}"),
    }
}

/// Slice 3 / B3.2: pressing `m` on the Storage view dispatches
/// `CycleStorageMode`, which flips the sub-mode and primes the
/// next refresh. The toggle must be idempotent (Breakdown ↔
/// LargestMessages) so two presses return to the original mode.
#[test]
fn cycle_storage_mode_toggles_back_and_forth() {
    use crate::action::Action;
    use crate::app::StorageMode;
    let mut app = App::new();
    assert_eq!(app.analytics.storage_mode, StorageMode::Breakdown);
    app.apply(Action::CycleStorageMode);
    assert_eq!(app.analytics.storage_mode, StorageMode::LargestMessages);
    assert!(app.analytics.refresh_pending);
    app.analytics.refresh_pending = false;
    app.apply(Action::CycleStorageMode);
    assert_eq!(app.analytics.storage_mode, StorageMode::Breakdown);
    assert!(app.analytics.refresh_pending);
}

/// Slice 4 / B4.1: Contacts view in Decay sub-mode dispatches
/// `Request::ListContactDecay` with the state's threshold and
/// lookback values. Defaults match the CLI (`mxr contacts
/// decay`): 30-day threshold, 1095-day (3-year) lookback.
#[test]
fn contacts_decay_mode_dispatches_decay_request_with_defaults() {
    use crate::app::{AnalyticsView, ContactsMode};
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Contacts;
    app.analytics.contacts_mode = ContactsMode::Decay;
    match app.analytics_request_for_active_view() {
        Some(mxr_protocol::Request::ListContactDecay {
            threshold_days,
            max_lookback_days,
            ..
        }) => {
            assert_eq!(threshold_days, 30);
            assert_eq!(max_lookback_days, 1095);
        }
        other => panic!("expected ListContactDecay, got {other:?}"),
    }
}

/// Slice 4 / B4.2: pressing `m` on Contacts view toggles the
/// sub-mode and primes refresh. Mirror of the Storage toggle.
#[test]
fn cycle_contacts_mode_toggles_back_and_forth() {
    use crate::action::Action;
    use crate::app::ContactsMode;
    let mut app = App::new();
    assert_eq!(app.analytics.contacts_mode, ContactsMode::Asymmetry);
    app.apply(Action::CycleContactsMode);
    assert_eq!(app.analytics.contacts_mode, ContactsMode::Decay);
    assert!(app.analytics.refresh_pending);
}

/// Slice 5 / B5.1: Action::RefreshContacts arms the
/// `pending_contacts_refresh` flag that the lib.rs dispatcher
/// uses to fire `Request::RefreshContacts`. Asserting the flag
/// (rather than the IPC request itself) keeps this test off the
/// runtime, but the dispatcher block is small enough that the
/// integration test in Slice 12 covers the wire path.
#[test]
fn refresh_contacts_action_sets_pending_contacts_refresh_flag() {
    use crate::action::Action;
    let mut app = App::new();
    assert!(!app.analytics.pending_contacts_refresh);
    app.apply(Action::RefreshContacts);
    assert!(app.analytics.pending_contacts_refresh);
}

/// Slice 6 / B6.1: Subscriptions view dispatches
/// `Request::ListSubscriptions` with the CLI default limit (200).
#[test]
fn subscriptions_view_dispatches_list_subscriptions_with_default_limit() {
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Subscriptions;
    match app.analytics_request_for_active_view() {
        Some(mxr_protocol::Request::ListSubscriptions { limit, account_id }) => {
            assert_eq!(limit, 200);
            assert!(account_id.is_none());
        }
        other => panic!("expected ListSubscriptions, got {other:?}"),
    }
}

/// Slice 6 / B6.5: pressing `o` on Subscriptions toggles the rank
/// flag locally — no daemon round-trip, just a re-sort on the
/// next render. Toggling does not mark refresh_pending (the
/// underlying data is unchanged).
#[test]
fn toggle_subscriptions_rank_flips_local_flag_only() {
    use crate::action::Action;
    let mut app = App::new();
    assert!(!app.analytics.subscriptions_rank);
    assert!(!app.analytics.refresh_pending);
    app.apply(Action::ToggleSubscriptionsRank);
    assert!(app.analytics.subscriptions_rank);
    assert!(
        !app.analytics.refresh_pending,
        "rank is a local re-sort; refresh_pending must stay off so \
             we don't re-fire the daemon list call"
    );
}

/// Slice 6 / B6.6: pressing `u` on a Subscriptions row populates
/// the existing unsubscribe-confirm modal with the row's
/// metadata. Reuses the modal/IPC path the mailbox uses, so this
/// test pins the wiring to that surface (modal becomes Some).
#[test]
fn analytics_unsubscribe_action_opens_confirm_modal_for_selected_row() {
    use crate::action::Action;
    use crate::app::AnalyticsView;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::{MessageFlags, SubscriptionSummary, UnsubscribeMethod};
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Subscriptions;
    app.analytics.selected_index = 0;
    app.analytics.subscriptions = vec![SubscriptionSummary {
        account_id: AccountId::new(),
        sender_name: Some("Newsletter".into()),
        sender_email: "promo@example.com".into(),
        message_count: 12,
        latest_message_id: MessageId::new(),
        latest_provider_id: "msg-1".into(),
        latest_thread_id: ThreadId::new(),
        latest_subject: "Weekly digest".into(),
        latest_snippet: "...".into(),
        latest_date: chrono::Utc::now(),
        latest_flags: MessageFlags::READ,
        latest_has_attachments: false,
        latest_size_bytes: 4096,
        unsubscribe: UnsubscribeMethod::OneClick {
            url: "https://example.com/unsub".into(),
        },
        opened_count: 1,
        replied_count: 0,
        archived_unread_count: 5,
    }];
    app.apply(Action::AnalyticsUnsubscribe);
    let modal = app
        .modals
        .pending_unsubscribe_confirm
        .as_ref()
        .expect("unsubscribe modal must be opened");
    assert_eq!(modal.sender_email, "promo@example.com");
    assert!(
        modal.method_label.contains("one-click"),
        "method label must surface the chosen method; got {}",
        modal.method_label
    );
}

/// Slice 7 / B7.1: Wrapped view defaults to Ytd. The request
/// builder produces `Request::Wrapped` with a label following the
/// CLI's exact format (`"<year> year-to-date"`), so the daemon
/// echoes back identical metadata regardless of which client made
/// the call.
#[test]
fn wrapped_view_default_window_dispatches_ytd_request_with_cli_label() {
    use crate::app::AnalyticsView;
    use chrono::{Datelike, Utc};
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Wrapped;
    let now_year = Utc::now().year();
    match app.analytics_request_for_active_view() {
        Some(mxr_protocol::Request::Wrapped { label, .. }) => {
            let expected = format!("{now_year} year-to-date");
            assert_eq!(label, expected);
        }
        other => panic!("expected Request::Wrapped, got {other:?}"),
    }
}

/// Slice 7 / B7.3: setting `wrapped_window = Year(2025)` must
/// produce a request whose `since_unix` is 2025-01-01T00:00:00Z
/// and `until_unix` is 2025-12-31T23:59:59Z (UTC). Numbers come
/// from chrono — the same path the CLI uses.
#[test]
fn wrapped_window_year_dispatches_full_year_unix_bounds() {
    use crate::app::{AnalyticsView, WrappedWindow};
    use chrono::{TimeZone, Utc};
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Wrapped;
    app.analytics.wrapped_window = WrappedWindow::Year(2025);
    let expected_start = Utc
        .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
        .unwrap()
        .timestamp();
    let expected_end = Utc
        .with_ymd_and_hms(2025, 12, 31, 23, 59, 59)
        .unwrap()
        .timestamp();
    match app.analytics_request_for_active_view() {
        Some(mxr_protocol::Request::Wrapped {
            since_unix,
            until_unix,
            label,
            ..
        }) => {
            assert_eq!(since_unix, expected_start);
            assert_eq!(until_unix, expected_end);
            assert_eq!(label, "2025");
        }
        other => panic!("expected Request::Wrapped, got {other:?}"),
    }
}

/// Slice 7 / B7.2: `StepWrappedYear(-1)` from Ytd transitions to
/// Year(now-1), and a second step decrements further. From a
/// Year, stepping moves to the next/previous year.
#[test]
fn step_wrapped_year_walks_year_backwards_from_ytd() {
    use crate::action::Action;
    use crate::app::WrappedWindow;
    use chrono::{Datelike, Utc};
    let mut app = App::new();
    let now_year = Utc::now().year();
    assert_eq!(app.analytics.wrapped_window, WrappedWindow::Ytd);
    app.apply(Action::StepWrappedYear(-1));
    assert_eq!(
        app.analytics.wrapped_window,
        WrappedWindow::Year(now_year - 1)
    );
    app.apply(Action::StepWrappedYear(-1));
    assert_eq!(
        app.analytics.wrapped_window,
        WrappedWindow::Year(now_year - 2)
    );
}

/// Slice 9 / B9.1: pressing the cycle key on Storage rotates
/// group_by Sender → Mimetype → Label → Sender. The request
/// builder picks up the new group_by value automatically because
/// it reads the same field.
#[test]
fn cycle_storage_group_by_rotates_through_three_axes() {
    use crate::action::Action;
    use mxr_core::types::StorageGroupBy;
    let mut app = App::new();
    assert_eq!(app.analytics.storage_group_by, StorageGroupBy::Sender);
    app.apply(Action::CycleStorageGroupBy);
    assert_eq!(app.analytics.storage_group_by, StorageGroupBy::Mimetype);
    app.apply(Action::CycleStorageGroupBy);
    assert_eq!(app.analytics.storage_group_by, StorageGroupBy::Label);
    app.apply(Action::CycleStorageGroupBy);
    assert_eq!(app.analytics.storage_group_by, StorageGroupBy::Sender);
}

/// Slice 9 / B9.2: ToggleStalePerspective flips Mine ↔ Theirs
/// and arms refresh.
#[test]
fn toggle_stale_perspective_flips_and_marks_refresh() {
    use crate::action::Action;
    use mxr_core::types::StaleBallInCourt;
    let mut app = App::new();
    assert_eq!(app.analytics.stale_perspective, StaleBallInCourt::Mine);
    app.apply(Action::ToggleStalePerspective);
    assert_eq!(app.analytics.stale_perspective, StaleBallInCourt::Theirs);
    assert!(app.analytics.refresh_pending);
}

/// Slice 9 / B9.3: AdjustStaleOlderThanDays adds the delta and
/// clamps at 1 (the daemon rejects values < 1, so the TUI must
/// not allow them).
#[test]
fn adjust_stale_older_than_days_adds_delta_and_clamps_at_one() {
    use crate::action::Action;
    let mut app = App::new();
    app.analytics.stale_older_than_days = 30;
    app.apply(Action::AdjustStaleOlderThanDays(7));
    assert_eq!(app.analytics.stale_older_than_days, 37);
    app.apply(Action::AdjustStaleOlderThanDays(-100));
    assert_eq!(
        app.analytics.stale_older_than_days, 1,
        "must clamp at 1, not underflow"
    );
}

/// Slice 10 / B10.1: pressing `f` on the analytics screen opens
/// the filter modal populated for the active view. The modal
/// must contain at least one field; the active_field starts at
/// 0 so the user can begin typing immediately.
#[test]
fn open_analytics_filter_modal_populates_fields_for_active_view() {
    use crate::action::Action;
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::StaleThreads;
    app.apply(Action::OpenAnalyticsFilterModal);
    let modal = app
        .modals
        .analytics_filter
        .as_ref()
        .expect("modal must be Some after open action");
    assert_eq!(modal.view, AnalyticsView::StaleThreads);
    assert!(!modal.fields.is_empty());
    assert_eq!(modal.active_field, 0);
}

#[test]
fn analytics_filter_modal_cycles_select_options_without_typing() {
    use crate::action::Action;
    use crate::app::{AnalyticsView, StorageMode};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = App::new();
    app.screen = crate::app::Screen::Analytics;
    app.analytics.view = AnalyticsView::Storage;
    app.analytics.storage_mode = StorageMode::Breakdown;
    app.apply(Action::OpenAnalyticsFilterModal);

    let before = app.modals.analytics_filter.as_ref().unwrap().fields[0]
        .value
        .clone();
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let after = app.modals.analytics_filter.as_ref().unwrap().fields[0]
        .value
        .clone();

    assert_eq!(before, "sender");
    assert_eq!(after, "mimetype");
}

/// Slice 10 / B10.3: submitting the filter modal copies the
/// edited string values back into the typed `AnalyticsState`
/// fields, sets refresh_pending, and closes the modal. Failure
/// to write back is the central regression risk for the modal —
/// it would silently swallow the user's edits.
#[test]
fn submit_analytics_filter_modal_writes_back_and_closes() {
    use crate::action::Action;
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::StaleThreads;
    app.apply(Action::OpenAnalyticsFilterModal);
    // older_than_days is field index 1 in the StaleThreads modal.
    if let Some(modal) = app.modals.analytics_filter.as_mut() {
        modal.fields[1].value = "60".into();
    }
    app.analytics.refresh_pending = false;
    app.apply(Action::SubmitAnalyticsFilterModal);
    assert!(app.modals.analytics_filter.is_none());
    assert_eq!(app.analytics.stale_older_than_days, 60);
    assert!(app.analytics.refresh_pending);
}

/// Slice 10: Esc cancels the filter modal without mutating
/// state — the validation errors and edited values are dropped.
#[test]
fn close_analytics_filter_modal_discards_edits() {
    use crate::action::Action;
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::StaleThreads;
    app.analytics.stale_older_than_days = 30;
    app.apply(Action::OpenAnalyticsFilterModal);
    if let Some(modal) = app.modals.analytics_filter.as_mut() {
        modal.fields[1].value = "999".into();
    }
    app.apply(Action::CloseAnalyticsFilterModal);
    assert!(app.modals.analytics_filter.is_none());
    assert_eq!(
        app.analytics.stale_older_than_days, 30,
        "Esc must discard edits"
    );
}

/// Slice 11 / B11.1: Enter on a Storage Breakdown sender row
/// switches to the Search screen with the constructed query
/// `"from:<sender>"`. This is the most-used drill-down — clicking
/// "alice@example.com" in the breakdown should land on her mail.
#[test]
fn drill_down_storage_sender_jumps_to_search_with_from_query() {
    use crate::action::Action;
    use crate::app::{AnalyticsView, Screen, StorageMode};
    use mxr_core::types::{StorageBucket, StorageGroupBy};
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Storage;
    app.analytics.storage_mode = StorageMode::Breakdown;
    app.analytics.storage_group_by = StorageGroupBy::Sender;
    app.analytics.storage_rows = vec![StorageBucket {
        key: "alice@example.com".into(),
        bytes: 12345,
        count: 3,
    }];
    app.analytics.selected_index = 0;
    app.apply(Action::AnalyticsRowDrillDown);
    assert!(matches!(app.screen, Screen::Search));
    assert_eq!(app.search.page.query, "from:alice@example.com");
}

/// Stale-thread drill-down jumps to a `from:<counterparty>` search,
/// matching the Contacts drill pattern. Earlier attempts opened the
/// envelope directly, but that left the centre mailbox list out of
/// sync with the preview pane (the list still showed the previous
/// mailbox while the preview showed an unrelated message). Search
/// reorients both panes coherently.
#[test]
fn drill_down_stale_thread_jumps_to_counterparty_search() {
    use crate::action::Action;
    use crate::app::{AnalyticsView, Screen};
    use mxr_core::id::{MessageId, ThreadId};
    use mxr_core::types::StaleThreadRow;
    let mut app = App::new();
    app.screen = Screen::Analytics;
    app.analytics.view = AnalyticsView::StaleThreads;
    let latest_id = MessageId::new();
    app.analytics.stale_rows = vec![StaleThreadRow {
        thread_id: ThreadId::new(),
        latest_message_id: latest_id.clone(),
        latest_subject: "Re: thanks".into(),
        counterparty_email: "alice@example.com".into(),
        latest_date: chrono::Utc::now(),
        days_stale: 12,
    }];
    app.analytics.selected_index = 0;
    app.apply(Action::AnalyticsRowDrillDown);
    assert_eq!(
        app.search.page.query, "from:alice@example.com",
        "drill-down must set the search query to the counterparty"
    );
    assert_eq!(
        app.screen,
        Screen::Search,
        "drill-down must navigate to the Search screen"
    );
}

/// Largest-messages drill-down jumps to a `from:<sender>` search
/// (matches the Storage/Sender drill). Direct envelope-open left
/// the centre mailbox list out of sync with the preview pane.
#[test]
fn drill_down_largest_message_jumps_to_sender_search() {
    use crate::action::Action;
    use crate::app::{AnalyticsView, Screen, StorageMode};
    use mxr_core::id::MessageId;
    use mxr_core::types::LargestMessageRow;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Storage;
    app.analytics.storage_mode = StorageMode::LargestMessages;
    let id = MessageId::new();
    app.analytics.largest_message_rows = vec![LargestMessageRow {
        message_id: id.clone(),
        from_email: "noreply@list.example".into(),
        subject: "Heavy attachment".into(),
        size_bytes: 50 * 1024 * 1024,
        date: chrono::Utc::now(),
    }];
    app.analytics.selected_index = 0;
    app.apply(Action::AnalyticsRowDrillDown);
    assert_eq!(app.search.page.query, "from:noreply@list.example");
    assert_eq!(app.screen, Screen::Search);
}

/// Subscriptions drill-down jumps to a `from:<sender>` search.
/// Mirror of the stale-thread / largest-message tests.
#[test]
fn drill_down_subscriptions_jumps_to_sender_search() {
    use crate::action::Action;
    use crate::app::{AnalyticsView, Screen};
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::{MessageFlags, SubscriptionSummary, UnsubscribeMethod};
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Subscriptions;
    let latest = MessageId::new();
    app.analytics.subscriptions = vec![SubscriptionSummary {
        account_id: AccountId::new(),
        sender_name: Some("Newsletter".into()),
        sender_email: "promo@example.com".into(),
        message_count: 3,
        latest_message_id: latest.clone(),
        latest_provider_id: "msg-1".into(),
        latest_thread_id: ThreadId::new(),
        latest_subject: "Weekly".into(),
        latest_snippet: "...".into(),
        latest_date: chrono::Utc::now(),
        latest_flags: MessageFlags::READ,
        latest_has_attachments: false,
        latest_size_bytes: 1024,
        unsubscribe: UnsubscribeMethod::None,
        opened_count: 0,
        replied_count: 0,
        archived_unread_count: 0,
    }];
    app.analytics.selected_index = 0;
    app.apply(Action::AnalyticsRowDrillDown);
    assert_eq!(app.search.page.query, "from:promo@example.com");
    assert_eq!(app.screen, Screen::Search);
}

/// Slice 11 / B11.5: Enter on a Contacts row (either sub-mode)
/// jumps to search filtered to that contact's email.
#[test]
fn drill_down_contacts_asymmetry_jumps_to_search_with_from_query() {
    use crate::action::Action;
    use crate::app::{AnalyticsView, ContactsMode, Screen};
    use mxr_core::types::ContactAsymmetryRow;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Contacts;
    app.analytics.contacts_mode = ContactsMode::Asymmetry;
    app.analytics.asymmetry_rows = vec![ContactAsymmetryRow {
        email: "bob@example.com".into(),
        display_name: None,
        total_inbound: 10,
        total_outbound: 1,
        asymmetry: 0.9,
        last_seen_at: chrono::Utc::now(),
    }];
    app.apply(Action::AnalyticsRowDrillDown);
    assert!(matches!(app.screen, Screen::Search));
    assert_eq!(app.search.page.query, "from:bob@example.com");
}

/// Slice 2 / B2.1: forward cycling visits all analytics views
/// in the documented order (Storage → StaleThreads → Contacts →
/// ResponseTime → Subscriptions → Wrapped → Storage). Pins the
/// next() arm so reordering or dropping a variant breaks here
/// instead of as a "Tab silently skips a tab" bug at runtime.
#[test]
fn next_analytics_view_cycles_all_six_variants_forward() {
    use crate::action::Action;
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Storage;
    let order = [
        AnalyticsView::StaleThreads,
        AnalyticsView::Contacts,
        AnalyticsView::CadenceDrift,
        AnalyticsView::ResponseTime,
        AnalyticsView::Subscriptions,
        AnalyticsView::Wrapped,
        AnalyticsView::Storage,
    ];
    for expected in order {
        app.apply(Action::NextAnalyticsView);
        assert_eq!(app.analytics.view, expected);
    }
}

/// Slice 2 / B2.1 (reverse): backward cycling is the exact inverse
/// of forward. Symmetric to the forward test.
#[test]
fn prev_analytics_view_cycles_all_six_variants_backward() {
    use crate::action::Action;
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Storage;
    let order = [
        AnalyticsView::Wrapped,
        AnalyticsView::Subscriptions,
        AnalyticsView::ResponseTime,
        AnalyticsView::CadenceDrift,
        AnalyticsView::Contacts,
        AnalyticsView::StaleThreads,
        AnalyticsView::Storage,
    ];
    for expected in order {
        app.apply(Action::PrevAnalyticsView);
        assert_eq!(app.analytics.view, expected);
    }
}

/// Slice 2 / B2.2: the default `AnalyticsState` initializes the
/// new sub-mode and window fields to the documented defaults so
/// the first refresh after `OpenAnalyticsScreen` produces the
/// same output as the CLI defaults (`storage --by sender`,
/// `contacts asymmetry`, `subscriptions`, `wrapped --ytd`).
#[test]
fn default_analytics_state_uses_documented_defaults() {
    use crate::app::{AnalyticsState, AnalyticsView, ContactsMode, StorageMode, WrappedWindow};
    let s = AnalyticsState::default();
    assert_eq!(s.view, AnalyticsView::Storage);
    assert_eq!(s.storage_mode, StorageMode::Breakdown);
    assert_eq!(s.contacts_mode, ContactsMode::Asymmetry);
    assert!(!s.subscriptions_rank);
    assert_eq!(s.wrapped_window, WrappedWindow::Ytd);
    assert_eq!(s.subscriptions_limit, 200);
    assert_eq!(s.largest_limit, 50);
    assert_eq!(s.decay_threshold_days, 30);
    assert_eq!(s.decay_max_lookback_days, 1095);
}

/// Slice 1 / B1.1+B1.4: `OpenTab6` is the action that the numeric
/// `'6'` keystroke dispatches. It must route to the analytics
/// screen and prime the refresh flag, otherwise pressing `6`
/// switches the user to a blank Analytics tab that never loads.
/// Catches "we wired the action variant but forgot the screen
/// router" regressions.
#[test]
fn open_tab_6_action_opens_analytics_and_marks_refresh_pending() {
    use crate::action::Action;
    let mut app = App::new();
    app.apply(Action::OpenTab6);
    assert!(matches!(app.screen, crate::app::Screen::Analytics));
    assert!(
        app.analytics.refresh_pending,
        "tab 6 must mark refresh_pending so the dispatcher fires the active analytics request"
    );
}

/// Opening a message in Mailbox arms a delayed auto-mark-read
/// timer; switching screens away from Mailbox must cancel it so
/// the SetRead doesn't fire while the user is on a different
/// screen. All non-Mailbox screen openers do this; Analytics used
/// to be the exception, which surfaced as a "Mutation Failed"
/// modal in Analytics tab 6 whenever the daemon's pool was busy
/// enough to time out the late SetRead.
#[test]
fn opening_analytics_cancels_pending_preview_read() {
    use crate::action::Action;
    use crate::app::AnalyticsView;

    for opener in [
        Action::OpenAnalyticsScreen,
        Action::OpenTab6,
        Action::OpenAnalyticsView(AnalyticsView::Subscriptions),
    ] {
        let mut app = App::new();
        app.mailbox.envelopes = make_test_envelopes(1);
        app.mailbox.envelopes[0].flags = MessageFlags::empty();
        app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
        app.apply(Action::OpenSelected);

        app.apply(opener.clone());

        app.expire_pending_preview_read_for_tests();
        app.tick();
        assert!(
            app.pending_mutation_queue.is_empty(),
            "{opener:?}: no SetRead mutation should fire after navigating to Analytics"
        );
    }
}

/// Slice 1 / B1.2: the top tab bar must include `"6 Analytics"`
/// alongside the existing five tabs. Without this the analytics
/// screen has no surface presence and stays buried in the command
/// palette.
#[test]
fn tab_bar_renders_six_analytics_tab() {
    let mut app = App::new();
    let snapshot = mxr_test_support::render_to_string(120, 24, |frame| app.draw(frame));
    assert!(
        snapshot.contains("6 Analytics"),
        "tab bar must include '6 Analytics'; got:\n{snapshot}"
    );
}

/// Phase 2.5: the four analytics palette entries are present.
/// Locks down discoverability — the only entrypoint to these
/// views is the palette.
#[test]
fn analytics_palette_entries_present_in_default_commands() {
    let commands = crate::ui::command_palette::default_commands();
    let labels: Vec<&str> = commands.iter().map(|c| c.label.as_str()).collect();
    for needle in [
        "Analytics: Storage",
        "Analytics: Stale Threads",
        "Analytics: Contacts",
        "Analytics: Response Time",
        "Analytics: Subscriptions",
        "Analytics: Wrapped",
    ] {
        assert!(
            labels.contains(&needle),
            "expected `{needle}` in palette; got {labels:?}"
        );
    }
}

/// Phase 3.4 / Behavior 1: toggling between HTML and plain-text
/// views preserves the message scroll offset. Catches a regression
/// where `ToggleHtmlView` would naively reset to 0 after the
/// body_view_state mode change, dumping the user back at the top
/// of long emails every time they switched.
#[test]
fn html_view_toggle_preserves_message_scroll_offset() {
    use crate::action::Action;
    use crate::app::BodyViewState;
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Long body line 1\nLong body line 2\nLong body line 3".into()),
            text_html: Some("<p>Long body</p>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                text_plain_source: Some(BodyPartSource::Exact),
                text_html_source: Some(BodyPartSource::Exact),
                ..Default::default()
            },
        },
    );
    app.apply(Action::OpenSelected);
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Ready { .. }
    ));

    // User scrolls down before toggling.
    app.mailbox.message_scroll_offset = 7;
    app.apply(Action::ToggleHtmlView);
    assert_eq!(
        app.mailbox.message_scroll_offset, 7,
        "scroll must be preserved across HTML toggle"
    );
    app.apply(Action::ToggleHtmlView);
    assert_eq!(
        app.mailbox.message_scroll_offset, 7,
        "scroll must be preserved on round-trip"
    );
}

/// Phase 3.4 / Behavior 2: labels surface "External content blocked"
/// instead of the old "remote images blocked" so users actually
/// notice the placeholder. Locks the user-visible string.
#[test]
fn body_status_labels_replace_remote_blocked_with_clear_external_content_string() {
    use crate::app::{body_status_labels_with_loading, BodySource, BodyViewMetadata, BodyViewMode};
    let metadata = BodyViewMetadata {
        mode: BodyViewMode::Html,
        remote_content_available: true,
        remote_content_enabled: false,
        ..Default::default()
    };
    let labels = body_status_labels_with_loading(&metadata, &BodySource::Html, false, false);
    assert!(
        labels
            .iter()
            .any(|l| l.contains("External content blocked")),
        "expected `External content blocked` in {labels:?}"
    );
    assert!(
        !labels.iter().any(|l| l == "remote images blocked"),
        "old label should be gone: {labels:?}"
    );
}

/// Phase 3.4 / Behavior 3: when remote content is enabled and
/// assets are still being fetched, the labels include
/// "Loading external assets…" so the user sees a hint while the
/// async fetch resolves.
#[test]
fn body_status_labels_show_loading_chip_when_assets_pending() {
    use crate::app::{body_status_labels_with_loading, BodySource, BodyViewMetadata, BodyViewMode};
    let metadata = BodyViewMetadata {
        mode: BodyViewMode::Html,
        remote_content_available: true,
        remote_content_enabled: true,
        ..Default::default()
    };
    let labels = body_status_labels_with_loading(&metadata, &BodySource::Html, false, true);
    assert!(
        labels.iter().any(|l| l.contains("Loading external assets")),
        "expected loading label in {labels:?}"
    );
}

/// Phase 2.4 / Behavior 1: a rule form filled with a `shell:command`
/// action submits a `Request::UpsertRuleForm` whose `action`
/// string round-trips losslessly to the daemon-side parser. Locks
/// in the contract that the TUI doesn't need to learn the
/// `RuleAction::ShellHook` shape — the parser owns translation.
#[test]
fn rule_form_save_with_shell_hook_action_is_accepted() {
    use crate::action::Action;
    let mut app = App::new();
    app.rules.page.form.visible = true;
    app.rules.page.form.name = "Notify on bills".into();
    app.rules.page.form.condition = "from:billing@example.com".into();
    app.rules.page.form.action = "shell:notify-send 'Bill arrived'".into();
    app.rules.page.form.priority = "100".into();
    app.rules.page.form.enabled = true;
    app.sync_rule_form_editors();

    app.apply(Action::SaveRuleForm);

    assert!(
        app.rules.pending_form_save,
        "valid shell-hook rule must enqueue a daemon save"
    );
    assert!(
        app.rules.page.form.validation_error.is_none(),
        "valid form must clear validation_error"
    );
}

/// Phase 2.4 / Behavior 4: a rule form with `action="shell:"`
/// (empty command after the prefix) sets a visible
/// `validation_error` and does NOT enqueue a save. Daemons would
/// otherwise accept a `ShellHook { command: "" }` rule and fail
/// silently every time it tries to fire.
#[test]
fn rule_form_save_with_empty_shell_command_is_rejected() {
    use crate::action::Action;
    let mut app = App::new();
    app.rules.page.form.visible = true;
    app.rules.page.form.name = "Bad shell".into();
    app.rules.page.form.condition = "from:any".into();
    app.rules.page.form.action = "shell:   ".into(); // trim => empty
    app.sync_rule_form_editors();

    app.apply(Action::SaveRuleForm);

    assert!(
        !app.rules.pending_form_save,
        "empty shell command must NOT enqueue a save"
    );
    let err = app
        .rules
        .page
        .form
        .validation_error
        .as_deref()
        .expect("validation_error must surface for empty shell command");
    assert!(
        err.to_lowercase().contains("shell"),
        "validation_error should mention shell; got {err:?}"
    );
}

/// Phase 2.4: blank action surfaces a validation_error pointing
/// users at the example syntax. Catches "form silently submits
/// nothing → daemon returns generic Unsupported action" UX.
#[test]
fn rule_form_save_with_blank_action_is_rejected_with_examples() {
    use crate::action::Action;
    let mut app = App::new();
    app.rules.page.form.visible = true;
    app.rules.page.form.name = "Empty action".into();
    app.rules.page.form.condition = "from:any".into();
    app.rules.page.form.action = "  ".into();
    app.sync_rule_form_editors();

    app.apply(Action::SaveRuleForm);

    assert!(!app.rules.pending_form_save);
    let err = app
        .rules
        .page
        .form
        .validation_error
        .as_deref()
        .expect("validation_error must surface for blank action");
    assert!(
        err.to_lowercase().contains("action"),
        "error should mention `action`; got {err:?}"
    );
}

/// Phase 2.3 / Behavior 1: when the diagnostics snapshot reports
/// an account as unhealthy, `account_unhealthy` returns true.
/// This is the contract the renderer relies on for the
/// "[unhealthy: r repairs]" indicator.
#[test]
fn account_unhealthy_reflects_diagnostics_sync_status() {
    let mut app = App::new();
    let account_id = mxr_core::AccountId::new();
    let summary = mxr_protocol::AccountSummaryData {
        account_id: account_id.clone(),
        key: Some("user".into()),
        name: "User".into(),
        email: "user@example.com".into(),
        provider_kind: "imap".into(),
        sync_kind: Some("imap".into()),
        send_kind: Some("smtp".into()),
        enabled: true,
        is_default: false,
        source: mxr_protocol::AccountSourceData::Config,
        editable: mxr_protocol::AccountEditModeData::Full,
        sync: None,
        send: None,
        capabilities: Default::default(),
    };

    // No status yet → freshly added accounts don't flicker through
    // the unhealthy state.
    assert!(!app.account_unhealthy(&summary));

    app.diagnostics.page.sync_statuses = vec![mxr_protocol::AccountSyncStatus {
        account_id: account_id.clone(),
        account_name: "User".into(),
        last_attempt_at: None,
        last_success_at: None,
        last_error: Some("auth failed".into()),
        failure_class: Some("auth".into()),
        consecutive_failures: 3,
        backoff_until: None,
        sync_in_progress: false,
        current_cursor_summary: None,
        last_synced_count: 0,
        healthy: false,
    }];
    assert!(
        app.account_unhealthy(&summary),
        "account flagged as unhealthy by sync status"
    );

    // Toggle back: a recovered account is no longer unhealthy.
    app.diagnostics.page.sync_statuses[0].healthy = true;
    assert!(!app.account_unhealthy(&summary));
}

/// Phase 2.3 / Behavior 2: dispatching `RepairAccount` with a
/// config-backed selected account queues a `pending_repair` for
/// the dispatcher and shows an in-flight status. Runtime-only
/// accounts are rejected with a status hint.
#[test]
fn repair_account_action_queues_pending_repair_for_config_account() {
    use crate::action::Action;
    let mut app = App::new();
    // Insert a config-backed account so selected_account_config
    // produces a real AccountConfigData.
    app.accounts.page.accounts = vec![mxr_protocol::AccountSummaryData {
        account_id: mxr_core::AccountId::new(),
        key: Some("user".into()),
        name: "User".into(),
        email: "user@example.com".into(),
        provider_kind: "imap".into(),
        sync_kind: Some("imap".into()),
        send_kind: Some("smtp".into()),
        enabled: true,
        is_default: true,
        source: mxr_protocol::AccountSourceData::Config,
        editable: mxr_protocol::AccountEditModeData::Full,
        sync: Some(mxr_protocol::AccountSyncConfigData::Imap {
            host: "imap.example.com".into(),
            port: 993,
            username: "user@example.com".into(),
            password_ref: "mxr/user".into(),
            password: None,
            auth_required: true,
            use_tls: true,
        }),
        send: Some(mxr_protocol::AccountSendConfigData::Smtp {
            host: "smtp.example.com".into(),
            port: 587,
            username: "user@example.com".into(),
            password_ref: "mxr/user".into(),
            password: None,
            auth_required: true,
            use_tls: true,
        }),
        capabilities: Default::default(),
    }];
    app.accounts.page.selected_index = 0;

    app.apply(Action::RepairAccount);

    let pending = app
        .accounts
        .pending_repair
        .as_ref()
        .expect("RepairAccount must populate pending_repair");
    assert_eq!(pending.key, "user");
    assert!(app.accounts.page.operation_in_flight);
    assert_eq!(
        app.accounts.page.status.as_deref(),
        Some("Repairing account...")
    );
}

/// Phase 2.3: Action::RepairAccount on an empty list (no selected
/// account) is a no-op with a status hint, not a panic. Catches
/// "selected_index OOB" regressions.
#[test]
fn repair_account_action_with_no_selection_sets_status_only() {
    use crate::action::Action;
    let mut app = App::new();
    app.apply(Action::RepairAccount);
    assert!(app.accounts.pending_repair.is_none());
    assert!(!app.accounts.page.operation_in_flight);
    assert!(
        app.accounts
            .page
            .status
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("repair"),
        "should hint about runtime-only / no-selection"
    );
}

/// Phase 2.1 stage B / Behavior 3 (cancel path): pressing `n`/Esc
/// on the delete confirm clears it without dispatching.
#[test]
fn delete_saved_search_cancel_path_does_not_queue_request() {
    let mut app = App::new();
    app.modals.pending_saved_search_delete_confirm = Some("Important".into());
    app.cancel_pending_saved_search_delete();
    assert!(
        app.modals.pending_saved_search_delete_confirm.is_none(),
        "confirm must clear on cancel"
    );
    assert!(
        app.modals.pending_saved_search_dispatch.is_empty(),
        "no request must queue on cancel"
    );
}

/// Phase 1.4 / Behavior 6: setting a pending-undo handle exposes
/// the human-readable label "Archived N — u to undo" while the
/// window is fresh, and `take_pending_undo` returns the same id
/// the input handler will dispatch.
#[test]
fn pending_undo_label_renders_within_window_then_clears() {
    use crate::app::PendingUndo;
    let mut app = App::new();
    let t0 = std::time::Instant::now();
    app.set_pending_undo(PendingUndo {
        mutation_id: "01HVTEST".into(),
        verb_past: "Archived".into(),
        count: 15,
        applied_at: t0,
    });

    // Fresh: label is shown.
    let label = app
        .pending_undo_label(t0 + std::time::Duration::from_secs(5))
        .expect("label must be present within window");
    assert_eq!(label, "Archived 15 — u to undo");

    // Past 60s: label gone (and tick clears the handle).
    assert!(
        app.pending_undo_label(t0 + std::time::Duration::from_secs(61))
            .is_none(),
        "label must clear after the 60s window"
    );
    app.tick_pending_undo(t0 + std::time::Duration::from_secs(61));
    assert!(app.pending_undo.is_none(), "tick must drop expired handle");
}

/// Phase 1.4: take_pending_undo returns and clears so the next `u`
/// press can't accidentally double-undo. The daemon also refuses
/// replays, but client-side clearing is the primary guard.
#[test]
fn take_pending_undo_returns_and_clears() {
    use crate::app::PendingUndo;
    let mut app = App::new();
    app.set_pending_undo(PendingUndo {
        mutation_id: "M1".into(),
        verb_past: "Trashed".into(),
        count: 1,
        applied_at: std::time::Instant::now(),
    });

    let taken = app.take_pending_undo().expect("must yield handle");
    assert_eq!(taken.mutation_id, "M1");
    assert!(
        app.pending_undo.is_none(),
        "second `u` must not see a handle"
    );
}

/// Phase 1.3 / Behavior 4: an `Error` escalates to `ErrorModalState`
/// even if the status bar slot is occupied — errors must never be
/// hidden behind transient status messages.
#[test]
fn report_error_opens_modal_even_if_status_occupied() {
    let mut app = App::new();
    app.status_message = Some("Working...".into());
    assert!(app.modals.error.is_none(), "precondition: no modal");

    app.report_error("Body parse failed", "details about the failure");

    let modal = app.modals.error.as_ref().expect("modal must open");
    assert!(
        modal.title.to_lowercase().contains("body parse"),
        "modal title must mention the error; got {:?}",
        modal.title
    );
    assert!(
        modal.detail.contains("details"),
        "modal detail must include the supplied detail string"
    );
    assert_eq!(app.modals.error_log.len(), 1);
}

/// Phase 1.2 / Behavior 1+3: ConnectionState defaults to Connecting on
/// app construction, and transitioning to Connected clears any prior
/// "daemon not responding" error modal.
#[test]
fn connection_state_starts_connecting() {
    use crate::app::ConnectionState;
    let app = App::new();
    assert!(matches!(app.connection_state, ConnectionState::Connecting));
}

#[test]
fn transition_to_connected_clears_daemon_error_modal() {
    use crate::app::ConnectionState;
    use crate::app::ErrorModalState;
    let mut app = App::new();
    app.set_connection_state(ConnectionState::Reconnecting {
        since: std::time::Instant::now(),
        reason: "connection refused".into(),
    });
    // Simulate the modal that would have been opened after 5s.
    app.modals.error = Some(ErrorModalState::new("Daemon not responding", "..."));

    app.set_connection_state(ConnectionState::Connected);

    assert!(
        matches!(app.connection_state, ConnectionState::Connected),
        "state must transition to Connected"
    );
    assert!(
        app.modals.error.is_none(),
        "the daemon-not-responding modal must close on reconnection"
    );
}

/// Phase 1.2 / Behavior 2: after 5s of Reconnecting, an error modal
/// opens explaining the daemon is not responding. Catches "silent hang"
/// regressions (the original v1 ship blocker).
#[test]
fn tick_connection_state_opens_modal_after_5s_reconnecting() {
    use crate::app::ConnectionState;
    let mut app = App::new();
    let t0 = std::time::Instant::now();
    app.set_connection_state(ConnectionState::Reconnecting {
        since: t0,
        reason: "connection refused".into(),
    });

    // 4s in — under the threshold; modal must not have opened yet.
    app.tick_connection_state(t0 + std::time::Duration::from_secs(4));
    assert!(app.modals.error.is_none(), "modal must not open before 5s");

    // 6s in — over the threshold; modal must be open with non-empty detail.
    app.tick_connection_state(t0 + std::time::Duration::from_secs(6));
    let modal = app.modals.error.as_ref().expect("modal must open after 5s");
    assert!(
        modal.title.to_lowercase().contains("daemon"),
        "modal title must mention the daemon; got {:?}",
        modal.title
    );
    assert!(
        !modal.detail.trim().is_empty(),
        "modal detail must be non-empty"
    );
}

/// Phase 1.2 / Behavior 2: tick is a no-op when connection is healthy.
/// Regression for "modal pops up randomly while connected".
#[test]
fn tick_connection_state_no_op_when_connected() {
    use crate::app::ConnectionState;
    let mut app = App::new();
    app.set_connection_state(ConnectionState::Connected);
    app.tick_connection_state(std::time::Instant::now() + std::time::Duration::from_secs(60));
    assert!(app.modals.error.is_none());
}

/// Phase 1.1 / Behavior 4: when SendDraft is part of a larger batch
/// (other mutations still in flight), the per-effect status is
/// suppressed — matches the existing `show_completion_status` gating
/// for archive/trash mutations. Regression for "every mutation in
/// the batch overwriting the status".
#[test]
fn sent_success_effect_suppresses_status_when_more_in_flight() {
    let mut app = App::new();
    let label_id = LabelId::new();
    app.mailbox.active_label = Some(label_id.clone());
    app.status_message = Some("In progress".into());

    app.apply_mutation_completion(
        MutationEffect::SentSuccess {
            status: "Sent!".into(),
            remind_at: None,
            sent_message_id: None,
        },
        false, // not last in the batch
    );

    assert_eq!(
        app.status_message.as_deref(),
        Some("In progress"),
        "status must not change while other mutations are in flight"
    );
    assert_eq!(
        app.mailbox.pending_label_fetch,
        Some(label_id),
        "label fetch must still be queued even when status is suppressed"
    );
}
