//! Snoozed (`g n`) and wake (`w`). Snoozed is the mail list over the
//! daemon's snoozed messages (`Request::ListSnoozed`, the request the web's
//! Snoozed page and `mxr snoozed` use), waking soonest first, so the list's
//! keys work on it as on any label. `w` wakes the lit conversation now
//! (`Request::Unsnooze`, as `mxr unsnooze` and the web do); in the reply
//! queue it takes the lit message out of the queue. Both undo a Not now
//! (`b`), and `b` sets either again.

use super::*;

impl App {
    pub(super) fn apply_snoozed_action(&mut self, action: Action) {
        match action {
            Action::OpenSnoozed => self.open_snoozed(),
            Action::Wake => self.wake(),
            _ => {}
        }
    }

    fn open_snoozed(&mut self) {
        self.mailbox.mailbox_view = MailboxView::Snoozed;
        self.mailbox.active_label = None;
        self.mailbox.pending_active_label = None;
        self.mailbox.pending_label_fetch = None;
        self.mailbox.pending_preview_read = None;
        self.mailbox.desired_system_mailbox = None;
        self.search.active = false;
        self.screen = Screen::Mailbox;
        self.mailbox.active_pane = ActivePane::MailList;
        self.mailbox.layout_mode = LayoutMode::TwoPane;
        self.mailbox.selected_index = 0;
        self.mailbox.scroll_offset = 0;
        self.mailbox.pending_snoozed_refresh = true;
    }

    /// The runtime fetched what is snoozed and its envelopes. Messages whose
    /// envelope is gone are left out, as the web leaves them out.
    pub(crate) fn set_snoozed(
        &mut self,
        snoozed: Vec<mxr_core::types::Snoozed>,
        envelopes: Vec<Envelope>,
    ) {
        let selected = self
            .selected_mail_row()
            .filter(|_| self.mailbox.mailbox_view == MailboxView::Snoozed)
            .map(|row| row.thread_id);
        let wake_at: HashMap<MessageId, chrono::DateTime<chrono::Utc>> = snoozed
            .into_iter()
            .map(|entry| (entry.message_id, entry.wake_at))
            .collect();
        let mut envelopes: Vec<Envelope> = envelopes
            .into_iter()
            .filter(|envelope| wake_at.contains_key(&envelope.id))
            .collect();
        envelopes.sort_by_key(|envelope| wake_at.get(&envelope.id).copied());
        let page = &mut self.mailbox.snoozed_page;
        page.envelopes = envelopes;
        page.wake_at = wake_at;
        page.loaded = true;
        if self.mailbox.mailbox_view != MailboxView::Snoozed {
            return;
        }
        let rows = self.mail_list_rows();
        self.mailbox.selected_index = selected
            .and_then(|thread_id| rows.iter().position(|row| row.thread_id == thread_id))
            .unwrap_or(self.mailbox.selected_index)
            .min(rows.len().saturating_sub(1));
        if self.status_message.is_none() && rows.is_empty() {
            self.status_message =
                Some("Nothing snoozed. b on a conversation sends it away until later".into());
        }
    }

    fn wake(&mut self) {
        if self.modals.reply_queue.visible {
            self.wake_from_reply_queue();
            return;
        }
        if self.screen != Screen::Mailbox || self.mailbox.mailbox_view != MailboxView::Snoozed {
            self.status_message =
                Some("w wakes snoozed mail and reply-later messages; g n opens Snoozed".into());
            return;
        }
        let Some(row) = self.selected_mail_row() else {
            self.status_message = Some("Nothing snoozed to wake".into());
            return;
        };
        // The lit row only, as the web's row action: every snoozed message
        // in the conversation, or the one message in the messages view.
        let ids: Vec<MessageId> = self
            .mailbox
            .snoozed_page
            .envelopes
            .iter()
            .filter(|envelope| match self.mailbox.mail_list_mode {
                MailListMode::Threads => envelope.thread_id == row.thread_id,
                MailListMode::Messages => envelope.id == row.representative.id,
            })
            .map(|envelope| envelope.id.clone())
            .collect();
        // Off the list at once; a failure fetches it again.
        self.apply_removed_message_ids(&ids);
        // Back in the inbox, so its counts and the list behind it change.
        self.mailbox.pending_labels_refresh = true;
        self.mailbox.pending_all_envelopes_refresh = true;
        let last = ids.len().saturating_sub(1);
        for (index, message_id) in ids.into_iter().enumerate() {
            let done = if index == last {
                "Back in your inbox".to_string()
            } else {
                String::new()
            };
            self.queue_mutation(
                Request::Unsnooze { message_id },
                MutationEffect::StatusOnly(done),
                "Waking...".into(),
            );
        }
    }

    /// The reply queue's `w`: out of the queue, as the web's reply-later
    /// page does.
    fn wake_from_reply_queue(&mut self) {
        let queue = &mut self.modals.reply_queue;
        if queue.selected_index >= queue.messages.len() {
            self.status_message = Some("Nothing in the reply queue".into());
            return;
        }
        let envelope = queue.messages.remove(queue.selected_index);
        queue.selected_index = queue
            .selected_index
            .min(queue.messages.len().saturating_sub(1));
        let effect = MutationEffect::ReplyLater {
            message_id: envelope.id.clone(),
            flag: false,
            status: "Out of the reply queue".into(),
        };
        let snapshot = self.snapshot_for_effect(&effect);
        self.apply_local_mutation_effect(&effect);
        let mutation_id = self.queue_mutation(
            Request::SetReplyLater {
                message_id: envelope.id,
                flag: false,
            },
            effect,
            "Taking it out of the queue...".into(),
        );
        self.mutation_snapshots.insert(mutation_id, snapshot);
    }
}
