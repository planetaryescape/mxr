//! Messages in the TUI: the bands the daemon serves (`ListMessages`), the
//! selected person's page (`GetPerson`), Messages' teaching copy
//! (`GetModeGuide`) and a Got it waiting out its countdown. The daemon owns
//! the bands, the order and every message's new text; this state holds
//! what came back and what the user is doing with it.

use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_protocol::{
    AckPlanData, MessagesData, MessagesRowData, ModeGuideData, PersonPageData, TopicStateData,
};
use std::collections::{HashMap, HashSet};

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
    /// Ask the runtime for the bands and the guide.
    pub pending_refresh: bool,
    /// Ask the runtime for this row's page, on this topic.
    pub pending_person: Option<(String, Option<ThreadId>)>,
    /// The page last asked for (`ask_person`). Only its answer is shown,
    /// and verbs wait until it is: an older answer landing late (a refetch
    /// of the topic just done) can't replace it.
    pub person_target: Option<(String, Option<ThreadId>)>,
    /// Ask the daemon for a Got it preview on this thread.
    pub pending_ack_preview: Option<ThreadId>,
    /// The thread `.` asked a preview for: only its preview may start a
    /// countdown, and only while the lens is still on it.
    pub ack_requested: Option<ThreadId>,
    pub ack: Option<AckCountdown>,
    /// What each done here moved to, by its mutation, for its toast:
    /// "Done: Invoice." and "Next: Pricing.", joined with the daemon's copy.
    pub done_notes: HashMap<crate::app::MutationId, DoneNote>,
}

/// A row in the list: the same address in two accounts is two rows.
pub type MessagesRowKey = (AccountId, String);

pub fn row_key(row: &MessagesRowData) -> MessagesRowKey {
    (row.account_id.clone(), row.id.clone())
}

/// Done here's toast, decided when `e` moved on: what was done and what
/// opened next. The daemon's copy goes between when it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoneNote {
    pub head: String,
    pub next: Option<String>,
}

impl DoneNote {
    /// "Done: Invoice. Next: Pricing. Archived in Gmail. u to undo": the
    /// daemon's copy after its first sentence says where the thread went.
    pub fn line(&self, daemon_copy: &str) -> String {
        let handoff = daemon_copy
            .split_once(". ")
            .map_or("", |(_, rest)| rest)
            .trim();
        let next = self.next.as_ref().map(|next| format!("Next: {next}."));
        [Some(self.head.clone()), next, Some(handoff.to_string())]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
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

    /// Row keys in display order, for keeping the selection on a row.
    pub fn row_keys(&self) -> Vec<MessagesRowKey> {
        self.items()
            .into_iter()
            .filter_map(|item| match item {
                MessagesItem::Row(row) => Some(row_key(row)),
                MessagesItem::QuietToggle(_) => None,
            })
            .collect()
    }

    /// Where a row sits among the items, if it is shown.
    pub fn index_of(&self, key: &MessagesRowKey) -> Option<usize> {
        self.items()
            .iter()
            .position(|item| matches!(item, MessagesItem::Row(row) if row_key(row) == *key))
    }

    /// Ask for a row's page on a topic, and make it the one to show.
    pub fn ask_person(&mut self, row_id: String, topic: Option<ThreadId>) {
        self.person_target = Some((row_id.clone(), topic.clone()));
        self.pending_person = Some((row_id, topic));
    }

    /// Whether the page asked for last is the one on screen.
    pub fn target_shown(&self) -> bool {
        let Some((row_id, topic)) = &self.person_target else {
            return true;
        };
        self.page_for.as_ref() == Some(row_id)
            && topic.as_ref().is_none_or(|topic| {
                self.page
                    .as_ref()
                    .and_then(|p| p.conversation.as_ref())
                    .is_some_and(|c| &c.thread_id == topic)
            })
    }

    /// Whether done here already took this thread (a second `e` before
    /// the next topic's page arrives must not send it again).
    pub fn is_done_here(&self, thread_id: &ThreadId) -> bool {
        self.messages.as_ref().is_some_and(|m| {
            m.your_turn
                .iter()
                .chain(&m.pinned)
                .chain(&m.recent)
                .chain(&m.quiet)
                .flat_map(|row| &row.topics)
                .any(|t| &t.thread_id == thread_id && t.state == TopicStateData::Done)
        })
    }

    /// Whether a row sits in the folded Quiet band.
    pub fn in_quiet(&self, key: &MessagesRowKey) -> bool {
        self.messages
            .as_ref()
            .is_some_and(|m| m.quiet.iter().any(|row| row_key(row) == *key))
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

    /// Done here before the daemon answers: the topic is done, and a row
    /// leaves Your turn only once nothing of it is left, so a person with
    /// topics still open keeps their place.
    pub fn mark_done_here(&mut self, thread_id: &ThreadId) {
        let Some(messages) = self.messages.as_mut() else {
            return;
        };
        for row in messages
            .your_turn
            .iter_mut()
            .chain(messages.pinned.iter_mut())
            .chain(messages.recent.iter_mut())
            .chain(messages.quiet.iter_mut())
        {
            for topic in row.topics.iter_mut().filter(|t| &t.thread_id == thread_id) {
                topic.state = TopicStateData::Done;
            }
        }
        messages.your_turn.retain(|row| {
            row.topics
                .iter()
                .any(|topic| topic.state != TopicStateData::Done)
        });
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
