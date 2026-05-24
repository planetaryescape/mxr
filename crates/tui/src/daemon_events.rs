use crate::app::{self, App, MutationId};
use mxr_core::MxrError;
use mxr_protocol::{DaemonEvent, Request};

/// Past-tense verb for the status bar text "X 15 — u to undo". Defaults
/// to "Done" for non-`Mutation` requests so the status bar still reads
/// sensibly even when an unexpected response carries a mutation_id.
pub(super) fn mutation_verb_past(req: &Request) -> &'static str {
    use mxr_protocol::MutationCommand;
    if let Request::Mutation { mutation: cmd, .. } = req {
        match cmd {
            MutationCommand::Archive { .. } => "Archived",
            MutationCommand::ReadAndArchive { .. } => "Marked read & archived",
            MutationCommand::Trash { .. } => "Trashed",
            MutationCommand::Spam { .. } => "Marked as spam",
            MutationCommand::SetRead { read: true, .. } => "Marked read",
            MutationCommand::SetRead { read: false, .. } => "Marked unread",
            _ => "Done",
        }
    } else {
        "Done"
    }
}

/// Render a no-success mutation result as something the user can act on.
/// Joining the per-account error strings is the difference between
/// "mutation skipped 1 message(s)" (useless) and "pool timed out while
/// waiting for an open connection" (a real lead).
pub(super) fn format_mutation_failure(result: &mxr_protocol::MutationResultData) -> String {
    let errors: Vec<&str> = result
        .accounts
        .iter()
        .filter_map(|account| account.error.as_deref())
        .collect();
    let header = format!("mutation skipped {} message(s)", result.skipped);
    if errors.is_empty() {
        header
    } else {
        format!("{header}: {}", errors.join("; "))
    }
}

pub(super) fn handle_daemon_event(app: &mut App, event: DaemonEvent) {
    match event {
        DaemonEvent::SyncCompleted {
            messages_synced, ..
        } => {
            app.mailbox.pending_labels_refresh = true;
            app.mailbox.pending_all_envelopes_refresh = true;
            app.mailbox.pending_subscriptions_refresh = true;
            app.diagnostics.pending_status_refresh = true;
            if let Some(label_id) = app.mailbox.active_label.clone() {
                app.mailbox.pending_label_fetch = Some(label_id);
            }
            if messages_synced > 0 {
                app.status_message = Some(format!("Synced {messages_synced} messages"));
            }
        }
        DaemonEvent::LabelCountsUpdated { counts } => {
            let selected_sidebar = app.selected_sidebar_key();
            for count in &counts {
                if let Some(label) = app
                    .mailbox
                    .labels
                    .iter_mut()
                    .find(|label| label.id == count.label_id)
                {
                    label.unread_count = count.unread_count;
                    label.total_count = count.total_count;
                }
            }
            app.restore_sidebar_selection(selected_sidebar);
        }
        DaemonEvent::SyncError { account_id, error } => {
            app.modals.error = Some(app::ErrorModalState::new(
                "Sync Failed",
                format!("Account: {account_id}\n\n{error}"),
            ));
            app.status_message = Some(format!("Sync error: {error}"));
            app.diagnostics.pending_status_refresh = true;
        }
        DaemonEvent::ReminderTriggered { sent_message_id } => {
            app.mailbox.reply_later_message_ids.insert(sent_message_id);
            if app.modals.reply_queue.visible {
                app.pending_reply_queue_refresh = true;
            }
            app.status_message = Some("Reminder due; added to reply queue".into());
        }
        // Surface long-running operation progress (sync, rebuild
        // analytics, reindex) in the status bar so the user can see
        // the daemon is actually doing work, not hung.
        DaemonEvent::OperationStarted {
            operation, message, ..
        } => {
            app.status_message = Some(format!("{operation}: {message}"));
        }
        DaemonEvent::OperationProgress {
            operation,
            current,
            total,
            message,
            ..
        } => {
            let total_str = total.map_or_else(|| "?".into(), |t| t.to_string());
            app.status_message = Some(format!("{operation} [{current}/{total_str}]: {message}"));
        }
        DaemonEvent::OperationCompleted {
            operation, message, ..
        } => {
            app.status_message = Some(format!("{operation}: {message}"));
            // Contacts-related rebuild may have changed analytics
            // data; nudge the active analytics view to refresh.
            if operation == "rebuild-analytics" && app.screen == app::Screen::Analytics {
                app.analytics.refresh_pending = true;
            }
        }
        DaemonEvent::OperationFailed {
            operation, error, ..
        } => {
            app.status_message = Some(format!("{operation} failed: {error}"));
        }
        DaemonEvent::MutationReconciliationFailed {
            client_correlation_id,
            error_summary,
        } => {
            if let Ok(raw) = client_correlation_id.parse::<u64>() {
                let mid = MutationId::from_raw(raw);
                app.handle_mutation_reconciliation_failed(mid);
                app.pending_optimistic.clear(mid);
                app.refresh_mailbox_after_mutation_failure();
                app.status_message = Some(format!("Mutation failed: {error_summary}"));
            }
        }
        _ => {}
    }
}

/// Apply a `ThreadSummaryLoaded` async result to the app. Always
/// clears `thread_summary_loading` when it matches this thread — even
/// when the user has navigated away — so a subsequent `y` press on
/// the same thread isn't blocked by a stale "already running" guard.
/// Visible UI updates (summary preview, status, modal) only fire when
/// the thread is still focused so a stale response can't overwrite
/// the user's current view.
pub(super) fn apply_thread_summary_loaded(
    app: &mut App,
    thread_id: mxr_core::ThreadId,
    result: Result<(String, String), MxrError>,
) {
    if app.mailbox.thread_summary_loading.as_ref() == Some(&thread_id) {
        app.mailbox.thread_summary_loading = None;
    }
    let still_relevant = app
        .context_envelope()
        .is_some_and(|env| env.thread_id == thread_id);
    if !still_relevant {
        return;
    }
    match result {
        Ok((text, model)) => {
            app.mailbox.thread_summary = Some(app::ThreadSummaryPreview {
                text: text.clone(),
                model: model.clone(),
            });
            app.mailbox.thread_summary_error = None;
            if app.modals.summary.visible {
                app.modals.summary.set_summary(text, model);
            }
            app.status_message = Some("Summary ready".into());
        }
        Err(e) => {
            let message = e.to_string();
            app.mailbox.thread_summary_error = Some(message.clone());
            if app.modals.summary.visible {
                app.modals.summary.set_error(message);
            }
            app.status_message = Some(format!("Summarize failed: {e}"));
        }
    }
}

pub(super) fn apply_all_envelopes_refresh(app: &mut App, envelopes: Vec<mxr_core::Envelope>) {
    let switched_accounts = app.mailbox.mailbox_loading_message.take().is_some();
    let selected_id = (app.mailbox.active_label.is_none()
        && app.mailbox.pending_active_label.is_none()
        && !app.search.active
        && app.mailbox.mailbox_view == app::MailboxView::Messages)
        .then(|| app.selected_mail_row().map(|row| row.representative.id))
        .flatten();
    let mut envelopes = envelopes;
    app.pending_optimistic.apply(&mut envelopes);
    app.mailbox.all_envelopes = envelopes;
    app.mailbox.pending_commitment_counts_refresh = true;
    if app.mailbox.active_label.is_none()
        && app.mailbox.pending_active_label.is_none()
        && !app.search.active
    {
        app.mailbox.envelopes = app
            .mailbox
            .all_envelopes
            .iter()
            .filter(|envelope| !envelope.flags.contains(mxr_core::MessageFlags::TRASH))
            .cloned()
            .collect();
        if app.mailbox.mailbox_view == app::MailboxView::Messages {
            restore_mail_list_selection(app, selected_id);
        } else {
            app.mailbox.selected_index = app.mailbox.selected_index.min(
                app.mailbox
                    .subscriptions_page
                    .entries
                    .len()
                    .saturating_sub(1),
            );
        }
        app.queue_body_window();
    }
    if switched_accounts {
        app.status_message = Some("Account switched".into());
    }
}

pub(super) fn apply_labels_refresh(app: &mut App, mut labels: Vec<mxr_core::Label>) {
    let selected_sidebar = app.selected_sidebar_key();
    let mut preserved_label_ids = std::collections::HashSet::new();
    if let Some(app::SidebarSelectionKey::Label(label_id)) = selected_sidebar.as_ref() {
        preserved_label_ids.insert(label_id.clone());
    }
    if let Some(label_id) = app.mailbox.pending_active_label.as_ref() {
        preserved_label_ids.insert(label_id.clone());
    }
    if let Some(label_id) = app.mailbox.active_label.as_ref() {
        preserved_label_ids.insert(label_id.clone());
    }

    for label_id in preserved_label_ids {
        if labels.iter().any(|label| label.id == label_id) {
            continue;
        }
        if let Some(existing) = app
            .mailbox
            .labels
            .iter()
            .find(|label| label.id == label_id)
            .cloned()
        {
            labels.push(mxr_core::Label {
                unread_count: 0,
                total_count: 0,
                ..existing
            });
        }
    }

    app.mailbox.labels = labels;
    app.restore_sidebar_selection(selected_sidebar);
    app.resolve_desired_system_mailbox();
}

pub(super) fn restore_mail_list_selection(app: &mut App, selected_id: Option<mxr_core::MessageId>) {
    let row_count = app.mail_list_rows().len();
    if row_count == 0 {
        app.mailbox.selected_index = 0;
        app.mailbox.scroll_offset = 0;
        return;
    }

    if let Some(id) = selected_id {
        if let Some(position) = app
            .mail_list_rows()
            .iter()
            .position(|row| row.representative.id == id)
        {
            app.mailbox.selected_index = position;
        } else {
            app.mailbox.selected_index =
                app.mailbox.selected_index.min(row_count.saturating_sub(1));
        }
    } else {
        app.mailbox.selected_index = 0;
    }

    let visible_height = app.visible_height.max(1);
    if app.mailbox.selected_index < app.mailbox.scroll_offset {
        app.mailbox.scroll_offset = app.mailbox.selected_index;
    } else if app.mailbox.selected_index >= app.mailbox.scroll_offset + visible_height {
        app.mailbox.scroll_offset = app.mailbox.selected_index + 1 - visible_height;
    }
}
