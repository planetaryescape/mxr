use chrono::{DateTime, Utc};
use mxr_core::id::MessageId;
use mxr_protocol::{
    ArrivalBucketData, ArrivalItemData, ArrivalListData, ModeKindData, ScreenerQuestionData,
    SenderKindData,
};
use std::collections::{HashMap, HashSet};

pub const CHIP_BATCH: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveMenu {
    pub message_id: MessageId,
    pub display: String,
    pub sender_only: bool,
    pub not_sure: bool,
    pub current: Option<ModeKindData>,
    pub new_sender: Option<ScreenerQuestionData>,
    pub selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveOption {
    Email(ModeKindData),
    Sender(ModeKindData),
    NewSender(SenderKindData),
}

impl MoveMenu {
    pub fn options(&self) -> Vec<MoveOption> {
        let mut options = Vec::new();
        if !self.sender_only {
            options.extend(
                crate::app::MOVE_CHOICES
                    .iter()
                    .map(|(_, mode)| MoveOption::Email(*mode)),
            );
        }
        match &self.new_sender {
            Some(question) => options.extend(
                question
                    .choices
                    .iter()
                    .map(|choice| MoveOption::NewSender(choice.kind)),
            ),
            None => options.extend(
                crate::app::MOVE_CHOICES
                    .iter()
                    .filter(|(_, mode)| crate::app::sender_mode(*mode))
                    .map(|(_, mode)| MoveOption::Sender(*mode)),
            ),
        }
        options
    }
}

impl MoveOption {
    pub fn key(self) -> char {
        let letter = |mode: ModeKindData| {
            crate::app::MOVE_CHOICES
                .iter()
                .find(|(_, choice)| *choice == mode)
                .map_or('?', |(letter, _)| *letter)
        };
        match self {
            Self::Email(mode) => letter(mode),
            Self::Sender(mode) => letter(mode).to_ascii_uppercase(),
            Self::NewSender(kind) => crate::app::new_sender_key(kind),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderAsk {
    pub message_id: MessageId,
    pub mode: ModeKindData,
    pub yes_no: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrivalsListState {
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
    pub buckets: Vec<ArrivalBucketData>,
    pub bucket: Option<ArrivalBucketData>,
    pub list: Option<ArrivalListData>,
    pub selected: usize,
}

impl ArrivalsListState {
    pub fn next_bucket(&self) -> Option<ArrivalBucketData> {
        match self.bucket {
            None => self.buckets.first().copied(),
            Some(current) => {
                let at = self.buckets.iter().position(|b| *b == current)?;
                self.buckets.get(at + 1).copied()
            }
        }
    }

    pub fn selected_item(&self) -> Option<&ArrivalItemData> {
        self.list.as_ref()?.items.get(self.selected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrivalsListFetch {
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
    pub bucket: Option<ArrivalBucketData>,
}

#[derive(Debug, Clone, Default)]
pub struct TrustState {
    pub move_menu: Option<MoveMenu>,
    pub sender_ask: Option<SenderAsk>,
    pub arrivals_list: Option<ArrivalsListState>,
    pub pending_list: Option<ArrivalsListFetch>,
    pub not_sure_hint_shown: bool,
    pub move_hint_shown: bool,
    pub chips: HashMap<MessageId, ArrivalItemData>,
    pub chips_requested: HashSet<MessageId>,
    pub pending_chips: Option<Vec<MessageId>>,
    pub chips_in_flight: bool,
    pub chip_window: Option<(usize, usize, bool)>,
}

impl TrustState {
    pub fn clear_chips(&mut self) {
        self.chips.clear();
        self.chips_requested.clear();
        self.chip_window = None;
    }

    pub fn set_chips(&mut self, items: Vec<ArrivalItemData>) {
        for item in items {
            self.chips.insert(item.message_id.clone(), item);
        }
    }

    pub fn want_chips<'a>(&mut self, visible: impl Iterator<Item = &'a MessageId>) {
        if self.pending_chips.is_some() || self.chips_in_flight {
            return;
        }
        let wanted: Vec<MessageId> = visible
            .filter(|id| !self.chips_requested.contains(*id))
            .take(CHIP_BATCH)
            .cloned()
            .collect();
        if wanted.is_empty() {
            return;
        }
        self.chips_requested.extend(wanted.iter().cloned());
        self.pending_chips = Some(wanted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chips_are_asked_for_once_in_batches() {
        let mut state = TrustState::default();
        let ids: Vec<MessageId> = (0..3).map(|_| MessageId::new()).collect();
        state.want_chips(ids.iter());
        assert_eq!(state.pending_chips.as_ref().map(Vec::len), Some(3));
        state.pending_chips = None;
        state.chips_in_flight = true;
        state.want_chips(ids.iter());
        assert!(state.pending_chips.is_none());
        state.chips_in_flight = false;
        state.want_chips(ids.iter());
        assert!(state.pending_chips.is_none(), "asked already");
        state.clear_chips();
        state.want_chips(ids.iter());
        assert_eq!(state.pending_chips.as_ref().map(Vec::len), Some(3));
    }
}
