use super::HandlerResult;
use crate::handler::relationship_profile::commitment_data;
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_protocol::{CommitmentStatusData, ResponseData};
use mxr_store::CommitmentStatus;

pub(super) async fn list_commitments(
    state: &AppState,
    account_id: &AccountId,
    email: Option<&str>,
    status: Option<CommitmentStatusData>,
) -> HandlerResult {
    let status = status.map(|status| match status {
        CommitmentStatusData::Open => CommitmentStatus::Open,
        CommitmentStatusData::Resolved => CommitmentStatus::Resolved,
        CommitmentStatusData::Expired => CommitmentStatus::Expired,
    });
    let commitments = state
        .store
        .list_contact_commitments(account_id, email, status)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(commitment_data)
        .collect();
    Ok(ResponseData::CommitmentList { commitments })
}

pub(super) async fn resolve_commitment(state: &AppState, commitment_id: &str) -> HandlerResult {
    state
        .store
        .resolve_contact_commitment(commitment_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(ResponseData::Ack)
}
