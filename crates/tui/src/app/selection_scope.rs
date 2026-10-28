use super::*;

/// What the mail list is showing. A selection is only valid for the list it
/// was made in, so a change to any part of this drops the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ListScope {
    screen: Screen,
    mailbox_view: MailboxView,
    active_label: Option<mxr_core::LabelId>,
    pending_label_fetch: Option<mxr_core::LabelId>,
    /// Bumped by every mailbox filter change.
    mailbox_filter: u64,
    search_query: Option<String>,
}

impl App {
    pub(super) fn list_scope(&self) -> ListScope {
        ListScope {
            screen: self.screen,
            mailbox_view: self.mailbox.mailbox_view,
            active_label: self.mailbox.active_label.clone(),
            pending_label_fetch: self.mailbox.pending_label_fetch.clone(),
            mailbox_filter: self.search.mailbox_session_id,
            search_query: (self.screen == Screen::Search).then(|| self.search.page.query.clone()),
        }
    }

    /// Pattern keys select the flat list that mutations read: the search
    /// results, or the plain message list with its pane focused.
    pub(super) fn pattern_select_applies(&self) -> bool {
        self.screen == Screen::Search
            // From the sidebar and the reader too: both sit beside the list,
            // and the web's `*` reaches the list from them the same way.
            || (self.screen == Screen::Mailbox
                && self.mailbox.mailbox_view == MailboxView::Messages)
    }

    /// Without this, `*a` or `V` in one label and then `gs` would leave the
    /// old selection behind, and archive would act on messages off screen.
    pub(super) fn drop_selection_on_view_change(&mut self, before: &ListScope) {
        if self.list_scope() == *before {
            return;
        }
        self.mailbox.visual_count = None;
        if self.mailbox.selected_set.is_empty() && !self.mailbox.visual_mode {
            return;
        }
        self.clear_selection();
        if self.status_message.is_none() {
            self.status_message = Some("Selection cleared".into());
        }
    }
}
