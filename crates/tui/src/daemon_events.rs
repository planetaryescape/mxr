use crate::app::{self, App, MutationId, Toast};
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
            MutationCommand::Route { .. } => "Routed",
            MutationCommand::Trash { .. } => "Trashed",
            MutationCommand::Spam { .. } => "Marked as spam",
            MutationCommand::SetRead { read: true, .. } => "Marked read",
            MutationCommand::SetRead { read: false, .. } => "Marked unread",
            MutationCommand::Star { starred: true, .. } => "Starred",
            MutationCommand::Star { starred: false, .. } => "Unstarred",
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
    if app.user_sync_pending
        && app.user_sync_acknowledgements_pending > 0
        && is_sync_outcome(&event)
    {
        app.user_sync_events_before_ack.push(event.clone());
    } else {
        apply_user_sync_outcome(app, &event);
    }
    match event {
        DaemonEvent::SyncCompleted {
            account_id,
            messages_synced,
        } => {
            app.mailbox.pending_labels_refresh = true;
            app.mailbox.pending_all_envelopes_refresh = true;
            app.mailbox.pending_subscriptions_refresh = true;
            app.refresh_places();
            if app.mailbox.mailbox_view == crate::app::MailboxView::Todo {
                app.refresh_todo();
            }
            if app.mailbox.mailbox_view == crate::app::MailboxView::Updates {
                app.refresh_updates();
            }
            if app.mailbox.mailbox_view == crate::app::MailboxView::Now {
                app.refresh_now();
            } else {
                app.mailbox.pending_rail_refresh = true;
            }
            app.diagnostics.pending_status_refresh = true;
            if let Some(label_id) = app.mailbox.active_label.clone() {
                app.mailbox.pending_label_fetch = Some(label_id);
            }
            if messages_synced > 0
                && !sync_event_hidden_by_batch_failure(app, "sync", Some(&account_id))
            {
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
        DaemonEvent::SyncError { .. } => {
            // A background retry never takes the keyboard: the status line's
            // sync health (from the freshness refresh below) says which
            // account is failing and when it tries again. A sync you asked
            // for gets one line naming the account.
            app.diagnostics.pending_status_refresh = true;
        }
        DaemonEvent::ReminderTriggered { sent_message_id } => {
            app.mailbox.reply_later_message_ids.insert(sent_message_id);
            if app.modals.reply_queue.visible {
                app.pending_reply_queue_refresh = true;
            }
            // Nobody replied by the time set: back on Waiting on.
            app.mailbox.pending_desk_refresh = true;
            app.status_message = Some("Reminder due; added to reply queue".into());
        }
        DaemonEvent::ReplyLaterReturned { message_id } => {
            app.mailbox.reply_later_message_ids.insert(message_id);
            if app.modals.reply_queue.visible {
                app.pending_reply_queue_refresh = true;
            }
            app.mailbox.pending_desk_refresh = true;
            app.status_message =
                Some("Back from reply later: in You owe and the reply queue".into());
        }
        // Surface long-running operation progress (sync, rebuild
        // analytics, reindex) in the status bar so the user can see
        // the daemon is actually doing work, not hung.
        DaemonEvent::OperationStarted {
            operation,
            account_id,
            message,
            ..
        } => {
            if !sync_event_hidden_by_batch_failure(app, &operation, account_id.as_ref()) {
                app.status_message = Some(format!("{operation}: {message}"));
            }
        }
        DaemonEvent::OperationProgress {
            operation,
            account_id,
            current,
            total,
            message,
            ..
        } => {
            if !sync_event_hidden_by_batch_failure(app, &operation, account_id.as_ref()) {
                let total_str = total.map_or_else(|| "?".into(), |t| t.to_string());
                app.status_message =
                    Some(format!("{operation} [{current}/{total_str}]: {message}"));
            }
        }
        DaemonEvent::OperationCompleted {
            operation,
            account_id,
            message,
            ..
        } => {
            if !sync_event_hidden_by_batch_failure(app, &operation, account_id.as_ref()) {
                app.status_message = Some(format!("{operation}: {message}"));
            }
            // Contacts-related rebuild may have changed analytics
            // data; nudge the active analytics view to refresh.
            if operation == "rebuild-analytics" && app.screen == app::Screen::Analytics {
                app.analytics.refresh_pending = true;
            }
        }
        DaemonEvent::OperationFailed {
            operation,
            account_id,
            error,
            ..
        } => {
            let selected_sync_failure = if operation == "sync" {
                account_id.as_ref().is_some_and(|account_id| {
                    if app.user_sync_batch_accounts.contains(account_id) {
                        app.user_sync_batch_failures
                            .insert(account_id.clone(), error.clone());
                        true
                    } else {
                        false
                    }
                })
            } else {
                false
            };
            if selected_sync_failure {
                app.status_message = sync_batch_failure_message(app);
            } else {
                app.status_message = Some(format!("{operation} failed: {error}"));
            }
        }
        DaemonEvent::MutationReconciliationFailed {
            client_correlation_id,
            error_summary,
        } => {
            match client_correlation_id.parse::<u64>() {
                Ok(raw) => {
                    let mid = MutationId::from_raw(raw);
                    app.handle_mutation_reconciliation_failed(mid);
                    app.pending_optimistic.clear(mid);
                    app.refresh_mailbox_after_mutation_failure();
                    app.push_toast(Toast::error(format!("Mutation failed: {error_summary}")));
                }
                Err(_) => {
                    tracing::warn!(
                        client_correlation_id,
                        "unparseable correlation id on reconciliation failure; forcing refresh"
                    );
                    app.push_toast(Toast::error(format!("Mutation failed: {error_summary}")));
                    // Same full-resync idiom as the EventsLagged arm below: we can't
                    // roll back a specific optimistic change, so refetch everything.
                    app.mailbox.pending_labels_refresh = true;
                    app.mailbox.pending_all_envelopes_refresh = true;
                    app.mailbox.pending_subscriptions_refresh = true;
                    if let Some(label_id) = app.mailbox.active_label.clone() {
                        app.mailbox.pending_label_fetch = Some(label_id);
                    }
                }
            }
        }
        DaemonEvent::ThreadGistReady { gist } => app.row_gists.put(&gist),
        // An email or sender moved in some client, or new mail was sorted:
        // Now's line, the mode on screen and Inbox's chips refetch.
        DaemonEvent::ModesChanged { .. } => app.modes_changed(),
        // An address change moves stored mail between sent and received, so the
        // mode views refresh as they do for a move. The owner's To/Cc for a
        // reply may also now differ from what was prewarmed.
        DaemonEvent::AccountAddressesChanged { .. } => {
            app.modes_changed();
            app.compose.invalidate_reply_contexts();
            // Analytics count the same direction. Forget what was fetched, so
            // entering the screen refetches, and refresh it if it is open.
            app.analytics.last_refresh_at.clear();
            if app.screen == app::Screen::Analytics {
                app.analytics.refresh_pending = true;
            }
        }
        DaemonEvent::AssistantUpdated {
            conversation_id,
            status,
            progress,
        } => app.assistant_updated(conversation_id, status, progress),
        DaemonEvent::NewMessages { envelopes, total } => {
            // The status bar's "latest mail" moves with every arrival.
            app.diagnostics.pending_status_refresh = true;
            // A capped event is only a sample: any conversation may have
            // changed.
            if total > envelopes.len() {
                app.row_gists.invalidate_all();
            } else {
                app.row_gists
                    .changed(envelopes.iter().map(|envelope| &envelope.thread_id));
            }
        }
        DaemonEvent::EventsLagged { skipped } => {
            refresh_after_event_gap(app);
            tracing::debug!(skipped, "event stream lagged; resyncing TUI views");
        }
        _ => {}
    }
}

/// Reconnect and reported event loss need the same authoritative reads. Private
/// compose state remains under its original workspace owner.
pub(crate) fn refresh_after_event_gap(app: &mut App) {
    // The missed events may have included an address change.
    app.compose.invalidate_reply_contexts();
    app.row_gists.invalidate_all();
    app.assistant.refetch();
    app.mailbox.pending_labels_refresh = true;
    app.mailbox.pending_all_envelopes_refresh = true;
    if app.is_paired_remote_read_only() {
        if let Some(label_id) = app.mailbox.active_label.clone() {
            app.mailbox.pending_label_fetch = Some(label_id);
        }
        return;
    }
    app.mailbox.pending_subscriptions_refresh = true;
    app.refresh_places();
    app.diagnostics.pending_status_refresh = true;
    if let Some(label_id) = app.mailbox.active_label.clone() {
        app.mailbox.pending_label_fetch = Some(label_id);
    }
}

pub(super) fn handle_sync_batch(app: &mut App, results: Vec<mxr_protocol::SyncAccountResult>) {
    if !app.user_sync_pending {
        return;
    }
    app.user_sync_acknowledgements_pending =
        app.user_sync_acknowledgements_pending.saturating_sub(1);
    for result in &results {
        app.user_sync_batch_accounts
            .insert(result.account_id.clone());
        if result.result == mxr_protocol::SyncAccountResultKind::Failed {
            let error = result
                .error
                .clone()
                .unwrap_or_else(|| "sync couldn't start".into());
            app.user_sync_batch_failures
                .insert(result.account_id.clone(), error.clone());
            app.status_message = Some(format!("Sync failed for {}: {error}", result.account_name));
        }
    }
    app.user_sync_targets.extend(
        results
            .iter()
            .filter(|result| result.result != mxr_protocol::SyncAccountResultKind::Failed)
            .map(|result| result.account_id.clone()),
    );
    flush_user_sync_events(app);
    settle_user_sync_pending(app);
}

pub(super) fn handle_sync_request_failed(app: &mut App) {
    if !app.user_sync_pending {
        return;
    }
    app.user_sync_acknowledgements_pending =
        app.user_sync_acknowledgements_pending.saturating_sub(1);
    flush_user_sync_events(app);
    settle_user_sync_pending(app);
}

fn flush_user_sync_events(app: &mut App) {
    let mut unmatched = Vec::new();
    for event in std::mem::take(&mut app.user_sync_events_before_ack) {
        if sync_event_account_id(&event)
            .is_some_and(|account_id| app.user_sync_targets.contains(account_id))
        {
            apply_user_sync_outcome(app, &event);
        } else if app.user_sync_acknowledgements_pending > 0 {
            unmatched.push(event);
        }
    }
    app.user_sync_events_before_ack = unmatched;
}

fn is_sync_outcome(event: &DaemonEvent) -> bool {
    matches!(
        event,
        DaemonEvent::SyncCompleted { .. } | DaemonEvent::SyncError { .. }
    )
}

fn sync_event_account_id(event: &DaemonEvent) -> Option<&mxr_core::AccountId> {
    match event {
        DaemonEvent::SyncCompleted { account_id, .. }
        | DaemonEvent::SyncError { account_id, .. } => Some(account_id),
        _ => None,
    }
}

fn apply_user_sync_outcome(app: &mut App, event: &DaemonEvent) {
    match event {
        DaemonEvent::SyncCompleted { account_id, .. } => {
            app.user_sync_targets.remove(account_id);
            if app.user_sync_batch_failures.remove(account_id).is_some() {
                app.status_message = sync_batch_failure_message(app).or_else(|| {
                    Some(format!(
                        "Sync recovered for {}",
                        app.account_display_name(account_id)
                    ))
                });
            }
        }
        DaemonEvent::SyncError { account_id, error }
            if app.user_sync_targets.remove(account_id)
                || (app.user_sync_pending
                    && app.user_sync_batch_accounts.contains(account_id)
                    && !app.user_sync_batch_failures.contains_key(account_id)) =>
        {
            app.user_sync_batch_accounts.insert(account_id.clone());
            app.user_sync_batch_failures
                .insert(account_id.clone(), error.clone());
            app.status_message = Some(format!(
                "Sync failed for {}: {error}",
                app.account_display_name(account_id)
            ));
        }
        _ => {}
    }
    settle_user_sync_pending(app);
}

fn sync_event_hidden_by_batch_failure(
    app: &App,
    operation: &str,
    account_id: Option<&mxr_core::AccountId>,
) -> bool {
    operation == "sync"
        && !app.user_sync_batch_failures.is_empty()
        && account_id.is_some_and(|id| app.user_sync_batch_accounts.contains(id))
}

fn sync_batch_failure_message(app: &App) -> Option<String> {
    let (account_id, error) = app
        .user_sync_batch_failures
        .iter()
        .min_by_key(|(account_id, _)| account_id.to_string())?;
    Some(format!(
        "Sync failed for {}: {error}",
        app.account_display_name(account_id)
    ))
}

fn settle_user_sync_pending(app: &mut App) {
    if app.user_sync_acknowledgements_pending == 0 && app.user_sync_targets.is_empty() {
        app.user_sync_pending = false;
        app.user_sync_events_before_ack.clear();
    }
}

/// Keep a thread's context facts when that thread is still open. A failed
/// read leaves the reader as it was: the facts are extra, not required.
pub(super) fn apply_thread_context_loaded(
    app: &mut App,
    thread_id: &mxr_core::ThreadId,
    result: Result<Box<mxr_protocol::ThreadContextData>, MxrError>,
) {
    let open = app
        .context_envelope()
        .is_some_and(|env| &env.thread_id == thread_id);
    match result {
        Ok(context) if open => app.mailbox.thread_context = Some(*context),
        Ok(_) => {}
        Err(error) => tracing::debug!(%thread_id, %error, "thread context unavailable"),
    }
}

/// Keep a thread's gist when that thread is still open. No model, or a
/// failed one, simply shows no gist: the reader never nags about it.
pub(super) fn apply_thread_gist_loaded(
    app: &mut App,
    thread_id: &mxr_core::ThreadId,
    result: Result<Box<mxr_protocol::ThreadGistData>, MxrError>,
) {
    let open = app
        .context_envelope()
        .is_some_and(|env| &env.thread_id == thread_id);
    match result {
        Ok(gist) if open => app.mailbox.thread_gist = Some(*gist),
        Ok(_) => {}
        Err(error) => tracing::debug!(%thread_id, %error, "thread gist unavailable"),
    }
}

/// Apply a `ThreadSummaryLoaded` async result to the app. Always
/// clears this thread's in-flight marker — even when the user has
/// navigated away — so a subsequent `y` press on the same thread isn't
/// blocked by a stale "already running" guard.
/// Visible UI updates (summary preview, status, modal) only fire when
/// the thread is still focused so a stale response can't overwrite
/// the user's current view.
pub(super) fn apply_thread_summary_loaded(
    app: &mut App,
    thread_id: mxr_core::ThreadId,
    result: Result<(String, String), MxrError>,
) {
    app.mailbox.thread_summary_in_flight.remove(&thread_id);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use mxr_protocol::DaemonEvent;

    #[test]
    fn unparseable_correlation_id_triggers_refresh_and_toast() {
        let mut app = App::new();
        assert!(!app.mailbox.pending_all_envelopes_refresh);
        assert!(app.toasts.is_empty());

        handle_daemon_event(
            &mut app,
            DaemonEvent::MutationReconciliationFailed {
                client_correlation_id: "not-a-number".into(),
                error_summary: "server error".into(),
            },
        );

        assert!(
            app.mailbox.pending_all_envelopes_refresh,
            "unparseable correlation id must trigger a full resync"
        );
        assert!(
            !app.toasts.is_empty(),
            "unparseable correlation id must still push an error toast"
        );
    }

    fn app_with_account(account_id: &mxr_core::AccountId) -> App {
        let mut app = App::new();
        app.freshness = Some(mxr_protocol::FreshnessData {
            generated_at: chrono::Utc::now(),
            newest_message_at: None,
            stale_after_secs: 900,
            worst_account_id: None,
            accounts: vec![mxr_protocol::AccountFreshnessData {
                account_id: account_id.clone(),
                account_name: "Work".into(),
                label: "Work".into(),
                newest_message_at: None,
                last_sync_ok_at: None,
                last_sync_attempt_at: None,
                sync_in_progress: false,
                health: mxr_protocol::SyncHealthData::Ok,
                last_sync_error: None,
            }],
            arrivals: vec![],
        });
        app
    }

    #[test]
    fn paired_remote_reconnect_refreshes_only_mailbox_reads() {
        let mut app = App::from_config(&mxr_config::MxrConfig::default());
        app.enable_paired_remote_read_only_session();
        let inbox_id = mxr_core::LabelId::new();
        app.mailbox.active_label = Some(inbox_id.clone());

        refresh_after_event_gap(&mut app);

        assert!(app.mailbox.pending_labels_refresh);
        assert!(app.mailbox.pending_all_envelopes_refresh);
        assert_eq!(app.mailbox.pending_label_fetch, Some(inbox_id));
        assert!(!app.mailbox.pending_subscriptions_refresh);
        assert!(!app.mailbox.pending_desk_refresh);
        assert!(!app.diagnostics.pending_status_refresh);
    }

    #[test]
    fn a_background_sync_failure_never_opens_a_modal() {
        let account_id = mxr_core::AccountId::new();
        let mut app = app_with_account(&account_id);
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncError {
                account_id,
                error: "Provider error: connection refused".into(),
            },
        );
        assert!(
            app.modals.error.is_none(),
            "no modal for a background retry"
        );
        assert!(app.status_message.is_none());
        assert!(
            app.diagnostics.pending_status_refresh,
            "the status line's sync health refreshes instead"
        );
    }

    #[test]
    fn a_sync_you_asked_for_names_the_account_when_it_fails() {
        let account_id = mxr_core::AccountId::new();
        let mut app = app_with_account(&account_id);
        app.user_sync_pending = true;
        app.user_sync_targets.insert(account_id.clone());
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncError {
                account_id: account_id.clone(),
                error: "connection refused".into(),
            },
        );
        assert!(app.modals.error.is_none());
        assert_eq!(
            app.status_message.as_deref(),
            Some("Sync failed for Work: connection refused")
        );
        // The next background retry is quiet again.
        app.status_message = None;
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncError {
                account_id,
                error: "connection refused".into(),
            },
        );
        assert!(app.status_message.is_none());
    }

    #[test]
    fn global_sync_keeps_later_account_failure_after_first_account_completes() {
        let first = mxr_core::AccountId::new();
        let second = mxr_core::AccountId::new();
        let mut app = app_with_account(&second);
        app.user_sync_pending = true;
        app.user_sync_acknowledgements_pending = 1;
        handle_sync_batch(
            &mut app,
            vec![sync_result(&first, "First"), sync_result(&second, "Work")],
        );
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncCompleted {
                account_id: first.clone(),
                messages_synced: 1,
            },
        );
        assert!(app.user_sync_pending);
        assert!(app.user_sync_targets.contains(&second));
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncError {
                account_id: second,
                error: "connection refused".into(),
            },
        );
        assert!(!app.user_sync_pending);
        assert_eq!(
            app.status_message.as_deref(),
            Some("Sync failed for Work: connection refused")
        );
    }

    #[test]
    fn global_sync_failure_survives_later_healthy_completion_and_operation_ack() {
        let failed = mxr_core::AccountId::new();
        let healthy = mxr_core::AccountId::new();
        let mut app = app_with_account(&failed);
        app.user_sync_pending = true;
        app.user_sync_acknowledgements_pending = 1;
        handle_sync_batch(
            &mut app,
            vec![
                sync_result(&failed, "Work"),
                sync_result(&healthy, "Personal"),
            ],
        );
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncError {
                account_id: failed,
                error: "connection refused".into(),
            },
        );
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncCompleted {
                account_id: healthy.clone(),
                messages_synced: 3,
            },
        );
        handle_daemon_event(
            &mut app,
            DaemonEvent::OperationCompleted {
                operation_id: "healthy-sync".into(),
                operation: "sync".into(),
                account_id: Some(healthy),
                message: "Sync complete: 3 message(s) updated".into(),
            },
        );
        assert_eq!(
            app.status_message.as_deref(),
            Some("Sync failed for Work: connection refused")
        );
    }

    #[test]
    fn sync_operation_failure_keeps_the_selected_account_name_after_sync_error() {
        let account_id = mxr_core::AccountId::new();
        let mut app = app_with_account(&account_id);
        app.user_sync_pending = true;
        app.user_sync_acknowledgements_pending = 1;
        handle_sync_batch(&mut app, vec![sync_result(&account_id, "Work")]);

        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncError {
                account_id: account_id.clone(),
                error: "connection refused".into(),
            },
        );
        handle_daemon_event(
            &mut app,
            DaemonEvent::OperationFailed {
                operation_id: "selected-sync".into(),
                operation: "sync".into(),
                account_id: Some(account_id),
                error: "connection refused".into(),
                retryable: true,
            },
        );

        assert_eq!(
            app.status_message.as_deref(),
            Some("Sync failed for Work: connection refused")
        );
    }

    #[test]
    fn global_sync_failure_buffered_before_ack_beats_healthy_operation_completion() {
        let failed = mxr_core::AccountId::new();
        let healthy = mxr_core::AccountId::new();
        let mut app = app_with_account(&failed);
        app.user_sync_pending = true;
        app.user_sync_acknowledgements_pending = 1;
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncError {
                account_id: failed.clone(),
                error: "connection refused".into(),
            },
        );
        handle_daemon_event(
            &mut app,
            DaemonEvent::OperationCompleted {
                operation_id: "healthy-sync".into(),
                operation: "sync".into(),
                account_id: Some(healthy.clone()),
                message: "Sync complete: 3 message(s) updated".into(),
            },
        );
        handle_sync_batch(
            &mut app,
            vec![
                sync_result(&failed, "Work"),
                sync_result(&healthy, "Personal"),
            ],
        );
        assert_eq!(
            app.status_message.as_deref(),
            Some("Sync failed for Work: connection refused")
        );
    }

    #[test]
    fn a_later_success_for_the_failed_account_clears_its_batch_failure() {
        let account_id = mxr_core::AccountId::new();
        let mut app = app_with_account(&account_id);
        app.user_sync_pending = true;
        app.user_sync_acknowledgements_pending = 1;
        handle_sync_batch(&mut app, vec![sync_result(&account_id, "Work")]);
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncError {
                account_id: account_id.clone(),
                error: "connection refused".into(),
            },
        );
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncCompleted {
                account_id: account_id.clone(),
                messages_synced: 1,
            },
        );
        assert!(app.user_sync_batch_failures.is_empty());
        assert_eq!(app.status_message.as_deref(), Some("Synced 1 messages"));
    }

    #[test]
    fn recovery_keeps_another_account_failure_visible() {
        let first = mxr_core::AccountId::new();
        let second = mxr_core::AccountId::new();
        let mut app = app_with_account(&first);
        app.user_sync_pending = true;
        app.user_sync_acknowledgements_pending = 1;
        handle_sync_batch(
            &mut app,
            vec![
                sync_result(&first, "Work"),
                sync_result(&second, "Personal"),
            ],
        );
        for (account_id, error) in [
            (&first, "work connection refused"),
            (&second, "personal connection refused"),
        ] {
            handle_daemon_event(
                &mut app,
                DaemonEvent::SyncError {
                    account_id: account_id.clone(),
                    error: error.into(),
                },
            );
        }
        let recovered = if first.to_string() < second.to_string() {
            first.clone()
        } else {
            second.clone()
        };
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncCompleted {
                account_id: recovered.clone(),
                messages_synced: 0,
            },
        );
        let (remaining_id, error) = app.user_sync_batch_failures.iter().next().unwrap();
        assert_ne!(remaining_id, &recovered);
        assert_eq!(
            app.status_message.as_deref(),
            Some(
                format!(
                    "Sync failed for {}: {error}",
                    app.account_display_name(remaining_id)
                )
                .as_str()
            )
        );
    }

    #[test]
    fn global_sync_buffers_completion_that_races_its_acknowledgement() {
        let account_id = mxr_core::AccountId::new();
        let mut app = app_with_account(&account_id);
        app.user_sync_pending = true;
        app.user_sync_acknowledgements_pending = 1;
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncCompleted {
                account_id: account_id.clone(),
                messages_synced: 0,
            },
        );
        assert!(app.user_sync_pending);
        assert_eq!(app.user_sync_events_before_ack.len(), 1);
        handle_sync_batch(&mut app, vec![sync_result(&account_id, "Work")]);
        assert!(!app.user_sync_pending);
        assert!(app.user_sync_events_before_ack.is_empty());
    }

    #[test]
    fn a_rejected_global_sync_clears_pending_tracking_but_later_events_refresh() {
        let account_id = mxr_core::AccountId::new();
        let mut app = app_with_account(&account_id);
        app.user_sync_pending = true;
        app.user_sync_acknowledgements_pending = 1;
        handle_sync_request_failed(&mut app);
        assert!(!app.user_sync_pending);
        handle_daemon_event(
            &mut app,
            DaemonEvent::SyncCompleted {
                account_id,
                messages_synced: 0,
            },
        );
        assert!(app.diagnostics.pending_status_refresh);
        assert!(app.mailbox.pending_labels_refresh);
        assert!(app.user_sync_events_before_ack.is_empty());
    }

    #[test]
    fn a_second_acknowledged_join_keeps_the_same_account_tracked() {
        let account_id = mxr_core::AccountId::new();
        let mut app = app_with_account(&account_id);
        app.user_sync_pending = true;
        app.user_sync_acknowledgements_pending = 1;
        handle_sync_batch(&mut app, vec![sync_result(&account_id, "Work")]);
        app.user_sync_acknowledgements_pending = 1;
        handle_sync_batch(&mut app, vec![sync_result(&account_id, "Work")]);
        assert_eq!(app.user_sync_targets.len(), 1);
        assert!(app.user_sync_targets.contains(&account_id));
        assert!(app.user_sync_pending);
        assert_eq!(app.user_sync_acknowledgements_pending, 0);
    }

    fn sync_result(
        account_id: &mxr_core::AccountId,
        account_name: &str,
    ) -> mxr_protocol::SyncAccountResult {
        mxr_protocol::SyncAccountResult {
            account_id: account_id.clone(),
            account_name: account_name.into(),
            result: mxr_protocol::SyncAccountResultKind::Started,
            error: None,
        }
    }
}
