//! Reading (blueprint 22, phase 5): the edition, the reader and Later.
//!
//! Reading holds inbox mail the sender classifier puts in Reading, read
//! through `places::placed_inbox` minus Reading's done marks, so the
//! edition, mode membership and Now never disagree. Each message is cut
//! into readable items by `mxr_reading::extract` (cached in
//! `reading_items`), each source gets its fade window, and the edition
//! bands and ranks them.
//!
//! Expiry is done in Reading: an issue past its source's window gets
//! Reading's done mark, written here and by the post-sync sweep through
//! the same plan. It never archives at the provider (blueprint 22, "Expiry
//! lets go in its mode"); Later never expires.

mod article;
mod item;
mod sources;

pub(super) use article::fetch_article;
pub(super) use item::{
    export_highlights, get_item, record_engagement, save_highlight, set_later, set_source,
};

use super::places::{placed_inbox, scoped_accounts, Placed};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Duration, Local, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Envelope, UnsubscribeMethod};
use mxr_protocol::{
    reading_copy, MailKindData, ModeKindData, ReadingBandData, ReadingBandGroupData,
    ReadingEditionData, ReadingEmptyData, ReadingItemData, ReadingItemKindData, ReadingLinkData,
    ReadingShapeData, ResponseData, SenderKindData,
};
use mxr_reading::edition::{self, Band, Rankable, Visit};
use mxr_reading::extract::EXTRACTOR_VERSION;
use mxr_reading::{pace, Shape};
use mxr_store::{DeskDismissal, ModeDoneMark, ReadingItemRow, ReadingStateRow, ReadingVisitRow};
use sources::{SourceInfo, Sources};
use std::collections::{HashMap, HashSet};

/// Later items older than this are asked about once.
const STILL_WANT_IT_AFTER: Duration = Duration::days(30);
/// Finished items read for the reader's pace.
const PACE_SAMPLES: u32 = 50;

/// `<message id>:<index>`.
pub(super) fn item_key(message_id: &MessageId, idx: i64) -> String {
    format!("{message_id}:{idx}")
}

pub(super) fn parse_item_key(key: &str) -> Result<(MessageId, i64), HandlerError> {
    let invalid = || {
        HandlerError::InvalidRequest(format!(
            "\"{key}\" is not a Reading item; items look like <message id>:<index>"
        ))
    };
    let (message, idx) = key.trim().rsplit_once(':').ok_or_else(invalid)?;
    let message_id: MessageId = message.parse().map_err(|_| invalid())?;
    let idx: i64 = idx.parse().map_err(|_| invalid())?;
    if idx < 0 {
        return Err(invalid());
    }
    Ok((message_id, idx))
}

/// The message facts Reading needs, from an inbox row or an envelope.
#[derive(Debug, Clone)]
pub(super) struct Issue {
    pub id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    /// Lowercase.
    pub sender: String,
    /// As stored, for queries by address.
    pub raw_sender: String,
    pub source_name: String,
    pub subject: String,
    /// Shown when no body is stored.
    pub snippet: String,
    pub date: DateTime<Utc>,
    pub unsubscribe: UnsubscribeMethod,
    /// Why the classifier put it in Reading, when it is in the inbox.
    pub kind: Option<MailKindData>,
}

impl Issue {
    fn from_placed(placed: &Placed) -> Self {
        let message = &placed.message;
        Self {
            id: message.id.clone(),
            account_id: message.account_id.clone(),
            thread_id: message.thread_id.clone(),
            sender: message.from_email.to_ascii_lowercase(),
            raw_sender: message.from_email.clone(),
            source_name: source_name(message.from_name.as_deref(), &message.from_email),
            subject: message.subject.clone(),
            snippet: message.snippet.clone(),
            date: message.date,
            unsubscribe: message.unsubscribe.clone(),
            kind: Some(placed.kind.clone()),
        }
    }

    pub(super) fn from_envelope(envelope: &Envelope) -> Self {
        Self {
            id: envelope.id.clone(),
            account_id: envelope.account_id.clone(),
            thread_id: envelope.thread_id.clone(),
            sender: envelope.from.email.to_ascii_lowercase(),
            raw_sender: envelope.from.email.clone(),
            source_name: source_name(envelope.from.name.as_deref(), &envelope.from.email),
            subject: envelope.subject.clone(),
            snippet: envelope.snippet.clone(),
            date: envelope.date,
            unsubscribe: envelope.unsubscribe.clone(),
            kind: None,
        }
    }
}

impl Issue {
    /// What extraction reads from this issue and its stored body.
    pub(super) fn input<'a>(
        &'a self,
        body: Option<&'a mxr_core::types::MessageBody>,
    ) -> mxr_reading::IssueInput<'a> {
        mxr_reading::IssueInput {
            subject: &self.subject,
            source: Some(&self.source_name),
            html: body.and_then(|b| b.text_html.as_deref()),
            text: body.and_then(|b| b.text_plain.as_deref()),
            snippet: &self.snippet,
        }
    }
}

/// The demo mailbox, in `mxr demo` or the web app's end-to-end daemon.
pub(super) fn demo_mailbox() -> bool {
    mxr_config::is_demo_instance() || mxr_provider_fake::fixtures::demo_dataset_active()
}

fn source_name(name: Option<&str>, email: &str) -> String {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map_or_else(|| email.to_ascii_lowercase(), str::to_string)
}

/// The cached items of `issues`, extracting the ones missing or built by
/// an older extractor. Extraction runs off the async threads.
pub(super) async fn ensure_items(
    state: &AppState,
    issues: &[&Issue],
) -> Result<HashMap<MessageId, Vec<ReadingItemRow>>, HandlerError> {
    let ids: Vec<MessageId> = issues.iter().map(|issue| issue.id.clone()).collect();
    let mut by_message: HashMap<MessageId, Vec<ReadingItemRow>> = HashMap::new();
    for row in state.store.reading_items_for_messages(&ids).await? {
        by_message
            .entry(row.message_id.clone())
            .or_default()
            .push(row);
    }
    let stale: Vec<&Issue> = issues
        .iter()
        .copied()
        .filter(|issue| {
            by_message
                .get(&issue.id)
                .and_then(|rows| rows.first())
                .is_none_or(|row| row.extractor_version != EXTRACTOR_VERSION)
        })
        .collect();
    if stale.is_empty() {
        return Ok(by_message);
    }
    let mut inputs = Vec::with_capacity(stale.len());
    for issue in &stale {
        let body = state.store.get_body(&issue.id).await?;
        inputs.push(((*issue).clone(), body));
    }
    let extracted = tokio::task::spawn_blocking(move || {
        inputs
            .into_iter()
            .map(|(issue, body)| {
                let extraction = mxr_reading::extract(&issue.input(body.as_ref()));
                let rows = item_rows(&issue, &extraction);
                (issue.id, rows)
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|error| HandlerError::from(format!("reading extraction failed: {error}")))?;
    for (message_id, rows) in extracted {
        state
            .store
            .replace_reading_items(&message_id, &rows)
            .await?;
        by_message.insert(message_id, rows);
    }
    Ok(by_message)
}

fn item_rows(issue: &Issue, extraction: &mxr_reading::Extraction) -> Vec<ReadingItemRow> {
    let main = extraction.main_link.as_ref();
    let mut rows = vec![ReadingItemRow {
        message_id: issue.id.clone(),
        idx: 0,
        account_id: issue.account_id.clone(),
        kind: "issue".to_string(),
        shape: extraction.shape.id().to_string(),
        title: extraction.headline.clone(),
        standfirst: extraction.standfirst.clone(),
        url: main.map(|link| link.url.clone()),
        domain: main.map(|link| link.domain.clone()),
        tracked: main.is_some_and(|link| link.tracked),
        words: extraction.words,
        extractor_version: EXTRACTOR_VERSION,
    }];
    for (i, link) in extraction.links.iter().enumerate() {
        rows.push(ReadingItemRow {
            message_id: issue.id.clone(),
            idx: i64::try_from(i + 1).unwrap_or(i64::MAX),
            account_id: issue.account_id.clone(),
            kind: "link".to_string(),
            shape: Shape::Digest.id().to_string(),
            title: link.title.clone(),
            standfirst: link.blurb.clone(),
            url: Some(link.url.clone()),
            domain: Some(link.domain.clone()),
            tracked: link.tracked,
            words: 0,
            extractor_version: EXTRACTOR_VERSION,
        });
    }
    rows
}

fn shape_data(shape: &str) -> ReadingShapeData {
    match Shape::parse(shape) {
        Some(Shape::Digest) => ReadingShapeData::Digest,
        Some(Shape::Teaser) => ReadingShapeData::Teaser,
        Some(Shape::Notice) => ReadingShapeData::Notice,
        Some(Shape::Single) | None => ReadingShapeData::Single,
    }
}

fn band_data(band: Band) -> ReadingBandData {
    match band {
        Band::Since => ReadingBandData::SinceLastVisit,
        Band::Earlier => ReadingBandData::Earlier,
        Band::Fading => ReadingBandData::Fading,
    }
}

/// "Fades Sunday unless you keep it." in the daemon's time zone.
fn fades_line(expires_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let local = expires_at.with_timezone(&Local);
    let today = now.with_timezone(&Local).date_naive();
    let when = match (local.date_naive() - today).num_days() {
        ..=0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        2..=6 => local.format("%A").to_string(),
        _ => local.format("%a %-d %b").to_string(),
    };
    format!("Fades {when} unless you keep it.")
}

/// Everything item data is built from, read once per request.
pub(super) struct Context {
    pub states: HashMap<(MessageId, i64), ReadingStateRow>,
    pub articles: HashSet<(MessageId, i64)>,
    pub wpm: u32,
    pub pace_measured: bool,
    pub now: DateTime<Utc>,
}

impl Context {
    pub(super) async fn load(
        state: &AppState,
        message_ids: &[MessageId],
        now: DateTime<Utc>,
    ) -> Result<Self, HandlerError> {
        let states = state
            .store
            .reading_states_for_messages(message_ids)
            .await?
            .into_iter()
            .map(|row| ((row.message_id.clone(), row.idx), row))
            .collect();
        let articles = state
            .store
            .reading_articles_saved(message_ids)
            .await?
            .into_iter()
            .collect();
        let samples = state.store.reading_finished_samples(PACE_SAMPLES).await?;
        let measured = pace::reader_wpm(&samples);
        Ok(Self {
            states,
            articles,
            wpm: measured.unwrap_or(pace::DEFAULT_WPM),
            pace_measured: measured.is_some(),
            now,
        })
    }

    fn state(&self, message_id: &MessageId, idx: i64) -> Option<&ReadingStateRow> {
        self.states.get(&(message_id.clone(), idx))
    }
}

/// One item as the edition and Later show it.
pub(super) fn item_data(
    issue: &Issue,
    rows: &[ReadingItemRow],
    idx: i64,
    source: Option<&SourceInfo>,
    expires_at: Option<DateTime<Utc>>,
    ctx: &Context,
) -> Option<ReadingItemData> {
    let row = rows.iter().find(|row| row.idx == idx)?;
    let issue_row = rows.iter().find(|row| row.idx == 0).unwrap_or(row);
    let state = ctx.state(&issue.id, idx);
    let on_later = state.and_then(|s| s.later_at).is_some();
    let article_cached = ctx.articles.contains(&(issue.id.clone(), idx));
    let links: Vec<ReadingLinkData> = if idx == 0 {
        rows.iter()
            .filter(|link| link.idx > 0)
            .map(|link| ReadingLinkData {
                item_key: item_key(&issue.id, link.idx),
                title: link.title.clone(),
                blurb: link.standfirst.clone(),
                url: link.url.clone().unwrap_or_default(),
                domain: link.domain.clone().unwrap_or_default(),
                tracked: link.tracked,
                on_later: ctx
                    .state(&issue.id, link.idx)
                    .and_then(|s| s.later_at)
                    .is_some(),
                article_cached: ctx.articles.contains(&(issue.id.clone(), link.idx)),
            })
            .collect()
    } else {
        Vec::new()
    };
    let fades = (!on_later)
        .then(|| expires_at.map(|at| fades_line(at, ctx.now)))
        .flatten();
    let reason = issue.kind.as_ref().map_or_else(
        || "Here because: you put it on Later. Later never fades.".to_string(),
        |kind| {
            if kind.corrected {
                format!("Here because: {} (you).", kind.reason)
            } else if matches!(issue.unsubscribe, UnsubscribeMethod::None) {
                format!("Here because: {} (rule).", kind.reason)
            } else {
                "Here because: you subscribed, and it has an unsubscribe link (rule).".to_string()
            }
        },
    );
    let why = match &fades {
        Some(fades) => format!("{reason} {fades}"),
        None => reason,
    };
    let later_at = state.and_then(|s| s.later_at);
    Some(ReadingItemData {
        item_key: item_key(&issue.id, idx),
        account_id: issue.account_id.clone(),
        message_id: issue.id.clone(),
        thread_id: issue.thread_id.clone(),
        kind: if idx == 0 {
            ReadingItemKindData::Issue
        } else {
            ReadingItemKindData::Link
        },
        shape: shape_data(&issue_row.shape),
        title: row.title.clone(),
        standfirst: row.standfirst.clone(),
        source: issue.source_name.clone(),
        sender_email: issue.sender.clone(),
        words: row.words,
        minutes: pace::minutes(row.words, ctx.wpm),
        url: row.url.clone(),
        domain: row.domain.clone(),
        tracked: row.tracked,
        arrived_at: issue.date,
        expires_at: (!on_later).then_some(expires_at).flatten(),
        why,
        fades,
        lead: false,
        engagement: source.and_then(SourceInfo::engagement_line),
        links,
        on_later,
        later_at,
        still_want_it: later_at.is_some_and(|at| ctx.now - at > STILL_WANT_IT_AFTER)
            && state.and_then(|s| s.kept_at).is_none(),
        progress: state.map_or(0.0, |s| s.progress),
        opened: state.and_then(|s| s.opened_at).is_some(),
        finished: state.and_then(|s| s.finished_at).is_some(),
        article_cached,
        unsubscribe_offer: source
            .filter(|s| s.data.suggest_unsubscribe)
            .map(|s| s.data.evidence.clone()),
    })
}

/// Each account's visit, which bands its own mail.
pub(super) type Visits = HashMap<AccountId, ReadingVisitRow>;

pub(super) async fn load_visits(
    state: &AppState,
    accounts: &[AccountId],
) -> Result<Visits, HandlerError> {
    let mut visits = Visits::new();
    for account in accounts {
        visits.insert(account.clone(), state.store.reading_visit(account).await?);
    }
    Ok(visits)
}

/// The edition as planned at `now`, with the threads it found expired.
pub(super) struct Plan {
    pub edition: ReadingEditionData,
    pub expired: Vec<(AccountId, ThreadId)>,
}

/// Reading's current inbox mail, after its done marks.
async fn current_issues(
    state: &AppState,
    accounts: &[AccountId],
) -> Result<Vec<Issue>, HandlerError> {
    let placed: Vec<Placed> = placed_inbox(state, accounts, None)
        .await?
        .into_iter()
        .filter(|placed| placed.kind.kind == SenderKindData::Reading)
        .collect();
    let placed =
        super::modes::without_done(state, accounts, &[ModeKindData::Reading], placed).await?;
    Ok(placed.iter().map(Issue::from_placed).collect())
}

pub(super) async fn plan(
    state: &AppState,
    accounts: &[AccountId],
    visits: &Visits,
    now: DateTime<Utc>,
) -> Result<Plan, HandlerError> {
    let boundary = |account: &AccountId| visits.get(account).and_then(|v| v.boundary);
    let issues = current_issues(state, accounts).await?;
    let later_rows = state.store.reading_later(accounts).await?;
    let in_edition: HashSet<&MessageId> = issues.iter().map(|issue| &issue.id).collect();
    let later_ids: Vec<MessageId> = later_rows
        .iter()
        .map(|row| row.message_id.clone())
        .filter(|id| !in_edition.contains(id))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let later_issues: Vec<Issue> = state
        .store
        .list_envelopes_by_ids(&later_ids)
        .await?
        .iter()
        .map(Issue::from_envelope)
        .collect();
    let all: Vec<&Issue> = issues.iter().chain(later_issues.iter()).collect();
    let items = ensure_items(state, &all).await?;
    let message_ids: Vec<MessageId> = all.iter().map(|issue| issue.id.clone()).collect();
    let ctx = Context::load(state, &message_ids, now).await?;
    let sources = Sources::load(state, &all, visits, now).await?;

    let mut bands: HashMap<Band, Vec<(ReadingItemData, Rankable)>> = HashMap::new();
    let mut expired_threads: HashMap<(AccountId, ThreadId), bool> = HashMap::new();
    for issue in &issues {
        let Some(rows) = items.get(&issue.id) else {
            continue;
        };
        let source = sources.get(&issue.account_id, &issue.sender);
        let window = source.map_or_else(
            || mxr_reading::fade::source_window(&[issue.date]),
            |s| s.window,
        );
        let expires_at = mxr_reading::fade::expires_at(issue.date, &window);
        let key = (issue.account_id.clone(), issue.thread_id.clone());
        let Some(band) = edition::band(issue.date, expires_at, boundary(&issue.account_id), now)
        else {
            expired_threads.entry(key).or_insert(true);
            continue;
        };
        // A thread with a current issue is not expired.
        expired_threads.insert(key, false);
        let Some(data) = item_data(issue, rows, 0, source, Some(expires_at), &ctx) else {
            continue;
        };
        let rank = Rankable {
            arrived: issue.date,
            affinity: source.map_or(0.0, |s| s.stats.affinity()),
            new_source: source.is_none_or(|s| s.stats.is_new()),
        };
        bands.entry(band).or_default().push((data, rank));
    }

    let mut groups = Vec::new();
    for band in Band::ALL {
        let Some(mut entries) = bands.remove(&band) else {
            continue;
        };
        let leads = edition::rank(&mut entries, |(_, rank)| *rank, band == Band::Since);
        let items: Vec<ReadingItemData> = entries
            .into_iter()
            .enumerate()
            .map(|(i, (mut item, _))| {
                item.lead = i < leads;
                item
            })
            .collect();
        groups.push(ReadingBandGroupData {
            band: band_data(band),
            label: band.label().to_string(),
            note: (band == Band::Fading)
                .then(|| "Goes within a day, unless you keep it with b.".to_string()),
            items,
        });
    }

    let issue_by_id: HashMap<&MessageId, &Issue> =
        all.iter().map(|issue| (&issue.id, *issue)).collect();
    let later: Vec<ReadingItemData> = later_rows
        .iter()
        .filter_map(|row| {
            let issue = issue_by_id.get(&row.message_id)?;
            let rows = items.get(&row.message_id)?;
            let mut data = item_data(
                issue,
                rows,
                row.idx,
                sources.get(&issue.account_id, &issue.sender),
                None,
                &ctx,
            )?;
            data.why = format!(
                "On Later since {}. Later never fades.",
                row.later_at
                    .unwrap_or(now)
                    .with_timezone(&Local)
                    .format("%a %-d %b")
            );
            Some(data)
        })
        .collect();

    // The latest visit across the accounts shown.
    let last_visit_at = visits.values().filter_map(|visit| visit.boundary).max();
    let left_off_here = last_visit_at.is_some()
        && groups
            .iter()
            .any(|group| group.band == ReadingBandData::SinceLastVisit)
        && groups
            .iter()
            .any(|group| group.band != ReadingBandData::SinceLastVisit);
    let since_count = groups
        .iter()
        .find(|group| group.band == ReadingBandData::SinceLastVisit)
        .map_or(0, |group| group.items.len());
    let later_count = u32::try_from(later.len()).unwrap_or(u32::MAX);
    let empty = (since_count == 0).then(|| {
        let never = groups.is_empty() && later.is_empty() && issues.is_empty();
        ReadingEmptyData {
            never_had_any: never,
            line: if never {
                reading_copy::NEVER_HAD_ANY.to_string()
            } else {
                clear_line(last_visit_at, later_count, now)
            },
        }
    });
    let expired: Vec<(AccountId, ThreadId)> = expired_threads
        .into_iter()
        .filter_map(|(key, expired)| expired.then_some(key))
        .collect();
    Ok(Plan {
        edition: ReadingEditionData {
            generated_at: now,
            header: reading_copy::HEADER.to_string(),
            bands: groups,
            last_visit_at,
            left_off_here,
            later,
            later_count,
            sources: sources.ranked(),
            pace_wpm: ctx.wpm,
            pace_measured: ctx.pace_measured,
            expired_now: 0,
            empty,
        },
        expired,
    })
}

/// "Nothing new since Tuesday. Later has 4 things saved."
fn clear_line(boundary: Option<DateTime<Utc>>, later: u32, now: DateTime<Utc>) -> String {
    let since = boundary.map_or_else(
        || "Nothing new.".to_string(),
        |at| {
            let local = at.with_timezone(&Local);
            let days = (now.with_timezone(&Local).date_naive() - local.date_naive()).num_days();
            let when = match days {
                ..=0 => "earlier today".to_string(),
                1 => "yesterday".to_string(),
                2..=6 => local.format("%A").to_string(),
                _ => local.format("%a %-d %b").to_string(),
            };
            format!("Nothing new since {when}.")
        },
    );
    match later {
        0 => since,
        1 => format!("{since} Later has 1 thing saved."),
        n => format!("{since} Later has {n} things saved."),
    }
}

/// Mark expired threads done in Reading, without touching the provider.
/// Returns how many were marked.
pub(super) async fn expire(
    state: &AppState,
    expired: &[(AccountId, ThreadId)],
) -> Result<u32, HandlerError> {
    let mut by_account: HashMap<&AccountId, Vec<ThreadId>> = HashMap::new();
    for (account, thread) in expired {
        by_account.entry(account).or_default().push(thread.clone());
    }
    let mut marks = Vec::new();
    for (account, threads) in by_account {
        let messages = state
            .store
            .desk_messages_in_threads(account, &threads)
            .await?;
        for thread in messages.chunk_by(|a, b| a.thread_id == b.thread_id) {
            if let Some(through) = DeskDismissal::through(thread) {
                marks.push(ModeDoneMark {
                    account_id: account.clone(),
                    thread_id: thread[0].thread_id.clone(),
                    mode: "reading".to_string(),
                    through,
                });
            }
        }
    }
    state.store.mark_mode_done(&marks).await?;
    Ok(u32::try_from(marks.len()).unwrap_or(u32::MAX))
}

pub(super) async fn get_edition(
    state: &AppState,
    account_id: Option<&AccountId>,
    mark_visit: bool,
) -> HandlerResult {
    let started = std::time::Instant::now();
    let now = Utc::now();
    let accounts = scoped_accounts(state, account_id).await?;
    let mut visits = load_visits(state, &accounts).await?;
    // Opening Reading is a visit, unless activity is off or paused: one
    // privacy switch for everything Reading learns about you.
    let tracking = state.activity.is_enabled() && !state.activity.pause_status().0;
    for (account, stored) in &mut visits {
        if *stored == ReadingVisitRow::default() && demo_mailbox() {
            // The demo mailbox comes with a history: Reading was last opened
            // yesterday and has been watching for months, so every band and
            // the unsubscribe evidence show on the first look.
            *stored = ReadingVisitRow {
                first_seen: Some(now - Duration::days(120)),
                boundary: Some(now - Duration::days(1)),
                last_seen: Some(now - Duration::days(1)),
            };
            state.store.set_reading_visit(account, stored).await?;
        }
        if mark_visit && tracking {
            let opened = Visit {
                boundary: stored.boundary,
                last_seen: stored.last_seen,
            }
            .open(now);
            *stored = ReadingVisitRow {
                first_seen: stored.first_seen.or(Some(now)),
                boundary: opened.boundary,
                last_seen: opened.last_seen,
            };
            state.store.set_reading_visit(account, stored).await?;
        }
    }
    let Plan {
        mut edition,
        expired,
    } = plan(state, &accounts, &visits, now).await?;
    edition.expired_now = expire(state, &expired).await?;
    tracing::debug!(
        accounts = accounts.len(),
        items = edition.bands.iter().map(|b| b.items.len()).sum::<usize>(),
        later = edition.later_count,
        expired = edition.expired_now,
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "reading edition built"
    );
    Ok(ResponseData::ReadingEdition { edition })
}

/// After a sync: extract the new Reading mail and expire what faded, so
/// Now and the rail agree with the edition without it being opened.
pub(crate) async fn after_sync(state: &AppState, account_id: &AccountId) {
    let accounts = [account_id.clone()];
    let result = async {
        let visits = load_visits(state, &accounts).await?;
        let planned = plan(state, &accounts, &visits, Utc::now()).await?;
        expire(state, &planned.expired).await
    }
    .await;
    match result {
        Ok(expired) if expired > 0 => {
            tracing::info!(account = %account_id, expired, "reading items faded");
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(account = %account_id, %error, "reading sweep failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_keys_round_trip_and_reject_junk() {
        let id = MessageId::new();
        let key = item_key(&id, 3);
        assert_eq!(parse_item_key(&key).expect("parses"), (id, 3));
        for bad in [
            "",
            "nope",
            "abc:1",
            &format!("{}:-1", MessageId::new()),
            &format!("{}:x", MessageId::new()),
        ] {
            assert!(parse_item_key(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_clear_line_names_the_last_visit_and_the_later_count() {
        let now = Utc::now();
        assert_eq!(clear_line(None, 0, now), "Nothing new.");
        assert_eq!(
            clear_line(Some(now - Duration::days(1)), 4, now),
            "Nothing new since yesterday. Later has 4 things saved."
        );
        assert_eq!(
            clear_line(Some(now), 1, now),
            "Nothing new since earlier today. Later has 1 thing saved."
        );
    }
}
