//! Sender profile: per-(account, email) relationship aggregates.

use super::HandlerResult;
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_protocol::{ResponseData, SenderProfileData};

pub(super) async fn get_sender_profile(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
) -> HandlerResult {
    let profile = state
        .store
        .get_sender_profile(account_id, email)
        .await
        .map_err(|e| e.to_string())?;
    let data = profile.map(|p| SenderProfileData {
        account_id: p.account_id,
        email: p.email,
        display_name: p.display_name,
        first_seen_at: p.first_seen_at,
        last_seen_at: p.last_seen_at,
        last_inbound_at: p.last_inbound_at,
        last_outbound_at: p.last_outbound_at,
        total_inbound: p.total_inbound,
        total_outbound: p.total_outbound,
        replied_count: p.replied_count,
        cadence_days_p50: p.cadence_days_p50,
        is_list_sender: p.is_list_sender,
        list_id: p.list_id,
        open_thread_count: p.open_thread_count,
        inbound_storage_bytes: p.inbound_storage_bytes,
        outbound_storage_bytes: p.outbound_storage_bytes,
        attachment_count: p.attachment_count,
        attachment_bytes: p.attachment_bytes,
    });
    Ok(ResponseData::SenderProfile { profile: data })
}
