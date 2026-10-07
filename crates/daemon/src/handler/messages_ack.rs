//! Got it (`.`): a short acknowledgement in your own words for that person,
//! instead of a reaction, which degrades into extra email on every other
//! client (messages.md §3). The text is a template filled with the greeting
//! and sign-off you actually use with them (`mxr_relationship` habits), so
//! no model is involved (model-fit review).
//!
//! Got it sends real mail on one key, so the send is guarded in the daemon,
//! not only by the client's countdown:
//!
//! - A send needs the preview's token, issued by this daemon within the
//!   last minute, and the previewed text. The token is keyed with a
//!   per-process secret over the whole plan (account, sender, recipient,
//!   target message, subject, text), so a send goes only when all of it
//!   still matches what was previewed.
//! - The final recipient is checked, not just the sender: no lists,
//!   automated or no-reply addresses, and no Reply-To on another domain
//!   you have never written to. It never replies to all.
//! - Once anything of yours follows their message, it is answered: a
//!   second Got it for it is refused, and the next preview has nothing to
//!   acknowledge.

use super::desk::{self_matcher, Senders};
use super::desk_lanes::{current_messages, last_stored};
use super::mail_kind::looks_automated;
use super::modes::threads_by_account;
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use mxr_compose::{prefix_subject, SubjectMarker};
use mxr_core::id::{AccountId, DraftId, MessageId, ThreadId};
use mxr_core::types::{Address, Draft, DraftContent, DraftIntent, ReplyHeaders};
use mxr_protocol::{first_name, messages_copy, AckPlanData, ResponseData};
use mxr_relationship::{analyse_habits, clean_for_voice, WritingHabits};
use mxr_store::ScreenerDisposition;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Your own mail to them the habits are measured on.
const HABIT_SAMPLES: u32 = 25;
/// A greeting or sign-off is "yours" once you use it this often.
const HABIT_MIN_RATE: f64 = 0.3;
const BODY: &str = "Got it, thanks.";

/// The acknowledgement for `first` (their first name) in your habits, and
/// what it was built from.
pub(super) fn ack_text(first: &str, habits: &WritingHabits) -> (String, String) {
    let greeting = habits
        .greeting
        .as_ref()
        .filter(|_| habits.greeting_rate >= HABIT_MIN_RATE)
        .map(|habit| habit.text.replace("{name}", first));
    let sign_off = habits
        .sign_off
        .as_ref()
        .filter(|_| habits.sign_off_rate >= HABIT_MIN_RATE)
        .map(|habit| habit.text.replace("{name}", first));
    let mut text = match &greeting {
        Some(greeting) => format!("{greeting}\n\n{BODY}"),
        None => format!("Thanks {first}, got it."),
    };
    if let Some(sign_off) = &sign_off {
        text.push_str("\n\n");
        text.push_str(sign_off);
    }
    let built_from = match (&greeting, &sign_off) {
        (Some(_), Some(_)) => format!("Your usual greeting and sign-off with {first}."),
        (Some(_), None) => format!("Your usual greeting with {first}."),
        (None, Some(_)) => format!("Your usual sign-off with {first}."),
        (None, None) => {
            format!("No greeting or sign-off of yours with {first} to go on, so a plain thanks.")
        }
    };
    (text, built_from)
}

/// How you write to `email`: your replies and other mail to them.
async fn habits_with(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
    name: Option<&str>,
) -> Result<WritingHabits, HandlerError> {
    let store = &state.store;
    let mut bodies: Vec<String> = store
        .my_reply_samples(account_id, Some(email), None, None, HABIT_SAMPLES)
        .await?
        .iter()
        .map(|sample| clean_for_voice(&sample.reply_body))
        .collect();
    if bodies.len() < HABIT_SAMPLES as usize {
        bodies.extend(
            store
                .my_sent_to(account_id, email, None, None, HABIT_SAMPLES)
                .await?
                .iter()
                .map(|sample| clean_for_voice(&sample.body)),
        );
    }
    bodies.sort();
    bodies.dedup();
    let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let names: Vec<&str> = name.into_iter().collect();
    Ok(analyse_habits(&refs, &names))
}

/// Everything the send needs besides the plan the client sees.
struct Prepared {
    plan: AckPlanData,
    from: Option<Address>,
    headers: ReplyHeaders,
}

/// Why a Got it was refused, in words a person can act on.
fn refuse(reason: impl Into<String>) -> HandlerError {
    HandlerError::InvalidRequest(reason.into())
}

const CHANGED: &str =
    "The conversation changed since the preview, so nothing was sent. Preview Got it again.";

async fn plan(
    state: &AppState,
    thread_id: &ThreadId,
    dry_run: bool,
    now: DateTime<Utc>,
) -> Result<Prepared, HandlerError> {
    let Some((account_id, _)) = threads_by_account(state, std::slice::from_ref(thread_id))
        .await?
        .into_iter()
        .next()
    else {
        return Err(refuse(format!("No conversation {thread_id}.")));
    };
    let messages = state
        .store
        .desk_messages_in_threads(&account_id, std::slice::from_ref(thread_id))
        .await?;
    let senders = Senders::load(state, &account_id, &messages).await?;
    let is_self = self_matcher(state, &account_id).await?;
    let current = current_messages(&messages, now);
    let target = last_stored(current, |m| !m.trashed && senders.answers(m, &is_self))
        .ok_or_else(|| {
            refuse("Nobody in this conversation has written to you, so there's nothing to acknowledge.")
        })?;
    // Anything of yours after their message answers it: a repeat Got it,
    // or a reply you already sent.
    if current
        .iter()
        .any(|m| m.seq > target.seq && !m.trashed && super::desk_lanes::is_outbound(m, &is_self))
    {
        return Err(refuse(
            "Already answered: you've written in this conversation since their last message.",
        ));
    }
    let email = target.from.email.to_ascii_lowercase();
    let name = target
        .from
        .name
        .clone()
        .filter(|n| !n.trim().is_empty() && !n.contains('@'))
        .or_else(|| {
            senders
                .contacts
                .get(&email)
                .and_then(|c| c.display_name.clone())
        });
    let first = first_name(name.as_deref(), &email);
    let habits = habits_with(state, &account_id, &email, name.as_deref()).await?;
    let (text, built_from) = ack_text(&first, &habits);

    let ResponseData::ReplyContext { context } =
        super::mutations::prepare_reply(state, &target.id, false).await?
    else {
        return Err(HandlerError::Message(
            "could not prepare the reply".to_string(),
        ));
    };
    let recipient = context.reply_to.trim().to_ascii_lowercase();
    check_recipient(
        state,
        &account_id,
        &senders,
        &email,
        &recipient,
        target.list_id.is_some(),
    )
    .await?;
    let to_name = (recipient == email).then(|| name.clone()).flatten();
    let mut plan = AckPlanData {
        account_id: account_id.clone(),
        thread_id: thread_id.clone(),
        reply_to_message_id: target.id.clone(),
        from: context.from.trim().to_ascii_lowercase(),
        to: vec![Address {
            name: to_name,
            email: recipient,
        }],
        subject: prefix_subject(SubjectMarker::Reply, &context.subject),
        text,
        built_from,
        countdown_seconds: messages_copy::ACK_COUNTDOWN_SECONDS,
        dry_run,
        preview_token: None,
        preview_expires_at: None,
        sent_message_id: None,
    };
    if dry_run {
        plan.preview_token = Some(token(&plan, &context.in_reply_to, now.timestamp()));
        plan.preview_expires_at =
            Some(now + Duration::seconds(messages_copy::ACK_PREVIEW_TTL_SECONDS));
    }
    Ok(Prepared {
        from: (!context.from.trim().is_empty()).then(|| Address {
            name: None,
            email: context.from.clone(),
        }),
        headers: ReplyHeaders {
            in_reply_to: context.in_reply_to,
            references: context.references,
            thread_id: context.thread_id,
        },
        plan,
    })
}

/// The final recipient must be a person you'd expect: not a list, not a
/// machine, not screened out, and not a Reply-To that sends the answer to
/// a different organisation you've never written to.
async fn check_recipient(
    state: &AppState,
    account_id: &AccountId,
    senders: &Senders,
    from: &str,
    recipient: &str,
    on_a_list: bool,
) -> Result<(), HandlerError> {
    let not_a_person = |why: &str| {
        refuse(format!(
            "Got it won't reply to {recipient}: {why}. Reply by hand if you mean to."
        ))
    };
    if recipient.is_empty() || !recipient.contains('@') {
        return Err(not_a_person("there's no address to reply to"));
    }
    if on_a_list {
        return Err(not_a_person("the message came through a mailing list"));
    }
    if looks_automated(recipient) {
        return Err(not_a_person(
            "it looks like an automated or no-reply address",
        ));
    }
    if senders
        .contacts
        .get(recipient)
        .is_some_and(|c| c.is_list_sender)
    {
        return Err(not_a_person("it sends to mailing lists"));
    }
    if senders.screener.get(recipient) == Some(&ScreenerDisposition::Deny) {
        return Err(not_a_person("you screened it out"));
    }
    if recipient != from {
        let domain = |address: &str| {
            address
                .rsplit_once('@')
                .and_then(|(_, host)| mxr_todo::action_link::registrable_domain(host))
        };
        let same_org = domain(recipient).is_some() && domain(recipient) == domain(from);
        if !same_org {
            let written = state
                .store
                .addresses_written_to(account_id, &[recipient.to_string()])
                .await?;
            if !written.contains(recipient) {
                return Err(not_a_person(
                    "their Reply-To is on another domain you've never written to",
                ));
            }
        }
    }
    Ok(())
}

/// A per-process secret: a token can't be made without a preview from this
/// daemon. UUIDv7's random bits are enough for a key that lives a process.
fn key() -> &'static [u8; 32] {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    KEY.get_or_init(|| {
        let mut key = [0u8; 32];
        key[..16].copy_from_slice(uuid::Uuid::now_v7().as_bytes());
        key[16..].copy_from_slice(uuid::Uuid::now_v7().as_bytes());
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.update(std::process::id().to_le_bytes());
        hasher.finalize().into()
    })
}

/// `{issued}.{mac}`: issued is unix seconds; mac covers the whole plan.
fn token(plan: &AckPlanData, in_reply_to: &str, issued: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key());
    for part in [
        issued.to_string().as_str(),
        plan.account_id.as_str().as_str(),
        plan.thread_id.as_str().as_str(),
        plan.reply_to_message_id.as_str().as_str(),
        in_reply_to,
        plan.from.as_str(),
        plan.subject.as_str(),
        plan.text.as_str(),
    ] {
        hasher.update(part.len().to_le_bytes());
        hasher.update(part.as_bytes());
    }
    for to in &plan.to {
        hasher.update(to.email.len().to_le_bytes());
        hasher.update(to.email.as_bytes());
    }
    let mac: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{issued}.{mac}")
}

/// Messages a Got it is sending or has sent in this process: a second
/// request racing the first is refused before the store sees the reply.
fn acked() -> &'static Mutex<HashSet<MessageId>> {
    static ACKED: OnceLock<Mutex<HashSet<MessageId>>> = OnceLock::new();
    ACKED.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(super) async fn ack(
    state: &AppState,
    thread_id: &ThreadId,
    dry_run: bool,
    expect_text: Option<&str>,
    preview_token: Option<&str>,
) -> HandlerResult {
    ack_at(
        state,
        thread_id,
        dry_run,
        preview_token,
        expect_text,
        Utc::now(),
    )
    .await
}

/// Got it at `now` (tests move the clock).
pub(super) async fn ack_at(
    state: &AppState,
    thread_id: &ThreadId,
    dry_run: bool,
    preview_token: Option<&str>,
    expect_text: Option<&str>,
    now: DateTime<Utc>,
) -> HandlerResult {
    if dry_run {
        let prepared = plan(state, thread_id, true, now).await?;
        return Ok(ResponseData::MessagesAck { ack: prepared.plan });
    }
    let (Some(token_in), Some(expected)) = (
        preview_token.filter(|t| !t.is_empty()),
        expect_text.filter(|t| !t.is_empty()),
    ) else {
        return Err(refuse(
            "Got it sends only what you previewed: preview it first, then send with its preview_token and text.",
        ));
    };
    let issued = token_in
        .split_once('.')
        .and_then(|(issued, _)| issued.parse::<i64>().ok())
        .ok_or_else(|| refuse(CHANGED))?;
    let age = now.timestamp() - issued;
    if !(0..=messages_copy::ACK_PREVIEW_TTL_SECONDS).contains(&age) {
        return Err(refuse(
            "That Got it preview is more than a minute old, so nothing was sent. Preview it again.",
        ));
    }
    let prepared = plan(state, thread_id, false, now).await?;
    if expected != prepared.plan.text
        || token(&prepared.plan, &prepared.headers.in_reply_to, issued) != token_in
    {
        return Err(refuse(CHANGED));
    }
    let target = prepared.plan.reply_to_message_id.clone();
    {
        let mut acked = acked()
            .lock()
            .map_err(|_| HandlerError::Message("Got it state is unavailable".to_string()))?;
        if !acked.insert(target.clone()) {
            return Err(refuse("Already acknowledged: Got it went to that message."));
        }
    }
    let draft = Draft {
        id: DraftId::new(),
        account_id: prepared.plan.account_id.clone(),
        from: prepared.from,
        reply_headers: Some(prepared.headers),
        intent: DraftIntent::Reply,
        to: prepared.plan.to.clone(),
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: prepared.plan.subject.clone(),
        content: DraftContent::Markdown {
            source: prepared.plan.text.clone(),
        },
        attachments: Vec::new(),
        inline_assets: Vec::new(),
        created_at: now,
        updated_at: now,
        inline_calendar_reply: None,
    };
    let sent = match super::mutations::send_draft(state, &draft, None).await {
        Ok(ResponseData::SendReceipt {
            local_message_id, ..
        }) => local_message_id,
        Ok(_) | Err(_) => {
            // Nothing went out: let a later, previewed Got it try again.
            if let Ok(mut acked) = acked().lock() {
                acked.remove(&target);
            }
            return Err(HandlerError::Message(
                "The acknowledgement was not sent.".to_string(),
            ));
        }
    };
    // Got it is Messages' main verb: its first-encounter card retires.
    super::mode_guide::retire(state, "messages").await?;
    let mut plan = prepared.plan;
    plan.sent_message_id = Some(sent);
    Ok(ResponseData::MessagesAck { ack: plan })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_relationship::analyse_habits;

    #[test]
    fn greeting_and_sign_off_come_from_your_own_mail() {
        let mine = [
            "Hi Samir,\n\nThe draft is attached.\n\nCheers,\nAlex",
            "Hi Samir,\n\nSure, Thursday works.\n\nCheers,\nAlex",
            "Hi Samir,\n\nLooks good to me.\n\nCheers,\nAlex",
        ];
        let habits = analyse_habits(&mine, &["Samir Patel"]);
        let (text, built_from) = ack_text("Samir", &habits);
        assert_eq!(text, "Hi Samir,\n\nGot it, thanks.\n\nCheers,\nAlex");
        assert_eq!(built_from, "Your usual greeting and sign-off with Samir.");
    }

    #[test]
    fn a_greeting_learned_with_one_person_carries_their_name_not_the_old_one() {
        let mine = ["Hey Ruth,\n\nOn it.\n\nA", "Hey Ruth,\n\nDone.\n\nA"];
        let habits = analyse_habits(&mine, &["Ruth Vega"]);
        let (text, _) = ack_text("Ruth", &habits);
        assert!(text.starts_with("Hey Ruth,\n\nGot it, thanks."), "{text}");
    }

    #[test]
    fn with_nothing_to_go_on_it_is_a_plain_thanks() {
        let (text, built_from) = ack_text("Iris", &WritingHabits::default());
        assert_eq!(text, "Thanks Iris, got it.");
        assert!(built_from.starts_with("No greeting or sign-off"));
    }
}
