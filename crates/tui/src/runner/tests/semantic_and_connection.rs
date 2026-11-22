use super::*;

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

/// Slice 2 / B2.1: forward cycling visits all analytics views
/// in the documented order (Storage → StaleThreads → Contacts →
/// ResponseTime → Subscriptions → SearchGroups → Wrapped → Storage).
/// Pins the next() arm so reordering or dropping a variant breaks here
/// instead of as a "Tab silently skips a tab" bug at runtime.
#[test]
fn next_analytics_view_cycles_all_variants_forward() {
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
        AnalyticsView::SearchAggregation,
        AnalyticsView::Wrapped,
        AnalyticsView::Storage,
    ];
    for expected in order {
        app.apply(Action::NextAnalyticsView);
        assert_eq!(app.analytics.view, expected);
    }
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

/// Phase 2.3 / Behavior 1: when the diagnostics snapshot reports
/// an account as unhealthy, `account_unhealthy` returns true.
/// This is the contract the renderer relies on for the
/// "[unhealthy: r repairs]" indicator.
#[test]
fn account_unhealthy_reflects_diagnostics_sync_status() {
    let mut app = App::new();
    let account_id = mxr_core::AccountId::new();
    let summary = mxr_protocol::AccountSummaryData {
        identity: None,
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
        progress: None,
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
        identity: None,
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
            max_connections: 4,
            tls_cert_sha256: None,
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

/// Phase 1.4 / Behavior 6: setting a pending-undo handle exposes a
/// toast reading "Archived N" with an "u to undo" hint while the
/// window is fresh, and `take_pending_undo` returns the same id
/// the input handler will dispatch.
#[test]
fn pending_undo_toast_renders_within_window_then_clears() {
    use crate::app::PendingUndo;
    let mut app = App::new();
    let t0 = std::time::Instant::now();
    app.set_pending_undo(PendingUndo {
        action: crate::app::UndoAction::Mutations(vec!["01HVTEST".into()]),
        verb_past: "Archived".into(),
        count: 15,
        applied_at: t0,
    });

    // Fresh: toast is shown with the undo hint and a live countdown.
    let toast = app
        .pending_undo_toast(t0 + std::time::Duration::from_secs(5))
        .expect("toast must be present within window");
    assert_eq!(toast.text, "Archived 15");
    assert_eq!(toast.action_hint.as_deref(), Some("u to undo"));
    assert_eq!(
        toast
            .remaining(t0 + std::time::Duration::from_secs(5))
            .as_secs(),
        55,
        "countdown must reflect the remaining 60s undo window"
    );

    // Past 60s: toast gone (and tick clears the handle).
    assert!(
        app.pending_undo_toast(t0 + std::time::Duration::from_secs(61))
            .is_none(),
        "toast must clear after the 60s window"
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
        action: crate::app::UndoAction::Mutations(vec!["M1".into()]),
        verb_past: "Trashed".into(),
        count: 1,
        applied_at: std::time::Instant::now(),
    });

    let taken = app.take_pending_undo().expect("must yield handle");
    assert_eq!(
        taken.action,
        crate::app::UndoAction::Mutations(vec!["M1".to_string()])
    );
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

/// The IPC worker re-emits `Reconnecting` with a fresh `since` on every failed
/// retry. The watchdog must measure the whole outage, so the modal still opens
/// once retries have run past the threshold.
#[test]
fn tick_connection_state_opens_modal_when_retries_outlast_threshold() {
    use crate::app::ConnectionState;
    let mut app = App::new();
    let t0 = std::time::Instant::now();
    let secs = std::time::Duration::from_secs;
    for offset in [0, 2, 4, 6] {
        app.set_connection_state(ConnectionState::Reconnecting {
            since: t0 + secs(offset),
            reason: "connection refused".into(),
        });
    }

    app.tick_connection_state(t0 + secs(6));

    assert!(
        app.modals.error.is_some(),
        "retries that run past the threshold must open the daemon modal"
    );
}

/// A successful reconnect clears the outage. The next outage starts its own
/// clock, so an old outage must not make a fresh one escalate early.
#[test]
fn successful_reconnect_resets_outage_timer() {
    use crate::app::{ConnectionState, ErrorModalState};
    let mut app = App::new();
    let t0 = std::time::Instant::now();
    let secs = std::time::Duration::from_secs;
    app.set_connection_state(ConnectionState::Reconnecting {
        since: t0,
        reason: "connection refused".into(),
    });
    app.modals.error = Some(ErrorModalState::new("Daemon not responding", "..."));

    app.set_connection_state(ConnectionState::Connected);
    app.set_connection_state(ConnectionState::Reconnecting {
        since: t0 + secs(10),
        reason: "connection refused".into(),
    });
    app.tick_connection_state(t0 + secs(11));

    assert!(
        app.modals.error.is_none(),
        "a new outage must not inherit the timer from the one that recovered"
    );
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
            draft_path: None,
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
