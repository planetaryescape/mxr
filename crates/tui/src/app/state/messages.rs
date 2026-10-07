//! Messages in the TUI: the bands the daemon serves (`ListMessages`), the
//! selected person's page (`GetPerson`), Messages' teaching copy
//! (`GetModeGuide`) and a Got it waiting out its countdown. The daemon owns
//! the bands, the order and every message's new text; this state holds
//! what came back and what the user is doing with it.

use mxr_core::id::{MessageId, ThreadId};
use mxr_protocol::{
    AckPlanData, MessagesData, MessagesRowData, ModeGuideData, PersonPageData, TopicStateData,
};
use std::collections::HashSet;

/// One selectable item in the list, in band order.
#[derive(Debug, Clone, Copy)]
pub enum MessagesItem<'a> {
    Row(&'a MessagesRowData),
    /// The collapsed "Quiet (N)" line: Enter opens it.
    QuietToggle(u32),
}

/// Which pane of the lens has the keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MessagesFocus {
    #[default]
    List,
    Person,
}

/// A Got it shown with its exact text, sending when `send_at` passes
/// unless undone.
#[derive(Debug, Clone)]
pub struct AckCountdown {
    pub plan: AckPlanData,
    /// Who it goes to, for the line: "Samir".
    pub to: String,
    pub send_at: std::time::Instant,
}

#[derive(Debug, Clone, Default)]
pub struct MessagesPageState {
    pub messages: Option<MessagesData>,
    pub guide: Option<ModeGuideData>,
    /// The page for `page_for` (a row id).
    pub page: Option<PersonPageData>,
    pub page_for: Option<String>,
    pub focus: MessagesFocus,
    pub quiet_open: bool,
    /// Letters opened past their first paragraph.
    pub expanded: HashSet<MessageId>,
    /// Closed here before the daemon answered, so it never flickers back.
    pub card_closed: bool,
    pub card_close_mutation: Option<crate::app::MutationId>,
    /// Ask the runtime for the bands and the guide.
    pub pending_refresh: bool,
    /// Ask the runtime for this row's page, on this topic.
    pub pending_person: Option<(String, Option<ThreadId>)>,
    /// Ask the daemon for a Got it preview on this thread.
    pub pending_ack_preview: Option<ThreadId>,
    /// The thread `.` asked a preview for: only its preview may start a
    /// countdown, and only while the lens is still on it.
    pub ack_requested: Option<ThreadId>,
    pub ack: Option<AckCountdown>,
}

impl MessagesPageState {
    /// Selectable items in display order: Your turn, Pinned, Recent, then
    /// the Quiet line (and its rows once open).
    pub fn items(&self) -> Vec<MessagesItem<'_>> {
        let Some(messages) = &self.messages else {
            return Vec::new();
        };
        let mut items: Vec<MessagesItem<'_>> =
            messages.your_turn.iter().map(MessagesItem::Row).collect();
        items.extend(messages.pinned.iter().map(MessagesItem::Row));
        items.extend(messages.recent.iter().map(MessagesItem::Row));
        if messages.quiet_total > 0 {
            items.push(MessagesItem::QuietToggle(messages.quiet_total));
            if self.quiet_open {
                items.extend(messages.quiet.iter().map(MessagesItem::Row));
            }
        }
        items
    }

    pub fn item_count(&self) -> usize {
        self.items().len()
    }

    pub fn row_at(&self, index: usize) -> Option<&MessagesRowData> {
        match self.items().get(index) {
            Some(MessagesItem::Row(row)) => Some(row),
            _ => None,
        }
    }

    /// The page on screen, when it is the selected row's.
    pub fn page_for_row(&self, row: &MessagesRowData) -> Option<&PersonPageData> {
        self.page
            .as_ref()
            .filter(|_| self.page_for.as_deref() == Some(row.id.as_str()))
    }

    /// The first-encounter card shows once Messages has people, until it
    /// is closed here or retired anywhere.
    pub fn card_visible(&self) -> bool {
        !self.card_closed
            && self.guide.as_ref().is_some_and(|guide| !guide.card_seen)
            && self.messages.as_ref().is_some_and(|m| m.row_count > 0)
    }

    /// Take a thread's topic out of Your turn before the daemon answers.
    pub fn mark_done(&mut self, thread_id: &ThreadId) {
        let Some(messages) = self.messages.as_mut() else {
            return;
        };
        for row in messages
            .your_turn
            .iter_mut()
            .chain(messages.pinned.iter_mut())
            .chain(messages.recent.iter_mut())
        {
            for topic in row.topics.iter_mut().filter(|t| &t.thread_id == thread_id) {
                topic.state = TopicStateData::Done;
            }
        }
        messages.your_turn.retain(|row| {
            row.topics
                .iter()
                .any(|topic| topic.state == TopicStateData::YourTurn)
        });
    }
}
