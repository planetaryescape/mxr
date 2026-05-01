use mxr_core::{Envelope, Label, MessageFlags, MessageId};
use mxr_protocol::DaemonEvent;
use crate::app::{self, App, MailboxView, SidebarSelectionKey};

pub(crate) fn handle_daemon_event(app: &mut App, event: DaemonEvent) {
    match event {
        DaemonEvent::SyncCompleted {
            messages_synced, ..
        } => {
            app.pending_labels_refresh = true;
            app.pending_all_envelopes_refresh = true;
            app.pending_subscriptions_refresh = true;
            app.diagnostics.pending_status_refresh = true;
            if let Some(label_id) = app.active_label.clone() {
                app.pending_label_fetch = Some(label_id);
            }
            if messages_synced > 0 {
                app.status_message = Some(format!("Synced {messages_synced} messages"));
            }
        }
        DaemonEvent::LabelCountsUpdated { counts } => {
            let selected_sidebar = app.selected_sidebar_key();
            for count in &counts {
                if let Some(label) = app
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
        _ => {}
    }
}

pub(crate) fn apply_all_envelopes_refresh(app: &mut App, envelopes: Vec<Envelope>) {
    let selected_id = (app.active_label.is_none()
        && app.pending_active_label.is_none()
        && !app.search.active
        && app.mailbox_view == MailboxView::Messages)
        .then(|| app.selected_mail_row().map(|row| row.representative.id))
        .flatten();
    app.all_envelopes = envelopes;
    if app.active_label.is_none() && app.pending_active_label.is_none() && !app.search.active {
        app.envelopes = app
            .all_envelopes
            .iter()
            .filter(|envelope| {
                !envelope
                    .flags
                    .contains(MessageFlags::TRASH)
            })
            .cloned()
            .collect();
        if app.mailbox_view == MailboxView::Messages {
            restore_mail_list_selection(app, selected_id);
        } else {
            app.selected_index = app
                .selected_index
                .min(app.subscriptions_page.entries.len().saturating_sub(1));
        }
        app.queue_body_window();
    }
}

pub(crate) fn apply_labels_refresh(app: &mut App, mut labels: Vec<Label>) {
    let selected_sidebar = app.selected_sidebar_key();
    let mut preserved_label_ids = std::collections::HashSet::new();
    if let Some(SidebarSelectionKey::Label(label_id)) = selected_sidebar.as_ref() {
        preserved_label_ids.insert(label_id.clone());
    }
    if let Some(label_id) = app.pending_active_label.as_ref() {
        preserved_label_ids.insert(label_id.clone());
    }
    if let Some(label_id) = app.active_label.as_ref() {
        preserved_label_ids.insert(label_id.clone());
    }

    for label_id in preserved_label_ids {
        if labels.iter().any(|label| label.id == label_id) {
            continue;
        }
        if let Some(existing) = app
            .labels
            .iter()
            .find(|label| label.id == label_id)
            .cloned()
        {
            labels.push(Label {
                unread_count: 0,
                total_count: 0,
                ..existing
            });
        }
    }

    app.labels = labels;
    app.restore_sidebar_selection(selected_sidebar);
    app.resolve_desired_system_mailbox();
}

pub(crate) fn restore_mail_list_selection(app: &mut App, selected_id: Option<MessageId>) {
    let row_count = app.mail_list_rows().len();
    if row_count == 0 {
        app.selected_index = 0;
        app.scroll_offset = 0;
        return;
    }

    if let Some(id) = selected_id {
        if let Some(position) = app
            .mail_list_rows()
            .iter()
            .position(|row| row.representative.id == id)
        {
            app.selected_index = position;
        } else {
            app.selected_index = app.selected_index.min(row_count.saturating_sub(1));
        }
    } else {
        app.selected_index = 0;
    }

    let visible_height = app.visible_height.max(1);
    if app.selected_index < app.scroll_offset {
        app.scroll_offset = app.selected_index;
    } else if app.selected_index >= app.scroll_offset + visible_height {
        app.scroll_offset = app.selected_index + 1 - visible_height;
    }
}
