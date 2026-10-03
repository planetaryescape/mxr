//! Got it (`.`): a short acknowledgement in your own words for that person,
//! instead of a reaction, which degrades into extra email on every other
//! client (messages.md §3). The text is a template filled with the greeting
//! and sign-off you actually use with them (`mxr_relationship` habits), so
//! no model is involved (model-fit review). The preview and the send build
//! the same text through `plan`, and a send with `expect_text` refuses
//! anything else.

use super::desk::{self_matcher, Senders};
use super::desk_lanes::{current_messages, last_stored};
use super::modes::threads_by_account;
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::Utc;
use mxr_compose::{prefix_subject, SubjectMarker};
use mxr_core::id::{AccountId, DraftId, ThreadId};
use mxr_core::types::{Address, Draft, DraftContent, DraftIntent, ReplyHeaders};
use mxr_protocol::{first_name, messages_copy, AckPlanData, ResponseData};
use mxr_relationship::{analyse_habits, clean_for_voice, WritingHabits};

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
        (None, None) => format!(
            "No greeting or sign-off of yours with {first} to go on, so a plain thanks."
        ),
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

async fn plan(state: &AppState, thread_id: &ThreadId, dry_run: bool) -> Result<Prepared, HandlerError> {
    let now = Utc::now();
    let Some((account_id, _)) = threads_by_account(state, std::slice::from_ref(thread_id))
        .await?
        .into_iter()
        .next()
    else {
        return Err(HandlerError::InvalidRequest(format!(
            "No conversation {thread_id}."
        )));
    };
    let messages = state
        .store
        .desk_messages_in_threads(&account_id, std::slice::from_ref(thread_id))
        .await?;
    let senders = Senders::load(state, &account_id, &messages).await?;
    let is_self = self_matcher(state, &account_id).await?;
    let target = last_stored(current_messages(&messages, now), |m| {
        !m.trashed && senders.answers(m, &is_self)
    })
    .ok_or_else(|| {
        HandlerError::InvalidRequest(
            "Nobody in this conversation has written to you, so there's nothing to acknowledge."
                .to_string(),
        )
    })?;
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
    let to_name = (context.reply_to.eq_ignore_ascii_case(&target.from.email))
        .then(|| name.clone())
        .flatten();
    Ok(Prepared {
        plan: AckPlanData {
            account_id: account_id.clone(),
            thread_id: thread_id.clone(),
            reply_to_message_id: target.id.clone(),
            to: vec![Address {
                name: to_name,
                email: context.reply_to.clone(),
            }],
            subject: prefix_subject(SubjectMarker::Reply, &context.subject),
            text,
            built_from,
            countdown_seconds: messages_copy::ACK_COUNTDOWN_SECONDS,
            dry_run,
            sent_message_id: None,
        },
        from: (!context.from.trim().is_empty()).then(|| Address {
            name: None,
            email: context.from.clone(),
        }),
        headers: ReplyHeaders {
            in_reply_to: context.in_reply_to,
            references: context.references,
            thread_id: context.thread_id,
        },
    })
}

pub(super) async fn ack(
    state: &AppState,
    thread_id: &ThreadId,
    dry_run: bool,
    expect_text: Option<&str>,
) -> HandlerResult {
    let prepared = plan(state, thread_id, dry_run).await?;
    if dry_run {
        return Ok(ResponseData::MessagesAck {
            ack: prepared.plan,
        });
    }
    if let Some(expected) = expect_text {
        if expected != prepared.plan.text {
            return Err(HandlerError::InvalidRequest(
                "The acknowledgement changed since its preview, so nothing was sent. Preview it again."
                    .to_string(),
            ));
        }
    }
    let now = Utc::now();
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
    let sent = super::mutations::send_draft(state, &draft, None).await?;
    let ResponseData::SendReceipt {
        local_message_id, ..
    } = sent
    else {
        return Err(HandlerError::Message(
            "the acknowledgement was not sent".to_string(),
        ));
    };
    // Got it is Messages' main verb: its first-encounter card retires.
    super::mode_guide::retire(state, "messages").await?;
    let mut plan = prepared.plan;
    plan.sent_message_id = Some(local_message_id);
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
