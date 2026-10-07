//! Messages (blueprint 22, phase 3): people you talk with, one row each,
//! with your conversations inside as topics.
//!
//! - Your turn is the shared lane rule (`desk_lanes`: You owe and New from
//!   people), so Messages, Now's People and the rail never disagree.
//! - Each thread's shape (`conversation_shape`) decides its row: a
//!   one-to-one thread is a topic in that person's row, a group thread is
//!   its own row keyed by the thread, a copied thread is not here at all.
//! - People are merged across addresses only by hand (`person_links`, D117).
//! - Each message's text is its new text (`mxr_reader::new_text`), so every
//!   client shows the same words and the same "trimmed" marker.

use super::conversation_shape::Shape;
use super::desk::{self_matcher, Senders};
use super::desk_lanes::{
    apply_pace, clean_subject, current_messages, is_outbound, last_stored, thread_lanes,
    AccountInputs, DESK_WINDOW_DAYS,
};
use super::desk_timers::DeskTimers;
use super::messages_text::{cached_ask, new_texts, wrapped_lines, TextRequest};
use super::messages_view::{
    band, build_row, closeness, day_label, preview_text, rank_by_recency, rank_your_turn,
    turn_ratio, People, RowContext, RowKey, Topic, PINNED_CAP,
};
use super::modes::messages_dismissals;
use super::places::scoped_accounts;
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Duration, Local, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_protocol::{
    messages_copy, ClosenessData, ComposerData, ConversationAttachmentData, ConversationData,
    ConversationMessageData, DeskLaneKind, DeskRowData, MergeSuggestionData, MessageLayoutData,
    MessagesBandData, MessagesData, MessagesLapsedData, MessagesPreviewData,
    MessagesPreviewKindData, MessagesRowData, MessagesRowKindData, MessagesTopicData,
    MessagesTurnData, PersonMergeData, PersonPageData, PersonRefData, ResponseData, TopicStateData,
    TrimmedData,
};
use mxr_store::{DeskMessage, PersonFacts, PersonMerge};
use std::collections::{HashMap, HashSet};

/// How far back a person page looks for conversations.
const PERSON_HISTORY_DAYS: i64 = 730;
/// Most conversations a person page lists.
const PERSON_TOPICS_MAX: u32 = 40;
/// Most messages a conversation shows; earlier ones are counted.
const CONVERSATION_MAX: usize = 20;
/// A message of this many wrapped lines or fewer reads like chat.
const COMPACT_MAX_LINES: usize = 3;
/// Up to this many lapsed people follow the clear line.
const LAPSED_MAX: usize = 3;

/// One account's conversations, placed.
pub(super) struct Gathered {
    pub account_id: AccountId,
    pub topics: Vec<Topic>,
    pub threads: HashMap<ThreadId, Vec<DeskMessage>>,
    pub people: People,
    pub facts: HashMap<String, PersonFacts>,
    pub names: HashMap<String, String>,
    pub pinned: HashSet<String>,
    /// Messages you sent.
    pub outbound: HashSet<MessageId>,
}

impl Gathered {
    fn ctx(&self, now: DateTime<Utc>) -> RowContext<'_> {
        RowContext {
            account_id: &self.account_id,
            people: &self.people,
            facts: &self.facts,
            names: &self.names,
            pinned: &self.pinned,
            now,
        }
    }
}

/// Which threads to gather: the recent window, or a fixed set.
enum Scope<'a> {
    Window,
    Threads(&'a [ThreadId]),
}

async fn gather(
    state: &AppState,
    account_id: &AccountId,
    scope: Scope<'_>,
    now: DateTime<Utc>,
) -> Result<Gathered, HandlerError> {
    let store = &state.store;
    let window_start = now - Duration::days(DESK_WINDOW_DAYS);
    let timers = DeskTimers::new(
        store.desk_reply_later(account_id).await?,
        store.desk_reminders(account_id, window_start).await?,
    );
    let mut messages = match scope {
        Scope::Window => {
            let days = i64::from(state.config_snapshot().messages.recent_days.max(1));
            let mut messages = store
                .desk_thread_messages(account_id, now - Duration::days(days))
                .await?;
            // A conversation a time you set brought back may be older.
            let loaded: HashSet<&ThreadId> = messages.iter().map(|m| &m.thread_id).collect();
            let mut back: Vec<ThreadId> = timers
                .maybe_back(now)
                .filter(|thread| !loaded.contains(thread))
                .cloned()
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            back.sort_by_key(ThreadId::as_str);
            messages.extend(store.desk_messages_in_threads(account_id, &back).await?);
            messages
        }
        Scope::Threads(threads) => store.desk_messages_in_threads(account_id, threads).await?,
    };
    messages.sort_by(|a, b| {
        a.thread_id
            .as_str()
            .cmp(&b.thread_id.as_str())
            .then(a.date.cmp(&b.date))
            .then(a.seq.cmp(&b.seq))
    });

    let senders = Senders::load(state, account_id, &messages).await?;
    let is_self = self_matcher(state, account_id).await?;
    let dismissed = messages_dismissals(state, account_id).await?;
    let people = People::new(&store.person_links(account_id).await?);
    let pinned: HashSet<String> = store
        .list_cadence_watch(account_id)
        .await?
        .into_iter()
        .map(|entry| people.primary(&entry.email))
        .collect();
    let shape = super::conversation_shape::shape_config(state);

    // The lane rule and the shapes, with no await while `inputs` (which
    // borrows `dyn Fn`) is alive.
    let (mut topics, threads, names, outbound) = {
        let inputs = AccountInputs {
            account_id,
            messages: &messages,
            contacts: &senders.contacts,
            screener: &senders.screener,
            dismissed: &dismissed,
            timers: &timers,
            is_self: &is_self,
            shape,
            now,
        };
        let mut turns: HashMap<ThreadId, DeskRowData> = thread_lanes(&inputs)
            .rows
            .into_iter()
            .filter(|draft| matches!(draft.row.lane, DeskLaneKind::Owed | DeskLaneKind::PeopleNew))
            .map(|draft| (draft.row.thread_id.clone(), draft.row))
            .collect();
        let mut topics = Vec::new();
        let mut threads = HashMap::new();
        let mut names: HashMap<String, (DateTime<Utc>, String)> = HashMap::new();
        let mut outbound = HashSet::new();
        for thread in messages.chunk_by(|a, b| a.thread_id == b.thread_id) {
            let thread_id = thread[0].thread_id.clone();
            for message in thread {
                if is_outbound(message, &is_self) {
                    outbound.insert(message.id.clone());
                    for recipient in message.to.iter().chain(&message.cc) {
                        note_name(
                            &mut names,
                            &people,
                            &recipient.email,
                            recipient.name.as_deref(),
                            message.date,
                        );
                    }
                } else {
                    note_name(
                        &mut names,
                        &people,
                        &message.from.email,
                        message.from.name.as_deref(),
                        message.date,
                    );
                }
            }
            let shape = inputs.shape(thread);
            if !shape.in_messages() || thread.iter().all(|m| m.trashed) {
                continue;
            }
            if let Some(topic) = place_topic(
                account_id,
                thread,
                &shape,
                &people,
                turns.remove(&thread_id),
                dismissed
                    .get(&thread_id)
                    .is_some_and(|mark| mark.covers(thread)),
                &is_self,
                now,
            ) {
                topics.push(topic);
                threads.insert(thread_id, thread.to_vec());
            }
        }
        (topics, threads, names, outbound)
    };

    // Your usual pace with each person whose turn it is, as the desk
    // measures it.
    let mut owed: Vec<String> = topics
        .iter()
        .filter_map(|t| t.turn.as_ref())
        .filter(|row| row.lane == DeskLaneKind::Owed)
        .map(|row| row.counterparty_email.to_ascii_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    owed.sort_unstable();
    let mut latencies: HashMap<String, Vec<i64>> = HashMap::new();
    for latency in store.desk_reply_latencies(account_id, &owed).await? {
        if latency.direction == "i_replied" {
            latencies
                .entry(latency.email)
                .or_default()
                .push(latency.latency_seconds);
        }
    }
    for row in topics.iter_mut().filter_map(|t| t.turn.as_mut()) {
        if row.lane == DeskLaneKind::Owed {
            let key = row.counterparty_email.to_ascii_lowercase();
            apply_pace(row, latencies.get(&key).map(Vec::as_slice));
        }
    }

    let mut wanted: Vec<String> = topics
        .iter()
        .flat_map(|t| t.people.iter())
        .chain(pinned.iter())
        .flat_map(|primary| people.addresses(primary))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    wanted.sort_unstable();
    let facts = store
        .people_facts(account_id, &wanted)
        .await?
        .into_iter()
        .map(|facts| (facts.email.clone(), facts))
        .collect();

    Ok(Gathered {
        account_id: account_id.clone(),
        topics,
        threads,
        people,
        facts,
        names: names.into_iter().map(|(k, (_, name))| (k, name)).collect(),
        pinned,
        outbound,
    })
}

/// Keep the newest real name seen for each person.
fn note_name(
    names: &mut HashMap<String, (DateTime<Utc>, String)>,
    people: &People,
    email: &str,
    name: Option<&str>,
    at: DateTime<Utc>,
) {
    let Some(name) = name
        .map(str::trim)
        .filter(|n| !n.is_empty() && !n.contains('@'))
    else {
        return;
    };
    let key = people.primary(email);
    match names.get(&key) {
        Some((seen, _)) if *seen >= at => {}
        _ => {
            names.insert(key, (at, name.to_string()));
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "one constructor keeps a topic's facts together"
)]
fn place_topic(
    account_id: &AccountId,
    thread: &[DeskMessage],
    shape: &Shape,
    people: &People,
    turn: Option<DeskRowData>,
    done: bool,
    is_self: &dyn Fn(&str) -> bool,
    now: DateTime<Utc>,
) -> Option<Topic> {
    let current = current_messages(thread, now);
    let latest = last_stored(current, |m| !m.trashed)?;
    let mut others: Vec<String> = Vec::new();
    for person in shape.people() {
        let primary = people.primary(person);
        if !others.contains(&primary) {
            others.push(primary);
        }
    }
    let key = match (shape, others.as_slice()) {
        (_, [only]) => RowKey::Person(only.clone()),
        (Shape::Group(_), _) => RowKey::Group(thread[0].thread_id.clone()),
        _ => return None,
    };
    let you_last = is_outbound(latest, is_self);
    let state = if turn.is_some() {
        TopicStateData::YourTurn
    } else if done {
        TopicStateData::Done
    } else if you_last {
        TopicStateData::Waiting
    } else {
        TopicStateData::Quiet
    };
    let reply_to =
        last_stored(current, |m| !m.trashed && !is_outbound(m, is_self)).unwrap_or(latest);
    let subject = clean_subject(&latest.subject);
    let shape_data = match key {
        RowKey::Person(_) => mxr_protocol::ThreadShapeData::OneToOne,
        RowKey::Group(_) => mxr_protocol::ThreadShapeData::Group,
    };
    Some(Topic {
        key,
        unread: thread.iter().any(|m| {
            !is_outbound(m, is_self)
                && m.in_inbox
                && !m.flags.contains(mxr_core::MessageFlags::READ)
        }),
        data: MessagesTopicData {
            account_id: account_id.clone(),
            thread_id: thread[0].thread_id.clone(),
            subject: if subject.trim().is_empty() {
                "(no subject)".to_string()
            } else {
                subject
            },
            shape: shape_data,
            state,
            last_at: latest.date,
            message_count: u32::try_from(thread.len()).unwrap_or(u32::MAX),
            with: Vec::new(),
            message_ids: thread.iter().map(|m| m.id.clone()).collect(),
            reply_to_message_id: reply_to.id.clone(),
        },
        people: others,
        turn,
    })
}

/// Rows of one account, keyed and built, with each Your turn row's ratio.
fn account_rows(gathered: &Gathered, now: DateTime<Utc>) -> Vec<(MessagesRowData, Option<f64>)> {
    let mut grouped: HashMap<RowKey, Vec<Topic>> = HashMap::new();
    for topic in &gathered.topics {
        grouped
            .entry(topic.key.clone())
            .or_default()
            .push(topic.clone());
    }
    let ctx = gathered.ctx(now);
    let mut rows: Vec<(MessagesRowData, Option<f64>)> = grouped
        .into_iter()
        .map(|(key, topics)| {
            let ratio = turn_ratio(&topics);
            (build_row(key, topics, &ctx), ratio)
        })
        .collect();
    // A pinned person with nothing in the window still has a face.
    let present: HashSet<String> = rows
        .iter()
        .filter_map(|(row, _)| row.person.as_ref().map(|p| p.id.clone()))
        .collect();
    let mut pinned: Vec<&String> = gathered.pinned.iter().collect();
    pinned.sort();
    for primary in pinned {
        if present.contains(primary) {
            continue;
        }
        let facts = ctx.merged_facts(primary);
        let last_at = facts
            .as_ref()
            .and_then(|f| f.last_inbound_at.max(f.last_outbound_at))
            .unwrap_or(now);
        let person = ctx.person(primary);
        rows.push((
            MessagesRowData {
                id: RowKey::Person(primary.clone()).id(),
                kind: MessagesRowKindData::Person,
                account_id: gathered.account_id.clone(),
                band: MessagesBandData::Pinned,
                title: person.label().to_string(),
                person: Some(person),
                members: Vec::new(),
                closeness: closeness(facts.as_ref(), now),
                your_turn: false,
                last_at,
                turn_since: None,
                preview: None,
                topics: Vec::new(),
                usual_reply_seconds: None,
                pace_label: None,
                overdue: false,
                pinned: true,
                unread: false,
                why: "Here because: you pinned them (you).".to_string(),
            },
            None,
        ));
    }
    rows
}

pub(super) async fn list_messages(
    state: &AppState,
    account_id: Option<&AccountId>,
    turn: Option<MessagesTurnData>,
    limit: u32,
) -> HandlerResult {
    Ok(ResponseData::Messages {
        messages: messages_at(state, account_id, turn, limit, Utc::now()).await?,
    })
}

/// Messages as they stand at `now` (tests move the clock).
pub(super) async fn messages_at(
    state: &AppState,
    account_id: Option<&AccountId>,
    turn: Option<MessagesTurnData>,
    limit: u32,
    now: DateTime<Utc>,
) -> Result<MessagesData, HandlerError> {
    let started = std::time::Instant::now();
    let accounts = scoped_accounts(state, account_id).await?;
    let mut gathered = Vec::new();
    for account in &accounts {
        gathered.push(gather(state, account, Scope::Window, now).await?);
    }
    let mut your_turn = Vec::new();
    let mut pinned = Vec::new();
    let mut recent = Vec::new();
    let mut quiet = Vec::new();
    let mut thread_count = 0u32;
    for g in &gathered {
        thread_count += u32::try_from(g.topics.len()).unwrap_or(u32::MAX);
        for (row, ratio) in account_rows(g, now) {
            if row.pinned && row.band != MessagesBandData::Pinned {
                let mut face = row.clone();
                face.band = MessagesBandData::Pinned;
                pinned.push(face);
            }
            match row.band {
                MessagesBandData::YourTurn => your_turn.push((row, ratio)),
                MessagesBandData::Pinned => pinned.push(row),
                // A pinned person lives in Pinned, not again below it.
                MessagesBandData::Recent if !row.pinned => recent.push(row),
                MessagesBandData::Quiet if !row.pinned => quiet.push(row),
                _ => {}
            }
        }
    }
    rank_your_turn(&mut your_turn);
    let mut your_turn: Vec<MessagesRowData> = your_turn.into_iter().map(|(row, _)| row).collect();
    rank_by_recency(&mut pinned);
    pinned.truncate(PINNED_CAP);
    rank_by_recency(&mut recent);
    rank_by_recency(&mut quiet);
    let row_count = u32::try_from(your_turn.len() + recent.len() + quiet.len()).unwrap_or(u32::MAX)
        + u32::try_from(pinned.iter().filter(|r| !r.your_turn).count()).unwrap_or(0);
    match turn {
        Some(MessagesTurnData::Mine) => {
            recent.clear();
            quiet.clear();
            pinned.retain(|row| row.your_turn);
        }
        Some(MessagesTurnData::Theirs) => {
            your_turn.clear();
            let waiting = |row: &MessagesRowData| {
                row.topics
                    .iter()
                    .any(|t| t.state == TopicStateData::Waiting)
            };
            pinned.retain(waiting);
            recent.retain(waiting);
            quiet.retain(waiting);
        }
        None => {}
    }
    let recent_total = u32::try_from(recent.len()).unwrap_or(u32::MAX);
    let quiet_total = u32::try_from(quiet.len()).unwrap_or(u32::MAX);
    recent.truncate(limit as usize);
    quiet.truncate(limit as usize);

    for band in [&mut your_turn, &mut pinned, &mut recent, &mut quiet] {
        fill_previews(state, &gathered, band).await?;
    }

    let lapsed = if your_turn.is_empty() {
        lapsed(state, &accounts, now).await?
    } else {
        Vec::new()
    };
    let empty_state = your_turn.is_empty().then(|| {
        if row_count == 0 {
            messages_copy::NEVER_HAD_ANY.to_string()
        } else {
            messages_copy::CLEAR.to_string()
        }
    });
    let mut merge_suggestion_count = 0u32;
    for account in &accounts {
        merge_suggestion_count +=
            u32::try_from(suggestions_for(state, account).await?.len()).unwrap_or(u32::MAX);
    }
    tracing::debug!(
        accounts = accounts.len(),
        rows = row_count,
        threads = thread_count,
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "messages composed"
    );
    Ok(MessagesData {
        generated_at: now,
        header: messages_copy::HEADER.to_string(),
        your_turn,
        pinned,
        recent,
        quiet,
        recent_total,
        quiet_total,
        row_count,
        thread_count,
        empty_state,
        lapsed,
        merge_suggestion_count,
    })
}

/// The line under each shown row: for Your turn, the ask from the
/// conversation's gist when one is cached, else what they wrote last; for
/// the rest, the latest message, as "You: ..." when you wrote it.
async fn fill_previews(
    state: &AppState,
    gathered: &[Gathered],
    rows: &mut [MessagesRowData],
) -> Result<(), HandlerError> {
    for row in rows.iter_mut() {
        let Some(topic) = row.topics.first() else {
            continue;
        };
        let Some(g) = gathered.iter().find(|g| g.account_id == row.account_id) else {
            continue;
        };
        let Some(thread) = g.threads.get(&topic.thread_id) else {
            continue;
        };
        if topic.state == TopicStateData::YourTurn {
            if let Some(ask) = cached_ask(state, &topic.thread_id).await? {
                row.preview = Some(MessagesPreviewData {
                    kind: MessagesPreviewKindData::Ask,
                    text: preview_text(&ask.quote),
                    message_id: Some(ask.message_id),
                    model: Some(ask.model),
                });
                continue;
            }
        }
        let target = if topic.state == TopicStateData::YourTurn {
            topic.reply_to_message_id.clone()
        } else {
            match last_stored(thread, |m| !m.trashed) {
                Some(latest) => latest.id.clone(),
                None => continue,
            }
        };
        let texts = new_texts(
            state,
            thread,
            &TextRequest {
                wanted: std::slice::from_ref(&target),
                outbound: &g.outbound,
            },
        )
        .await?;
        let Some(text) = texts.get(&target) else {
            continue;
        };
        let (kind, text) = if g.outbound.contains(&target) {
            (
                MessagesPreviewKindData::You,
                format!("You: {}", preview_text(&text.text)),
            )
        } else {
            (MessagesPreviewKindData::Latest, preview_text(&text.text))
        };
        row.preview = Some(MessagesPreviewData {
            kind,
            text,
            message_id: Some(target),
            model: None,
        });
    }
    Ok(())
}

/// People whose usual pace lapsed, as facts: "Ari usually writes every
/// week. Last: 19 days ago."
async fn lapsed(
    state: &AppState,
    accounts: &[AccountId],
    now: DateTime<Utc>,
) -> Result<Vec<MessagesLapsedData>, HandlerError> {
    let mut out = Vec::new();
    for account in accounts {
        let people = People::new(&state.store.person_links(account).await?);
        for drift in state.store.list_cadence_drift(account).await? {
            let Some(last) = drift.last_contact_at else {
                continue;
            };
            let primary = people.primary(&drift.email);
            let first = mxr_protocol::first_name(drift.display_name.as_deref(), &primary);
            let days = (now - last).num_days().max(0);
            out.push((
                drift.drift_days,
                MessagesLapsedData {
                    account_id: account.clone(),
                    person: PersonRefData {
                        id: primary.clone(),
                        name: drift.display_name.clone(),
                        addresses: people.addresses(&primary),
                    },
                    line: format!(
                        "{first} usually writes {}. Last: {days} days ago.",
                        cadence_words(drift.expected_days)
                    ),
                },
            ));
        }
    }
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    Ok(out.into_iter().take(LAPSED_MAX).map(|(_, l)| l).collect())
}

fn cadence_words(days: f64) -> String {
    let days = days.round().max(1.0) as i64;
    match days {
        1 => "every day".to_string(),
        6..=8 => "every week".to_string(),
        13..=15 => "every two weeks".to_string(),
        28..=31 => "every month".to_string(),
        _ => format!("every {days} days"),
    }
}

/// A person argument: `person:<email>`, `group:<thread>`, or an address.
enum PersonArg {
    Person(String),
    Group(ThreadId),
}

fn parse_person(arg: &str) -> Result<PersonArg, HandlerError> {
    let arg = arg.trim();
    if let Some(email) = arg.strip_prefix("person:") {
        return Ok(PersonArg::Person(email.to_ascii_lowercase()));
    }
    if let Some(thread) = arg.strip_prefix("group:") {
        let id = thread
            .parse::<ThreadId>()
            .map_err(|_| HandlerError::InvalidRequest(format!("not a thread id: {thread}")))?;
        return Ok(PersonArg::Group(id));
    }
    if arg.contains('@') {
        return Ok(PersonArg::Person(arg.to_ascii_lowercase()));
    }
    Err(HandlerError::InvalidRequest(format!(
        "\"{arg}\" is not a person: pass an address, person:<email> or group:<thread id>"
    )))
}

pub(super) async fn get_person(
    state: &AppState,
    account_id: Option<&AccountId>,
    person: &str,
    topic: Option<&ThreadId>,
) -> HandlerResult {
    Ok(ResponseData::PersonPage {
        page: person_at(state, account_id, person, topic, Utc::now()).await?,
    })
}

pub(super) async fn person_at(
    state: &AppState,
    account_id: Option<&AccountId>,
    person: &str,
    topic: Option<&ThreadId>,
    now: DateTime<Utc>,
) -> Result<PersonPageData, HandlerError> {
    let arg = parse_person(person)?;
    for account in scoped_accounts(state, account_id).await? {
        let page = match &arg {
            PersonArg::Person(email) => person_page(state, &account, email, topic, now).await?,
            PersonArg::Group(thread) => group_page(state, &account, thread, now).await?,
        };
        if let Some(page) = page {
            return Ok(page);
        }
    }
    Err(HandlerError::Message(format!(
        "No conversations with {person} in Messages."
    )))
}

async fn person_page(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
    topic: Option<&ThreadId>,
    now: DateTime<Utc>,
) -> Result<Option<PersonPageData>, HandlerError> {
    let people = People::new(&state.store.person_links(account_id).await?);
    let primary = people.primary(email);
    let addresses = people.addresses(&primary);
    let mut threads = state
        .store
        .person_thread_ids(
            account_id,
            &addresses,
            now - Duration::days(PERSON_HISTORY_DAYS),
            PERSON_TOPICS_MAX,
        )
        .await?;
    if let Some(topic) = topic {
        if !threads.contains(topic) {
            threads.push(topic.clone());
        }
    }
    if threads.is_empty() {
        return Ok(None);
    }
    let g = gather(state, account_id, Scope::Threads(&threads), now).await?;
    let key = RowKey::Person(primary.clone());
    let mut own: Vec<Topic> = g.topics.iter().filter(|t| t.key == key).cloned().collect();
    let mut groups: Vec<Topic> = g
        .topics
        .iter()
        .filter(|t| matches!(t.key, RowKey::Group(_)) && t.people.contains(&primary))
        .cloned()
        .collect();
    if own.is_empty() && groups.is_empty() {
        return Ok(None);
    }
    for group in &mut groups {
        group.data.with = group
            .people
            .iter()
            .filter(|p| **p != primary)
            .map(|p| g.ctx(now).person(p).first_name())
            .collect();
    }
    super::messages_view::sort_topics(&mut own);
    super::messages_view::sort_topics(&mut groups);
    let ctx = g.ctx(now);
    let mut row = build_row(key, own.clone(), &ctx);
    if own.is_empty() {
        // Only group conversations: the row stands on them for its band.
        row.band = band(&groups, now);
        row.your_turn = row.band == MessagesBandData::YourTurn;
    }
    let topics: Vec<MessagesTopicData> = own
        .iter()
        .chain(groups.iter())
        .map(|t| t.data.clone())
        .collect();
    let mut rows = vec![row];
    fill_previews(state, std::slice::from_ref(&g), &mut rows).await?;
    let row = rows.remove(0);
    let selected = pick_topic(&topics, topic);
    let conversation = match selected {
        Some(selected) => Some(conversation(state, &g, selected, &row).await?),
        None => None,
    };
    let facts = ctx.merged_facts(&primary);
    let merge_suggestions = suggestions_for(state, account_id)
        .await?
        .into_iter()
        .filter(|s| s.addresses.iter().any(|a| addresses.contains(a)))
        .collect();
    Ok(Some(PersonPageData {
        relationship_line: relationship_line(facts.as_ref(), row.person.as_ref(), now),
        header_line: header_line(row.closeness, row.pace_label.as_deref()),
        row,
        topics,
        conversation,
        merge_suggestions,
    }))
}

async fn group_page(
    state: &AppState,
    account_id: &AccountId,
    thread_id: &ThreadId,
    now: DateTime<Utc>,
) -> Result<Option<PersonPageData>, HandlerError> {
    let g = gather(
        state,
        account_id,
        Scope::Threads(std::slice::from_ref(thread_id)),
        now,
    )
    .await?;
    let Some(topic) = g
        .topics
        .iter()
        .find(|t| &t.data.thread_id == thread_id)
        .cloned()
    else {
        return Ok(None);
    };
    let ctx = g.ctx(now);
    let mut rows = vec![build_row(topic.key.clone(), vec![topic.clone()], &ctx)];
    fill_previews(state, std::slice::from_ref(&g), &mut rows).await?;
    let row = rows.remove(0);
    let conversation = conversation(state, &g, &topic.data, &row).await?;
    let names: Vec<String> = row.members.iter().map(|m| m.label().to_string()).collect();
    Ok(Some(PersonPageData {
        relationship_line: format!("A conversation with {}.", join_names(&names)),
        header_line: header_line(row.closeness, None),
        topics: vec![topic.data],
        conversation: Some(conversation),
        merge_suggestions: Vec::new(),
        row,
    }))
}

fn pick_topic<'a>(
    topics: &'a [MessagesTopicData],
    wanted: Option<&ThreadId>,
) -> Option<&'a MessagesTopicData> {
    if let Some(wanted) = wanted {
        if let Some(found) = topics.iter().find(|t| &t.thread_id == wanted) {
            return Some(found);
        }
    }
    topics
        .iter()
        .find(|t| t.state == TopicStateData::YourTurn)
        .or_else(|| topics.iter().max_by_key(|t| t.last_at))
}

async fn conversation(
    state: &AppState,
    g: &Gathered,
    topic: &MessagesTopicData,
    row: &MessagesRowData,
) -> Result<ConversationData, HandlerError> {
    let thread = g.threads.get(&topic.thread_id).cloned().unwrap_or_default();
    let shown: Vec<&DeskMessage> = thread.iter().filter(|m| !m.trashed).collect();
    let earlier_count = shown.len().saturating_sub(CONVERSATION_MAX);
    let shown = &shown[earlier_count..];
    let wanted: Vec<MessageId> = shown.iter().map(|m| m.id.clone()).collect();
    let texts = new_texts(
        state,
        &thread,
        &TextRequest {
            wanted: &wanted,
            outbound: &g.outbound,
        },
    )
    .await?;
    let ask = cached_ask(state, &topic.thread_id).await?;
    let messages = shown
        .iter()
        .map(|message| {
            let text = texts.get(&message.id);
            let body = text.map(|t| t.text.clone()).unwrap_or_default();
            let trimmed = text.map_or_else(TrimmedData::default, |t| t.trimmed);
            let ask_quote = ask
                .as_ref()
                .filter(|ask| ask.message_id == message.id && body.contains(&ask.quote))
                .map(|ask| ask.quote.clone());
            ConversationMessageData {
                message_id: message.id.clone(),
                from: message.from.clone(),
                from_me: g.outbound.contains(&message.id),
                date: message.date,
                layout: if wrapped_lines(&body) <= COMPACT_MAX_LINES {
                    MessageLayoutData::Compact
                } else {
                    MessageLayoutData::Letter
                },
                paragraphs: u32::try_from(paragraphs(&body)).unwrap_or(u32::MAX),
                trimmed_label: trimmed.label(),
                trimmed,
                only_quoted: text.is_some_and(|t| t.only_quoted),
                attachments: text
                    .map(|t| {
                        t.attachments
                            .iter()
                            .map(|a| ConversationAttachmentData {
                                attachment_id: a.id.clone(),
                                filename: a.filename.clone(),
                                size_bytes: a.size_bytes,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                ask_quote,
                text: body,
            }
        })
        .collect();
    let to = match row.kind {
        MessagesRowKindData::Group => row.title.clone(),
        MessagesRowKindData::Person => row
            .person
            .as_ref()
            .map_or_else(|| row.title.clone(), PersonRefData::first_name),
    };
    Ok(ConversationData {
        account_id: topic.account_id.clone(),
        thread_id: topic.thread_id.clone(),
        subject: topic.subject.clone(),
        shape: topic.shape,
        state: topic.state,
        messages,
        earlier_count: u32::try_from(earlier_count).unwrap_or(u32::MAX),
        composer: ComposerData {
            label: format!("Reply to {to} · {}", topic.subject),
            reply_to_message_id: topic.reply_to_message_id.clone(),
            reply_all: topic.shape == mxr_protocol::ThreadShapeData::Group,
        },
    })
}

fn paragraphs(text: &str) -> usize {
    text.split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .count()
}

fn join_names(names: &[String]) -> String {
    match names {
        [] => "nobody".to_string(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// "You've written 48 times since 2023. Last: Tuesday."
fn relationship_line(
    facts: Option<&PersonFacts>,
    person: Option<&PersonRefData>,
    now: DateTime<Utc>,
) -> String {
    let first = person.map_or_else(|| "them".to_string(), PersonRefData::first_name);
    let Some(facts) = facts else {
        return format!("You and {first} are new to each other.");
    };
    let since = facts.first_seen_at.with_timezone(&Local).format("%Y");
    if facts.total_outbound == 0 {
        return format!(
            "You haven't written to {first} yet. {first} has written {} since {since}.",
            times(facts.total_inbound)
        );
    }
    let last = facts
        .last_inbound_at
        .max(facts.last_outbound_at)
        .map(|at| format!(" Last: {}.", day_label(at, now, &Local)))
        .unwrap_or_default();
    format!(
        "You've written {} since {since}.{last}",
        times(facts.total_outbound)
    )
}

fn times(n: u32) -> String {
    match n {
        1 => "once".to_string(),
        2 => "twice".to_string(),
        n => format!("{n} times"),
    }
}

/// "close · usually 47m".
fn header_line(closeness: ClosenessData, pace: Option<&str>) -> String {
    match pace {
        Some(pace) => format!("{} · {pace}", closeness.label()),
        None => closeness.label().to_string(),
    }
}

// ----- Merging people -----

fn clean_address(address: &str) -> Result<String, HandlerError> {
    let address = address
        .trim()
        .trim_start_matches("person:")
        .to_ascii_lowercase();
    if address.contains('@') && !address.contains(char::is_whitespace) {
        Ok(address)
    } else {
        Err(HandlerError::InvalidRequest(format!(
            "\"{address}\" is not an email address"
        )))
    }
}

pub(super) async fn merge(
    state: &AppState,
    account_id: &AccountId,
    into: &str,
    addresses: &[String],
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let people = People::new(&state.store.person_links(account_id).await?);
    let primary = people.primary(&clean_address(into)?);
    let mut existing = people.addresses(&primary);
    let mut changed: Vec<String> = Vec::new();
    for address in addresses {
        let theirs = people.primary(&clean_address(address)?);
        for moving in people.addresses(&theirs) {
            if !existing.contains(&moving) && !changed.contains(&moving) {
                changed.push(moving);
            }
        }
    }
    if changed.is_empty() {
        return Err(HandlerError::InvalidRequest(format!(
            "Nothing to merge: those addresses are already {primary}."
        )));
    }
    existing.extend(changed.iter().cloned());
    if !dry_run {
        state
            .store
            .merge_people(
                account_id,
                &PersonMerge {
                    person_email: primary.clone(),
                    addresses: changed.clone(),
                },
                now,
            )
            .await?;
    }
    merge_outcome(state, account_id, primary, existing, changed, dry_run, now).await
}

pub(super) async fn split(
    state: &AppState,
    account_id: &AccountId,
    address: &str,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let address = clean_address(address)?;
    let people = People::new(&state.store.person_links(account_id).await?);
    let primary = people.primary(&address);
    let mut remaining = people.addresses(&primary);
    if remaining.len() < 2 {
        return Err(HandlerError::InvalidRequest(format!(
            "{address} is not merged with anyone."
        )));
    }
    remaining.retain(|a| *a != address);
    if !dry_run {
        state.store.split_person(account_id, &address).await?;
    }
    let person = if primary == address {
        remaining[0].clone()
    } else {
        primary
    };
    merge_outcome(
        state,
        account_id,
        person,
        remaining,
        vec![address],
        dry_run,
        now,
    )
    .await
}

async fn merge_outcome(
    state: &AppState,
    account_id: &AccountId,
    primary: String,
    addresses: Vec<String>,
    changed: Vec<String>,
    dry_run: bool,
    now: DateTime<Utc>,
) -> HandlerResult {
    let facts = state.store.people_facts(account_id, &addresses).await?;
    let name = facts
        .iter()
        .find(|f| f.email == primary)
        .or_else(|| facts.first())
        .and_then(|f| f.display_name.clone())
        .filter(|n| !n.trim().is_empty());
    let threads = state
        .store
        .person_thread_ids(
            account_id,
            &addresses,
            now - Duration::days(PERSON_HISTORY_DAYS),
            u32::MAX,
        )
        .await?;
    let label = name.clone().unwrap_or_else(|| primary.clone());
    let n = addresses.len();
    let summary = match (dry_run, n) {
        (true, 1) => format!("Would leave {label} with one address."),
        (true, _) => format!(
            "Would make {label} one person with {n} addresses: {}.",
            addresses.join(", ")
        ),
        (false, 1) => format!("{label} has one address now."),
        (false, _) => format!("{label} is now one person with {n} addresses."),
    };
    Ok(ResponseData::PersonMerge {
        merge: PersonMergeData {
            account_id: account_id.clone(),
            person: PersonRefData {
                id: primary,
                name,
                addresses,
            },
            changed,
            thread_count: u32::try_from(threads.len()).unwrap_or(u32::MAX),
            dry_run,
            summary,
        },
    })
}

pub(super) async fn merge_suggestions(
    state: &AppState,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let mut suggestions = Vec::new();
    for account in scoped_accounts(state, account_id).await? {
        suggestions.extend(suggestions_for(state, &account).await?);
    }
    Ok(ResponseData::MergeSuggestions { suggestions })
}

/// Same name, written to both, not merged yet (D117). Never applied here.
async fn suggestions_for(
    state: &AppState,
    account_id: &AccountId,
) -> Result<Vec<MergeSuggestionData>, HandlerError> {
    let candidates = state.store.merge_suggestion_candidates(account_id).await?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let people = People::new(&state.store.person_links(account_id).await?);
    let emails: Vec<String> = candidates.iter().map(|c| c.email.clone()).collect();
    let outbound: HashMap<String, u32> = state
        .store
        .people_facts(account_id, &emails)
        .await?
        .into_iter()
        .map(|f| (f.email, f.total_outbound))
        .collect();
    let mut by_name: Vec<(String, Vec<String>)> = Vec::new();
    for candidate in candidates {
        let key = candidate.display_name.to_lowercase();
        match by_name
            .iter_mut()
            .find(|(name, _)| name.to_lowercase() == key)
        {
            Some((_, emails)) => emails.push(candidate.email),
            None => by_name.push((candidate.display_name, vec![candidate.email])),
        }
    }
    Ok(by_name
        .into_iter()
        .filter_map(|(name, mut addresses)| {
            let persons: HashSet<String> = addresses.iter().map(|a| people.primary(a)).collect();
            if persons.len() < 2 {
                return None;
            }
            addresses.sort_by(|a, b| {
                outbound
                    .get(b)
                    .copied()
                    .unwrap_or(0)
                    .cmp(&outbound.get(a).copied().unwrap_or(0))
                    .then(a.cmp(b))
            });
            Some(MergeSuggestionData {
                account_id: account_id.clone(),
                name,
                addresses,
                reason: "Same name, and you've written to both.".to_string(),
            })
        })
        .collect())
}
