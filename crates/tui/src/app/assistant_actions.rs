//! The assistant pane's keys and requests. The pane only asks the daemon
//! and shows what it answers: every change the assistant makes waits for
//! an `AssistantDecide` from these keys, never for anything the
//! conversation's text says.

use super::*;
use mxr_protocol::{
    AssistantAnswerData, AssistantConversationData, AssistantDecisionData,
    AssistantDecisionKindData, AssistantPendingData, AssistantProgressData, AssistantStatusData,
    AssistantViewContextData,
};
use ratatui::crossterm::event::KeyEvent;

/// How many past conversations the history list asks for.
const HISTORY_LIMIT: u32 = 30;
/// Selected ids sent as context, at most (the web sends as many).
const CONTEXT_SELECTION_LIMIT: usize = 200;

fn error_words(error: &MxrError) -> String {
    match error {
        MxrError::Ipc(message) => message.clone(),
        other => other.to_string(),
    }
}

fn typed_char(key: &KeyEvent) -> Option<char> {
    match (key.code, key.modifiers) {
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => Some(c),
        _ => None,
    }
}

impl App {
    /// A text field outside the pane has the keys, so Ctrl-a stays its own.
    fn text_entry_active(&self) -> bool {
        (self.screen == Screen::Rules && self.rules.page.form.visible)
            || (self.screen == Screen::Accounts && self.accounts.page.form.editing_field)
            || (self.screen == Screen::Search && self.search.page.editing)
    }

    /// Keys for the pane. `None`: not the pane's key, so the screen behind
    /// it gets it. While the pane has focus it takes every key, so typing
    /// never reaches the screen's shortcuts.
    pub(super) fn assistant_key(&mut self, key: KeyEvent) -> Option<Option<Action>> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('a') && !self.text_entry_active() {
            return Some(Some(Action::ToggleAssistant));
        }
        if !self.assistant.open || self.assistant.focus == AssistantFocus::Away {
            return None;
        }
        if ctrl && key.code == KeyCode::Char('p') {
            return Some(Some(Action::OpenCommandPalette));
        }
        if ctrl && key.code == KeyCode::Char('c') {
            self.stop_assistant_turn();
            return Some(None);
        }
        // A delete confirm can sit over the history list and answers first.
        let confirming = matches!(
            self.assistant.prompt,
            Some(AssistantPrompt::ConfirmDelete(_))
        );
        if self.assistant.list.is_some() && !confirming {
            self.assistant_list_key(key);
            return Some(None);
        }
        if self.assistant.prompt.is_some() {
            self.assistant_prompt_key(key);
            return Some(None);
        }
        match self.assistant.focus {
            AssistantFocus::Input => self.assistant_input_key(key),
            AssistantFocus::Pane => return Some(self.assistant_pane_key(key)),
            AssistantFocus::Away => {}
        }
        Some(None)
    }

    fn assistant_input_key(&mut self, key: KeyEvent) {
        let input = &mut self.assistant.input;
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => self.assistant.focus = AssistantFocus::Pane,
            // Terminals rarely report Shift-Enter, so a new line is
            // Alt-Enter or Ctrl-j.
            (KeyCode::Enter, modifiers) if modifiers.contains(KeyModifiers::ALT) => {
                input.push('\n');
            }
            (KeyCode::Char('j'), KeyModifiers::CONTROL) => input.push('\n'),
            (KeyCode::Enter, _) => self.send_assistant_message(),
            (KeyCode::Backspace, _) => {
                input.pop();
            }
            _ => {
                if let Some(c) = typed_char(&key) {
                    input.push(c);
                }
            }
        }
    }

    fn assistant_pane_key(&mut self, key: KeyEvent) -> Option<Action> {
        let question = self
            .assistant
            .pending()
            .iter()
            .find_map(|pending| match pending {
                AssistantPendingData::Question { .. } => Some(pending.clone()),
                AssistantPendingData::Approval { .. } => None,
            });
        let approval = self
            .assistant
            .pending()
            .first()
            .and_then(|pending| match pending {
                AssistantPendingData::Approval { call_id, .. } => Some(call_id.clone()),
                AssistantPendingData::Question { .. } => None,
            });
        if let Some(call_id) = approval {
            match (key.code, key.modifiers) {
                (KeyCode::Char('y'), KeyModifiers::NONE) => {
                    self.decide_assistant_call(call_id, AssistantDecisionKindData::Approve);
                    return None;
                }
                (KeyCode::Char('n'), KeyModifiers::NONE) => {
                    self.assistant.prompt = Some(AssistantPrompt::RejectReason {
                        call_id,
                        text: String::new(),
                    });
                    return None;
                }
                (KeyCode::Char('x'), KeyModifiers::NONE) => {
                    let shown = self.assistant.exact_call_for.as_deref() == Some(call_id.as_str());
                    self.assistant.exact_call_for = (!shown).then_some(call_id);
                    return None;
                }
                _ => {}
            }
        } else if let Some(AssistantPendingData::Question {
            call_id, questions, ..
        }) = question
        {
            let active = self
                .assistant
                .active_question
                .min(questions.len().saturating_sub(1));
            match (key.code, key.modifiers) {
                (KeyCode::Char(digit @ '1'..='9'), KeyModifiers::NONE) => {
                    let index = (digit as usize) - ('1' as usize);
                    if let Some(question) = questions.get(active) {
                        if let Some(option) = question.options.get(index) {
                            self.toggle_assistant_option(
                                active,
                                &option.label,
                                question.multi_select,
                            );
                        }
                    }
                    return None;
                }
                (KeyCode::Char('o'), KeyModifiers::NONE) => {
                    if questions
                        .get(active)
                        .is_some_and(|question| question.allow_other)
                    {
                        let text = self
                            .assistant
                            .answers
                            .get(active)
                            .map(|answer| answer.other.clone())
                            .unwrap_or_default();
                        self.assistant.prompt = Some(AssistantPrompt::Other { text });
                    }
                    return None;
                }
                (KeyCode::Tab, _) if questions.len() > 1 => {
                    self.assistant.active_question = (active + 1) % questions.len();
                    return None;
                }
                (KeyCode::BackTab, _) if questions.len() > 1 => {
                    self.assistant.active_question =
                        (active + questions.len() - 1) % questions.len();
                    return None;
                }
                (KeyCode::Enter, _) => {
                    self.submit_assistant_answers(call_id);
                    return None;
                }
                (KeyCode::Char('s'), KeyModifiers::NONE) | (KeyCode::Esc, _) => {
                    self.decide_assistant_call(
                        call_id,
                        AssistantDecisionKindData::Reject { reason: None },
                    );
                    return None;
                }
                _ => {}
            }
        }

        let page = 8;
        let assistant = &mut self.assistant;
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => assistant.focus = AssistantFocus::Away,
            (KeyCode::Char('i') | KeyCode::Enter, KeyModifiers::NONE) => {
                if assistant.available() {
                    assistant.focus = AssistantFocus::Input;
                }
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                assistant.scroll_from_bottom = assistant.scroll_from_bottom.saturating_sub(1);
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                assistant.scroll_from_bottom =
                    (assistant.scroll_from_bottom + 1).min(assistant.max_scroll);
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) | (KeyCode::PageDown, _) => {
                assistant.scroll_from_bottom = assistant.scroll_from_bottom.saturating_sub(page);
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::PageUp, _) => {
                assistant.scroll_from_bottom =
                    (assistant.scroll_from_bottom + page).min(assistant.max_scroll);
            }
            (KeyCode::Char('G'), _) => assistant.scroll_from_bottom = 0,
            (KeyCode::Char('e'), KeyModifiers::NONE) => {
                assistant.steps_expanded = !assistant.steps_expanded;
            }
            (KeyCode::Char('N'), _) => return Some(Action::NewAssistantConversation),
            (KeyCode::Char('L'), _) => return Some(Action::OpenAssistantConversations),
            (KeyCode::Char('T'), _) => return Some(Action::ToggleAssistantTrust),
            (KeyCode::Char('D'), _) => return Some(Action::DeleteAssistantConversation),
            _ => {}
        }
        None
    }

    fn assistant_list_key(&mut self, key: KeyEvent) {
        let Some(list) = self.assistant.list.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if list.selected + 1 < list.conversations.len() {
                    list.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => list.selected = list.selected.saturating_sub(1),
            KeyCode::Enter => {
                let picked = list
                    .conversations
                    .get(list.selected)
                    .map(|summary| summary.id.clone());
                self.assistant.list = None;
                if let Some(id) = picked {
                    self.show_assistant_conversation(id);
                }
            }
            KeyCode::Char('D') => {
                if let Some(summary) = list.conversations.get(list.selected) {
                    let conversation_id = summary.id.clone();
                    self.assistant.error = None;
                    self.assistant.queue(
                        AssistantCall::DeletePreview,
                        Request::DeleteAssistantConversation {
                            conversation_id,
                            dry_run: true,
                        },
                    );
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.assistant.list = None;
                self.assistant.error = None;
            }
            _ => {}
        }
    }

    fn assistant_prompt_key(&mut self, key: KeyEvent) {
        let Some(prompt) = self.assistant.prompt.as_mut() else {
            return;
        };
        match prompt {
            AssistantPrompt::ConfirmDelete(delete) => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let conversation_id = delete.conversation_id.clone();
                    self.assistant.prompt = None;
                    self.assistant.queue(
                        AssistantCall::Delete,
                        Request::DeleteAssistantConversation {
                            conversation_id,
                            dry_run: false,
                        },
                    );
                }
                KeyCode::Char('n') | KeyCode::Esc => self.assistant.prompt = None,
                _ => {}
            },
            AssistantPrompt::RejectReason { call_id, text } => match key.code {
                KeyCode::Esc => self.assistant.prompt = None,
                KeyCode::Enter => {
                    let call_id = call_id.clone();
                    let reason = Some(text.trim().to_string()).filter(|reason| !reason.is_empty());
                    self.decide_assistant_call(
                        call_id,
                        AssistantDecisionKindData::Reject { reason },
                    );
                }
                KeyCode::Backspace => {
                    text.pop();
                }
                _ => {
                    if let Some(c) = typed_char(&key) {
                        text.push(c);
                    }
                }
            },
            AssistantPrompt::Other { text } => match key.code {
                KeyCode::Esc => self.assistant.prompt = None,
                KeyCode::Enter => {
                    let typed = std::mem::take(text);
                    self.assistant.prompt = None;
                    self.save_assistant_other(typed);
                }
                KeyCode::Backspace => {
                    text.pop();
                }
                _ => {
                    if let Some(c) = typed_char(&key) {
                        text.push(c);
                    }
                }
            },
        }
    }

    /// One option chosen or unchosen. A single answer is an option or
    /// typed text, not both.
    fn toggle_assistant_option(&mut self, question: usize, label: &str, multi_select: bool) {
        let Some(answer) = self.assistant.answers.get_mut(question) else {
            return;
        };
        let chosen = answer.selected.iter().any(|selected| selected == label);
        if multi_select {
            if chosen {
                answer.selected.retain(|selected| selected != label);
            } else {
                answer.selected.push(label.to_string());
            }
        } else if chosen {
            answer.selected.clear();
        } else {
            answer.selected = vec![label.to_string()];
            answer.other.clear();
        }
    }

    /// The user's own answer for the active question; with every question
    /// answered, it goes.
    fn save_assistant_other(&mut self, typed: String) {
        let active = self.assistant.active_question;
        let multi_select = self
            .assistant
            .pending()
            .iter()
            .find_map(|pending| match pending {
                AssistantPendingData::Question { questions, .. } => {
                    questions.get(active).map(|question| question.multi_select)
                }
                AssistantPendingData::Approval { .. } => None,
            })
            .unwrap_or(false);
        let Some(answer) = self.assistant.answers.get_mut(active) else {
            return;
        };
        answer.other = typed;
        if !multi_select && !answer.other.trim().is_empty() {
            answer.selected.clear();
        }
        if let Some(next) = self
            .assistant
            .answers
            .iter()
            .position(|answer| !answer.is_complete())
        {
            self.assistant.active_question = next;
        } else if let Some(call_id) = self.assistant.answers_for.clone() {
            self.submit_assistant_answers(call_id);
        }
    }

    fn submit_assistant_answers(&mut self, call_id: String) {
        if self.assistant.answers_for.as_deref() != Some(call_id.as_str()) {
            return;
        }
        if let Some(missing) = self
            .assistant
            .answers
            .iter()
            .position(|answer| !answer.is_complete())
        {
            self.assistant.active_question = missing;
            self.assistant.error = Some("Choose an answer first, or press s to skip.".into());
            return;
        }
        let answers = self
            .assistant
            .answers
            .iter()
            .map(|answer| AssistantAnswerData {
                selected: answer.selected.clone(),
                other: Some(answer.other.trim().to_string()).filter(|other| !other.is_empty()),
            })
            .collect();
        self.decide_assistant_call(call_id, AssistantDecisionKindData::Answer { answers });
    }

    fn decide_assistant_call(&mut self, call_id: String, decision: AssistantDecisionKindData) {
        let Some(conversation_id) = self.assistant.conversation_id.clone() else {
            return;
        };
        // One decision per waiting call: a second key press before the
        // daemon answers must not decide again.
        if self.assistant.busy {
            return;
        }
        self.assistant.busy = true;
        self.assistant.prompt = None;
        self.assistant.error = None;
        self.assistant.queue(
            AssistantCall::Conversation,
            Request::AssistantDecide {
                conversation_id,
                decisions: vec![AssistantDecisionData { call_id, decision }],
            },
        );
    }

    fn send_assistant_message(&mut self) {
        let message = self.assistant.input.trim().to_string();
        if message.is_empty() || self.assistant.running() || !self.assistant.available() {
            return;
        }
        let context = self.assistant_view_context();
        let assistant = &mut self.assistant;
        assistant.queue(
            AssistantCall::Conversation,
            Request::AssistantSend {
                conversation_id: assistant.conversation_id.clone(),
                message: message.clone(),
                context,
            },
        );
        assistant.input.clear();
        assistant.input_sent = Some(message);
        assistant.sending = true;
        assistant.started_at = Some(Instant::now());
        assistant.progress = None;
        assistant.error = None;
        assistant.prompt = None;
        assistant.scroll_from_bottom = 0;
    }

    fn stop_assistant_turn(&mut self) {
        if !self.assistant.running() {
            return;
        }
        if let Some(conversation_id) = self.assistant.conversation_id.clone() {
            self.assistant.queue(
                AssistantCall::Conversation,
                Request::AssistantCancel { conversation_id },
            );
        }
    }

    fn open_assistant(&mut self) {
        let assistant = &mut self.assistant;
        if !assistant.open {
            assistant.open = true;
            assistant.queue(
                AssistantCall::Availability,
                Request::GetAssistantAvailability,
            );
            assistant.refetch();
        }
        assistant.focus = if assistant.pending().is_empty() && assistant.available() {
            AssistantFocus::Input
        } else {
            AssistantFocus::Pane
        };
    }

    fn show_assistant_conversation(&mut self, conversation_id: String) {
        self.assistant.reset_conversation();
        self.assistant.conversation_id = Some(conversation_id);
        self.assistant.refetch();
        self.assistant.focus = AssistantFocus::Pane;
    }

    pub(super) fn apply_assistant_action(&mut self, action: Action) {
        match action {
            // Opens the pane, moves the keys back into it, or hides it.
            Action::ToggleAssistant => {
                if self.assistant.open && self.assistant.focus != AssistantFocus::Away {
                    self.assistant.open = false;
                    self.assistant.focus = AssistantFocus::Away;
                    self.assistant.list = None;
                    if matches!(
                        self.assistant.prompt,
                        Some(AssistantPrompt::ConfirmDelete(_))
                    ) {
                        self.assistant.prompt = None;
                    }
                } else {
                    self.open_assistant();
                }
            }
            Action::NewAssistantConversation => {
                self.open_assistant();
                self.assistant.reset_conversation();
                self.assistant.list = None;
                if self.assistant.available() {
                    self.assistant.focus = AssistantFocus::Input;
                }
            }
            Action::OpenAssistantConversations => {
                self.open_assistant();
                self.assistant.focus = AssistantFocus::Pane;
                self.assistant.list = Some(AssistantListState {
                    loading: true,
                    ..AssistantListState::default()
                });
                self.assistant.queue(
                    AssistantCall::List,
                    Request::ListAssistantConversations {
                        limit: Some(HISTORY_LIMIT),
                    },
                );
            }
            Action::ToggleAssistantTrust => {
                let Some(conversation) = self.assistant.conversation.as_ref() else {
                    self.status_message =
                        Some("Start a conversation first, then choose what it may do".into());
                    return;
                };
                let enabled = !conversation.auto_approve;
                let conversation_id = conversation.id.clone();
                self.assistant.queue(
                    AssistantCall::Conversation,
                    Request::SetAssistantAutoApprove {
                        conversation_id,
                        enabled,
                    },
                );
                self.status_message = Some(if enabled {
                    "Reversible changes will run without asking in this conversation".into()
                } else {
                    "Every change will ask first again".into()
                });
            }
            Action::DeleteAssistantConversation => {
                let Some(conversation_id) = self.assistant.conversation_id.clone() else {
                    self.status_message = Some("No conversation to delete".into());
                    return;
                };
                if self.assistant.running() {
                    self.assistant.error = Some("Stop the turn first (Ctrl-c).".into());
                    return;
                }
                self.open_assistant();
                self.assistant.focus = AssistantFocus::Pane;
                self.assistant.queue(
                    AssistantCall::DeletePreview,
                    Request::DeleteAssistantConversation {
                        conversation_id,
                        dry_run: true,
                    },
                );
            }
            _ => {}
        }
    }

    /// What is open, so "this" and "these" mean something to the assistant.
    pub(crate) fn assistant_view_context(&self) -> AssistantViewContextData {
        let view = match self.screen {
            Screen::Mailbox => match self.mailbox.mailbox_view {
                MailboxView::Messages => "mail".to_string(),
                MailboxView::Subscriptions => "subscriptions".into(),
                MailboxView::Owed => "owed".into(),
                MailboxView::Desk => "desk".into(),
                MailboxView::CalendarInvites => "invites".into(),
                MailboxView::Place(mxr_protocol::MailPlaceData::Reading) => "reading".into(),
                MailboxView::Place(mxr_protocol::MailPlaceData::PaperTrail) => "paper-trail".into(),
                MailboxView::Todo => "todo".into(),
                MailboxView::Now => "now".into(),
                MailboxView::Reading => "reading".into(),
                MailboxView::Updates => "updates".into(),
                MailboxView::ArchiveMode => "archive".into(),
                MailboxView::People => "messages".into(),
            },
            Screen::Search => "search".into(),
            Screen::Rules => "rules".into(),
            Screen::Diagnostics => "diagnostics".into(),
            Screen::Accounts => "accounts".into(),
            Screen::Analytics => "analytics".into(),
            Screen::Deliveries => "deliveries".into(),
        };
        // A reader is showing only outside the two-pane list layout.
        let reading = (self.screen == Screen::Mailbox
            && self.mailbox.layout_mode != LayoutMode::TwoPane)
            .then_some(self.mailbox.viewing_envelope.as_ref())
            .flatten();
        let open_message = reading.map(|envelope| {
            self.focused_thread_envelope()
                .unwrap_or(envelope)
                .id
                .to_string()
        });
        let mut selected: Vec<String> = self
            .mailbox
            .selected_set
            .iter()
            .map(ToString::to_string)
            .collect();
        selected.sort();
        selected.truncate(CONTEXT_SELECTION_LIMIT);
        let search_query = (self.screen == Screen::Search)
            .then(|| self.search.page.query.trim().to_string())
            .filter(|query| !query.is_empty());
        AssistantViewContextData {
            view: Some(view),
            open_thread_id: reading.map(|envelope| envelope.thread_id.to_string()),
            open_message_id: open_message,
            selected_message_ids: selected,
            search_query,
        }
    }

    /// `AssistantUpdated`: the open conversation changed in the daemon.
    pub(crate) fn assistant_updated(
        &mut self,
        conversation_id: String,
        status: AssistantStatusData,
        progress: Option<AssistantProgressData>,
    ) {
        let assistant = &mut self.assistant;
        if assistant.conversation_id.as_deref() != Some(conversation_id.as_str()) {
            return;
        }
        if status == AssistantStatusData::Running {
            assistant.started_at.get_or_insert_with(Instant::now);
            if progress.is_some() {
                assistant.progress = progress;
            }
        } else {
            assistant.progress = None;
            assistant.started_at = None;
        }
        assistant.refetch();
    }

    /// A daemon answer to one of the pane's requests.
    pub(crate) fn apply_assistant_result(
        &mut self,
        call: AssistantCall,
        result: Result<ResponseData, MxrError>,
    ) {
        if let AssistantCall::Refetch { conversation_id } = &call {
            let again = self.assistant.refetch_settled();
            let current = self.assistant.conversation_id.as_deref() == Some(conversation_id);
            if current {
                match result {
                    Ok(ResponseData::AssistantConversation { conversation }) => {
                        self.settle_assistant_conversation(conversation);
                    }
                    Ok(_) => {}
                    Err(error) => self.assistant.error = Some(error_words(&error)),
                }
            }
            if again {
                self.assistant.refetch();
            }
            return;
        }
        match (call, result) {
            (
                AssistantCall::Availability,
                Ok(ResponseData::AssistantAvailability { availability }),
            ) => {
                if !availability.available && self.assistant.focus == AssistantFocus::Input {
                    self.assistant.focus = AssistantFocus::Pane;
                }
                self.assistant.availability = Some(availability);
            }
            (
                AssistantCall::Conversation,
                Ok(ResponseData::AssistantConversation { conversation }),
            ) => {
                self.assistant.sending = false;
                self.assistant.busy = false;
                self.assistant.input_sent = None;
                self.settle_assistant_conversation(conversation);
            }
            (AssistantCall::List, Ok(ResponseData::AssistantConversations { conversations })) => {
                if let Some(list) = self.assistant.list.as_mut() {
                    list.loading = false;
                    list.selected = conversations
                        .iter()
                        .position(|summary| {
                            self.assistant.conversation_id.as_deref() == Some(summary.id.as_str())
                        })
                        .unwrap_or(0);
                    list.conversations = conversations;
                }
            }
            (AssistantCall::DeletePreview, Ok(ResponseData::AssistantDelete { delete })) => {
                let open = self.assistant.conversation_id.as_deref()
                    == Some(delete.conversation_id.as_str());
                let listed = self.assistant.list.as_ref().is_some_and(|list| {
                    list.conversations
                        .iter()
                        .any(|summary| summary.id == delete.conversation_id)
                });
                if open || listed {
                    self.assistant.prompt = Some(AssistantPrompt::ConfirmDelete(delete));
                }
            }
            (AssistantCall::Delete, Ok(ResponseData::AssistantDelete { delete })) => {
                if delete.deleted {
                    if self.assistant.conversation_id.as_deref()
                        == Some(delete.conversation_id.as_str())
                    {
                        self.assistant.reset_conversation();
                    }
                    if let Some(list) = self.assistant.list.as_mut() {
                        list.conversations
                            .retain(|summary| summary.id != delete.conversation_id);
                        list.selected = list
                            .selected
                            .min(list.conversations.len().saturating_sub(1));
                    }
                    self.push_toast(Toast::success(format!(
                        "Deleted \"{}\"",
                        crate::ui::sanitize::one_line(&delete.title)
                    )));
                }
            }
            (_, Err(error)) => {
                let assistant = &mut self.assistant;
                assistant.sending = false;
                assistant.busy = false;
                // A send that failed keeps its words, to try again.
                if let Some(text) = assistant.input_sent.take() {
                    if assistant.input.is_empty() {
                        assistant.input = text;
                    }
                }
                if assistant.conversation.is_none() {
                    assistant.started_at = None;
                }
                if let Some(list) = assistant.list.as_mut() {
                    list.loading = false;
                }
                assistant.error = Some(error_words(&error));
            }
            (_, Ok(_)) => {
                self.assistant.sending = false;
                self.assistant.busy = false;
                self.assistant.error = Some("Unexpected answer from the daemon".into());
            }
        }
    }

    fn settle_assistant_conversation(&mut self, conversation: AssistantConversationData) {
        let assistant = &mut self.assistant;
        // The answer for a conversation since left behind changes nothing.
        if assistant
            .conversation_id
            .as_deref()
            .is_some_and(|id| id != conversation.id)
        {
            return;
        }
        assistant.conversation_id = Some(conversation.id.clone());
        if conversation.status == AssistantStatusData::Running {
            assistant.started_at.get_or_insert_with(Instant::now);
        } else {
            assistant.progress = None;
            assistant.started_at = None;
        }

        let waiting = conversation.status == AssistantStatusData::AwaitingUser;
        let question = waiting
            .then(|| {
                conversation
                    .pending
                    .iter()
                    .find_map(|pending| match pending {
                        AssistantPendingData::Question {
                            call_id, questions, ..
                        } => Some((call_id.clone(), questions.len())),
                        AssistantPendingData::Approval { .. } => None,
                    })
            })
            .flatten();
        match question {
            Some((call_id, count)) if assistant.answers_for.as_deref() != Some(&call_id) => {
                assistant.answers_for = Some(call_id);
                assistant.answers = vec![AnswerDraft::default(); count];
                assistant.active_question = 0;
            }
            Some(_) => {}
            None => {
                assistant.answers_for = None;
                assistant.answers.clear();
                assistant.active_question = 0;
            }
        }
        // A prompt about a call that no longer waits goes with it.
        let still_waiting = |call: &str| {
            waiting
                && conversation.pending.iter().any(|pending| match pending {
                    AssistantPendingData::Approval { call_id, .. }
                    | AssistantPendingData::Question { call_id, .. } => call_id == call,
                })
        };
        match &assistant.prompt {
            Some(AssistantPrompt::RejectReason { call_id, .. }) if !still_waiting(call_id) => {
                assistant.prompt = None;
            }
            Some(AssistantPrompt::Other { .. }) if assistant.answers_for.is_none() => {
                assistant.prompt = None;
            }
            _ => {}
        }
        // Something waits and nothing is being written: the keys go to it.
        if waiting
            && !conversation.pending.is_empty()
            && assistant.focus == AssistantFocus::Input
            && assistant.input.is_empty()
        {
            assistant.focus = AssistantFocus::Pane;
        }
        assistant.conversation = Some(conversation);
    }
}
