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

pub(super) async fn list_queue(
    state: &AppState,
    account_id: &AccountId,
    limit: u32,
) -> HandlerResult {
    let entries = state
        .store
        .list_screener_queue(account_id, limit)
        .await
        ?;
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
    Ok(ResponseData::ScreenerQueue { entries: data })
}

pub(super) async fn list_decisions(state: &AppState, account_id: &AccountId) -> HandlerResult {
    let decisions = state
        .store
        .list_screener_decisions(account_id)
        .await
        ?;
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
    if sender_email.trim().is_empty() {
        return Err(crate::handler::HandlerError::Message("sender email cannot be empty".to_string()));
    }
    let decision = ScreenerDecision {
        account_id: account_id.clone(),
        sender_email,
        disposition: disposition_from_proto(disposition),
        route_label,
        decided_at: Utc::now(),
    };
    state
        .store
        .set_screener_decision(&decision)
        .await
        ?;
    Ok(ResponseData::Ack)
}

pub(super) async fn clear_decision(
    state: &AppState,
    account_id: &AccountId,
    sender_email: &str,
) -> HandlerResult {
    state
        .store
        .delete_screener_decision(account_id, sender_email)
        .await
        ?;
    Ok(ResponseData::Ack)
}
