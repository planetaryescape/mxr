//! Who needs your attention, counted once for every surface.
//!
//! The desk's You owe, New from people and due explicit follow-ups say whose turn it is, per
//! conversation. Messages lists them as rows: a person once however many
//! conversations they have with you, a group thread once. Now's headline,
//! its People total, the rail's Messages badge, Messages' Your turn header,
//! the Owed page and Focus all count those same rows, so this module turns
//! desk rows into them with the keys `place_topic` gives Messages' rows.
//!
//! The split is by lane, never by a second rule: a unit with a You owe or
//! due follow-up conversation is someone you write with; every other unit is someone who
//! wrote to you (first time or repeat).

use super::desk_lanes::needs_attention;
use super::messages_view::{People, RowKey};
use mxr_core::id::{AccountId, ThreadId};
use mxr_protocol::{DeskLaneKind, DeskRowData, PeopleSplitData};
use std::collections::{HashMap, HashSet};

/// Each turn conversation's Messages row key.
#[derive(Debug, Default)]
pub(super) struct TurnKeys(HashMap<(AccountId, ThreadId), RowKey>);

impl TurnKeys {
    /// Keys for one account's turn threads. `turn_people` is each thread's
    /// other people (the shape rule's); `people` merges one person's
    /// addresses, as Messages does.
    pub(super) fn add_account<'a>(
        &mut self,
        account: &AccountId,
        people: &People,
        turn_people: &HashMap<ThreadId, Vec<String>>,
        rows: impl Iterator<Item = &'a DeskRowData>,
    ) {
        for row in rows {
            let Some(others) = turn_people.get(&row.thread_id) else {
                continue;
            };
            let mut primaries: Vec<String> = Vec::new();
            for person in others {
                let primary = people.primary(person);
                if !primaries.contains(&primary) {
                    primaries.push(primary);
                }
            }
            let key = match primaries.as_slice() {
                [only] => RowKey::Person(only.clone()),
                [] => RowKey::Person(people.primary(&row.counterparty_email)),
                _ => RowKey::Group(row.thread_id.clone()),
            };
            self.0.insert((account.clone(), row.thread_id.clone()), key);
        }
    }

    pub(super) fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
    }

    /// The row Messages lists this turn under. Messages reads it from here
    /// and never derives it again, so the two cannot differ. A conversation
    /// the shape rule gave no key counts as its sender.
    pub(super) fn key(&self, row: &DeskRowData) -> RowKey {
        self.0
            .get(&(row.account_id.clone(), row.thread_id.clone()))
            .cloned()
            .unwrap_or_else(|| RowKey::Person(row.counterparty_email.to_ascii_lowercase()))
    }

    fn unit(&self, row: &DeskRowData) -> (AccountId, RowKey) {
        (row.account_id.clone(), self.key(row))
    }
}

/// The people waiting on you: one desk row per unit, at their most
/// pressing conversation, with the split.
#[derive(Debug, Default, Clone)]
pub(super) struct TurnUnits {
    pub rows: Vec<DeskRowData>,
    pub split: PeopleSplitData,
}

impl TurnUnits {
    /// Lane rows in their lane order; ordinary waiting never qualifies.
    /// A unit shows once, at its first attention row.
    pub(super) fn from_lanes(
        keys: &TurnKeys,
        owed: &[DeskRowData],
        people_new: &[DeskRowData],
        waiting: &[DeskRowData],
    ) -> Self {
        let mut seen = HashSet::new();
        let mut units = Self::default();
        for row in owed
            .iter()
            .chain(people_new)
            .chain(waiting.iter().filter(|row| needs_attention(row)))
        {
            if !seen.insert(keys.unit(row)) {
                continue;
            }
            if matches!(row.lane, DeskLaneKind::Owed | DeskLaneKind::Waiting) {
                units.split.write_with += 1;
            } else {
                units.split.others += 1;
            }
            units.rows.push(row.clone());
        }
        units
    }
}
