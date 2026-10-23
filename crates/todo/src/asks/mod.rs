//! Deterministic asks judge. Facts and votes are retained before any item projection.
mod candidates;
mod document;
mod facts;
pub mod items;
mod votes;

use chrono::{DateTime, TimeZone, Utc};
use mxr_core::asks::*;
use mxr_core::{AccountId, Address, MessageId};

#[derive(Clone, Copy)]
pub struct Predecessor<'a> {
    pub id: &'a MessageId,
    pub text: &'a str,
    pub same_author: bool,
}

pub struct AskInput<'a> {
    pub message_id: &'a MessageId,
    pub account_id: &'a AccountId,
    pub subject: &'a str,
    pub body: Option<&'a str>,
    pub from: &'a Address,
    pub to: &'a [Address],
    pub cc: &'a [Address],
    pub bcc: &'a [Address],
    pub list_mail: bool,
    pub bulk: bool,
    pub owner_names: &'a [String],
    pub owner_addresses: &'a [String],
    /// Supplied by the canonical OwnedAddresses/self_matcher path.
    pub is_owner: &'a dyn Fn(&str) -> bool,
    pub sent: DateTime<Utc>,
    pub predecessor: Option<Predecessor<'a>>,
    pub owner_replied_afterwards: bool,
}

pub fn assess<Tz>(input: &AskInput<'_>, tz: &Tz, fingerprint: String) -> AskAssessment
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let mut facts = facts::read(input);
    candidates::extract(&mut facts, input.sent.with_timezone(tz), &input.from.email);
    let mut units = Vec::new();
    for (zone, source) in facts.zones.iter().enumerate() {
        if !matches!(
            source.kind,
            ZoneKind::Authored | ZoneKind::Subject | ZoneKind::Advertising
        ) {
            continue;
        }
        for span in facts::unit_spans(&source.text) {
            let text = &source.text[span.start..span.end];
            let document = document::read(text, zone, &span, &facts);
            let votes = votes::label(
                text,
                &source.text[..span.start],
                zone,
                &span,
                &facts,
                &document,
                input,
            );
            let tags = votes
                .first()
                .map(|v| v.tags.clone())
                .filter(|first| votes.iter().all(|vote| &vote.tags == first));
            let mut answer = resolve(tags.as_ref(), input.owner_replied_afterwards);
            if answer == AskAnswer::NotSure
                && tags.is_none()
                && !votes::eligible_uncertainty(text, &document)
            {
                answer = AskAnswer::Nothing;
            }
            if text.len() > 2048 && matches!(answer, AskAnswer::AsksYou | AskAnswer::YouPromised) {
                answer = AskAnswer::NotSure;
            }
            units.push(AskUnit {
                zone,
                span,
                votes,
                tags,
                document,
                answer,
            });
        }
    }
    // A subject can name the old pending act while the authored body
    // cancels or confirms that same kind. Preserve both readings and
    // abstain on the subject instead of creating an outdated task.
    let terminal: Vec<_> = units
        .iter()
        .filter(|u| facts.zones[u.zone].kind == ZoneKind::Authored)
        .filter_map(|u| u.tags.as_ref())
        .filter(|t| matches!(t.state, AskState::Done | AskState::Cancelled))
        .map(|t| t.action)
        .collect();
    for unit in &mut units {
        if facts.zones[unit.zone].kind == ZoneKind::Subject
            && unit
                .tags
                .as_ref()
                .is_some_and(|t| terminal.contains(&t.action) || input.body.is_none())
        {
            unit.tags = None;
            unit.answer = AskAnswer::Nothing;
        }
    }
    AskAssessment {
        message_id: input.message_id.clone(),
        account_id: input.account_id.clone(),
        policy_version: ASKS_POLICY_VERSION.into(),
        input_fingerprint: fingerprint,
        pending_body: input.body.is_none(),
        context_token: String::new(),
        facts,
        units,
    }
}

fn resolve(tags: Option<&AskTags>, owner_replied: bool) -> AskAnswer {
    let Some(tags) = tags else {
        return AskAnswer::NotSure;
    };
    if !matches!(tags.state, AskState::Pending | AskState::Failed) {
        return AskAnswer::Nothing;
    }
    match (tags.act, tags.actor, tags.action) {
        (Act::Request | Act::Question, Actor::Owner, ActionKind::Reply) => {
            if owner_replied {
                AskAnswer::Nothing
            } else {
                AskAnswer::AsksYouToReply
            }
        }
        (Act::Request | Act::Obligation, Actor::Owner, _) => AskAnswer::AsksYou,
        (Act::Commitment, Actor::Owner, _) => AskAnswer::YouPromised,
        (Act::Request | Act::Question | Act::Obligation, Actor::Correspondent, _) => {
            AskAnswer::YouAskedThem
        }
        (Act::Commitment, Actor::Correspondent, _) => AskAnswer::TheyPromisedYou,
        (Act::Request | Act::Obligation | Act::Commitment, Actor::Everyone, _) => {
            AskAnswer::NotSure
        }
        _ => AskAnswer::Nothing,
    }
}

#[cfg(test)]
mod tests;
