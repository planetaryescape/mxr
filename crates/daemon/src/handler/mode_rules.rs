use chrono::{DateTime, Duration, TimeZone, Timelike, Utc};
use mxr_core::id::AccountId;
use mxr_core::types::ProviderKind;
use mxr_protocol::{
    DeskLaneKind, DeskRowData, ModeKindData, ModeMembershipData, PeopleSplitData,
    ScreenerChoiceData, ScreenerQuestionData, SenderKindData,
};
use mxr_store::DeskDismissal;
use std::collections::HashMap;

pub(super) fn mark_covers<'a>(
    mark: &DeskDismissal,
    messages: impl IntoIterator<Item = (DateTime<Utc>, &'a mxr_core::id::MessageId)>,
) -> bool {
    messages.into_iter().all(|(date, id)| mark.saw(date, id))
}

pub(super) fn merge_marks(
    mut dismissals: HashMap<mxr_core::id::ThreadId, DeskDismissal>,
    marks: HashMap<mxr_core::id::ThreadId, DeskDismissal>,
) -> HashMap<mxr_core::id::ThreadId, DeskDismissal> {
    for (thread, mark) in marks {
        match dismissals.entry(thread) {
            std::collections::hash_map::Entry::Occupied(mut existing) => {
                if mark.order_key() > existing.get().order_key() {
                    existing.insert(mark);
                }
            }
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(mark);
            }
        }
    }
    dismissals
}

pub(super) const fn mode_for_kind(kind: SenderKindData) -> Option<ModeKindData> {
    match kind {
        SenderKindData::People => Some(ModeKindData::Messages),
        SenderKindData::Reading => Some(ModeKindData::Reading),
        SenderKindData::PaperTrail => Some(ModeKindData::Updates),
        SenderKindData::ScreenedOut => None,
    }
}

pub(super) fn screener_question(
    account_id: &AccountId,
    sender_email: &str,
    landed: SenderKindData,
) -> Option<ScreenerQuestionData> {
    let mode = mode_for_kind(landed)?;
    let label = |kind: SenderKindData| match kind {
        SenderKindData::People => "Messages",
        SenderKindData::Reading => "Reading",
        SenderKindData::PaperTrail => "Updates",
        SenderKindData::ScreenedOut => "Block",
    };
    let mut choices = vec![landed];
    choices.extend(
        [
            SenderKindData::People,
            SenderKindData::PaperTrail,
            SenderKindData::Reading,
            SenderKindData::ScreenedOut,
        ]
        .into_iter()
        .filter(|kind| *kind != landed),
    );
    Some(ScreenerQuestionData {
        account_id: account_id.clone(),
        sender_email: sender_email.to_ascii_lowercase(),
        question: format!("New sender. Keep in {}?", mode.name()),
        choices: choices
            .into_iter()
            .map(|kind| ScreenerChoiceData {
                kind,
                label: label(kind).to_string(),
            })
            .collect(),
    })
}

pub(super) fn messages_summary(row: &DeskRowData) -> String {
    let who = row
        .counterparty_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&row.counterparty_email);
    match row.lane {
        DeskLaneKind::Owed | DeskLaneKind::Due => format!("your turn with {who}"),
        DeskLaneKind::Waiting => format!("waiting on {who}"),
        DeskLaneKind::PeopleNew => format!("new from {who}"),
    }
}

pub(super) fn age_label(since: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let age = now - since;
    if age < Duration::hours(1) {
        format!("{}m", age.num_minutes().max(1))
    } else if age < Duration::hours(48) {
        format!("{}h", age.num_hours())
    } else {
        format!("{}d", age.num_days())
    }
}

pub(super) fn messages_membership(row: &DeskRowData, early: bool) -> ModeMembershipData {
    let summary = messages_summary(row);
    membership(
        ModeKindData::Messages,
        format!("Here because: {summary}, {} (rule).", row.reason),
        format!("Also in Messages: {summary}"),
        early,
        PERSON_TAG,
    )
}

pub(super) fn quiet_membership(
    from: &mxr_core::types::Address,
    date: DateTime<Utc>,
    now: DateTime<Utc>,
    early: bool,
) -> ModeMembershipData {
    let who = from
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&from.email);
    membership(
        ModeKindData::Messages,
        format!(
            "Here because: quiet, {who} wrote {} ago and it is still in your inbox (rule).",
            age_label(date, now)
        ),
        format!("Also in Messages: quiet, from {who}"),
        early,
        PERSON_TAG,
    )
}

const PERSON_TAG: &str = "person";

pub(super) fn membership(
    mode: ModeKindData,
    reason: String,
    also_in: String,
    early: bool,
    tag: &str,
) -> ModeMembershipData {
    ModeMembershipData {
        mode,
        name: mode.name().to_string(),
        key: mode.key().to_string(),
        reason,
        also_in,
        early,
        todo_ids: Vec::new(),
        tag: Some(tag.to_string()),
    }
}

pub(super) fn provider_name(kind: Option<&ProviderKind>) -> &'static str {
    match kind {
        Some(ProviderKind::Gmail) => "Gmail",
        Some(ProviderKind::OutlookPersonal | ProviderKind::OutlookWork) => "Outlook",
        _ => "the mail server",
    }
}

fn provider_place(provider: &str) -> String {
    if provider.starts_with("the ") {
        format!("on {provider}")
    } else {
        format!("in {provider}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StillIn {
    pub mode: ModeKindData,
    pub detail: Option<String>,
}

pub(super) struct Handoff<'a> {
    pub mode: ModeKindData,
    pub still_in: &'a [StillIn],
    pub archived: bool,
    pub left_in_inbox: bool,
    pub filed: bool,
    pub provider: &'a str,
}

pub(super) fn handoff_copy(handoff: &Handoff<'_>) -> String {
    let mut parts = Vec::new();
    parts.push(match handoff.mode {
        ModeKindData::Todo => "Ticked off.".to_string(),
        _ if handoff.archived => "Done.".to_string(),
        mode => format!("Done in {}.", mode.name()),
    });
    if handoff.filed {
        parts.push("Filed in Archive.".to_string());
    }
    if !handoff.still_in.is_empty() {
        let held: Vec<String> = handoff
            .still_in
            .iter()
            .map(|still| match &still.detail {
                Some(detail) => format!("{} ({detail})", still.mode.name()),
                None => still.mode.name().to_string(),
            })
            .collect();
        parts.push(format!("Still in {}.", join_and(&held)));
    }
    if handoff.archived {
        parts.push(format!("Archived {}.", provider_place(handoff.provider)));
    } else if handoff.left_in_inbox && handoff.still_in.is_empty() {
        parts.push(format!(
            "Left in the inbox {}.",
            provider_place(handoff.provider)
        ));
    }
    parts.join(" ")
}

pub(super) fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

pub(super) fn due_detail<Tz: TimeZone>(due: DateTime<Utc>, now: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let local = due.with_timezone(tz);
    if due < now {
        format!("was due {}", local.format("%a %-d %b"))
    } else if due - now < Duration::days(6) {
        format!("due {}", local.format("%a"))
    } else {
        format!("due {}", local.format("%a %-d %b"))
    }
}

pub(super) fn day_part<Tz: TimeZone>(now: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let local = now.with_timezone(tz);
    let part = match local.hour() {
        5..=11 => "morning",
        12..=16 => "afternoon",
        17..=21 => "evening",
        _ => "night",
    };
    format!("{} {part}", local.format("%A"))
}

pub(super) fn count_phrase(count: u32, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

pub(super) fn now_headline(day_part: &str, people: u32, due: u32) -> String {
    let mut counts = Vec::new();
    if people > 0 {
        counts.push(format!(
            "{} waiting on you",
            count_phrase(people, "person", "people")
        ));
    }
    if due > 0 {
        counts.push(count_phrase(due, "thing to act on", "things to act on"));
    }
    if counts.is_empty() {
        format!("{day_part}.")
    } else {
        format!("{day_part}. {}.", counts.join(", "))
    }
}

pub(super) fn more_line(total: u32, shown: usize, mode: ModeKindData) -> Option<String> {
    let more = total.saturating_sub(u32::try_from(shown).unwrap_or(u32::MAX));
    (more > 0).then(|| format!("and {more} more in {}", mode.name()))
}

pub(super) const OVERLOAD_PEOPLE: u32 = 5;

pub(super) fn overload_line(split: PeopleSplitData, shown: usize) -> Option<String> {
    let PeopleSplitData { write_with, others } = split;
    let total = split.total();
    (total > OVERLOAD_PEOPLE).then(|| {
        let which = match shown {
            1 => "the one below",
            2 => "the two below",
            _ => "the three below",
        };
        if others == 0 {
            format!(
                "{total} people waiting on you. Start with {which}, furthest past your usual pace."
            )
        } else if write_with == 0 {
            format!("{total} others wrote to you and are waiting. Start with {which}.")
        } else {
            format!(
                "{total} people waiting on you: {write_with} you write with, {others} others. Start with {which}."
            )
        }
    })
}

pub(super) fn updates_line(messages: u32, sources: u32, top: &[String]) -> String {
    let counts = format!(
        "{} from {}.",
        count_phrase(messages, "update", "updates"),
        count_phrase(sources, "source", "sources")
    );
    if sources > 1 && !top.is_empty() {
        format!("{counts} Most from {}.", join_and(top))
    } else {
        counts
    }
}

pub(super) const EVENING_HOUR: u32 = 17;

pub(super) fn reading_fade(dates: Vec<DateTime<Utc>>) -> Duration {
    mxr_reading::fade::source_window(&dates).window
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_holds_until_a_newer_message_of_the_mode() {
        let at = |secs| DateTime::from_timestamp(secs, 0).unwrap();
        let (older, newest, later) = (
            mxr_core::id::MessageId::new(),
            mxr_core::id::MessageId::new(),
            mxr_core::id::MessageId::new(),
        );
        let mark = DeskDismissal {
            through_date: 10,
            through_id: *newest.as_uuid(),
            through_count: 3,
            covered: None,
        };
        assert!(mark_covers(&mark, [(at(4), &older), (at(10), &newest)]));
        assert!(!mark_covers(&mark, [(at(4), &older), (at(11), &later)]));
        assert!(!mark_covers(&mark, [(at(10), &later)]));
        assert!(mark_covers(&mark, []));

        let mut seen = vec![*older.as_uuid(), *newest.as_uuid()];
        seen.sort_unstable();
        let mark = DeskDismissal {
            covered: Some(seen.into()),
            ..mark
        };
        assert!(mark_covers(&mark, [(at(4), &older), (at(10), &newest)]));
        assert!(!mark_covers(&mark, [(at(1), &later)]));
    }

    #[test]
    fn merged_marks_keep_the_later_watermark() {
        let thread = mxr_core::id::ThreadId::new();
        let other = mxr_core::id::ThreadId::new();
        let mark = |date| DeskDismissal {
            through_date: date,
            through_id: uuid::Uuid::nil(),
            through_count: 2,
            covered: None,
        };
        let merged = merge_marks(
            HashMap::from([(thread.clone(), mark(5))]),
            HashMap::from([(thread.clone(), mark(9)), (other.clone(), mark(3))]),
        );
        assert_eq!(merged[&thread].through_date, 9);
        assert_eq!(merged[&other].through_date, 3);
        let merged = merge_marks(
            HashMap::from([(thread.clone(), mark(9))]),
            HashMap::from([(thread.clone(), mark(5))]),
        );
        assert_eq!(merged[&thread].through_date, 9);
    }

    #[test]
    fn a_new_sender_is_asked_where_their_mail_landed_first() {
        let account = AccountId::new();
        let question =
            screener_question(&account, "New@Example.com", SenderKindData::PaperTrail).unwrap();
        assert_eq!(question.question, "New sender. Keep in Updates?");
        assert_eq!(question.sender_email, "new@example.com");
        let labels: Vec<&str> = question.choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Updates", "Messages", "Reading", "Block"]);
        assert!(screener_question(&account, "x@y", SenderKindData::ScreenedOut).is_none());
    }

    #[test]
    fn now_lines_count_and_cap() {
        assert_eq!(
            now_headline("Friday afternoon", 3, 2),
            "Friday afternoon. 3 people waiting on you, 2 things to act on."
        );
        assert_eq!(
            now_headline("Friday afternoon", 1, 0),
            "Friday afternoon. 1 person waiting on you."
        );
        assert_eq!(now_headline("Friday night", 0, 0), "Friday night.");
        assert_eq!(
            more_line(11, 3, ModeKindData::Messages).as_deref(),
            Some("and 8 more in Messages")
        );
        assert!(more_line(3, 3, ModeKindData::Todo).is_none());
        let split = |write_with, others| PeopleSplitData { write_with, others };
        assert!(overload_line(split(2, OVERLOAD_PEOPLE - 2), 3).is_none());
        assert_eq!(
            overload_line(split(7, 4), 3).as_deref(),
            Some(
                "11 people waiting on you: 7 you write with, 4 others. Start with the three below."
            )
        );
        assert_eq!(
            overload_line(split(0, 6), 2).as_deref(),
            Some("6 others wrote to you and are waiting. Start with the two below.")
        );
        assert_eq!(
            overload_line(split(6, 0), 3).as_deref(),
            Some("6 people waiting on you. Start with the three below, furthest past your usual pace.")
        );
        assert_eq!(
            updates_line(23, 9, &["GitHub".into(), "Vercel".into(), "Stripe".into()]),
            "23 updates from 9 sources. Most from GitHub, Vercel and Stripe."
        );
        assert_eq!(
            updates_line(1, 1, &["GitHub".into()]),
            "1 update from 1 source."
        );
    }
}
