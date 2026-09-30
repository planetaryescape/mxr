//! Everything a draft needs to sound like the user, gathered from the store:
//! who they are, how they write (real examples first, habits second), and
//! the conversation as ME / THEM turns with the message to answer marked.
//!
//! `before` rebuilds what was known at a moment in time, so the offline
//! draft evaluation can replay a real reply without seeing it.

use super::draft_prompt::{Me, Turn, VoiceExample};
use crate::state::AppState;
use chrono::{DateTime, Local, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::Envelope;
use mxr_protocol::DraftSourceData;
use mxr_relationship::{analyse_habits, clean_for_voice};
use std::collections::BTreeSet;

const MAX_EXAMPLES: usize = 5;
/// Replies shorter than this say nothing about voice ("ok"); longer ones are
/// usually templates or documents, not how the user talks.
const EXAMPLE_MIN_WORDS: usize = 3;
const EXAMPLE_MAX_WORDS: usize = 350;
const HABIT_SAMPLES: u32 = 25;

pub(crate) struct VoiceMaterial {
    pub examples: Vec<VoiceExample>,
    pub habits: Vec<String>,
    /// Median length of the user's own emails in the sample.
    pub median_words: Option<u32>,
    pub samples: usize,
}

impl VoiceMaterial {
    /// Nothing from the user's other mail (a cloud model without opt-in).
    pub fn none() -> Self {
        Self {
            examples: Vec::new(),
            habits: Vec::new(),
            median_words: None,
            samples: 0,
        }
    }
}

/// The person being written to, as the voice material is chosen for them.
pub(crate) struct Counterparty {
    pub email: String,
    pub name: Option<String>,
}

impl Counterparty {
    pub fn label(&self) -> String {
        match self.name.as_deref().filter(|name| !name.trim().is_empty()) {
            Some(name) => format!("{name} <{}>", self.email),
            None => self.email.clone(),
        }
    }
}

pub(crate) async fn me(state: &AppState, account_id: &AccountId) -> Me {
    let mut emails: Vec<(bool, String)> = state
        .store
        .list_account_addresses(account_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|address| (address.is_primary, address.email))
        .collect();
    emails.sort_by_key(|(primary, _)| !primary);
    Me {
        name: state.store.my_display_name(account_id).await.ok().flatten(),
        emails: emails.into_iter().map(|(_, email)| email).collect(),
    }
}

pub(crate) fn owned(me: &Me) -> BTreeSet<String> {
    me.emails
        .iter()
        .map(|email| email.to_ascii_lowercase())
        .collect()
}

pub(crate) fn today() -> String {
    Local::now().format("%A %-d %B %Y").to_string()
}

fn when(date: DateTime<Utc>) -> String {
    date.with_timezone(&Local)
        .format("%a %-d %b %Y, %H:%M")
        .to_string()
}

fn is_mine(envelope: &Envelope, owned: &BTreeSet<String>) -> bool {
    owned.contains(&envelope.from.email.to_ascii_lowercase())
}

/// The message to answer: the one the user picked, else the newest message
/// someone else sent, else the newest message.
pub(crate) fn pick_target<'a>(
    envelopes: &'a [Envelope],
    owned: &BTreeSet<String>,
    source: Option<&MessageId>,
) -> Option<&'a Envelope> {
    if let Some(found) = source.and_then(|id| envelopes.iter().find(|envelope| &envelope.id == id))
    {
        return Some(found);
    }
    envelopes
        .iter()
        .filter(|envelope| !is_mine(envelope, owned))
        .max_by_key(|envelope| envelope.date)
        .or_else(|| envelopes.iter().max_by_key(|envelope| envelope.date))
}

/// Who the reply is really to: the target's sender, or when the user wrote
/// the target, the first other recipient.
pub(crate) fn counterparty_of(target: &Envelope, owned: &BTreeSet<String>) -> Option<Counterparty> {
    if !owned.contains(&target.from.email.to_ascii_lowercase()) {
        return Some(Counterparty {
            email: target.from.email.to_ascii_lowercase(),
            name: target.from.name.clone(),
        });
    }
    target
        .to
        .iter()
        .chain(target.cc.iter())
        .find(|address| !owned.contains(&address.email.to_ascii_lowercase()))
        .map(|address| Counterparty {
            email: address.email.to_ascii_lowercase(),
            name: address.name.clone(),
        })
}

/// The thread as cleaned turns, oldest first, target marked.
pub(crate) async fn conversation(
    state: &AppState,
    envelopes: &[Envelope],
    owned: &BTreeSet<String>,
    target: Option<&MessageId>,
) -> Vec<Turn> {
    let mut sorted: Vec<&Envelope> = envelopes.iter().collect();
    sorted.sort_by_key(|envelope| envelope.date);
    let mut turns = Vec::with_capacity(sorted.len());
    for envelope in sorted {
        let raw = match state.store.get_body(&envelope.id).await {
            Ok(Some(body)) => body.text_plain.or(body.text_html).unwrap_or_default(),
            _ => String::new(),
        };
        let mut text = clean_for_voice(&raw);
        if text.is_empty() {
            // All quote (a bare forward) or no body: the snippet beats nothing.
            text = if raw.trim().is_empty() {
                envelope.snippet.clone()
            } else {
                raw.trim().to_string()
            };
        }
        let from_me = is_mine(envelope, owned);
        let other = counterparty_of(envelope, owned);
        turns.push(Turn {
            from_me,
            who: match envelope
                .from
                .name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
            {
                Some(name) => format!("{name} <{}>", envelope.from.email),
                None => envelope.from.email.clone(),
            },
            when: when(envelope.date),
            text,
            target: target == Some(&envelope.id),
            source: DraftSourceData {
                message_id: envelope.id.clone(),
                thread_id: envelope.thread_id.clone(),
                date: envelope.date,
                from_me,
                person: other
                    .as_ref()
                    .map(|person| person.email.clone())
                    .unwrap_or_default(),
                person_name: other.and_then(|person| person.name),
            },
        });
    }
    turns
}

/// Real emails the user wrote, to `counterparty` first, and the habits
/// they show. `before` and `exclude_thread` keep the conversation being
/// drafted (and anything after it) out.
pub(crate) async fn voice_material(
    state: &AppState,
    account_id: &AccountId,
    counterparty: Option<&Counterparty>,
    exclude_thread: Option<&ThreadId>,
    before: Option<DateTime<Utc>>,
) -> VoiceMaterial {
    let store = &state.store;
    let mut examples: Vec<VoiceExample> = Vec::new();
    let mut habit_bodies: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    let mut take = |examples: &mut Vec<VoiceExample>,
                    habit_bodies: &mut Vec<String>,
                    to: String,
                    theirs: Option<String>,
                    mine_raw: &str,
                    source: DraftSourceData,
                    for_habits: bool| {
        let mine = clean_for_voice(mine_raw);
        let words = mine.split_whitespace().count();
        if !(EXAMPLE_MIN_WORDS..=EXAMPLE_MAX_WORDS).contains(&words) {
            return;
        }
        let key: String = mine.chars().take(80).collect::<String>().to_lowercase();
        if !seen.insert(key) {
            return;
        }
        if for_habits {
            habit_bodies.push(mine.clone());
        }
        if examples.len() < MAX_EXAMPLES {
            examples.push(VoiceExample {
                to,
                their_message: theirs
                    .map(|text| clean_for_voice(&text))
                    .filter(|text| !text.is_empty()),
                my_email: mine,
                source,
            });
        }
    };

    if let Some(person) = counterparty {
        if let Some(name) = &person.name {
            names.push(name.clone());
        }
        let pairs = store
            .my_reply_samples(
                account_id,
                Some(&person.email),
                exclude_thread,
                before,
                HABIT_SAMPLES,
            )
            .await
            .unwrap_or_default();
        for pair in &pairs {
            take(
                &mut examples,
                &mut habit_bodies,
                person.label(),
                Some(pair.parent_body.clone()),
                &pair.reply_body,
                reply_source(pair, person.name.clone()),
                true,
            );
        }
        let sent = store
            .my_sent_to(
                account_id,
                &person.email,
                exclude_thread,
                before,
                HABIT_SAMPLES,
            )
            .await
            .unwrap_or_default();
        for message in &sent {
            take(
                &mut examples,
                &mut habit_bodies,
                person.label(),
                None,
                &message.body,
                DraftSourceData {
                    message_id: message.message_id.clone(),
                    thread_id: message.thread_id.clone(),
                    date: message.date,
                    from_me: true,
                    person: person.email.clone(),
                    person_name: person.name.clone(),
                },
                true,
            );
        }
    }

    // Too little with this person: fill from how the user replies to anyone,
    // and describe habits from that wider sample.
    let personal = habit_bodies.len();
    if personal < 3 {
        let pairs = store
            .my_reply_samples(account_id, None, exclude_thread, before, HABIT_SAMPLES)
            .await
            .unwrap_or_default();
        for pair in &pairs {
            if let Some(name) = &pair.parent_from_name {
                names.push(name.clone());
            }
            let to = match pair.parent_from_name.as_deref() {
                Some(name) => format!("{name} <{}>", pair.parent_from_email),
                None => pair.parent_from_email.clone(),
            };
            take(
                &mut examples,
                &mut habit_bodies,
                to,
                Some(pair.parent_body.clone()),
                &pair.reply_body,
                reply_source(pair, pair.parent_from_name.clone()),
                true,
            );
        }
    }

    let bodies: Vec<&str> = habit_bodies.iter().map(String::as_str).collect();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let habits = analyse_habits(&bodies, &name_refs);
    let mut described = habits.describe();
    if personal < 3 && !described.is_empty() {
        described.insert(
            0,
            "(From my emails in general; I haven't written to this person much.)".to_string(),
        );
    }
    VoiceMaterial {
        examples,
        habits: described,
        median_words: (habits.samples >= 3).then_some(habits.median_words),
        samples: habits.samples,
    }
}

/// A reply the user sent, as a draft source.
fn reply_source(pair: &mxr_store::MyReplySample, person_name: Option<String>) -> DraftSourceData {
    DraftSourceData {
        message_id: pair.reply_message_id.clone(),
        thread_id: pair.thread_id.clone(),
        date: pair.replied_at,
        from_me: true,
        person: pair.counterparty_email.to_ascii_lowercase(),
        person_name,
    }
}
