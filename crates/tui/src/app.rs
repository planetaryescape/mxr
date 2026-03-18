use crate::action::{Action, PatternKind};
use crate::client::Client;
use crate::input::InputHandler;
use crate::ui;
use crate::ui::command_palette::CommandPalette;
use crate::ui::label_picker::{LabelPicker, LabelPickerMode};
use crate::ui::search_bar::SearchBar;
use crossterm::event::{KeyCode, KeyModifiers};
use mxr_core::id::MessageId;
use mxr_core::types::*;
use mxr_core::MxrError;
use mxr_protocol::{MutationCommand, Request};
use ratatui::prelude::*;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub enum MutationEffect {
    RemoveFromList(MessageId),
    UpdateFlags {
        message_id: MessageId,
        flags: MessageFlags,
    },
    RefreshList,
    StatusOnly(String),
}

#[derive(Debug, Clone)]
pub enum ComposeAction {
    New,
    Reply { message_id: MessageId },
    ReplyAll { message_id: MessageId },
    Forward { message_id: MessageId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    Sidebar,
    MailList,
    MessageView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSection {
    Labels,
    SavedSearches,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    TwoPane,
    ThreePane,
    FullScreen,
}

pub struct App {
    pub envelopes: Vec<Envelope>,
    pub all_envelopes: Vec<Envelope>,
    pub labels: Vec<Label>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub active_pane: ActivePane,
    pub should_quit: bool,
    pub layout_mode: LayoutMode,
    pub search_bar: SearchBar,
    pub command_palette: CommandPalette,
    pub message_body: Option<String>,
    pub viewing_envelope: Option<Envelope>,
    pub message_scroll_offset: u16,
    pub last_sync_status: Option<String>,
    pub visible_height: usize,
    pub pending_body_fetch: bool,
    pub pending_search: Option<String>,
    pub search_active: bool,
    pub sidebar_selected: usize,
    pub sidebar_section: SidebarSection,
    pub saved_searches: Vec<mxr_core::SavedSearch>,
    pub active_label: Option<mxr_core::LabelId>,
    pub pending_label_fetch: Option<mxr_core::LabelId>,
    pub status_message: Option<String>,
    pub pending_mutation_queue: Vec<(Request, MutationEffect)>,
    pub pending_compose: Option<ComposeAction>,
    pub reader_mode: bool,
    pub raw_body: Option<String>,
    pub viewing_body: Option<MessageBody>,
    pub label_picker: LabelPicker,
    pub selected_set: HashSet<MessageId>,
    pub visual_mode: bool,
    pub visual_anchor: Option<usize>,
    pub pending_export_thread: Option<mxr_core::id::ThreadId>,
    pending_label_action: Option<(LabelPickerMode, String)>,
    input: InputHandler,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            envelopes: Vec::new(),
            all_envelopes: Vec::new(),
            labels: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            active_pane: ActivePane::MailList,
            should_quit: false,
            layout_mode: LayoutMode::TwoPane,
            search_bar: SearchBar::default(),
            command_palette: CommandPalette::default(),
            message_body: None,
            viewing_envelope: None,
            message_scroll_offset: 0,
            last_sync_status: None,
            visible_height: 20,
            pending_body_fetch: false,
            pending_search: None,
            search_active: false,
            sidebar_selected: 0,
            sidebar_section: SidebarSection::Labels,
            saved_searches: Vec::new(),
            active_label: None,
            pending_label_fetch: None,
            status_message: None,
            pending_mutation_queue: Vec::new(),
            pending_compose: None,
            reader_mode: false,
            raw_body: None,
            viewing_body: None,
            label_picker: LabelPicker::default(),
            selected_set: HashSet::new(),
            visual_mode: false,
            visual_anchor: None,
            pending_export_thread: None,
            pending_label_action: None,
            input: InputHandler::new(),
        }
    }

    pub fn selected_envelope(&self) -> Option<&Envelope> {
        self.envelopes.get(self.selected_index)
    }

    /// Get the contextual envelope: the one being viewed, or the selected one.
    fn context_envelope(&self) -> Option<&Envelope> {
        self.viewing_envelope
            .as_ref()
            .or_else(|| self.selected_envelope())
    }

    pub async fn load(&mut self, client: &mut Client) -> Result<(), MxrError> {
        self.envelopes = client.list_envelopes(5000, 0).await?;
        self.all_envelopes = self.envelopes.clone();
        self.labels = client.list_labels().await?;
        self.saved_searches = client.list_saved_searches().await.unwrap_or_default();
        Ok(())
    }

    pub fn input_pending(&self) -> bool {
        self.input.is_pending()
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        // Route keys to search bar when active
        if self.search_bar.active {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return Some(Action::SubmitSearch),
                (KeyCode::Esc, _) => return Some(Action::CloseSearch),
                (KeyCode::Backspace, _) => {
                    self.search_bar.on_backspace();
                    // Live filter as you type
                    self.trigger_live_search();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.search_bar.on_char(c);
                    // Live filter as you type
                    self.trigger_live_search();
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to label picker when active
        if self.label_picker.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => {
                    let mode = self.label_picker.mode;
                    if let Some(label_name) = self.label_picker.confirm() {
                        self.pending_label_action = Some((mode, label_name));
                        return match mode {
                            LabelPickerMode::Apply => Some(Action::ApplyLabel),
                            LabelPickerMode::Move => Some(Action::MoveToLabel),
                        };
                    }
                    return None;
                }
                (KeyCode::Esc, _) => {
                    self.label_picker.close();
                    return None;
                }
                (KeyCode::Backspace, _) => {
                    self.label_picker.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.label_picker.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.label_picker.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.label_picker.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys based on active pane
        match self.active_pane {
            ActivePane::MessageView => match (key.code, key.modifiers) {
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.message_scroll_offset = self.message_scroll_offset.saturating_add(1);
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.message_scroll_offset = self.message_scroll_offset.saturating_sub(1);
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.message_scroll_offset = self.message_scroll_offset.saturating_add(20);
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    self.message_scroll_offset = self.message_scroll_offset.saturating_sub(20);
                    None
                }
                (KeyCode::Char('G'), KeyModifiers::SHIFT) => {
                    self.message_scroll_offset = u16::MAX;
                    None
                }
                // h = move left to mail list
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.active_pane = ActivePane::MailList;
                    None
                }
                _ => self.input.handle_key(key),
            },
            ActivePane::Sidebar => match (key.code, key.modifiers) {
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.sidebar_move_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.sidebar_move_up();
                    None
                }
                (KeyCode::Enter | KeyCode::Char('o'), _) => self.sidebar_select(),
                // l = select label and move to mail list
                (KeyCode::Char('l') | KeyCode::Right, KeyModifiers::NONE) => self.sidebar_select(),
                _ => self.input.handle_key(key),
            },
            ActivePane::MailList => match (key.code, key.modifiers) {
                // h = move left to sidebar
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.active_pane = ActivePane::Sidebar;
                    None
                }
                // l = open selected message
                (KeyCode::Char('l') | KeyCode::Right, KeyModifiers::NONE) => {
                    Some(Action::OpenSelected)
                }
                _ => self.input.handle_key(key),
            },
        }
    }

    pub fn tick(&mut self) {
        self.input.check_timeout();
    }

    pub fn apply(&mut self, action: Action) {
        // Clear status message on any action
        self.status_message = None;

        match action {
            Action::MoveDown => {
                if self.selected_index + 1 < self.envelopes.len() {
                    self.selected_index += 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::MoveUp => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::JumpTop => {
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.auto_preview();
            }
            Action::JumpBottom => {
                if !self.envelopes.is_empty() {
                    self.selected_index = self.envelopes.len() - 1;
                }
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageDown => {
                let page = self.visible_height.max(1);
                self.selected_index =
                    (self.selected_index + page).min(self.envelopes.len().saturating_sub(1));
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageUp => {
                let page = self.visible_height.max(1);
                self.selected_index = self.selected_index.saturating_sub(page);
                self.ensure_visible();
                self.auto_preview();
            }
            Action::ViewportTop => {
                self.selected_index = self.scroll_offset;
                self.auto_preview();
            }
            Action::ViewportMiddle => {
                let visible_height = 20;
                self.selected_index = (self.scroll_offset + visible_height / 2)
                    .min(self.envelopes.len().saturating_sub(1));
                self.auto_preview();
            }
            Action::ViewportBottom => {
                let visible_height = 20;
                self.selected_index = (self.scroll_offset + visible_height)
                    .min(self.envelopes.len().saturating_sub(1));
                self.auto_preview();
            }
            Action::CenterCurrent => {
                let visible_height = 20;
                self.scroll_offset = self.selected_index.saturating_sub(visible_height / 2);
            }
            Action::SwitchPane => {
                self.active_pane = match (self.layout_mode, self.active_pane) {
                    // ThreePane: Sidebar → MailList → MessageView → Sidebar
                    (LayoutMode::ThreePane, ActivePane::Sidebar) => ActivePane::MailList,
                    (LayoutMode::ThreePane, ActivePane::MailList) => ActivePane::MessageView,
                    (LayoutMode::ThreePane, ActivePane::MessageView) => ActivePane::Sidebar,
                    // TwoPane: Sidebar → MailList → Sidebar
                    (_, ActivePane::Sidebar) => ActivePane::MailList,
                    (_, ActivePane::MailList) => ActivePane::Sidebar,
                    (_, ActivePane::MessageView) => ActivePane::Sidebar,
                };
            }
            Action::OpenSelected => {
                if let Some(env) = self.envelopes.get(self.selected_index) {
                    self.viewing_envelope = Some(env.clone());
                    self.layout_mode = LayoutMode::ThreePane;
                    self.active_pane = ActivePane::MessageView;
                    self.message_body = None;
                    self.message_scroll_offset = 0;
                    self.pending_body_fetch = true;
                }
            }
            Action::Back => {
                match self.active_pane {
                    ActivePane::MessageView => {
                        self.active_pane = ActivePane::MailList;
                    }
                    ActivePane::MailList => {
                        if self.search_active {
                            self.apply(Action::CloseSearch);
                        } else if self.active_label.is_some() {
                            self.apply(Action::ClearFilter);
                        } else if self.layout_mode == LayoutMode::ThreePane {
                            self.apply(Action::CloseMessageView);
                        }
                    }
                    ActivePane::Sidebar => {}
                }
            }
            Action::QuitView => {
                self.should_quit = true;
            }
            // Search
            Action::OpenSearch => {
                self.search_bar.activate();
            }
            Action::SubmitSearch => {
                let query = self.search_bar.query.clone();
                self.search_bar.deactivate();
                if !query.is_empty() {
                    self.pending_search = Some(query);
                    self.search_active = true;
                }
                // Return focus to mail list so j/k navigates results
                self.active_pane = ActivePane::MailList;
            }
            Action::CloseSearch => {
                self.search_bar.deactivate();
                self.search_active = false;
                // Restore full envelope list
                self.envelopes = self.all_envelopes.clone();
                self.selected_index = 0;
                self.scroll_offset = 0;
            }
            Action::NextSearchResult => {
                if self.search_active && self.selected_index + 1 < self.envelopes.len() {
                    self.selected_index += 1;
                    self.ensure_visible();
                    self.auto_preview();
                }
            }
            Action::PrevSearchResult => {
                if self.search_active && self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.ensure_visible();
                    self.auto_preview();
                }
            }
            // Navigation
            Action::GoToInbox => {
                if let Some(label) = self.labels.iter().find(|l| l.name == "INBOX") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                }
            }
            Action::GoToStarred => {
                if let Some(label) = self.labels.iter().find(|l| l.name == "STARRED") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                }
            }
            Action::GoToSent => {
                if let Some(label) = self.labels.iter().find(|l| l.name == "SENT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                }
            }
            Action::GoToDrafts => {
                if let Some(label) = self.labels.iter().find(|l| l.name == "DRAFT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                }
            }
            Action::GoToAllMail => {
                self.apply(Action::ClearFilter);
            }
            Action::GoToLabel => {
                self.apply(Action::ClearFilter);
            }
            // Command palette
            Action::OpenCommandPalette => {
                self.command_palette.toggle();
            }
            Action::CloseCommandPalette => {
                self.command_palette.visible = false;
            }
            // Sync
            Action::SyncNow => {
                self.pending_mutation_queue.push((
                    Request::SyncNow { account_id: None },
                    MutationEffect::RefreshList,
                ));
                self.status_message = Some("Syncing...".into());
            }
            // Message view
            Action::OpenMessageView => {
                if let Some(env) = self.envelopes.get(self.selected_index) {
                    self.viewing_envelope = Some(env.clone());
                    self.layout_mode = LayoutMode::ThreePane;
                    self.message_body = None;
                    self.message_scroll_offset = 0;
                    self.pending_body_fetch = true;
                }
            }
            Action::CloseMessageView => {
                self.layout_mode = LayoutMode::TwoPane;
                self.active_pane = ActivePane::MailList;
                self.message_body = None;
                self.viewing_envelope = None;
                self.message_scroll_offset = 0;
            }
            Action::SelectLabel(label_id) => {
                self.active_label = Some(label_id.clone());
                self.pending_label_fetch = Some(label_id);
                self.active_pane = ActivePane::MailList;
            }
            Action::SelectSavedSearch(query) => {
                self.pending_search = Some(query);
                self.search_active = true;
                self.active_pane = ActivePane::MailList;
            }
            Action::ClearFilter => {
                self.active_label = None;
                self.search_active = false;
                self.envelopes = self.all_envelopes.clone();
                self.selected_index = 0;
                self.scroll_offset = 0;
            }

            // Phase 2: Email actions (Gmail-native A005)
            Action::Compose => {
                self.pending_compose = Some(ComposeAction::New);
            }
            Action::Reply => {
                if let Some(env) = self.context_envelope() {
                    self.pending_compose = Some(ComposeAction::Reply {
                        message_id: env.id.clone(),
                    });
                }
            }
            Action::ReplyAll => {
                if let Some(env) = self.context_envelope() {
                    self.pending_compose = Some(ComposeAction::ReplyAll {
                        message_id: env.id.clone(),
                    });
                }
            }
            Action::Forward => {
                if let Some(env) = self.context_envelope() {
                    self.pending_compose = Some(ComposeAction::Forward {
                        message_id: env.id.clone(),
                    });
                }
            }
            Action::Archive => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let first = ids[0].clone();
                    self.pending_mutation_queue.push((
                        Request::Mutation(MutationCommand::Archive {
                            message_ids: ids,
                        }),
                        MutationEffect::RemoveFromList(first),
                    ));
                    self.status_message = Some("Archiving...".into());
                    self.clear_selection();
                }
            }
            Action::Trash => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let first = ids[0].clone();
                    self.pending_mutation_queue.push((
                        Request::Mutation(MutationCommand::Trash {
                            message_ids: ids,
                        }),
                        MutationEffect::RemoveFromList(first),
                    ));
                    self.status_message = Some("Trashing...".into());
                    self.clear_selection();
                }
            }
            Action::Spam => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let first = ids[0].clone();
                    self.pending_mutation_queue.push((
                        Request::Mutation(MutationCommand::Spam {
                            message_ids: ids,
                        }),
                        MutationEffect::RemoveFromList(first),
                    ));
                    self.status_message = Some("Marking as spam...".into());
                    self.clear_selection();
                }
            }
            Action::Star => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    // For single selection, toggle. For multi, always star.
                    let starred = if ids.len() == 1 {
                        if let Some(env) = self.context_envelope() {
                            !env.flags.contains(MessageFlags::STARRED)
                        } else {
                            true
                        }
                    } else {
                        true
                    };
                    let first = ids[0].clone();
                    // For single message, provide flag update
                    let effect = if ids.len() == 1 {
                        if let Some(env) = self.context_envelope() {
                            let mut new_flags = env.flags;
                            if starred {
                                new_flags.insert(MessageFlags::STARRED);
                            } else {
                                new_flags.remove(MessageFlags::STARRED);
                            }
                            MutationEffect::UpdateFlags {
                                message_id: first.clone(),
                                flags: new_flags,
                            }
                        } else {
                            MutationEffect::RefreshList
                        }
                    } else {
                        MutationEffect::RefreshList
                    };
                    self.pending_mutation_queue.push((
                        Request::Mutation(MutationCommand::Star {
                            message_ids: ids,
                            starred,
                        }),
                        effect,
                    ));
                    self.status_message = Some(if starred {
                        "Starring...".into()
                    } else {
                        "Unstarring...".into()
                    });
                    self.clear_selection();
                }
            }
            Action::MarkRead => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let first = ids[0].clone();
                    let effect = if ids.len() == 1 {
                        if let Some(env) = self.context_envelope() {
                            let mut new_flags = env.flags;
                            new_flags.insert(MessageFlags::READ);
                            MutationEffect::UpdateFlags {
                                message_id: first.clone(),
                                flags: new_flags,
                            }
                        } else {
                            MutationEffect::RefreshList
                        }
                    } else {
                        MutationEffect::RefreshList
                    };
                    self.pending_mutation_queue.push((
                        Request::Mutation(MutationCommand::SetRead {
                            message_ids: ids,
                            read: true,
                        }),
                        effect,
                    ));
                    self.status_message = Some("Marking as read...".into());
                    self.clear_selection();
                }
            }
            Action::MarkUnread => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let first = ids[0].clone();
                    let effect = if ids.len() == 1 {
                        if let Some(env) = self.context_envelope() {
                            let mut new_flags = env.flags;
                            new_flags.remove(MessageFlags::READ);
                            MutationEffect::UpdateFlags {
                                message_id: first.clone(),
                                flags: new_flags,
                            }
                        } else {
                            MutationEffect::RefreshList
                        }
                    } else {
                        MutationEffect::RefreshList
                    };
                    self.pending_mutation_queue.push((
                        Request::Mutation(MutationCommand::SetRead {
                            message_ids: ids,
                            read: false,
                        }),
                        effect,
                    ));
                    self.status_message = Some("Marking as unread...".into());
                    self.clear_selection();
                }
            }
            Action::ApplyLabel => {
                if let Some((_, ref label_name)) = self.pending_label_action.take() {
                    // Label picker confirmed — dispatch mutation
                    let ids = self.mutation_target_ids();
                    if !ids.is_empty() {
                        self.pending_mutation_queue.push((
                            Request::Mutation(MutationCommand::ModifyLabels {
                                message_ids: ids,
                                add: vec![label_name.clone()],
                                remove: vec![],
                            }),
                            MutationEffect::StatusOnly(format!("Applied label '{}'", label_name)),
                        ));
                        self.status_message = Some(format!("Applying label '{}'...", label_name));
                        self.clear_selection();
                    }
                } else {
                    // Open label picker
                    self.label_picker.open(self.labels.clone(), LabelPickerMode::Apply);
                }
            }
            Action::MoveToLabel => {
                if let Some((_, ref label_name)) = self.pending_label_action.take() {
                    // Label picker confirmed — dispatch move
                    let ids = self.mutation_target_ids();
                    if !ids.is_empty() {
                        let first_id = ids[0].clone();
                        self.pending_mutation_queue.push((
                            Request::Mutation(MutationCommand::Move {
                                message_ids: ids,
                                target_label: label_name.clone(),
                            }),
                            MutationEffect::RemoveFromList(first_id),
                        ));
                        self.status_message = Some(format!("Moving to '{}'...", label_name));
                        self.clear_selection();
                    }
                } else {
                    // Open label picker
                    self.label_picker.open(self.labels.clone(), LabelPickerMode::Move);
                }
            }
            Action::Unsubscribe => {
                if let Some(env) = self.context_envelope() {
                    let id = env.id.clone();
                    self.pending_mutation_queue.push((
                        Request::Unsubscribe { message_id: id },
                        MutationEffect::StatusOnly("Unsubscribed".into()),
                    ));
                    self.status_message = Some("Unsubscribing...".into());
                }
            }
            Action::Snooze => {
                self.status_message = Some(
                    "Snooze requires duration input — use `mxr snooze <id> --until <time>` from CLI"
                        .into(),
                );
            }
            Action::OpenInBrowser => {
                if let Some(env) = self.context_envelope() {
                    let url = format!(
                        "https://mail.google.com/mail/u/0/#inbox/{}",
                        env.provider_id
                    );
                    #[cfg(target_os = "macos")]
                    let _ = std::process::Command::new("open").arg(&url).spawn();
                    #[cfg(target_os = "linux")]
                    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                    self.status_message = Some("Opened in browser".into());
                }
            }

            // Phase 2: Reader mode
            Action::ToggleReaderMode => {
                if self.raw_body.is_some() {
                    self.reader_mode = !self.reader_mode;
                    if self.reader_mode {
                        let config = mxr_reader::ReaderConfig::default();
                        let output = mxr_reader::clean(
                            self.raw_body.as_deref(),
                            None,
                            &config,
                        );
                        self.message_body = Some(output.content);
                    } else {
                        self.message_body = self.raw_body.clone();
                    }
                }
            }

            // Phase 2: Batch operations (A007)
            Action::ToggleSelect => {
                if let Some(env) = self.envelopes.get(self.selected_index) {
                    let id = env.id.clone();
                    if self.selected_set.contains(&id) {
                        self.selected_set.remove(&id);
                    } else {
                        self.selected_set.insert(id);
                    }
                    // Move to next after toggling
                    if self.selected_index + 1 < self.envelopes.len() {
                        self.selected_index += 1;
                        self.ensure_visible();
                    }
                    let count = self.selected_set.len();
                    self.status_message = Some(format!("{count} selected"));
                }
            }
            Action::VisualLineMode => {
                if self.visual_mode {
                    // Exit visual mode
                    self.visual_mode = false;
                    self.visual_anchor = None;
                    self.status_message = Some("Visual mode off".into());
                } else {
                    self.visual_mode = true;
                    self.visual_anchor = Some(self.selected_index);
                    // Add current to selection
                    if let Some(env) = self.envelopes.get(self.selected_index) {
                        self.selected_set.insert(env.id.clone());
                    }
                    self.status_message = Some("-- VISUAL LINE --".into());
                }
            }
            Action::PatternSelect(pattern) => {
                match pattern {
                    PatternKind::All => {
                        self.selected_set = self.envelopes.iter().map(|e| e.id.clone()).collect();
                    }
                    PatternKind::None => {
                        self.selected_set.clear();
                        self.visual_mode = false;
                        self.visual_anchor = None;
                    }
                    PatternKind::Read => {
                        self.selected_set = self
                            .envelopes
                            .iter()
                            .filter(|e| e.flags.contains(MessageFlags::READ))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Unread => {
                        self.selected_set = self
                            .envelopes
                            .iter()
                            .filter(|e| !e.flags.contains(MessageFlags::READ))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Starred => {
                        self.selected_set = self
                            .envelopes
                            .iter()
                            .filter(|e| e.flags.contains(MessageFlags::STARRED))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Thread => {
                        if let Some(env) = self.context_envelope() {
                            let tid = env.thread_id.clone();
                            self.selected_set = self
                                .envelopes
                                .iter()
                                .filter(|e| e.thread_id == tid)
                                .map(|e| e.id.clone())
                                .collect();
                        }
                    }
                }
                let count = self.selected_set.len();
                self.status_message = Some(format!("{count} selected"));
            }

            // Phase 2: Other actions
            Action::AttachmentList => {
                if let Some(ref body) = self.viewing_body {
                    if body.attachments.is_empty() {
                        self.status_message = Some("No attachments".into());
                    } else {
                        let list: Vec<String> = body
                            .attachments
                            .iter()
                            .enumerate()
                            .map(|(i, a)| format!("{}:{}", i + 1, a.filename))
                            .collect();
                        self.status_message =
                            Some(format!("Attachments: {}", list.join(", ")));
                    }
                } else {
                    self.status_message = Some("No message body loaded".into());
                }
            }
            Action::ToggleFullscreen => {
                if self.layout_mode == LayoutMode::FullScreen {
                    self.layout_mode = LayoutMode::ThreePane;
                } else if self.viewing_envelope.is_some() {
                    self.layout_mode = LayoutMode::FullScreen;
                }
            }
            Action::ExportThread => {
                if let Some(env) = self.context_envelope() {
                    self.pending_export_thread = Some(env.thread_id.clone());
                    self.status_message = Some("Exporting thread...".into());
                } else {
                    self.status_message = Some("No message selected".into());
                }
            }
            Action::Help => {
                self.status_message = Some(
                    "j/k:nav o:open c:compose r:reply e:archive #:trash !:spam s:star /:search ?:help q:quit"
                        .into(),
                );
            }
            Action::Noop => {}
        }
    }

    /// Returns the ordered list of visible labels (system first, then user, no separator).
    pub fn ordered_visible_labels(&self) -> Vec<&Label> {
        let mut system: Vec<&Label> = self.labels.iter()
            .filter(|l| !crate::ui::sidebar::should_hide_label(&l.name))
            .filter(|l| l.kind == mxr_core::types::LabelKind::System)
            .filter(|l| {
                crate::ui::sidebar::is_primary_system_label(&l.name) || l.total_count > 0 || l.unread_count > 0
            })
            .collect();
        system.sort_by_key(|l| crate::ui::sidebar::system_label_order(&l.name));

        let mut user: Vec<&Label> = self.labels.iter()
            .filter(|l| !crate::ui::sidebar::should_hide_label(&l.name))
            .filter(|l| l.kind != mxr_core::types::LabelKind::System)
            .collect();
        user.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let mut result = system;
        result.extend(user);
        result
    }

    /// Number of visible (non-hidden) labels.
    pub fn visible_label_count(&self) -> usize {
        self.ordered_visible_labels().len()
    }

    /// Get the visible (filtered) labels.
    pub fn visible_labels(&self) -> Vec<&Label> {
        self.ordered_visible_labels()
    }

    fn sidebar_move_down(&mut self) {
        match self.sidebar_section {
            SidebarSection::Labels => {
                if self.sidebar_selected + 1 < self.visible_label_count() {
                    self.sidebar_selected += 1;
                } else if !self.saved_searches.is_empty() {
                    // Wrap into saved searches
                    self.sidebar_section = SidebarSection::SavedSearches;
                    self.sidebar_selected = 0;
                }
            }
            SidebarSection::SavedSearches => {
                if self.sidebar_selected + 1 < self.saved_searches.len() {
                    self.sidebar_selected += 1;
                }
            }
        }
    }

    fn sidebar_move_up(&mut self) {
        match self.sidebar_section {
            SidebarSection::Labels => {
                self.sidebar_selected = self.sidebar_selected.saturating_sub(1);
            }
            SidebarSection::SavedSearches => {
                if self.sidebar_selected > 0 {
                    self.sidebar_selected -= 1;
                } else {
                    // Wrap back into labels
                    self.sidebar_section = SidebarSection::Labels;
                    self.sidebar_selected = self.visible_label_count().saturating_sub(1);
                }
            }
        }
    }

    fn sidebar_select(&mut self) -> Option<Action> {
        match self.sidebar_section {
            SidebarSection::Labels => {
                let visible = self.visible_labels();
                visible
                    .get(self.sidebar_selected)
                    .map(|label| Action::SelectLabel(label.id.clone()))
            }
            SidebarSection::SavedSearches => self
                .saved_searches
                .get(self.sidebar_selected)
                .map(|search| Action::SelectSavedSearch(search.query.clone())),
        }
    }

    /// Live filter: instant client-side prefix matching on subject/from/snippet,
    /// plus async Tantivy search for full-text body matches.
    fn trigger_live_search(&mut self) {
        let query = self.search_bar.query.to_lowercase();
        if query.is_empty() {
            self.envelopes = self.all_envelopes.clone();
            self.search_active = false;
        } else {
            let query_words: Vec<&str> = query.split_whitespace().collect();
            // Instant client-side filter: every query word must prefix-match
            // some word in subject, from, or snippet
            self.envelopes = self
                .all_envelopes
                .iter()
                .filter(|e| {
                    let haystack = format!(
                        "{} {} {} {}",
                        e.subject,
                        e.from.email,
                        e.from.name.as_deref().unwrap_or(""),
                        e.snippet
                    )
                    .to_lowercase();
                    query_words.iter().all(|qw| {
                        haystack.split_whitespace().any(|hw| hw.starts_with(qw))
                            || haystack.contains(qw)
                    })
                })
                .cloned()
                .collect();
            self.search_active = true;
            // Also fire async Tantivy search to catch body matches
            self.pending_search = Some(self.search_bar.query.clone());
        }
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Compute the mail list title based on active filter/search.
    pub fn mail_list_title(&self) -> String {
        if self.search_active {
            format!("Search: {} ({})", self.search_bar.query, self.envelopes.len())
        } else if let Some(ref label_id) = self.active_label {
            if let Some(label) = self.labels.iter().find(|l| &l.id == label_id) {
                let name = crate::ui::sidebar::humanize_label(&label.name);
                format!("{} ({})", name, self.envelopes.len())
            } else {
                format!("Messages ({})", self.envelopes.len())
            }
        } else {
            format!("Messages ({})", self.envelopes.len())
        }
    }

    /// In ThreePane mode, auto-load the preview for the currently selected envelope.
    fn auto_preview(&mut self) {
        if self.layout_mode == LayoutMode::ThreePane {
            if let Some(env) = self.envelopes.get(self.selected_index) {
                if self.viewing_envelope.as_ref().map(|e| &e.id) != Some(&env.id) {
                    self.viewing_envelope = Some(env.clone());
                    self.message_body = None;
                    self.raw_body = None;
                    self.reader_mode = false;
                    self.message_scroll_offset = 0;
                    self.pending_body_fetch = true;
                }
            }
        }
    }

    /// Get IDs to mutate: selected_set if non-empty, else context_envelope.
    fn mutation_target_ids(&self) -> Vec<MessageId> {
        if !self.selected_set.is_empty() {
            self.selected_set.iter().cloned().collect()
        } else if let Some(env) = self.context_envelope() {
            vec![env.id.clone()]
        } else {
            vec![]
        }
    }

    fn clear_selection(&mut self) {
        self.selected_set.clear();
        self.visual_mode = false;
        self.visual_anchor = None;
    }

    /// Update visual selection range when moving in visual mode.
    fn update_visual_selection(&mut self) {
        if self.visual_mode {
            if let Some(anchor) = self.visual_anchor {
                let start = anchor.min(self.selected_index);
                let end = anchor.max(self.selected_index);
                self.selected_set.clear();
                for env in self.envelopes.iter().skip(start).take(end - start + 1) {
                    self.selected_set.insert(env.id.clone());
                }
            }
        }
    }

    /// Ensure selected_index is visible within the scroll viewport.
    fn ensure_visible(&mut self) {
        let h = self.visible_height.max(1);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + h {
            self.scroll_offset = self.selected_index + 1 - h;
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Layout: hint bar (1 line) | content | status bar (1 line)
        let outer_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // hint bar
                Constraint::Min(0),    // content
                Constraint::Length(1), // status bar
            ])
            .split(area);

        let hint_bar_area = outer_chunks[0];
        let content_area = outer_chunks[1];
        // Update visible height based on actual terminal size (subtract borders/header)
        self.visible_height = content_area.height.saturating_sub(2) as usize;
        let bottom_bar_area = outer_chunks[2];

        // Hint bar
        ui::hint_bar::draw(
            frame,
            hint_bar_area,
            &self.active_pane,
            self.search_bar.active,
        );

        match self.layout_mode {
            LayoutMode::TwoPane => {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
                    .split(content_area);

                ui::sidebar::draw(
                    frame,
                    chunks[0],
                    &self.labels,
                    &self.active_pane,
                    &self.saved_searches,
                    &self.sidebar_section,
                    self.sidebar_selected,
                    self.active_label.as_ref(),
                );

                let mail_title = self.mail_list_title();
                ui::mail_list::draw_with_selection(
                    frame,
                    chunks[1],
                    &self.envelopes,
                    self.selected_index,
                    self.scroll_offset,
                    &self.active_pane,
                    &mail_title,
                    &self.selected_set,
                );
            }
            LayoutMode::ThreePane => {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(15),
                        Constraint::Percentage(35),
                        Constraint::Percentage(50),
                    ])
                    .split(content_area);

                ui::sidebar::draw(
                    frame,
                    chunks[0],
                    &self.labels,
                    &self.active_pane,
                    &self.saved_searches,
                    &self.sidebar_section,
                    self.sidebar_selected,
                    self.active_label.as_ref(),
                );

                let mail_title = self.mail_list_title();
                ui::mail_list::draw_with_selection(
                    frame,
                    chunks[1],
                    &self.envelopes,
                    self.selected_index,
                    self.scroll_offset,
                    &self.active_pane,
                    &mail_title,
                    &self.selected_set,
                );
                ui::message_view::draw(
                    frame,
                    chunks[2],
                    self.message_body.as_deref(),
                    self.viewing_envelope.as_ref(),
                    self.message_scroll_offset,
                    &self.active_pane,
                );
            }
            LayoutMode::FullScreen => {
                ui::message_view::draw(
                    frame,
                    content_area,
                    self.message_body.as_deref(),
                    self.viewing_envelope.as_ref(),
                    self.message_scroll_offset,
                    &self.active_pane,
                );
            }
        }

        // Bottom bar: search bar takes priority over status bar
        if self.search_bar.active {
            ui::search_bar::draw(frame, bottom_bar_area, &self.search_bar);
        } else {
            ui::status_bar::draw(
                frame,
                bottom_bar_area,
                &self.envelopes,
                self.last_sync_status.as_deref(),
                self.status_message.as_deref(),
            );
        }

        // Command palette overlay
        ui::command_palette::draw(frame, area, &self.command_palette);

        // Label picker overlay
        ui::label_picker::draw(frame, area, &self.label_picker);
    }
}
