//! Messages in the TUI: open the people list, follow the cursor with the
//! person's page, step through topics, Got it with its countdown, done
//! here, pin, a new topic, and the email as sent. The daemon owns every
//! band, order and word; this file only sends requests and moves focus.

use super::input::plain_or_shift;
use super::*;
use crate::app::state::{AckCountdown, MessagesFocus, MessagesItem};
use mxr_core::id::ThreadId;
use mxr_protocol::{AckPlanData, MessagesData, MessagesRowData, ModeKindData, PersonPageData};

/// The mode id Messages' guide and its card are kept under.
pub(crate) const MESSAGES_MODE: &str = mxr_protocol::MESSAGES_GUIDE.mode;

/// The conversation an action on the selected row acts on.
pub(crate) struct MessagesTopic {
    pub account_id: mxr_core::AccountId,
    pub thread_id: ThreadId,
    pub reply_to: MessageId,
    /// The message `o` shows as sent: the conversation's latest.
    pub latest: MessageId,
    /// "Samir", or a group's title.
    pub who: String,
}

impl App {
    pub(super) fn apply_messages_action(&mut self, action: Action) {
        match action {
            Action::MessagesOpen => self.messages_open(),
            Action::MessagesAck => self.messages_ack(),
            Action::MessagesCancelAck => self.cancel_messages_ack(),
            Action::MessagesDone => self.messages_done(),
            Action::MessagesPin => self.messages_pin(),
            Action::MessagesNewTopic => self.messages_new_topic(),
            Action::MessagesPrevTopic => self.step_topic(false),
            Action::MessagesNextTopic => self.step_topic(true),
            Action::MessagesPersonPage => {
                if self.selected_messages_row().is_some() {
                    self.mailbox.messages_page.focus = MessagesFocus::Person;
                }
            }
            Action::MessagesAsSent => self.messages_as_sent(),
            Action::MessagesBack => self.messages_back(),
            _ => {}
        }
    }

    pub(super) fn open_messages(&mut self) {
        self.enter_mode_view(MailboxView::People);
        let page = &mut self.mailbox.messages_page;
        page.focus = MessagesFocus::List;
        page.pending_refresh = true;
        self.mailbox.pending_rail_refresh = true;
    }

    /// The Messages lens owns the keyboard.
    pub(crate) fn messages_list_focused(&self) -> bool {
        self.screen == Screen::Mailbox
            && self.mailbox.mailbox_view == MailboxView::People
            && self.mailbox.active_pane == ActivePane::MailList
    }

    pub fn selected_messages_row(&self) -> Option<&MessagesRowData> {
        self.mailbox
            .messages_page
            .row_at(self.mailbox.selected_index)
    }

    /// Refetch the bands (and the page on screen) after anything that can
    /// move them.
    pub(crate) fn refresh_messages(&mut self) {
        if self.mailbox.mailbox_view != MailboxView::People {
            return;
        }
        let page = &mut self.mailbox.messages_page;
        page.pending_refresh = true;
        if let Some(row) = page.page_for.clone() {
            let topic = page
                .page
                .as_ref()
                .and_then(|p| p.conversation.as_ref())
                .map(|c| c.thread_id.clone());
            page.pending_person = Some((row, topic));
        }
    }

    /// The runtime fetched the bands and the guide.
    pub(crate) fn set_messages(&mut self, messages: MessagesData, guide: Option<mxr_protocol::ModeGuideData>) {
        let page = &mut self.mailbox.messages_page;
        page.messages = Some(messages);
        if guide.is_some() {
            page.guide = guide;
        }
        if self.mailbox.mailbox_view == MailboxView::People {
            self.mailbox.selected_index = self
                .mailbox
                .selected_index
                .min(self.mailbox.messages_page.item_count().saturating_sub(1));
        }
        self.sync_messages_page();
    }

    /// The runtime fetched a person's page.
    pub(crate) fn set_person_page(&mut self, row_id: String, page: PersonPageData) {
        let state = &mut self.mailbox.messages_page;
        state.page_for = Some(row_id);
        state.page = Some(page);
    }

    /// Ask for the selected row's page when it isn't the one on screen.
    pub(crate) fn sync_messages_page(&mut self) {
        if self.mailbox.mailbox_view != MailboxView::People {
            return;
        }
        let Some(id) = self.selected_messages_row().map(|row| row.id.clone()) else {
            return;
        };
        let page = &mut self.mailbox.messages_page;
        let shown = page.page_for.as_deref() == Some(id.as_str());
        let asked = page
            .pending_person
            .as_ref()
            .is_some_and(|(pending, _)| *pending == id);
        if !shown && !asked {
            page.pending_person = Some((id, None));
            page.expanded.clear();
        }
    }

    /// The topic actions act on: the conversation open on the page, else
    /// the row's most pressing topic.
    pub(crate) fn selected_messages_topic(&self) -> Option<MessagesTopic> {
        let row = self.selected_messages_row()?;
        let who = row
            .person
            .as_ref()
            .map_or_else(|| row.title.clone(), |person| person.first_name());
        let page = self.mailbox.messages_page.page_for_row(row);
        if let Some(conversation) = page.and_then(|page| page.conversation.as_ref()) {
            return Some(MessagesTopic {
                account_id: conversation.account_id.clone(),
                thread_id: conversation.thread_id.clone(),
                reply_to: conversation.composer.reply_to_message_id.clone(),
                latest: conversation
                    .messages
                    .last()
                    .map_or_else(
                        || conversation.composer.reply_to_message_id.clone(),
                        |m| m.message_id.clone(),
                    ),
                who,
            });
        }
        let topic = row.topics.first()?;
        Some(MessagesTopic {
            account_id: topic.account_id.clone(),
            thread_id: topic.thread_id.clone(),
            reply_to: topic.reply_to_message_id.clone(),
            latest: topic
                .message_ids
                .last()
                .cloned()
                .unwrap_or_else(|| topic.reply_to_message_id.clone()),
            who,
        })
    }

    fn messages_open(&mut self) {
        let index = self.mailbox.selected_index;
        let page = &mut self.mailbox.messages_page;
        match page.items().get(index) {
            Some(MessagesItem::QuietToggle(_)) => page.quiet_open = !page.quiet_open,
            Some(MessagesItem::Row(_)) if page.focus == MessagesFocus::Person => {
                // Enter on the page opens every letter, or closes them.
                let ids: Vec<MessageId> = page
                    .page
                    .as_ref()
                    .and_then(|p| p.conversation.as_ref())
                    .map(|c| {
                        c.messages
                            .iter()
                            .filter(|m| m.layout == mxr_protocol::MessageLayoutData::Letter)
                            .map(|m| m.message_id.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                if ids.iter().all(|id| page.expanded.contains(id)) {
                    page.expanded.clear();
                } else {
                    page.expanded.extend(ids);
                }
            }
            Some(MessagesItem::Row(_)) => page.focus = MessagesFocus::Person,
            None => {}
        }
    }

    /// `.`: ask the daemon for the exact acknowledgement; it shows with a
    /// countdown and sends when the countdown ends, unless undone.
    fn messages_ack(&mut self) {
        let Some(topic) = self.selected_messages_topic() else {
            self.status_message = Some("No conversation selected".into());
            return;
        };
        self.retire_messages_card();
        self.mailbox.messages_page.pending_ack_preview = Some(topic.thread_id);
        self.status_message = Some("Preparing got it…".into());
    }

    /// The daemon's preview of Got it: show it and start the countdown.
    pub(crate) fn show_messages_ack(&mut self, plan: AckPlanData, now: std::time::Instant) {
        self.status_message = None;
        let to = plan
            .to
            .first()
            .map(|address| {
                mxr_protocol::first_name(address.name.as_deref(), &address.email)
            })
            .unwrap_or_default();
        let send_at = now + std::time::Duration::from_secs(u64::from(plan.countdown_seconds));
        self.mailbox.messages_page.ack = Some(AckCountdown { plan, to, send_at });
    }

    /// Send a Got it whose countdown has ended: exactly the previewed text.
    pub fn tick_messages_ack(&mut self, now: std::time::Instant) {
        let due = self
            .mailbox
            .messages_page
            .ack
            .as_ref()
            .is_some_and(|ack| now >= ack.send_at);
        if !due {
            return;
        }
        let Some(ack) = self.mailbox.messages_page.ack.take() else {
            return;
        };
        self.mailbox.messages_page.mark_done(&ack.plan.thread_id);
        self.queue_mutation(
            Request::AckMessage {
                thread_id: ack.plan.thread_id.clone(),
                dry_run: false,
                expect_text: Some(ack.plan.text.clone()),
            },
            MutationEffect::Messages(format!(
                "Sent got it to {}",
                crate::ui::sanitize::one_line(&ack.to)
            )),
            "Sending got it…".into(),
        );
    }

    fn cancel_messages_ack(&mut self) {
        if self.mailbox.messages_page.ack.take().is_some() {
            self.push_toast(Toast::info("Got it not sent"));
        }
    }

    /// `e`: done here for the selected topic. The topic leaves Your turn at
    /// once; the toast is the daemon's handoff copy and `u` undoes it.
    fn messages_done(&mut self) {
        let Some(topic) = self.selected_messages_topic() else {
            return;
        };
        self.retire_messages_card();
        self.mailbox.messages_page.mark_done(&topic.thread_id);
        self.mailbox.selected_index = self
            .mailbox
            .selected_index
            .min(self.mailbox.messages_page.item_count().saturating_sub(1));
        self.queue_mode_done(ModeKindData::Messages, topic.thread_id);
    }

    /// `s`: pin or unpin the person (the cadence watchlist).
    fn messages_pin(&mut self) {
        let Some(row) = self.selected_messages_row() else {
            return;
        };
        let Some(person) = row.person.clone() else {
            self.status_message = Some("Pin a person; a group thread can't be pinned".into());
            return;
        };
        let account_id = row.account_id.clone();
        let label = crate::ui::sanitize::one_line(person.label());
        let (request, status) = if row.pinned {
            (
                Request::UnwatchCadence {
                    account_id,
                    email: person.id.clone(),
                },
                format!("Unpinned {label}"),
            )
        } else {
            (
                Request::WatchCadence {
                    account_id,
                    email: person.id.clone(),
                    expected_days: None,
                    note: None,
                    allow_list_sender: false,
                },
                format!("Pinned {label}"),
            )
        };
        self.queue_mutation(request, MutationEffect::Messages(status), "Saving…".into());
    }

    /// `c`: a new topic with the person, the subject left for you.
    fn messages_new_topic(&mut self) {
        let Some(row) = self.selected_messages_row() else {
            return;
        };
        let to = match &row.person {
            Some(person) => person.id.clone(),
            None => row
                .members
                .iter()
                .map(|member| member.id.clone())
                .collect::<Vec<_>>()
                .join(", "),
        };
        self.compose.pending_compose = Some(ComposeAction::New {
            to,
            subject: String::new(),
        });
    }

    /// `[` and `]`: the previous or next topic on the person's page.
    fn step_topic(&mut self, forward: bool) {
        let Some(row) = self.selected_messages_row() else {
            return;
        };
        let row_id = row.id.clone();
        let Some(page) = self.mailbox.messages_page.page_for_row(row) else {
            return;
        };
        let topics = &page.topics;
        if topics.len() < 2 {
            return;
        }
        let current = page
            .conversation
            .as_ref()
            .and_then(|c| topics.iter().position(|t| t.thread_id == c.thread_id))
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % topics.len()
        } else {
            (current + topics.len() - 1) % topics.len()
        };
        let thread = topics[next].thread_id.clone();
        let state = &mut self.mailbox.messages_page;
        state.pending_person = Some((row_id, Some(thread)));
        state.expanded.clear();
    }

    /// `o`/`v`: the conversation's latest message as sent, in the reader.
    fn messages_as_sent(&mut self) {
        let Some(topic) = self.selected_messages_topic() else {
            return;
        };
        self.mailbox.pending_invite_open = Some(topic.latest);
        self.status_message = Some("Opening the email as sent…".into());
    }

    fn messages_back(&mut self) {
        if self.mailbox.messages_page.card_visible() {
            self.retire_messages_card();
        } else if self.mailbox.messages_page.focus == MessagesFocus::Person {
            self.mailbox.messages_page.focus = MessagesFocus::List;
        }
    }

    /// Retire Messages' first-encounter card here and in every client.
    fn retire_messages_card(&mut self) {
        let page = &mut self.mailbox.messages_page;
        if !page.card_visible() {
            return;
        }
        page.card_closed = true;
        let id = self.queue_best_effort_mutation(
            Request::SetModeGuideSeen {
                mode: MESSAGES_MODE.into(),
                seen: true,
            },
            MutationEffect::StatusOnly(String::new()),
            String::new(),
        );
        self.mailbox.messages_page.card_close_mutation = Some(id);
    }

    /// The daemon didn't store the closed card: show it again.
    pub(crate) fn reopen_messages_card_after_failure(&mut self, failed: crate::app::MutationId) {
        let page = &mut self.mailbox.messages_page;
        if page.card_close_mutation == Some(failed) {
            page.card_close_mutation = None;
            page.card_closed = false;
        }
    }

    /// Key handling for the Messages lens.
    pub(super) fn messages_lens_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let plain = key.modifiers == KeyModifiers::NONE;
        let shifted = plain_or_shift(key.modifiers);
        if self.mailbox.messages_page.ack.is_some() {
            match key.code {
                KeyCode::Char('u') if plain => return Some(Action::MessagesCancelAck),
                KeyCode::Esc => return Some(Action::MessagesCancelAck),
                _ => {}
            }
        }
        let action = match key.code {
            KeyCode::Char('/') if plain => Some(Action::OpenGlobalSearch),
            KeyCode::Char('h') | KeyCode::Left if plain => {
                if self.mailbox.messages_page.focus == MessagesFocus::Person {
                    self.mailbox.messages_page.focus = MessagesFocus::List;
                } else {
                    self.mailbox.active_pane = ActivePane::Sidebar;
                }
                return None;
            }
            KeyCode::Esc => Some(Action::MessagesBack),
            KeyCode::Enter => Some(Action::MessagesOpen),
            KeyCode::Char('.') if plain => Some(Action::MessagesAck),
            KeyCode::Char('e') if plain => Some(Action::MessagesDone),
            KeyCode::Char('s') if plain => Some(Action::MessagesPin),
            KeyCode::Char('c') if plain => Some(Action::MessagesNewTopic),
            KeyCode::Char('[') if plain => Some(Action::MessagesPrevTopic),
            KeyCode::Char(']') if plain => Some(Action::MessagesNextTopic),
            KeyCode::Char('p') if plain => Some(Action::MessagesPersonPage),
            KeyCode::Char('o' | 'v') if plain => Some(Action::MessagesAsSent),
            KeyCode::Char('r') if plain => Some(Action::Reply),
            KeyCode::Char('a') if plain => Some(Action::ReplyAll),
            KeyCode::Char('t') if plain => Some(Action::CreateTodoFromMessage),
            KeyCode::Char('b') if plain => Some(Action::FlagReplyLater),
            KeyCode::Char('u') if plain => Some(Action::UndoLastMutation),
            KeyCode::Char('A') if shifted => None,
            _ => None,
        };
        if action.is_some() {
            return action;
        }
        self.contextual_input_action(key).filter(|action| {
            !matches!(
                action,
                Action::OpenSelected
                    | Action::ToggleSelect
                    | Action::VisualLineMode
                    | Action::AttachmentList
                    | Action::ExportThread
                    | Action::ToggleFullscreen
                    | Action::NextSearchResult
                    | Action::PrevSearchResult
                    | Action::SwitchPane
                    | Action::Back
                    | Action::Archive
                    | Action::MarkReadAndArchive
                    | Action::Trash
                    | Action::Spam
                    | Action::Star
                    | Action::MarkRead
                    | Action::MarkUnread
                    | Action::Snooze
                    | Action::ApplyLabel
                    | Action::MoveToLabel
                    | Action::RouteToLabel
                    | Action::Forward
                    | Action::RespondInvite(_)
                    | Action::RespondInviteWithComment(_)
            )
        })
    }
}
