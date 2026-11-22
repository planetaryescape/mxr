use super::desk_lanes::{pace_ratio, RECENT_DAYS};
use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_protocol::{
    ClosenessData, DeskRowData, MessagesBandData, MessagesRowData, MessagesRowKindData,
    MessagesTopicData, PersonRefData, TopicStateData,
};
use mxr_store::{PersonFacts, PersonLink};
use std::collections::{HashMap, HashSet};

pub(super) const PINNED_CAP: usize = 9;
const CLOSE_MIN_OUTBOUND: u32 = 10;
const CLOSE_MIN_RECIPROCAL: u32 = 5;
const CLOSE_RECENT_DAYS: i64 = 90;
const CLOSE_MIN_KNOWN_DAYS: i64 = 30;
const REGULAR_MIN_OUTBOUND: u32 = 3;
const REGULAR_RECENT_DAYS: i64 = 365;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum RowKey {
    Person(String),
    Group(ThreadId),
}

impl RowKey {
    pub(super) fn id(&self) -> String {
        match self {
            Self::Person(email) => format!("person:{email}"),
            Self::Group(thread) => format!("group:{thread}"),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(super) struct People {
    primary_of: HashMap<String, String>,
    aliases: HashMap<String, Vec<String>>,
}

impl People {
    pub(super) fn new(links: &[PersonLink]) -> Self {
        let mut people = Self::default();
        for link in links {
            people
                .primary_of
                .insert(link.email.clone(), link.person_email.clone());
            people
                .aliases
                .entry(link.person_email.clone())
                .or_default()
                .push(link.email.clone());
        }
        people
    }

    pub(super) fn primary(&self, email: &str) -> String {
        let email = email.to_ascii_lowercase();
        self.primary_of.get(&email).cloned().unwrap_or(email)
    }

    pub(super) fn addresses(&self, primary: &str) -> Vec<String> {
        let mut out = vec![primary.to_string()];
        if let Some(aliases) = self.aliases.get(primary) {
            let mut aliases = aliases.clone();
            aliases.sort();
            out.extend(aliases);
        }
        out
    }
}

#[derive(Debug, Clone)]
pub(super) struct Topic {
    pub key: RowKey,
    pub data: MessagesTopicData,
    pub people: Vec<String>,
    pub turn: Option<DeskRowData>,
    pub unread: bool,
}

pub(super) struct RowContext<'a> {
    pub account_id: &'a AccountId,
    pub people: &'a People,
    pub facts: &'a HashMap<String, PersonFacts>,
    pub names: &'a HashMap<String, String>,
    pub pinned: &'a HashSet<String>,
    pub now: DateTime<Utc>,
}

impl RowContext<'_> {
    pub(super) fn person(&self, primary: &str) -> PersonRefData {
        PersonRefData {
            id: primary.to_string(),
            name: self.names.get(primary).cloned().or_else(|| {
                self.facts
                    .get(primary)
                    .and_then(|f| f.display_name.clone())
                    .filter(|n| !n.trim().is_empty() && !n.contains('@'))
            }),
            addresses: self.people.addresses(primary),
        }
    }

    pub(super) fn merged_facts(&self, primary: &str) -> Option<PersonFacts> {
        let mut merged: Option<PersonFacts> = None;
        for address in self.people.addresses(primary) {
            let Some(facts) = self.facts.get(&address) else {
                continue;
            };
            merged = Some(match merged {
                None => facts.clone(),
                Some(mut sum) => {
                    sum.first_seen_at = sum.first_seen_at.min(facts.first_seen_at);
                    sum.last_inbound_at = sum.last_inbound_at.max(facts.last_inbound_at);
                    sum.last_outbound_at = sum.last_outbound_at.max(facts.last_outbound_at);
                    sum.total_inbound += facts.total_inbound;
                    sum.total_outbound += facts.total_outbound;
                    sum.replied_count += facts.replied_count;
                    sum.cadence_days_p50 = sum.cadence_days_p50.or(facts.cadence_days_p50);
                    sum
                }
            });
        }
        merged
    }
}

pub(super) fn closeness(facts: Option<&PersonFacts>, now: DateTime<Utc>) -> ClosenessData {
    let Some(facts) = facts.filter(|f| f.total_outbound > 0) else {
        return ClosenessData::New;
    };
    let last = facts.last_inbound_at.max(facts.last_outbound_at);
    let within = |days: i64| last.is_some_and(|at| at >= now - Duration::days(days));
    let reciprocal = facts.total_inbound.min(facts.total_outbound);
    if facts.total_outbound >= CLOSE_MIN_OUTBOUND
        && reciprocal >= CLOSE_MIN_RECIPROCAL
        && within(CLOSE_RECENT_DAYS)
        && facts.first_seen_at <= now - Duration::days(CLOSE_MIN_KNOWN_DAYS)
    {
        ClosenessData::Close
    } else if facts.total_outbound >= REGULAR_MIN_OUTBOUND && within(REGULAR_RECENT_DAYS) {
        ClosenessData::Regular
    } else {
        ClosenessData::Occasional
    }
}

pub(super) fn sort_topics(topics: &mut [Topic]) {
    let rank = |state: TopicStateData| match state {
        TopicStateData::YourTurn => 0,
        TopicStateData::Waiting => 1,
        TopicStateData::Quiet => 2,
        TopicStateData::Done => 3,
    };
    topics.sort_by(|a, b| {
        rank(a.data.state)
            .cmp(&rank(b.data.state))
            .then(b.data.last_at.cmp(&a.data.last_at))
            .then_with(|| a.data.thread_id.as_str().cmp(&b.data.thread_id.as_str()))
    });
}

pub(super) fn band(topics: &[Topic], now: DateTime<Utc>) -> MessagesBandData {
    if topics
        .iter()
        .any(|t| t.data.state == TopicStateData::YourTurn)
    {
        return MessagesBandData::YourTurn;
    }
    if topics.iter().all(|t| t.data.state == TopicStateData::Done) {
        return MessagesBandData::Quiet;
    }
    let recent = now - Duration::days(RECENT_DAYS);
    let live = topics.iter().any(|t| {
        t.data.state == TopicStateData::Waiting
            || (t.data.state != TopicStateData::Done && t.data.last_at >= recent)
    });
    if live {
        MessagesBandData::Recent
    } else {
        MessagesBandData::Quiet
    }
}

pub(super) fn build_row(
    key: RowKey,
    mut topics: Vec<Topic>,
    ctx: &RowContext<'_>,
) -> MessagesRowData {
    sort_topics(&mut topics);
    let band = band(&topics, ctx.now);
    let turn = topics.iter().find_map(|t| t.turn.as_ref());
    let last_at = topics
        .iter()
        .map(|t| t.data.last_at)
        .max()
        .unwrap_or(ctx.now);
    let (kind, title, person, members, closeness) = match &key {
        RowKey::Person(primary) => {
            let person = ctx.person(primary);
            let closeness = closeness(ctx.merged_facts(primary).as_ref(), ctx.now);
            (
                MessagesRowKindData::Person,
                person.label().to_string(),
                Some(person),
                Vec::new(),
                closeness,
            )
        }
        RowKey::Group(_) => {
            let members: Vec<PersonRefData> = topics
                .first()
                .map(|t| t.people.iter().map(|p| ctx.person(p)).collect())
                .unwrap_or_default();
            let closeness = members
                .iter()
                .map(|m| closeness(ctx.merged_facts(&m.id).as_ref(), ctx.now))
                .min()
                .unwrap_or(ClosenessData::New);
            (
                MessagesRowKindData::Group,
                group_title(&members),
                None,
                members,
                closeness,
            )
        }
    };
    let pinned = person
        .as_ref()
        .is_some_and(|p| p.addresses.iter().any(|a| ctx.pinned.contains(a)));
    let why = why_line(
        band,
        turn,
        person.as_ref(),
        &members,
        closeness,
        last_at,
        ctx.now,
    );
    MessagesRowData {
        id: key.id(),
        kind,
        account_id: ctx.account_id.clone(),
        band,
        title,
        person,
        members,
        closeness,
        your_turn: band == MessagesBandData::YourTurn,
        last_at,
        turn_since: turn.map(|row| row.since),
        preview: None,
        draft: None,
        usual_reply_seconds: turn.and_then(|row| row.usual_seconds),
        pace_label: turn.and_then(|row| row.usual_seconds).map(pace_label),
        overdue: turn.is_some_and(|row| row.overdue),
        pinned,
        unread: topics.iter().any(|t| t.unread),
        topics: topics.into_iter().map(|t| t.data).collect(),
        why,
    }
}

pub(super) fn group_title(members: &[PersonRefData]) -> String {
    let names: Vec<String> = members.iter().map(PersonRefData::first_name).collect();
    match names.len() {
        0 => "Group".to_string(),
        1..=3 => names.join(", "),
        n => format!("{}, {} and {} others", names[0], names[1], n - 2),
    }
}

fn why_line(
    band: MessagesBandData,
    turn: Option<&DeskRowData>,
    person: Option<&PersonRefData>,
    members: &[PersonRefData],
    closeness: ClosenessData,
    last_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> String {
    let who = person.map_or_else(|| group_title(members), PersonRefData::first_name);
    let history = match closeness {
        ClosenessData::Close => "you write to each other often",
        ClosenessData::Regular => "you write now and then",
        ClosenessData::Occasional => "you've written before",
        ClosenessData::New => "you haven't written to them yet",
    };
    match (band, turn) {
        (MessagesBandData::YourTurn, Some(row)) => {
            format!("Here because: {who} {}, and {history} (rule).", row.reason)
        }
        (MessagesBandData::Quiet, _) => format!(
            "Here because: nothing needs you here since {} (rule).",
            day_label(last_at, now, &Local)
        ),
        _ => format!(
            "Here because: {history}; last {} (rule).",
            day_label(last_at, now, &Local)
        ),
    }
}

pub(super) fn pace_label(seconds: i64) -> String {
    let minutes = (seconds / 60).max(1);
    if minutes < 60 {
        format!("usually {minutes}m")
    } else if minutes < 48 * 60 {
        format!("usually {}h", minutes / 60)
    } else {
        format!("usually {}d", minutes / (24 * 60))
    }
}

pub(super) fn day_label<Tz: TimeZone>(at: DateTime<Utc>, now: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let day = at.with_timezone(tz).date_naive();
    let today = now.with_timezone(tz).date_naive();
    let days = (today - day).num_days();
    match days {
        i64::MIN..=0 => "today".to_string(),
        1 => "yesterday".to_string(),
        2..=6 => at.with_timezone(tz).format("%A").to_string(),
        _ if day.year() == today.year() => at.with_timezone(tz).format("%-d %b").to_string(),
        _ => at.with_timezone(tz).format("%-d %b %Y").to_string(),
    }
}

pub(super) fn rank_your_turn(rows: &mut [(MessagesRowData, Option<f64>)]) {
    rows.sort_by(|(a, a_ratio), (b, b_ratio)| {
        a.closeness
            .cmp(&b.closeness)
            .then(b_ratio.unwrap_or(0.0).total_cmp(&a_ratio.unwrap_or(0.0)))
            .then(a.turn_since.cmp(&b.turn_since))
            .then_with(|| a.id.cmp(&b.id))
    });
}

pub(super) fn rank_by_recency(rows: &mut [MessagesRowData]) {
    rows.sort_by(|a, b| b.last_at.cmp(&a.last_at).then_with(|| a.id.cmp(&b.id)));
}

pub(super) fn turn_ratio(topics: &[Topic]) -> Option<f64> {
    topics
        .iter()
        .filter_map(|t| t.turn.as_ref())
        .map(pace_ratio)
        .reduce(f64::max)
}

pub(super) fn preview_text(text: &str) -> String {
    const PREVIEW_CHARS: usize = 140;
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= PREVIEW_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(PREVIEW_CHARS).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end_matches([',', ';', ':', ' ']))
}

#[cfg(test)]
mod tests;
