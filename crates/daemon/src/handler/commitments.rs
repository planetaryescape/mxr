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

/// Shared by the direct command and Desk Done, including Archive filing.
pub(super) async fn resolve_promise(
    state: &AppState,
    account: &AccountId,
    id: &str,
) -> Result<Option<mxr_store::PromiseResolution>, super::HandlerError> {
    let resolution = state
        .store
        .resolve_promise_with_todos(account, id, chrono::Utc::now())
        .await?;
    if let Some(resolution) = &resolution {
        let changed: Vec<_> = resolution
            .todos
            .iter()
            .map(|(_, after)| after.clone())
            .collect();
        super::records::file_ticked_todos(state, &changed).await;
    }
    Ok(resolution)
}

pub(super) async fn resolve_commitment(state: &AppState, commitment_id: &str) -> HandlerResult {
    if let Some(commitment) = state.store.get_contact_commitment(commitment_id).await? {
        resolve_promise(state, &commitment.account_id, commitment_id).await?;
    }
    Ok(ResponseData::Ack)
}
