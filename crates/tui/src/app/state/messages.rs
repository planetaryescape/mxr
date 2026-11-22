use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_protocol::{
    AckPlanData, MessagesData, MessagesFilterData, MessagesRowData, ModeGuideData, PersonPageData,
    TopicStateData,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub enum MessagesItem<'a> {
    Row(&'a MessagesRowData),
    QuietToggle(u32),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MessagesFocus {
    #[default]
    List,
    Person,
}

#[derive(Debug, Clone)]
pub struct AckCountdown {
    pub plan: AckPlanData,
    pub to: String,
    pub send_at: std::time::Instant,
}

#[derive(Debug, Clone, Default)]
pub struct MessagesPageState {
    pub filter: MessagesFilterData,
    pub messages: Option<MessagesData>,
    pub guide: Option<ModeGuideData>,
    pub page: Option<PersonPageData>,
    pub page_for: Option<String>,
    pub focus: MessagesFocus,
    pub quiet_open: bool,
    pub expanded: HashSet<MessageId>,
    pub pending_refresh: bool,
    pub pending_person: Option<(String, Option<ThreadId>)>,
    pub person_target: Option<(String, Option<ThreadId>)>,
    pub select_row: Option<String>,
    pub select_thread: Option<ThreadId>,
    pub pending_ack_preview: Option<ThreadId>,
    pub ack_requested: Option<ThreadId>,
    pub ack: Option<AckCountdown>,
    pub done_notes: HashMap<crate::app::MutationId, DoneNote>,
}

pub type MessagesRowKey = (AccountId, String);

pub fn row_key(row: &MessagesRowData) -> MessagesRowKey {
    (row.account_id.clone(), row.id.clone())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoneNote {
    pub head: String,
    pub next: Option<String>,
}

impl DoneNote {
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

    pub fn row_keys(&self) -> Vec<MessagesRowKey> {
        self.items()
            .into_iter()
            .filter_map(|item| match item {
                MessagesItem::Row(row) => Some(row_key(row)),
                MessagesItem::QuietToggle(_) => None,
            })
            .collect()
    }

    pub fn index_of(&self, key: &MessagesRowKey) -> Option<usize> {
        self.items()
            .iter()
            .position(|item| matches!(item, MessagesItem::Row(row) if row_key(row) == *key))
    }

    pub fn ask_person(&mut self, row_id: String, topic: Option<ThreadId>) {
        self.person_target = Some((row_id.clone(), topic.clone()));
        self.pending_person = Some((row_id, topic));
    }

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

    pub fn page_for_row(&self, row: &MessagesRowData) -> Option<&PersonPageData> {
        self.page
            .as_ref()
            .filter(|_| self.page_for.as_deref() == Some(row.id.as_str()))
    }

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
