//! Screener: per-(account, sender) consent classifications.

use super::HandlerResult;
use crate::state::AppState;
use chrono::Utc;
use mxr_core::id::AccountId;
use mxr_protocol::{
    ResponseData, ScreenerDecisionData, ScreenerDispositionData, ScreenerQueueEntryData,
};
use mxr_store::{ScreenerDecision, ScreenerDisposition};

fn disposition_from_proto(d: ScreenerDispositionData) -> ScreenerDisposition {
    match d {
        ScreenerDispositionData::Allow => ScreenerDisposition::Allow,
        ScreenerDispositionData::Deny => ScreenerDisposition::Deny,
        ScreenerDispositionData::Feed => ScreenerDisposition::Feed,
        ScreenerDispositionData::PaperTrail => ScreenerDisposition::PaperTrail,
        ScreenerDispositionData::Unknown => ScreenerDisposition::Unknown,
    }
}

fn disposition_to_proto(d: ScreenerDisposition) -> ScreenerDispositionData {
    match d {
        ScreenerDisposition::Allow => ScreenerDispositionData::Allow,
        ScreenerDisposition::Deny => ScreenerDispositionData::Deny,
        ScreenerDisposition::Feed => ScreenerDispositionData::Feed,
        ScreenerDisposition::PaperTrail => ScreenerDispositionData::PaperTrail,
        ScreenerDisposition::Unknown => ScreenerDispositionData::Unknown,
    }
}

/// The Screener page. By default the set the desk's Screener count counts
/// (inbox mail from senders first seen in the last 14 days, no decision),
/// so the page and the badge always agree; `all_senders` lists every
/// sender without a decision, in any folder, as before. The default
/// lists every counted sender whatever `limit` says, so the page never
/// shows fewer than the badge; `limit` applies to `all_senders`.
pub(super) async fn list_queue(
    state: &AppState,
    account_id: &AccountId,
    limit: u32,
    all_senders: bool,
) -> HandlerResult {
    if all_senders {
        let entries = state.store.list_screener_queue(account_id, limit).await?;
        return Ok(queue_response(entries));
    }
    let desk = super::desk::compose_desk(state, Some(account_id), Utc::now()).await?;
    let mut counted: Vec<String> = desk
        .screener_senders
        .into_iter()
        .map(|(_, email)| email)
        .collect();
    counted.sort_unstable();
    // Narrowed in the query, so no counted sender is pushed out by newer
    // ones the badge does not count.
    let entries = state
        .store
        .list_screener_queue_for(account_id, &counted)
        .await?;
    Ok(queue_response(entries))
}

fn queue_response(entries: Vec<mxr_store::ScreenerQueueEntry>) -> ResponseData {
    let data: Vec<ScreenerQueueEntryData> = entries
        .into_iter()
        .map(|e| ScreenerQueueEntryData {
            sender_email: e.sender_email,
            display_name: e.display_name,
            message_count: e.message_count,
            latest_subject: e.latest_subject,
            latest_at: e.latest_at,
        })
        .collect();
    ResponseData::ScreenerQueue { entries: data }
}

pub(super) async fn list_decisions(state: &AppState, account_id: &AccountId) -> HandlerResult {
    let decisions = state.store.list_screener_decisions(account_id).await?;
    let data: Vec<ScreenerDecisionData> = decisions
        .into_iter()
        .map(|d| ScreenerDecisionData {
            account_id: d.account_id,
            sender_email: d.sender_email,
            disposition: disposition_to_proto(d.disposition),
            route_label: d.route_label,
            decided_at: d.decided_at,
        })
        .collect();
    Ok(ResponseData::ScreenerDecisions { decisions: data })
}

pub(super) async fn set_decision(
    state: &AppState,
    account_id: &AccountId,
    sender_email: String,
    disposition: ScreenerDispositionData,
    route_label: Option<String>,
) -> HandlerResult {
    let _sorting = state.rule_mutation_gate.lock().await;
    if sender_email.trim().is_empty() {
        return Err(crate::handler::HandlerError::Message(
            "sender email cannot be empty".to_string(),
        ));
    }
    let decision = ScreenerDecision {
        account_id: account_id.clone(),
        sender_email,
        disposition: disposition_from_proto(disposition),
        route_label,
        decided_at: Utc::now(),
    };
    // A decision is a sender's kind: it moves mail in or out of the places.
    let _change = state.sweep_gate.change([account_id]).await;
    state.store.set_screener_decision(&decision).await?;
    Ok(ResponseData::Ack)
}

pub(super) async fn clear_decision(
    state: &AppState,
    account_id: &AccountId,
    sender_email: &str,
) -> HandlerResult {
    let _sorting = state.rule_mutation_gate.lock().await;
    let _change = state.sweep_gate.change([account_id]).await;
    state
        .store
        .delete_screener_decision(account_id, sender_email)
        .await?;
    Ok(ResponseData::Ack)
}
