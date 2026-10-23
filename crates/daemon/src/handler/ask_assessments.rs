//! Independently versioned asks facet; the home materializer remains inbound-only.
use super::HandlerError;
use crate::state::AppState;
use mxr_core::asks::{AskAssessment, ASKS_POLICY_VERSION};
use mxr_core::{AccountId, ThreadId};
use mxr_store::DeskMessage;
use mxr_todo::asks::{assess, AskInput, Predecessor};
use std::collections::HashMap;

pub(super) async fn prepare(
    state: &AppState,
    account: &AccountId,
    messages: &[DeskMessage],
    is_owner: &(dyn Fn(&str) -> bool + Sync),
) -> Result<Vec<AskAssessment>, HandlerError> {
    let timezone =
        mxr_store::AsksTimezone::resolve().map_err(|e| HandlerError::from(e.to_string()))?;
    let context_token = timezone.token().to_string();
    let ids: Vec<_> = messages.iter().map(|m| m.id.clone()).collect();
    let stored = state.store.asks_for_messages(account, &ids).await?;
    let envelopes = state.store.list_envelopes_by_ids(&ids).await?;
    let envelopes: HashMap<_, _> = envelopes.iter().map(|e| (&e.id, e)).collect();
    let source_bodies = if stored.len() == ids.len() {
        HashMap::new()
    } else {
        state.store.asks_source_bodies(account, &ids).await?
    };
    let plain_bodies: HashMap<_, _> = source_bodies
        .iter()
        .map(|(id, (plain, html))| {
            (
                id.clone(),
                mxr_reader::plain_text(plain.as_deref(), html.as_deref()),
            )
        })
        .collect();
    let owner_addresses: Vec<_> = state
        .owned_addresses(account)
        .await?
        .iter()
        .map(str::to_string)
        .collect();
    // Names are observations from this account's envelopes, never derived
    // from an email address or an account's UI label.
    let mut answers: Vec<AskAssessment> = Vec::new();
    let mut grouped: HashMap<&ThreadId, Vec<&DeskMessage>> = HashMap::new();
    for message in messages {
        grouped.entry(&message.thread_id).or_default().push(message);
    }
    for thread in grouped.values() {
        let mut names: Vec<_> = thread
            .iter()
            .flat_map(|m| {
                std::iter::once(&m.from)
                    .chain(&m.to)
                    .chain(&m.cc)
                    .chain(&m.bcc)
            })
            .filter(|a| is_owner(&a.email))
            .filter_map(|a| a.name.clone())
            .collect();
        names.sort();
        names.dedup();
        let mut ordered = thread.clone();
        ordered.sort_by_key(|m| m.seq);
        let last_owner = ordered
            .iter()
            .filter(|m| is_owner(&m.from.email))
            .map(|m| m.seq)
            .max();
        for (position, message) in ordered.iter().enumerate() {
            if stored.contains_key(&message.id) {
                continue;
            }
            let linked = envelopes
                .get(&message.id)
                .and_then(|e| e.in_reply_to.as_deref());
            let predecessor = linked
                .and_then(|header| {
                    ordered.iter().take(position).rev().find(|p| {
                        envelopes
                            .get(&p.id)
                            .is_some_and(|e| e.message_id_header.as_deref() == Some(header))
                    })
                })
                .or_else(|| position.checked_sub(1).and_then(|p| ordered.get(p)));
            let body = plain_bodies.get(&message.id);
            let calendar = if !is_owner(&message.from.email) {
                state.store.invite_awaiting_reply(&message.id).await?
            } else {
                None
            };
            let previous_body = predecessor.and_then(|p| plain_bodies.get(&p.id));
            let replied = last_owner.is_some_and(|seq| seq > message.seq);
            let owned: Vec<_> = std::iter::once(&message.from)
                .chain(&message.to)
                .chain(&message.cc)
                .chain(&message.bcc)
                .map(|a| (a, is_owner(&a.email)))
                .collect();
            let fingerprint = mxr_store::assessment_fingerprint(&serde_json::json!({
                "policy": ASKS_POLICY_VERSION,
                "message": [message.id, message.from, message.to, message.cc, message.bcc, message.subject, message.date, body, message.evidence.bulk, message.evidence.auto_generated, message.list_id],
                "owner": [owned, names, owner_addresses],
                "predecessor": predecessor.map(|p| (&p.id, &p.from, previous_body)),
                "owner_replied_afterwards": replied,
                "context_token": context_token,
                "calendar": calendar,
            }).to_string());
            timezone
                .validate_timestamp(message.date)
                .map_err(|e| HandlerError::from(e.to_string()))?;
            let input = AskInput {
                message_id: &message.id,
                account_id: account,
                subject: &message.subject,
                body: body.map(String::as_str),
                from: &message.from,
                to: &message.to,
                cc: &message.cc,
                bcc: &message.bcc,
                list_mail: message.list_id.is_some() || message.evidence.list_id.is_some(),
                bulk: message.evidence.bulk,
                owner_names: &names,
                owner_addresses: &owner_addresses,
                is_owner,
                sent: message.date,
                predecessor: predecessor.map(|p| Predecessor {
                    id: &p.id,
                    text: previous_body.map(String::as_str).unwrap_or_default(),
                    same_author: p.from.email.eq_ignore_ascii_case(&message.from.email),
                }),
                owner_replied_afterwards: replied,
                reply_capable: !message.evidence.auto_generated
                    && !super::mail_kind::no_reply_address(&message.from.email),
            };
            let mut answer = assess(&input, &timezone, fingerprint);
            mxr_todo::asks::include_calendar(&mut answer, calendar);
            answer.context_token = context_token.clone();
            answers.push(answer);
        }
    }
    // A timezone change during the read cannot publish an answer for the old context.
    if mxr_store::asks_context_token().map_err(|e| HandlerError::from(e.to_string()))?
        != context_token
    {
        return Err(HandlerError::from(
            "asks timezone changed during materialization",
        ));
    }
    Ok(answers)
}
