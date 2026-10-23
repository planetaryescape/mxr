//! Independently versioned asks facet; the home materializer remains inbound-only.
use super::HandlerError;
use crate::state::AppState;
use chrono::Local;
use mxr_core::asks::{AskAssessment, ASKS_POLICY_VERSION};
use mxr_core::{AccountId, ThreadId};
use mxr_store::DeskMessage;
use mxr_todo::asks::{assess, AskInput, Predecessor};
use std::collections::HashMap;

pub(super) async fn materialize(
    state: &AppState,
    account: &AccountId,
    messages: &[DeskMessage],
    revisions: &HashMap<ThreadId, i64>,
    is_owner: &(dyn Fn(&str) -> bool + Sync),
) -> Result<bool, HandlerError> {
    let ids: Vec<_> = messages.iter().map(|m| m.id.clone()).collect();
    let stored = state.store.asks_for_messages(account, &ids).await?;
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
                mxr_reader::new_text::plain_text(plain.as_deref(), html.as_deref()),
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
    for thread in messages.chunk_by(|a, b| a.thread_id == b.thread_id) {
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
        let mut ordered: Vec<_> = thread.iter().collect();
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
            let predecessor = position.checked_sub(1).and_then(|p| ordered.get(p));
            let body = plain_bodies.get(&message.id);
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
                "message": [message.id, message.from, message.to, message.cc, message.bcc, message.subject, message.date, body, message.evidence.bulk, message.list_id],
                "owner": [owned, names, owner_addresses],
                "predecessor": predecessor.map(|p| (&p.id, &p.from, previous_body)),
                "owner_replied_afterwards": replied,
                "timezone_offset": message.date.with_timezone(&Local).offset().to_string(),
            }).to_string());
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
            };
            answers.push(assess(&input, &Local, fingerprint));
        }
    }
    Ok(state
        .store
        .save_asks_batch(account, revisions, &answers)
        .await?)
}
