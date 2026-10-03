//! The rules that place a thread in modes and the words each mode uses
//! for it (blueprint 22). Pure functions: the handlers in `modes.rs`,
//! `mode_done.rs` and `now.rs` read the store and pass plain data in, so
//! every rule here is unit-tested without a daemon.
//!
//! Rules only, no model, so every placement can say exactly why.

use chrono::{DateTime, Duration, TimeZone, Timelike, Utc};
use mxr_core::id::AccountId;
use mxr_core::types::ProviderKind;
use mxr_protocol::{
    DeskLaneKind, DeskRowData, ModeKindData, ModeMembershipData, ScreenerChoiceData,
    ScreenerQuestionData, SenderKindData,
};
use mxr_store::DeskDismissal;
use std::collections::HashMap;

/// Local parts of senders whose mail is a record: receipts, bills,
/// orders, bookings and statements.
const RECORD_LOCAL_PARTS: &[&str] = &[
    "receipt",
    "billing",
    "invoice",
    "order",
    "booking",
    "reservation",
    "statement",
];

/// Subject phrases that make an automated message a record, matched in
/// lowercase. Notifications ("your order has shipped") stay Updates as
/// well: a record is also an update until it's let go.
const RECORD_SUBJECT_PHRASES: &[&str] = &[
    "receipt",
    "invoice",
    "order confirmation",
    "order confirmed",
    "your order",
    "booking confirmation",
    "booking confirmed",
    "reservation confirmed",
    "your reservation",
    "itinerary",
    "e-ticket",
    "statement is ready",
    "your statement",
    "payment confirmation",
    "payment received",
];

/// What makes an automated message a record, for its why line: the
/// subject phrase, or the sender's address.
pub(super) fn record_evidence(from_email: &str, subject: &str) -> Option<String> {
    let subject_lower = subject.to_lowercase();
    if let Some(phrase) = RECORD_SUBJECT_PHRASES
        .iter()
        .find(|phrase| subject_lower.contains(*phrase))
    {
        return Some(format!("\"{phrase}\" in the subject"));
    }
    let local = from_email
        .split_once('@')
        .map_or(from_email, |(local, _)| local)
        .to_ascii_lowercase();
    RECORD_LOCAL_PARTS
        .iter()
        .find(|part| local.contains(*part))
        .map(|part| format!("sent from a {part} address"))
}

/// A mode's done mark still holds when every message of the mode was
/// there when it was made. Updates and Reading read only their own
/// messages, so your reply in a notification thread doesn't bring the
/// notification back.
pub(super) fn mark_covers<'a>(
    mark: &DeskDismissal,
    messages: impl IntoIterator<Item = (DateTime<Utc>, &'a mxr_core::id::MessageId)>,
) -> bool {
    messages
        .into_iter()
        .all(|(date, id)| mark.reaches_at(date, id))
}

/// The desk's own dismissals and done-in-Messages marks, as one map: a
/// thread put away by either stays off Messages, Now and the desk alike.
/// Where both exist the later watermark wins.
pub(super) fn merge_marks(
    mut dismissals: HashMap<mxr_core::id::ThreadId, DeskDismissal>,
    marks: HashMap<mxr_core::id::ThreadId, DeskDismissal>,
) -> HashMap<mxr_core::id::ThreadId, DeskDismissal> {
    for (thread, mark) in marks {
        dismissals
            .entry(thread)
            .and_modify(|existing| {
                if mark.order_key() > existing.order_key() {
                    *existing = mark;
                }
            })
            .or_insert(mark);
    }
    dismissals
}

/// The mode a sender kind lands in.
pub(super) const fn mode_for_kind(kind: SenderKindData) -> Option<ModeKindData> {
    match kind {
        SenderKindData::People => Some(ModeKindData::Messages),
        SenderKindData::Reading => Some(ModeKindData::Reading),
        SenderKindData::PaperTrail => Some(ModeKindData::Updates),
        SenderKindData::ScreenedOut => None,
    }
}

/// A first-time sender's one question, asked where their mail landed.
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

/// Whose turn it is with whom, in the words the row's lane uses:
/// "your turn with Maya Ortiz", "waiting on Sam", "new from Iris".
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

/// "22h", "3d", "40m": how long a row has waited.
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

/// Messages' membership for a desk row.
pub(super) fn messages_membership(row: &DeskRowData, early: bool) -> ModeMembershipData {
    let summary = messages_summary(row);
    membership(
        ModeKindData::Messages,
        format!("Here because: {summary}, {} (rule).", row.reason),
        format!("Also in Messages: {summary}"),
        early,
    )
}

/// Messages' membership for person mail no lane holds: "quiet".
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
    )
}

/// One membership line, with the mode's name and key filled in.
pub(super) fn membership(
    mode: ModeKindData,
    reason: String,
    also_in: String,
    early: bool,
) -> ModeMembershipData {
    ModeMembershipData {
        mode,
        name: mode.name().to_string(),
        key: mode.key().to_string(),
        reason,
        also_in,
        early,
        todo_ids: Vec::new(),
    }
}

/// How a provider is named in a toast: "Archived in Gmail."
pub(super) fn provider_name(kind: Option<&ProviderKind>) -> &'static str {
    match kind {
        Some(ProviderKind::Gmail) => "Gmail",
        Some(ProviderKind::OutlookPersonal | ProviderKind::OutlookWork) => "Outlook",
        _ => "the mail server",
    }
}

/// "in Gmail", "on the mail server".
fn provider_place(provider: &str) -> String {
    if provider.starts_with("the ") {
        format!("on {provider}")
    } else {
        format!("in {provider}")
    }
}

/// A mode still holding a thread, with what it says about it: "To do (due
/// Wed)".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StillIn {
    pub mode: ModeKindData,
    pub detail: Option<String>,
}

/// What a mode's done did, for the toast.
pub(super) struct Handoff<'a> {
    pub mode: ModeKindData,
    pub still_in: &'a [StillIn],
    /// Archived at the provider because the last mode let go.
    pub archived: bool,
    /// Still in the provider's inbox although no mode holds it (the
    /// setting is off).
    pub left_in_inbox: bool,
    /// To do: the source is a record, so it is filed in Archive.
    pub filed: bool,
    pub provider: &'a str,
}

/// The handoff toast (blueprint 22): "Done in Messages. Still in To do
/// (due Wed)." or "Done. Archived in Gmail." Archive the mode and archive
/// the provider action share a word, so the copy keeps them apart: "Filed
/// in Archive" for a record, "Archived in Gmail" for the provider.
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

/// "A", "A and B", "A, B and C".
pub(super) fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// "due Wed" within the week, "due Wed 21 Oct" further out, "was due Fri"
/// once past.
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

/// "Friday afternoon".
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

fn count_phrase(count: u32, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

/// "Friday afternoon. 3 people, 2 things to act on."
pub(super) fn now_headline(day_part: &str, people: u32, due: u32) -> String {
    let mut counts = Vec::new();
    if people > 0 {
        counts.push(count_phrase(people, "person", "people"));
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

/// "and 8 more in Messages".
pub(super) fn more_line(total: u32, shown: usize, mode: ModeKindData) -> Option<String> {
    let more = total.saturating_sub(u32::try_from(shown).unwrap_or(u32::MAX));
    (more > 0).then(|| format!("and {more} more in {}", mode.name()))
}

/// You owe past this many people becomes one line instead of a pile.
pub(super) const OVERLOAD_OWED: u32 = 5;

/// "11 people are waiting on you. The three below are furthest past your
/// usual pace." (Sunsama's workload warning; the rows are already sorted
/// by how far past the usual pace they are.)
pub(super) fn overload_line(owed: u32, shown: usize) -> Option<String> {
    (owed > OVERLOAD_OWED).then(|| {
        let which = match shown {
            1 => "The one below is".to_string(),
            2 => "The two below are".to_string(),
            _ => "The three below are".to_string(),
        };
        format!("{owed} people are waiting on you. {which} furthest past your usual pace.")
    })
}

/// "23 updates from 9 sources. Most from GitHub, Vercel and Stripe."
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

/// The hour the Reading pick appears (Things' This Evening).
pub(super) const EVENING_HOUR: u32 = 17;

/// Updates' two digest cuts a day, local time (blueprint 22, Updates'
/// rhythm).
const DIGEST_CUTS: [(u32, u32); 2] = [(8, 0), (16, 30)];

/// Now's Updates card never reaches back further than this, however long
/// since you last looked.
const UPDATES_CARD_MAX_DAYS: i64 = 2;

/// Where Now's Updates card starts: the cut before the latest one, so the
/// card holds the latest digest and what has arrived since, and never more
/// than two days back.
pub(super) fn updates_card_since<Tz: TimeZone>(now: DateTime<Utc>, tz: &Tz) -> DateTime<Utc> {
    let today = now.with_timezone(tz).date_naive();
    let mut cuts: Vec<DateTime<Utc>> = (0..3)
        .filter_map(|back| today.checked_sub_days(chrono::Days::new(back)))
        .flat_map(|day| {
            DIGEST_CUTS.iter().filter_map(move |(hour, minute)| {
                tz.from_local_datetime(&day.and_hms_opt(*hour, *minute, 0)?)
                    .earliest()
                    .map(|at| at.with_timezone(&Utc))
            })
        })
        .filter(|cut| *cut <= now)
        .collect();
    cuts.sort_unstable_by(|a, b| b.cmp(a));
    let floor = now - Duration::days(UPDATES_CARD_MAX_DAYS);
    cuts.get(1).copied().unwrap_or(floor).max(floor)
}

/// How long a newsletter's issues stay in Reading before they fade: twice
/// the source's median interval, clamped to 2 to 14 days (blueprint 22,
/// Reading's rhythm). One issue alone fades at the longest.
pub(super) fn reading_fade(mut dates: Vec<DateTime<Utc>>) -> Duration {
    let (shortest, longest) = (Duration::days(2), Duration::days(14));
    dates.sort_unstable();
    let mut gaps: Vec<Duration> = dates.windows(2).map(|pair| pair[1] - pair[0]).collect();
    if gaps.is_empty() {
        return longest;
    }
    gaps.sort_unstable();
    let median = gaps[gaps.len() / 2];
    (median * 2).clamp(shortest, longest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn at(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn records_come_from_the_subject_or_the_sending_address() {
        assert_eq!(
            record_evidence("no-reply@shop.example", "Your receipt from Shop").as_deref(),
            Some("\"receipt\" in the subject")
        );
        assert_eq!(
            record_evidence("billing@thameswater.co.uk", "Reminder: your water bill").as_deref(),
            Some("sent from a billing address")
        );
        assert!(record_evidence("notifications@github.com", "Build failed on main").is_none());
        assert!(record_evidence("alerts@bank.example", "New sign-in to your account").is_none());
    }

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
        };
        assert!(mark_covers(&mark, [(at(4), &older), (at(10), &newest)]));
        assert!(!mark_covers(&mark, [(at(4), &older), (at(11), &later)]));
        // Same second: the id breaks the tie, and ids made later sort later.
        assert!(!mark_covers(&mark, [(at(10), &later)]));
        assert!(mark_covers(&mark, []));
    }

    #[test]
    fn merged_marks_keep_the_later_watermark() {
        let thread = mxr_core::id::ThreadId::new();
        let other = mxr_core::id::ThreadId::new();
        let mark = |date| DeskDismissal {
            through_date: date,
            through_id: uuid::Uuid::nil(),
            through_count: 2,
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
    fn the_toast_names_where_the_item_went() {
        let todo = [StillIn {
            mode: ModeKindData::Todo,
            detail: Some("due Wed".into()),
        }];
        let copy = |handoff: Handoff<'_>| handoff_copy(&handoff);
        assert_eq!(
            copy(Handoff {
                mode: ModeKindData::Messages,
                still_in: &todo,
                archived: false,
                left_in_inbox: false,
                filed: false,
                provider: "Gmail",
            }),
            "Done in Messages. Still in To do (due Wed)."
        );
        assert_eq!(
            copy(Handoff {
                mode: ModeKindData::Messages,
                still_in: &[],
                archived: true,
                left_in_inbox: false,
                filed: false,
                provider: "Gmail",
            }),
            "Done. Archived in Gmail."
        );
        assert_eq!(
            copy(Handoff {
                mode: ModeKindData::Todo,
                still_in: &[],
                archived: true,
                left_in_inbox: false,
                filed: true,
                provider: "Gmail",
            }),
            "Ticked off. Filed in Archive. Archived in Gmail."
        );
        assert_eq!(
            copy(Handoff {
                mode: ModeKindData::Updates,
                still_in: &[],
                archived: false,
                left_in_inbox: true,
                filed: false,
                provider: "the mail server",
            }),
            "Done in Updates. Left in the inbox on the mail server."
        );
        let two = [
            StillIn {
                mode: ModeKindData::Todo,
                detail: Some("due Wed".into()),
            },
            StillIn {
                mode: ModeKindData::Updates,
                detail: None,
            },
        ];
        assert_eq!(
            copy(Handoff {
                mode: ModeKindData::Messages,
                still_in: &two,
                archived: false,
                left_in_inbox: false,
                filed: false,
                provider: "Gmail",
            }),
            "Done in Messages. Still in To do (due Wed) and Updates."
        );
    }

    #[test]
    fn dates_and_day_parts_read_the_way_people_say_them() {
        let tz = FixedOffset::east_opt(0).unwrap();
        let now = at("2026-10-02T15:10:00Z"); // a Friday
        assert_eq!(day_part(now, &tz), "Friday afternoon");
        assert_eq!(due_detail(at("2026-10-07T12:00:00Z"), now, &tz), "due Wed");
        assert_eq!(
            due_detail(at("2026-10-21T12:00:00Z"), now, &tz),
            "due Wed 21 Oct"
        );
        assert_eq!(
            due_detail(at("2026-09-30T12:00:00Z"), now, &tz),
            "was due Wed 30 Sep"
        );
        assert_eq!(age_label(now - Duration::hours(22), now), "22h");
        assert_eq!(age_label(now - Duration::days(3), now), "3d");
        assert_eq!(age_label(now, now), "1m");
    }

    #[test]
    fn now_lines_count_and_cap() {
        assert_eq!(
            now_headline("Friday afternoon", 3, 2),
            "Friday afternoon. 3 people, 2 things to act on."
        );
        assert_eq!(
            now_headline("Friday afternoon", 1, 0),
            "Friday afternoon. 1 person."
        );
        assert_eq!(now_headline("Friday night", 0, 0), "Friday night.");
        assert_eq!(
            more_line(11, 3, ModeKindData::Messages).as_deref(),
            Some("and 8 more in Messages")
        );
        assert!(more_line(3, 3, ModeKindData::Todo).is_none());
        assert!(overload_line(OVERLOAD_OWED, 3).is_none());
        assert_eq!(
            overload_line(11, 3).as_deref(),
            Some(
                "11 people are waiting on you. The three below are furthest past your usual pace."
            )
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
