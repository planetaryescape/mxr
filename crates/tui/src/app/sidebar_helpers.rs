use super::*;

impl App {
    pub fn sidebar_items(&self) -> Vec<SidebarItem> {
        let mut items = Vec::new();
        // Accounts section (only shown when multiple accounts exist)
        let sync_accounts: Vec<_> = self
            .accounts
            .page
            .accounts
            .iter()
            .filter(|a| a.sync_kind.is_some())
            .collect();
        if sync_accounts.len() > 1 && self.mailbox.sidebar_accounts_expanded {
            items.extend(sync_accounts.into_iter().cloned().map(SidebarItem::Account));
        }
        let mut system_labels = Vec::new();
        let mut user_labels = Vec::new();
        for label in self.visible_labels() {
            if label.kind == LabelKind::System {
                system_labels.push(label.clone());
            } else {
                user_labels.push(label.clone());
            }
        }
        if self.mailbox.sidebar_system_expanded {
            items.extend(system_labels.into_iter().map(SidebarItem::Label));
        }
        items.push(SidebarItem::AllMail);
        items.push(SidebarItem::Subscriptions);
        if self.mailbox.sidebar_user_expanded {
            items.extend(user_labels.into_iter().map(SidebarItem::Label));
        }
        if self.mailbox.sidebar_saved_searches_expanded {
            items.extend(
                self.mailbox
                    .saved_searches
                    .iter()
                    .cloned()
                    .map(SidebarItem::SavedSearch),
            );
        }
        items
    }

    pub fn sidebar_view(&self) -> crate::ui::sidebar::SidebarView<'_> {
        use crate::ui::sidebar::{AccountInfo, SidebarView};
        let accounts: Vec<AccountInfo> = self
            .accounts
            .page
            .accounts
            .iter()
            .filter(|a| a.sync_kind.is_some())
            .map(|a| AccountInfo {
                email: a.email.clone(),
                is_default: a.is_default,
            })
            .collect();
        SidebarView {
            labels: &self.mailbox.labels,
            active_pane: &self.mailbox.active_pane,
            saved_searches: &self.mailbox.saved_searches,
            sidebar_selected: self.mailbox.sidebar_selected,
            all_mail_active: !self.search.active
                && self.mailbox.mailbox_view == MailboxView::Messages
                && self.mailbox.active_label.is_none()
                && self.mailbox.pending_active_label.is_none(),
            subscriptions_active: self.mailbox.mailbox_view == MailboxView::Subscriptions,
            subscription_count: self.mailbox.subscriptions_page.entries.len(),
            accounts,
            accounts_expanded: self.mailbox.sidebar_accounts_expanded,
            system_expanded: self.mailbox.sidebar_system_expanded,
            user_expanded: self.mailbox.sidebar_user_expanded,
            saved_searches_expanded: self.mailbox.sidebar_saved_searches_expanded,
            active_label: self
                .mailbox
                .pending_active_label
                .as_ref()
                .or(self.mailbox.active_label.as_ref()),
        }
    }

    pub fn selected_sidebar_item(&self) -> Option<SidebarItem> {
        self.sidebar_items()
            .get(self.mailbox.sidebar_selected)
            .cloned()
    }

    pub(crate) fn selected_sidebar_key(&self) -> Option<SidebarSelectionKey> {
        self.selected_sidebar_item().map(|item| match item {
            SidebarItem::Account(account) => {
                SidebarSelectionKey::Account(account.key.clone().unwrap_or_default())
            }
            SidebarItem::AllMail => SidebarSelectionKey::AllMail,
            SidebarItem::Subscriptions => SidebarSelectionKey::Subscriptions,
            SidebarItem::Label(label) => SidebarSelectionKey::Label(label.id),
            SidebarItem::SavedSearch(search) => SidebarSelectionKey::SavedSearch(search.name),
        })
    }

    pub(crate) fn restore_sidebar_selection(&mut self, selection: Option<SidebarSelectionKey>) {
        let items = self.sidebar_items();
        match selection.and_then(|selection| {
            items.iter().position(|item| match (item, &selection) {
                (SidebarItem::Account(account), SidebarSelectionKey::Account(key)) => {
                    account.key.as_deref() == Some(key.as_str())
                }
                (SidebarItem::AllMail, SidebarSelectionKey::AllMail) => true,
                (SidebarItem::Subscriptions, SidebarSelectionKey::Subscriptions) => true,
                (SidebarItem::Label(label), SidebarSelectionKey::Label(label_id)) => {
                    label.id == *label_id
                }
                (SidebarItem::SavedSearch(search), SidebarSelectionKey::SavedSearch(name)) => {
                    search.name == *name
                }
                _ => false,
            })
        }) {
            Some(index) => self.mailbox.sidebar_selected = index,
            None => {
                self.mailbox.sidebar_selected = self
                    .mailbox
                    .sidebar_selected
                    .min(items.len().saturating_sub(1));
            }
        }
        self.sync_sidebar_section();
    }

    pub fn ordered_visible_labels(&self) -> Vec<&Label> {
        let mut system: Vec<&Label> = self
            .mailbox
            .labels
            .iter()
            .filter(|l| !crate::ui::sidebar::should_hide_label(&l.name))
            .filter(|l| l.kind == mxr_core::types::LabelKind::System)
            .filter(|l| {
                crate::ui::sidebar::is_primary_system_label(&l.name)
                    || l.total_count > 0
                    || l.unread_count > 0
            })
            .collect();
        system.sort_by_key(|l| crate::ui::sidebar::system_label_order(&l.name));

        let mut user: Vec<&Label> = self
            .mailbox
            .labels
            .iter()
            .filter(|l| !crate::ui::sidebar::should_hide_label(&l.name))
            .filter(|l| l.kind != mxr_core::types::LabelKind::System)
            .collect();
        user.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let mut result = system;
        result.extend(user);
        result
    }

    pub fn visible_label_count(&self) -> usize {
        self.ordered_visible_labels().len()
    }

    pub fn visible_labels(&self) -> Vec<&Label> {
        self.ordered_visible_labels()
    }

    pub(super) fn sidebar_move_down(&mut self) {
        if self.mailbox.sidebar_selected + 1 < self.sidebar_items().len() {
            self.mailbox.sidebar_selected += 1;
        }
        self.sync_sidebar_section();
    }

    pub(super) fn sidebar_move_up(&mut self) {
        self.mailbox.sidebar_selected = self.mailbox.sidebar_selected.saturating_sub(1);
        self.sync_sidebar_section();
    }

    pub(super) fn sidebar_select(&mut self) -> Option<Action> {
        match self.selected_sidebar_item() {
            Some(SidebarItem::Account(account)) => {
                if let Some(key) = account.key {
                    if !account.is_default {
                        Some(Action::SwitchAccount(key))
                    } else {
                        None // already active
                    }
                } else {
                    None
                }
            }
            Some(SidebarItem::AllMail) => Some(Action::GoToAllMail),
            Some(SidebarItem::Subscriptions) => Some(Action::OpenSubscriptions),
            Some(SidebarItem::Label(label)) => Some(Action::SelectLabel(label.id)),
            Some(SidebarItem::SavedSearch(search)) => {
                Some(Action::SelectSavedSearch(search.query, search.search_mode))
            }
            None => None,
        }
    }

    pub(super) fn sync_sidebar_section(&mut self) {
        self.mailbox.sidebar_section = match self.selected_sidebar_item() {
            Some(SidebarItem::SavedSearch(_)) => SidebarSection::SavedSearches,
            _ => SidebarSection::Labels,
        };
    }

    pub(super) fn current_sidebar_group(&self) -> SidebarGroup {
        match self.selected_sidebar_item() {
            Some(SidebarItem::SavedSearch(_)) => SidebarGroup::SavedSearches,
            Some(SidebarItem::Label(label)) if label.kind == LabelKind::System => {
                SidebarGroup::SystemLabels
            }
            Some(SidebarItem::Label(_)) => SidebarGroup::UserLabels,
            Some(SidebarItem::Account(_))
            | Some(SidebarItem::AllMail)
            | Some(SidebarItem::Subscriptions)
            | None => SidebarGroup::SystemLabels,
        }
    }

    pub(super) fn collapse_current_sidebar_section(&mut self) {
        match self.current_sidebar_group() {
            SidebarGroup::SystemLabels => self.mailbox.sidebar_system_expanded = false,
            SidebarGroup::UserLabels => self.mailbox.sidebar_user_expanded = false,
            SidebarGroup::SavedSearches => self.mailbox.sidebar_saved_searches_expanded = false,
        }
        self.mailbox.sidebar_selected = self
            .mailbox
            .sidebar_selected
            .min(self.sidebar_items().len().saturating_sub(1));
        self.sync_sidebar_section();
    }

    pub(super) fn expand_current_sidebar_section(&mut self) {
        match self.current_sidebar_group() {
            SidebarGroup::SystemLabels => self.mailbox.sidebar_system_expanded = true,
            SidebarGroup::UserLabels => self.mailbox.sidebar_user_expanded = true,
            SidebarGroup::SavedSearches => self.mailbox.sidebar_saved_searches_expanded = true,
        }
        self.mailbox.sidebar_selected = self
            .mailbox
            .sidebar_selected
            .min(self.sidebar_items().len().saturating_sub(1));
        self.sync_sidebar_section();
    }
}
