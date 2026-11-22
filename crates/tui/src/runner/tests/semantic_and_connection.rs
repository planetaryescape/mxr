use super::*;

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

#[test]
fn rule_form_save_with_empty_shell_command_is_rejected() {
    use crate::action::Action;
    let mut app = App::new();
    app.rules.page.form.visible = true;
    app.rules.page.form.name = "Bad shell".into();
    app.rules.page.form.condition = "from:any".into();
    app.rules.page.form.action = "shell:   ".into();
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

    assert!(
        app.pending_undo_toast(t0 + std::time::Duration::from_secs(61))
            .is_none(),
        "toast must clear after the 60s window"
    );
    app.tick_pending_undo(t0 + std::time::Duration::from_secs(61));
    assert!(app.pending_undo.is_none(), "tick must drop expired handle");
}

#[test]
fn tick_connection_state_opens_modal_after_5s_reconnecting() {
    use crate::app::ConnectionState;
    let mut app = App::new();
    let t0 = std::time::Instant::now();
    app.set_connection_state(ConnectionState::Reconnecting {
        since: t0,
        reason: "connection refused".into(),
    });

    app.tick_connection_state(t0 + std::time::Duration::from_secs(4));
    assert!(app.modals.error.is_none(), "modal must not open before 5s");

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
