//! Field-wise three-way merging for explicitly linked tasks. Neither wall
//! clocks nor an application's "last modified" timestamp choose a winner.

use chrono::{NaiveDate, TimeZone};
use mxr_store::{TodoRecord, TodoState};
use serde::{Deserialize, Serialize};

/// Fields a linked task manager can represent without changing their meaning.
/// Date-only peers cannot rewrite the time on an unchanged Mixer date.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkedTodoFields {
    pub title: String,
    pub due: Option<NaiveDate>,
    pub scheduled: Option<NaiveDate>,
    pub completed: bool,
}

impl LinkedTodoFields {
    pub fn from_record<Tz: TimeZone>(record: &TodoRecord, zone: &Tz) -> Self {
        Self {
            title: record.title.clone(),
            due: record.due_at.map(|at| at.with_timezone(zone).date_naive()),
            scheduled: record
                .scheduled_for
                .map(|at| at.with_timezone(zone).date_naive()),
            completed: record.state != TodoState::Open,
        }
    }
}

/// Merge disjoint edits, retain edits made on only one side, and refuse the
/// whole row when both sides changed a field differently. Nothing is applied
/// partially: a conflict preserves both versions until the user resolves it.
pub fn merge(
    base: &LinkedTodoFields,
    mixer: &LinkedTodoFields,
    peer: &LinkedTodoFields,
) -> Result<LinkedTodoFields, Vec<&'static str>> {
    let mut conflicts = Vec::new();
    let merged = LinkedTodoFields {
        title: field(
            "title",
            &base.title,
            &mixer.title,
            &peer.title,
            &mut conflicts,
        ),
        due: field("due date", &base.due, &mixer.due, &peer.due, &mut conflicts),
        scheduled: field(
            "schedule",
            &base.scheduled,
            &mixer.scheduled,
            &peer.scheduled,
            &mut conflicts,
        ),
        completed: field(
            "completion",
            &base.completed,
            &mixer.completed,
            &peer.completed,
            &mut conflicts,
        ),
    };
    if conflicts.is_empty() {
        Ok(merged)
    } else {
        Err(conflicts)
    }
}

fn field<T: Clone + Eq>(
    name: &'static str,
    base: &T,
    mixer: &T,
    peer: &T,
    conflicts: &mut Vec<&'static str>,
) -> T {
    if mixer == peer || peer == base {
        mixer.clone()
    } else if mixer == base {
        peer.clone()
    } else {
        conflicts.push(name);
        mixer.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> LinkedTodoFields {
        LinkedTodoFields {
            title: "Pay council tax".into(),
            due: NaiveDate::from_ymd_opt(2026, 11, 1),
            scheduled: None,
            completed: false,
        }
    }

    #[test]
    fn independent_edits_in_both_apps_survive() {
        let original = base();
        let mut mixer = original.clone();
        mixer.title = "Pay the remaining council tax".into();
        let mut peer = original.clone();
        peer.due = NaiveDate::from_ymd_opt(2026, 11, 5);
        let mut expected = mixer.clone();
        expected.due = peer.due;
        assert_eq!(merge(&original, &mixer, &peer), Ok(expected));
    }

    #[test]
    fn competing_title_edits_are_a_conflict_not_last_writer_wins() {
        let original = base();
        let mut mixer = original.clone();
        mixer.title = "Pay half".into();
        let mut peer = original.clone();
        peer.title = "Pay all".into();
        assert_eq!(merge(&original, &mixer, &peer), Err(vec!["title"]));
    }

    #[test]
    fn completion_and_reopening_flow_in_either_direction() {
        for completed in [false, true] {
            let mut original = base();
            original.completed = completed;
            let mut changed = original.clone();
            changed.completed = !completed;
            assert_eq!(merge(&original, &changed, &original), Ok(changed.clone()));
            assert_eq!(merge(&original, &original, &changed), Ok(changed));
        }
    }

    #[test]
    fn clearing_dates_is_an_edit_not_a_missing_value() {
        let original = base();
        let mut peer = original.clone();
        peer.due = None;
        assert_eq!(merge(&original, &original, &peer), Ok(peer));
    }

    #[test]
    fn equal_edits_are_not_a_conflict() {
        let original = base();
        let mut changed = original.clone();
        changed.title = "Paid".into();
        assert_eq!(merge(&original, &changed, &changed), Ok(changed));
    }
}
