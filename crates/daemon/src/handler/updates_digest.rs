//! Building the Updates briefing from facts (blueprint 22, phase 4).
//!
//! A source line folds everything one source sent into its latest fact
//! per template; things with a state (parcels, builds, incidents) show
//! where they are now. Lines sort into Needs a look, Changed and Routine
//! by their strongest signal, never by date. Counts, latest states and
//! deltas are code. The digest's selection (what letting go acts on) is
//! computed here once and shared by the preview and the run.

use super::mode_rules::count_phrase;
use super::updates::Item;
use chrono::{DateTime, Duration, TimeZone, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_deliveries::DeliveryStatus;
use mxr_protocol::{
    updates_copy, UpdateDeltaData, UpdateExpiredData, UpdateLineData, UpdateLinkData,
    UpdateNumberData, UpdateProvenanceData, UpdateSectionData, UpdateSignalData,
    UpdateSourceSettingData, UpdateTrackerData, UpdatesCutData, UpdatesDigestData,
    UpdatesSinceData,
};
use mxr_store::{Delivery, PlaceMessage, TodoRecord, UpdateSourceRow};
use mxr_updates::{
    cuts::{digest_title, time_label},
    delta, signal, Cuts, Delta, Fact, FactSource, History, Quoted, Signal, TrackedOutcome,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Digests let go without opening a source before mxr asks about muting it.
pub(super) const MUTE_SUGGEST_AFTER: i64 = 8;
/// The mute question comes back no sooner than this.
const MUTE_SUGGEST_EVERY_DAYS: i64 = 30;
/// A parcel that went wrong stays in Needs a look this long.
pub(super) const PARCEL_WRONG_DAYS: i64 = 7;
/// A delivered parcel shows for a day, then leaves on its own.
const PARCEL_DELIVERED_DAYS: i64 = 1;
/// No news for this long past its latest arrival date, a parcel goes quiet.
const PARCEL_QUIET_PAST_ETA_DAYS: i64 = 7;
/// Or this long since its last event when it has no arrival date.
const PARCEL_QUIET_NO_ETA_DAYS: i64 = 14;

/// What a digest request is about.
#[derive(Debug, Clone, Default)]
pub(super) struct Scope {
    pub account_id: Option<AccountId>,
    /// A past cut; the latest one when absent.
    pub cut: Option<DateTime<Utc>>,
    /// One source only, for letting go of a source.
    pub source_key: Option<String>,
    pub list_expired: bool,
}

struct Past {
    date: DateTime<Utc>,
    id: MessageId,
    template_key: String,
    numbers: Vec<Quoted>,
}

/// Each source's earlier facts, oldest first.
#[derive(Default)]
pub(super) struct Histories(HashMap<(AccountId, String), Vec<Past>>);

impl Histories {
    pub(super) fn add(&mut self, account: &AccountId, message: &PlaceMessage, fact: &Fact) {
        self.0
            .entry((account.clone(), fact.source_key.clone()))
            .or_default()
            .push(Past {
                date: message.date,
                id: message.id.clone(),
                template_key: fact.template_key.clone(),
                numbers: fact.numbers.clone(),
            });
    }

    pub(super) fn sort(&mut self) {
        for list in self.0.values_mut() {
            list.sort_by(|a, b| (a.date, a.id.as_uuid()).cmp(&(b.date, b.id.as_uuid())));
            list.dedup_by(|a, b| a.id == b.id);
        }
    }

    /// The signal an item carries given what its source sent before, and
    /// the delta against the previous message of its template.
    fn score(&self, item: &Item) -> (Signal, Option<Delta>) {
        let key = (
            item.message.account_id.clone(),
            item.fact.source_key.clone(),
        );
        let list = self.0.get(&key).map_or(&[][..], Vec::as_slice);
        // Sorted by (date, id): everything before this message is a prefix.
        let at = (item.message.date, item.message.id.as_uuid());
        let earlier = &list[..list.partition_point(|past| (past.date, past.id.as_uuid()) < at)];
        let previous = earlier
            .iter()
            .rev()
            .find(|past| past.template_key == item.fact.template_key);
        let found = previous.and_then(|past| {
            delta(
                &item.fact.numbers,
                &past.numbers,
                item.message.date,
                past.date,
            )
        });
        let history = History {
            source_seen: !earlier.is_empty(),
            template_seen: previous.is_some(),
            number_moved: found.as_ref().is_some_and(|d| d.change != 0.0),
        };
        (signal(item.fact.base_signal, history), found)
    }
}

pub(super) struct DigestInputs<'a, Tz: TimeZone> {
    pub items: &'a [Item],
    pub histories: &'a Histories,
    pub sources: &'a HashMap<(AccountId, String), UpdateSourceRow>,
    /// Open to-dos on the threads in view.
    pub todos: &'a [TodoRecord],
    pub deliveries: &'a [Delivery],
    pub cuts: &'a Cuts,
    pub scope: &'a Scope,
    pub last_seen: Option<DateTime<Utc>>,
    pub never_had_any: bool,
    pub now: DateTime<Utc>,
    pub tz: &'a Tz,
}

struct Scored<'a> {
    item: &'a Item,
    signal: Signal,
    delta: Option<Delta>,
}

/// What letting go of a digest acts on.
pub(super) struct Selection {
    pub thread_ids: Vec<ThreadId>,
    pub message_ids: Vec<MessageId>,
    /// Updates mail in those threads that arrived after the cut: it stays.
    pub keep: HashSet<MessageId>,
    pub token: String,
    pub source_count: usize,
    /// Selected but not shown: muted, changes only, or past its window.
    pub hidden: usize,
    /// Selected threads To do also holds.
    pub in_todo: usize,
}

fn in_scope(item: &Item, scope: &Scope) -> bool {
    scope
        .source_key
        .as_deref()
        .is_none_or(|key| item.fact.source_key == key)
}

fn token(ids: &[&MessageId], cut_at: DateTime<Utc>) -> String {
    let mut sorted: Vec<String> = ids.iter().map(|id| id.as_str()).collect();
    sorted.sort_unstable();
    let mut hasher = Sha256::new();
    hasher.update(cut_at.timestamp().to_be_bytes());
    for id in sorted {
        hasher.update(id.as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Everything in the cut and in scope, shown or hidden: the digest's
/// stable set, and what letting go of it does. Nothing that arrived after
/// the cut is in it. The preview and the run both read this one rule.
fn select(
    items: &[Item],
    scope: &Scope,
    cut_at: DateTime<Utc>,
    shown: &HashSet<&MessageId>,
    todo_threads: &HashSet<&ThreadId>,
) -> Selection {
    let picked: Vec<&Item> = items
        .iter()
        .filter(|item| item.message.date <= cut_at && in_scope(item, scope))
        .collect();
    let ids: Vec<&MessageId> = picked.iter().map(|item| &item.message.id).collect();
    let mut seen = HashSet::new();
    let thread_ids: Vec<ThreadId> = picked
        .iter()
        .map(|item| &item.message.thread_id)
        .filter(|thread| seen.insert(*thread))
        .cloned()
        .collect();
    let keep = items
        .iter()
        .filter(|item| item.message.date > cut_at && seen.contains(&item.message.thread_id))
        .map(|item| item.message.id.clone())
        .collect();
    let sources: HashSet<(&AccountId, &str)> = picked
        .iter()
        .map(|item| (&item.message.account_id, item.fact.source_key.as_str()))
        .collect();
    Selection {
        token: token(&ids, cut_at),
        hidden: ids.iter().filter(|id| !shown.contains(*id)).count(),
        in_todo: thread_ids
            .iter()
            .filter(|t| todo_threads.contains(t))
            .count(),
        message_ids: ids.into_iter().cloned().collect(),
        source_count: sources.len(),
        thread_ids,
        keep,
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    count_phrase(super::now::count(n), one, many)
}

/// "Let go of 31 updates from 12 sources; 2 also in To do stay there."
pub(super) fn let_go_line(
    messages: usize,
    sources: usize,
    hidden: usize,
    in_todo: usize,
) -> String {
    let mut line = format!(
        "Let go of {} from {}",
        plural(messages, "update", "updates"),
        plural(sources, "source", "sources")
    );
    if hidden > 0 {
        line.push_str(&format!(", {hidden} of them not shown (tuned or expired)"));
    }
    if in_todo > 0 {
        line.push_str(&format!(
            "; {} also in To do {} there",
            in_todo,
            if in_todo == 1 { "stays" } else { "stay" }
        ));
    }
    line.push('.');
    line
}

fn setting_of(
    sources: &HashMap<(AccountId, String), UpdateSourceRow>,
    account: &AccountId,
    key: &str,
) -> UpdateSourceSettingData {
    sources
        .get(&(account.clone(), key.to_string()))
        .and_then(|row| UpdateSourceSettingData::parse(&row.setting))
        .unwrap_or_default()
}

const fn signal_data(signal: Signal) -> UpdateSignalData {
    match signal {
        Signal::Routine => UpdateSignalData::Routine,
        Signal::Changed => UpdateSignalData::Changed,
        Signal::NewSource => UpdateSignalData::NewSource,
        Signal::Anomaly => UpdateSignalData::Anomaly,
        Signal::NeedsYou => UpdateSignalData::NeedsYou,
    }
}

fn numbers_data(numbers: &[Quoted]) -> Vec<UpdateNumberData> {
    numbers
        .iter()
        .map(|n| UpdateNumberData {
            raw: n.raw.clone(),
            value: n.value,
            unit: n.unit.clone(),
        })
        .collect()
}

fn delta_data(found: &Delta) -> UpdateDeltaData {
    UpdateDeltaData {
        raw: found.raw.clone(),
        previous_raw: found.previous_raw.clone(),
        change: found.change,
        text: found.text.clone(),
        against: found.against,
    }
}

fn provenance(fact: &Fact, found: Option<&Delta>) -> Vec<UpdateProvenanceData> {
    let entry = |field: &str, source: &str, evidence: String| UpdateProvenanceData {
        field: field.to_string(),
        source: source.to_string(),
        evidence,
    };
    let mut out = vec![entry(
        "fact",
        "rule",
        match fact.fact_source {
            FactSource::Subject => "the subject, cleaned".to_string(),
            FactSource::Body => "the first line of the body; the subject says nothing".to_string(),
        },
    )];
    if !fact.numbers.is_empty() {
        let raws: Vec<&str> = fact.numbers.iter().map(|n| n.raw.as_str()).collect();
        out.push(entry(
            "numbers",
            "rule",
            format!("quoted from the email: {}", raws.join(", ")),
        ));
    }
    if let Some(found) = found {
        out.push(entry(
            "delta",
            "code",
            format!(
                "{} against {} on {}",
                found.raw,
                found.previous_raw,
                found.against.format("%-d %b")
            ),
        ));
    }
    if let Some(window) = &fact.window {
        out.push(entry("window", &window.source, window.evidence.clone()));
    }
    if let Some(tracked) = &fact.tracked {
        out.push(entry(
            "state",
            "rule",
            format!("\"{}\" in the subject", tracked.state),
        ));
    }
    if fact.link.is_some() {
        out.push(entry(
            "link",
            "rule",
            "the first link named by the words around it".to_string(),
        ));
    }
    out
}

struct LineContext<'a, Tz: TimeZone> {
    inputs: &'a DigestInputs<'a, Tz>,
    /// "08:00" for the digest, the next cut for what arrives after it.
    digest_label: &'a str,
    todo_by_thread: &'a HashMap<&'a ThreadId, &'a TodoRecord>,
}

fn source_line<Tz: TimeZone>(
    section: UpdateSectionData,
    id: String,
    members: &[&Scored<'_>],
    ctx: &LineContext<'_, Tz>,
) -> Option<UpdateLineData>
where
    Tz::Offset: std::fmt::Display,
{
    let newest = members
        .iter()
        .max_by_key(|m| (m.item.message.date, *m.item.message.id.as_uuid()))?;
    let strongest = members
        .iter()
        .map(|m| m.signal)
        .max()
        .unwrap_or(Signal::Routine);
    // The fact to show: in Changed, the newest message that changed (one
    // with a delta first); otherwise the newest.
    // A tracked thing shows its latest state, whatever changed before it.
    let lead = if section == UpdateSectionData::Changed && newest.item.fact.tracked.is_none() {
        members
            .iter()
            .filter(|m| m.signal == strongest || m.delta.is_some())
            .max_by_key(|m| {
                (
                    m.delta.as_ref().is_some_and(|d| d.change != 0.0),
                    m.item.message.date,
                )
            })
            .unwrap_or(newest)
    } else {
        newest
    };
    let fact = &lead.item.fact;
    let message = &newest.item.message;
    let mut message_ids = Vec::with_capacity(members.len());
    let mut thread_ids: Vec<ThreadId> = Vec::new();
    for member in members {
        message_ids.push(member.item.message.id.clone());
        if !thread_ids.contains(&member.item.message.thread_id) {
            thread_ids.push(member.item.message.thread_id.clone());
        }
    }
    let todo = thread_ids
        .iter()
        .find_map(|thread| ctx.todo_by_thread.get(thread))
        .map(|todo| todo.id.clone());
    let tracker = fact.tracked.as_ref().map(|tracked| {
        let first = members
            .iter()
            .map(|m| m.item.message.date)
            .min()
            .unwrap_or(message.date);
        UpdateTrackerData {
            kind: tracked.kind.as_str().to_string(),
            state: tracked.state.clone(),
            state_label: tracked.state.clone(),
            outcome: tracked.outcome.as_str().to_string(),
            steps: Vec::new(),
            step: None,
            detail: Some(match tracked.outcome {
                TrackedOutcome::Bad => format!(
                    "{} since {}",
                    plural(members.len(), "email", "emails"),
                    time_label(first, ctx.inputs.tz)
                ),
                _ => plural(members.len(), "email", "emails"),
            }),
            delivery_id: None,
        }
    });
    let account = &message.account_id;
    let setting = setting_of(ctx.inputs.sources, account, &fact.source_key);
    let source = if newest.item.corrected { "you" } else { "rule" };
    Some(UpdateLineData {
        id,
        section,
        account_id: account.clone(),
        source_key: fact.source_key.clone(),
        source_name: fact.source_name.clone(),
        sender_email: message.from_email.to_ascii_lowercase(),
        fact: fact.text.clone(),
        fact_source: fact.fact_source.as_str().to_string(),
        numbers: numbers_data(&fact.numbers),
        delta: lead.delta.as_ref().map(delta_data),
        signal: signal_data(strongest),
        count: super::now::count(members.len()),
        latest_message_id: Some(message.id.clone()),
        latest_thread_id: Some(message.thread_id.clone()),
        latest_at: message.date,
        time_label: (section == UpdateSectionData::NeedsALook)
            .then(|| time_label(message.date, ctx.inputs.tz)),
        link: fact.link.as_deref().and_then(link_data),
        tracker,
        in_todo: todo.as_ref().map(|_| updates_copy::IN_TODO.to_string()),
        todo_id: todo,
        todo_title: fact.todo_title.clone(),
        why: format!(
            "Here because: {} ({source}). In the {} digest.",
            newest.item.reason, ctx.digest_label
        ),
        setting,
        suggestion: None,
        provenance: provenance(fact, lead.delta.as_ref()),
        message_ids,
        thread_ids,
    })
}

/// One source's lines. Tracked things fold to one line each, with the
/// latest state deciding the section; needs-a-look mail gets a line per
/// template; the rest folds into one Changed or Routine line.
fn lines_for_source<Tz: TimeZone>(
    members: &[Scored<'_>],
    ctx: &LineContext<'_, Tz>,
) -> Vec<UpdateLineData>
where
    Tz::Offset: std::fmt::Display,
{
    let mut out = Vec::new();
    let Some(first) = members.first() else {
        return out;
    };
    let source = format!(
        "{}|{}",
        first.item.message.account_id, first.item.fact.source_key
    );
    let mut tracked: BTreeMap<&str, Vec<&Scored<'_>>> = BTreeMap::new();
    let mut needs: BTreeMap<&str, Vec<&Scored<'_>>> = BTreeMap::new();
    let mut rest: Vec<&Scored<'_>> = Vec::new();
    for member in members {
        if let Some(t) = &member.item.fact.tracked {
            tracked.entry(t.key.as_str()).or_default().push(member);
        } else if member.signal.needs_a_look() {
            needs
                .entry(member.item.fact.template_key.as_str())
                .or_default()
                .push(member);
        } else {
            rest.push(member);
        }
    }
    for (key, group) in tracked {
        let Some(latest) = group.iter().max_by_key(|m| m.item.message.date) else {
            continue;
        };
        let outcome = latest.item.fact.tracked.as_ref().map(|t| t.outcome);
        let moved = group
            .iter()
            .any(|m| m.item.fact.tracked.as_ref().map(|t| t.outcome) != outcome)
            || group.iter().any(|m| m.signal.is_change());
        let section = match outcome {
            Some(TrackedOutcome::Bad) => UpdateSectionData::NeedsALook,
            _ if moved => UpdateSectionData::Changed,
            _ => {
                rest.extend(group.iter().copied());
                continue;
            }
        };
        out.extend(source_line(
            section,
            format!("{source}|track|{key}"),
            &group,
            ctx,
        ));
    }
    for (template, group) in needs {
        out.extend(source_line(
            UpdateSectionData::NeedsALook,
            format!("{source}|needs|{template}"),
            &group,
            ctx,
        ));
    }
    if !rest.is_empty() {
        let changed = rest.iter().any(|m| m.signal.is_change());
        let section = if changed {
            UpdateSectionData::Changed
        } else {
            UpdateSectionData::Routine
        };
        out.extend(source_line(section, format!("{source}|source"), &rest, ctx));
    }
    out
}

/// Scored mail grouped by account and source, in a stable order.
fn by_source(scored: Vec<Scored<'_>>) -> BTreeMap<(String, String), Vec<Scored<'_>>> {
    let mut groups: BTreeMap<(String, String), Vec<Scored<'_>>> = BTreeMap::new();
    for entry in scored {
        groups
            .entry((
                entry.item.message.account_id.to_string(),
                entry.item.fact.source_key.clone(),
            ))
            .or_default()
            .push(entry);
    }
    groups
}

/// Lines for a set of items, split by setting: muted sources are hidden,
/// changes-only sources keep only lines that aren't routine. Returns the
/// lines and how many messages tuning hid (muted, changes only).
fn build_lines<'a, Tz: TimeZone>(
    scored: Vec<Scored<'a>>,
    ctx: &LineContext<'_, Tz>,
) -> (Vec<UpdateLineData>, usize, usize)
where
    Tz::Offset: std::fmt::Display,
{
    let by_source = by_source(scored);
    let (mut lines, mut muted, mut changes_only) = (Vec::new(), 0, 0);
    for members in by_source.values() {
        let first = &members[0].item;
        match setting_of(
            ctx.inputs.sources,
            &first.message.account_id,
            &first.fact.source_key,
        ) {
            UpdateSourceSettingData::Muted => {
                muted += members.len();
                continue;
            }
            UpdateSourceSettingData::ChangesOnly => {
                for line in lines_for_source(members, ctx) {
                    if line.section == UpdateSectionData::Routine {
                        changes_only += line.message_ids.len();
                    } else {
                        lines.push(line);
                    }
                }
            }
            _ => lines.extend(lines_for_source(members, ctx)),
        }
    }
    (lines, muted, changes_only)
}

/// What arrived after the cut, one line per source: a glance at what the
/// next digest holds, not a second briefing. Muted sources stay hidden.
fn since_lines<Tz: TimeZone>(
    scored: Vec<Scored<'_>>,
    ctx: &LineContext<'_, Tz>,
) -> Vec<UpdateLineData>
where
    Tz::Offset: std::fmt::Display,
{
    let by_source = by_source(scored);
    by_source
        .into_iter()
        .filter(|(_, members)| {
            let first = &members[0].item;
            setting_of(
                ctx.inputs.sources,
                &first.message.account_id,
                &first.fact.source_key,
            ) != UpdateSourceSettingData::Muted
        })
        .filter_map(|((account, key), members)| {
            let strongest = members.iter().map(|m| m.signal).max()?;
            let section = if strongest.needs_a_look() {
                UpdateSectionData::NeedsALook
            } else if strongest.is_change() {
                UpdateSectionData::Changed
            } else {
                UpdateSectionData::Routine
            };
            let refs: Vec<&Scored<'_>> = members.iter().collect();
            source_line(section, format!("{account}|{key}|since"), &refs, ctx)
        })
        .collect()
}

/// The claim a parcel's breakthrough to-do holds, one per state.
pub(super) fn parcel_dedup_key(delivery: &Delivery) -> String {
    format!("update|delivery|{}|{}", delivery.id, delivery.status)
}

/// What `t` and the breakthrough call a parcel's to-do.
pub(super) fn parcel_todo_title(name: &str) -> String {
    format!("Check delivery from {name}")
}

fn link_data(url: &str) -> Option<UpdateLinkData> {
    Some(UpdateLinkData {
        domain: mxr_updates::text::link_domain(url)?,
        url: url.to_string(),
    })
}

pub(super) fn parcel_went_wrong(status: DeliveryStatus) -> bool {
    matches!(
        status,
        DeliveryStatus::Exception | DeliveryStatus::AttemptFail | DeliveryStatus::Returned
    )
}

/// "Bookshop", the carrier, or "Your parcel".
pub(super) fn parcel_name(delivery: &Delivery) -> String {
    delivery
        .merchant
        .clone()
        .or_else(|| delivery.carrier.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Your parcel".to_string())
}

/// When a parcel still in flight went quiet: 7 days past its latest
/// arrival date, or 14 days without news when it has none. `None` once
/// delivered.
pub(super) fn parcel_quiet_since(delivery: &Delivery) -> Option<DateTime<Utc>> {
    if delivery.delivered_at.is_some() {
        return None;
    }
    Some(delivery.eta_until.map_or_else(
        || delivery.last_event_at + Duration::days(PARCEL_QUIET_NO_ETA_DAYS),
        |eta| eta + Duration::days(PARCEL_QUIET_PAST_ETA_DAYS),
    ))
}

const PARCEL_STEPS: [&str; 4] = ["ordered", "shipped", "out for delivery", "delivered"];

fn parcel_step(status: DeliveryStatus) -> Option<u32> {
    Some(match status {
        DeliveryStatus::Ordered | DeliveryStatus::InfoReceived => 0,
        DeliveryStatus::InTransit | DeliveryStatus::AvailableForPickup => 1,
        DeliveryStatus::OutForDelivery => 2,
        DeliveryStatus::Delivered => 3,
        _ => return None,
    })
}

fn parcel_label(status: DeliveryStatus) -> &'static str {
    match status {
        DeliveryStatus::Ordered => "ordered",
        DeliveryStatus::InfoReceived => "label created",
        DeliveryStatus::InTransit => "on its way",
        DeliveryStatus::OutForDelivery => "out for delivery",
        DeliveryStatus::AttemptFail => "delivery attempt failed",
        DeliveryStatus::AvailableForPickup => "ready to collect",
        DeliveryStatus::Delivered => "delivered",
        DeliveryStatus::Exception => "delivery problem",
        DeliveryStatus::Returned => "returned to sender",
        DeliveryStatus::Expired => "tracking expired",
    }
}

/// A parcel as a tracker line, or nothing once it has left: delivered a
/// day ago, quiet before this digest, or a problem older than a week.
fn parcel_line<Tz: TimeZone>(
    delivery: &Delivery,
    previous_cut: DateTime<Utc>,
    todos: &[TodoRecord],
    now: DateTime<Utc>,
    tz: &Tz,
) -> Option<UpdateLineData>
where
    Tz::Offset: std::fmt::Display,
{
    let status = DeliveryStatus::parse(&delivery.status)?;
    let delivered_at = delivery
        .delivered_at
        .or_else(|| (status == DeliveryStatus::Delivered).then_some(delivery.last_event_at));
    let quiet_since = parcel_quiet_since(delivery);
    let (section, outcome, label) = if parcel_went_wrong(status) {
        if delivery.last_event_at < now - Duration::days(PARCEL_WRONG_DAYS) {
            return None;
        }
        (UpdateSectionData::NeedsALook, "bad", parcel_label(status))
    } else if let Some(at) = delivered_at {
        if at < now - Duration::days(PARCEL_DELIVERED_DAYS) {
            return None;
        }
        (UpdateSectionData::Changed, "good", "delivered")
    } else if status == DeliveryStatus::Expired {
        return None;
    } else if let Some(quiet) = quiet_since.filter(|quiet| *quiet <= now) {
        // One "went quiet" line in the digest after it happened, then gone.
        if quiet <= previous_cut {
            return None;
        }
        (UpdateSectionData::Changed, "quiet", "went quiet")
    } else if delivery.last_event_at > previous_cut {
        (UpdateSectionData::Changed, "progress", parcel_label(status))
    } else {
        (UpdateSectionData::Routine, "progress", parcel_label(status))
    };
    let name = parcel_name(delivery);
    let mut detail = Vec::new();
    match (outcome, delivery.eta_until.or(delivery.eta_from)) {
        ("quiet", _) => detail.push(format!(
            "No news since {}",
            delivery.last_event_at.with_timezone(tz).format("%a %-d %b")
        )),
        ("progress", Some(eta)) => detail.push(format!(
            "Arriving by {}",
            eta.with_timezone(tz).format("%a %-d %b")
        )),
        _ => {}
    }
    if let Some(carrier) = delivery
        .carrier
        .as_ref()
        // "amazon" under "Amazon.com" says nothing new.
        .filter(|c| {
            delivery
                .merchant
                .as_ref()
                .is_none_or(|merchant| !merchant.to_lowercase().starts_with(&c.to_lowercase()))
        })
    {
        detail.push(carrier.clone());
    }
    let dedup = parcel_dedup_key(delivery);
    let todo = todos
        .iter()
        .find(|todo| {
            todo.dedup_key == dedup
                || (delivery.thread_id.is_some() && todo.thread_id == delivery.thread_id)
        })
        .map(|todo| todo.id.clone());
    let fact = match outcome {
        "quiet" => format!("Parcel from {name} went quiet"),
        _ => format!("Parcel {label}"),
    };
    Some(UpdateLineData {
        id: format!("parcel|{}", delivery.id),
        section,
        account_id: delivery.account_id.clone(),
        source_key: format!("parcel:{}", delivery.id),
        source_name: name.clone(),
        sender_email: String::new(),
        fact,
        fact_source: "rule".to_string(),
        numbers: Vec::new(),
        delta: None,
        signal: if section == UpdateSectionData::NeedsALook {
            UpdateSignalData::NeedsYou
        } else {
            UpdateSignalData::Changed
        },
        count: 1,
        message_ids: Vec::new(),
        thread_ids: delivery.thread_id.iter().cloned().collect(),
        latest_message_id: None,
        latest_thread_id: delivery.thread_id.clone(),
        latest_at: delivery.last_event_at,
        time_label: None,
        link: delivery.tracking_url.as_deref().and_then(link_data),
        tracker: Some(UpdateTrackerData {
            kind: "parcel".to_string(),
            state: delivery.status.clone(),
            state_label: label.to_string(),
            outcome: outcome.to_string(),
            steps: PARCEL_STEPS.iter().map(|s| (*s).to_string()).collect(),
            step: parcel_step(status),
            detail: (!detail.is_empty()).then(|| detail.join(" · ")),
            delivery_id: Some(delivery.id.to_string()),
        }),
        in_todo: todo.as_ref().map(|_| updates_copy::IN_TODO.to_string()),
        todo_id: todo,
        todo_title: parcel_todo_title(&name),
        why: format!(
            "Here because: a parcel with a tracking state ({}). Leaves Updates when it ends.",
            delivery.source
        ),
        setting: UpdateSourceSettingData::EveryDigest,
        suggestion: None,
        provenance: vec![UpdateProvenanceData {
            field: "state".to_string(),
            source: delivery.source.clone(),
            evidence: format!("delivery status {}", delivery.status),
        }],
    })
}

fn sort_lines(lines: &mut [UpdateLineData], section: UpdateSectionData) {
    if section == UpdateSectionData::Routine {
        lines.sort_by(|a, b| {
            b.count.cmp(&a.count).then_with(|| {
                a.source_name
                    .to_lowercase()
                    .cmp(&b.source_name.to_lowercase())
            })
        });
    } else {
        lines.sort_by(|a, b| b.latest_at.cmp(&a.latest_at).then_with(|| a.id.cmp(&b.id)));
    }
}

/// "1 needs a look, 2 changed. 23 routine from 9 sources."
fn headline(needs: usize, changed: usize, routine: &[UpdateLineData]) -> String {
    let mut first = Vec::new();
    if needs > 0 {
        first.push(format!(
            "{needs} {}",
            if needs == 1 {
                "needs a look"
            } else {
                "need a look"
            }
        ));
    }
    if changed > 0 {
        first.push(format!("{changed} changed"));
    }
    let mut out = String::new();
    if !first.is_empty() {
        out.push_str(&first.join(", "));
        out.push('.');
    }
    if !routine.is_empty() {
        let messages: u32 = routine.iter().map(|line| line.count).sum();
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&format!(
            "{messages} routine from {}.",
            plural(routine.len(), "source", "sources")
        ));
    }
    out
}

fn mute_suggestion(
    line: &UpdateLineData,
    sources: &HashMap<(AccountId, String), UpdateSourceRow>,
    now: DateTime<Utc>,
) -> Option<String> {
    let row = sources.get(&(line.account_id.clone(), line.source_key.clone()))?;
    let asked_lately = row
        .suggested_at
        .is_some_and(|at| at > now - Duration::days(MUTE_SUGGEST_EVERY_DAYS));
    (line.setting == UpdateSourceSettingData::EveryDigest
        && row.let_go_streak >= MUTE_SUGGEST_AFTER
        && !asked_lately)
        .then(|| {
            format!(
                "You've let go of {} {} digests in a row without opening it. Mute it, or changes only?",
                line.source_name, row.let_go_streak
            )
        })
}

pub(super) fn compose<Tz: TimeZone>(inputs: &DigestInputs<'_, Tz>) -> (UpdatesDigestData, Selection)
where
    Tz::Offset: std::fmt::Display,
{
    let now = inputs.now;
    let tz = inputs.tz;
    let reference = inputs.scope.cut.map_or(now, |cut| cut.min(now));
    let window = inputs.cuts.window(reference, tz);
    let cut_label = time_label(window.at, tz);
    let next_label = time_label(window.next, tz);
    let todo_by_thread: HashMap<&ThreadId, &TodoRecord> = inputs
        .todos
        .iter()
        .filter_map(|todo| todo.thread_id.as_ref().map(|thread| (thread, todo)))
        .collect();

    let mut digest_scored = Vec::new();
    let mut since_scored = Vec::new();
    let mut expired = Vec::new();
    for item in inputs
        .items
        .iter()
        .filter(|item| in_scope(item, inputs.scope))
    {
        if let Some(window_end) = item.fact.window.as_ref().filter(|w| w.until < now) {
            expired.push((item, window_end.until, window_end.kind.label()));
            continue;
        }
        let (mut signal, found) = inputs.histories.score(item);
        // New or changed means since the last digest: mail carried over
        // from an earlier cut is routine now, whatever it was then.
        let fresh_from = if item.message.date <= window.at {
            window.previous
        } else {
            window.at
        };
        if signal.is_change() && item.message.date <= fresh_from {
            signal = Signal::Routine;
        }
        let scored = Scored {
            item,
            signal,
            delta: found,
        };
        if item.message.date <= window.at {
            digest_scored.push(scored);
        } else {
            since_scored.push(scored);
        }
    }

    let digest_ctx = LineContext {
        inputs,
        digest_label: &cut_label,
        todo_by_thread: &todo_by_thread,
    };
    let (mut lines, muted, changes_only) = build_lines(digest_scored, &digest_ctx);
    if inputs.scope.source_key.is_none() {
        lines.extend(
            inputs
                .deliveries
                .iter()
                .filter(|d| {
                    inputs
                        .scope
                        .account_id
                        .as_ref()
                        .is_none_or(|account| &d.account_id == account)
                })
                .filter_map(|d| parcel_line(d, window.previous, inputs.todos, now, tz)),
        );
    }
    let mut suggested: HashSet<(AccountId, String)> = HashSet::new();
    for line in &mut lines {
        if suggested.insert((line.account_id.clone(), line.source_key.clone())) {
            line.suggestion = mute_suggestion(line, inputs.sources, now);
        }
    }
    let mut needs_a_look = Vec::new();
    let mut changed = Vec::new();
    let mut routine = Vec::new();
    for line in lines {
        match line.section {
            UpdateSectionData::NeedsALook => needs_a_look.push(line),
            UpdateSectionData::Changed => changed.push(line),
            UpdateSectionData::Routine => routine.push(line),
        }
    }
    sort_lines(&mut needs_a_look, UpdateSectionData::NeedsALook);
    sort_lines(&mut changed, UpdateSectionData::Changed);
    sort_lines(&mut routine, UpdateSectionData::Routine);

    let since_ctx = LineContext {
        inputs,
        digest_label: &next_label,
        todo_by_thread: &todo_by_thread,
    };
    let since_count = since_scored.len();
    let mut since_lines = since_lines(since_scored, &since_ctx);
    since_lines.sort_by_key(|line| std::cmp::Reverse(line.latest_at));
    let since_sources: HashSet<(&AccountId, &str)> = since_lines
        .iter()
        .map(|line| (&line.account_id, line.source_key.as_str()))
        .collect();

    let shown = || needs_a_look.iter().chain(&changed).chain(&routine);
    let message_count: usize = shown().map(|line| line.message_ids.len()).sum();
    let source_count = shown()
        .filter(|line| line.tracker.as_ref().is_none_or(|t| t.kind != "parcel"))
        .map(|line| (&line.account_id, line.source_key.as_str()))
        .collect::<HashSet<_>>()
        .len();
    let line_count = needs_a_look.len() + changed.len() + routine.len();
    let headline = headline(needs_a_look.len(), changed.len(), &routine);
    let empty_state = (line_count == 0).then(|| {
        if inputs.never_had_any {
            updates_copy::NEVER_HAD_ANY.to_string()
        } else if since_count == 0 {
            updates_copy::CLEAR_FOR_NOW
                .replace("{cut}", &cut_label)
                .replace("{next}", &next_label)
        } else {
            format!("The {cut_label} digest is clear. Next digest at {next_label}.")
        }
    });
    let hidden_line = match (muted, changes_only) {
        (0, 0) => None,
        (m, 0) => Some(format!(
            "{} from muted sources not shown.",
            plural(m, "update", "updates")
        )),
        (0, c) => Some(format!(
            "{} from changes-only sources not shown.",
            plural(c, "routine update", "routine updates")
        )),
        (m, c) => Some(format!(
            "{} from muted sources and {c} routine from changes-only sources not shown.",
            plural(m, "update", "updates")
        )),
    };
    let since_last_look = inputs.last_seen.map_or(0, |seen| {
        expired
            .iter()
            .filter(|(item, until, _)| *until > seen && *until > item.message.date)
            .count()
    });
    let expired_list = if inputs.scope.list_expired {
        expired
            .iter()
            .map(|(item, until, kind)| UpdateExpiredData {
                account_id: item.message.account_id.clone(),
                message_id: item.message.id.clone(),
                thread_id: item.message.thread_id.clone(),
                source_name: item.fact.source_name.clone(),
                fact: item.fact.text.clone(),
                kind: (*kind).to_string(),
                expired_at: *until,
            })
            .collect()
    } else {
        Vec::new()
    };
    let shown_ids: HashSet<&MessageId> = shown().flat_map(|line| &line.message_ids).collect();
    let selection = select(
        inputs.items,
        inputs.scope,
        window.at,
        &shown_ids,
        &todo_by_thread.keys().copied().collect(),
    );
    let all_sources: HashSet<(&AccountId, &str)> = inputs
        .items
        .iter()
        .map(|item| (&item.message.account_id, item.fact.source_key.as_str()))
        .collect();
    let muted_total = inputs
        .sources
        .values()
        .filter(|row| row.setting == UpdateSourceSettingData::Muted.as_str())
        .count();

    let digest = UpdatesDigestData {
        generated_at: now,
        header: updates_copy::HEADER.to_string(),
        cut: UpdatesCutData {
            at: window.at,
            title: digest_title(window.at, now, tz),
            label: cut_label.clone(),
            previous_at: window.previous,
            next_at: window.next,
            next_label: next_label.clone(),
            cuts: inputs.cuts.labels(),
        },
        headline,
        message_count: super::now::count(message_count),
        source_count: super::now::count(source_count),
        since: UpdatesSinceData {
            label: format!("arriving for {next_label}"),
            message_count: super::now::count(since_count),
            source_count: super::now::count(since_sources.len()),
            lines: since_lines,
        },
        needs_a_look,
        changed,
        routine,
        hidden_line,
        expired_line: (since_last_look > 0)
            .then(|| format!("{since_last_look} expired since you last looked.")),
        expired_count: super::now::count(since_last_look),
        expired: expired_list,
        let_go_line: (!selection.message_ids.is_empty()).then(|| {
            let_go_line(
                selection.message_ids.len(),
                selection.source_count,
                selection.hidden,
                selection.in_todo,
            )
        }),
        selection_token: selection.token.clone(),
        empty_state,
        source_total: super::now::count(all_sources.len()),
        muted_total: super::now::count(muted_total),
    };
    (digest, selection)
}
