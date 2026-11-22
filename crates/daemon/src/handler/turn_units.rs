use super::desk_lanes::needs_attention;
use super::messages_view::{People, RowKey};
use mxr_core::id::{AccountId, ThreadId};
use mxr_protocol::{DeskLaneKind, DeskRowData, PeopleSplitData};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub(super) struct TurnKeys(HashMap<(AccountId, ThreadId), RowKey>);

impl TurnKeys {
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

#[derive(Debug, Default, Clone)]
pub(super) struct TurnUnits {
    pub rows: Vec<DeskRowData>,
    pub split: PeopleSplitData,
}

impl TurnUnits {
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
