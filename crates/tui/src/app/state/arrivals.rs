//! Sorting shows its work in the TUI (D119): the move menu `X` and `P`
//! open, the "Always for this sender?" ask after a move, the arrivals list a
//! count on Now opens, and Inbox's mode chips. The daemon owns every count,
//! line and reason; this state holds what came back and what the user is
//! doing with it.

use chrono::{DateTime, Utc};
use mxr_core::id::MessageId;
use mxr_protocol::{
    ArrivalBucketData, ArrivalItemData, ArrivalListData, ModeKindData, ScreenerQuestionData,
    SenderKindData,
};
use std::collections::{HashMap, HashSet};

/// The most emails one chip request names (the daemon's cap).
pub const CHIP_BATCH: usize = 200;

/// What `X` or `P` opened the menu for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveMenu {
    pub message_id: MessageId,
    /// Who sent it, for the menu's title.
    pub display: String,
    /// `P`: the sender's mode only (Messages, Updates or Reading).
    pub sender_only: bool,
    /// Opened on a Not-sure question: the answer is stored as one.
    pub not_sure: bool,
    /// Where a Not-sure question's email is for now: keeping it there is
    /// an answer too, so the highlight starts on it.
    pub current: Option<ModeKindData>,
    /// A new sender's question on the row: the sender's options answer it
    /// (`SetSenderKind`), and Block joins them.
    pub new_sender: Option<ScreenerQuestionData>,
    /// The highlighted option; Enter runs it.
    pub selected: usize,
}

/// One option in the move menu, with the key printed on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveOption {
    /// A lowercase `g` letter: this email.
    Email(ModeKindData),
    /// A capital: everything from this sender (`MoveMessage` with sender).
    Sender(ModeKindData),
    /// A capital on a new sender's row: the question's answer.
    NewSender(SenderKindData),
}

impl MoveMenu {
    /// The options in order: this email's five modes (not from `P`), then
    /// the sender's.
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
    /// The key that picks it, printed on it.
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

/// "Always for this sender?" after a move or a Not-sure answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderAsk {
    pub message_id: MessageId,
    pub mode: ModeKindData,
    /// After a Not-sure answer, y or n answers it; after a move, `K` does
    /// while it shows.
    pub yes_no: bool,
}

/// The emails behind the arrivals line's counts, one bucket at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrivalsListState {
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
    /// The line's buckets with a count, in its order, then its "also"s.
    pub buckets: Vec<ArrivalBucketData>,
    /// `None`: every arrival in the window.
    pub bucket: Option<ArrivalBucketData>,
    pub list: Option<ArrivalListData>,
    pub selected: usize,
}

impl ArrivalsListState {
    /// Every arrival, then each bucket in the line's order, round again.
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

/// A list the runtime should fetch.
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
    /// Each hint once per session. Moves to the daemon's hint seen state
    /// (`SetHintSeen`) when fix/contextual-hints lands, so a hint dismissed
    /// in the web app stays dismissed here.
    pub not_sure_hint_shown: bool,
    pub move_hint_shown: bool,
    /// Inbox's mode chips, by message.
    pub chips: HashMap<MessageId, ArrivalItemData>,
    /// Asked for, answered or not: an email older than the ledger has no
    /// chip and is never asked about again until the cache is cleared.
    pub chips_requested: HashSet<MessageId>,
    /// Chips to ask for on the next pass.
    pub pending_chips: Option<Vec<MessageId>>,
    /// A chip request is out: ask for no more until it lands.
    pub chips_in_flight: bool,
    /// The list window last asked about (envelopes, scroll, threads), so
    /// an unchanged screen costs nothing.
    pub chip_window: Option<(usize, usize, bool)>,
}

impl TrustState {
    /// Where mail sits changed: forget every chip so the visible rows ask
    /// again.
    pub fn clear_chips(&mut self) {
        self.chips.clear();
        self.chips_requested.clear();
        self.chip_window = None;
    }

    /// Store a chip answer. Ids the daemon had no row for stay requested.
    pub fn set_chips(&mut self, items: Vec<ArrivalItemData>) {
        for item in items {
            self.chips.insert(item.message_id.clone(), item);
        }
    }

    /// Ask for the chips of `visible` not asked for yet, at most one batch.
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
        // Sent: nothing more until it lands.
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

    #[test]
    fn tab_steps_through_the_lines_buckets() {
        let mut list = ArrivalsListState {
            since: Utc::now(),
            until: Utc::now(),
            buckets: vec![ArrivalBucketData::Messages, ArrivalBucketData::Reading],
            bucket: None,
            list: None,
            selected: 0,
        };
        assert_eq!(list.next_bucket(), Some(ArrivalBucketData::Messages));
        list.bucket = Some(ArrivalBucketData::Messages);
        assert_eq!(list.next_bucket(), Some(ArrivalBucketData::Reading));
        list.bucket = Some(ArrivalBucketData::Reading);
        assert_eq!(list.next_bucket(), None, "back to every arrival");
    }
}
