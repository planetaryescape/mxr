use super::HandlerResult;
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_protocol::{
    CommitmentData, CommitmentDirectionData, CommitmentStatusData, ContactRelationshipSummaryData,
    ContactStyleData, RelationshipProfileData, ResponseData,
};
use mxr_store::{CommitmentDirection, CommitmentStatus, ContactCommitmentRecord, Store};

pub(super) async fn get_relationship_profile(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
) -> HandlerResult {
    Ok(ResponseData::RelationshipProfile {
        profile: load_relationship_profile(state, account_id, email).await?,
    })
}

pub(super) async fn rebuild_relationship_profile(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
) -> HandlerResult {
    state
        .relationship
        .rebuild_contact(account_id.clone(), email.to_string())
        .await
        .map_err(|error| error.to_string())?;
    get_relationship_profile(state, account_id, email).await
}

pub(crate) async fn load_relationship_profile(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
) -> Result<Option<RelationshipProfileData>, String> {
    load_relationship_profile_for_store(&state.store, account_id, email).await
}

pub(crate) async fn load_relationship_profile_for_store(
    store: &Store,
    account_id: &AccountId,
    email: &str,
) -> Result<Option<RelationshipProfileData>, String> {
    let style = store
        .get_contact_style(account_id, email)
        .await
        .map_err(|error| error.to_string())?
        .map(|style| ContactStyleData {
            formality_score: style.formality_score,
            formality_score_theirs: style.formality_score_theirs,
            avg_sentence_len: style.avg_sentence_len,
            avg_sentence_len_theirs: style.avg_sentence_len_theirs,
            msg_count_used: style.msg_count_used,
            msg_count_used_theirs: style.msg_count_used_theirs,
            computed_at: style.computed_at,
            source_hash: style.source_hash,
        });
    let summary = store
        .get_contact_relationship_summary(account_id, email)
        .await
        .map_err(|error| error.to_string())?
        .map(|summary| ContactRelationshipSummaryData {
            text: summary.text,
            model: summary.model,
            known_topics: summary.known_topics,
            computed_at: summary.computed_at,
            source_hash: summary.source_hash,
        });
    let commitments: Vec<CommitmentData> = store
        .list_contact_commitments(account_id, Some(email), Some(CommitmentStatus::Open))
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(commitment_data)
        .collect();
    if style.is_none() && summary.is_none() && commitments.is_empty() {
        return Ok(None);
    }
    Ok(Some(RelationshipProfileData {
        account_id: account_id.clone(),
        email: email.to_ascii_lowercase(),
        style,
        summary,
        open_commitments: commitments,
    }))
}

pub(crate) fn commitment_data(record: ContactCommitmentRecord) -> CommitmentData {
    CommitmentData {
        id: record.id,
        account_id: record.account_id,
        email: record.email,
        thread_id: record.thread_id,
        direction: match record.direction {
            CommitmentDirection::Yours => CommitmentDirectionData::Yours,
            CommitmentDirection::Theirs => CommitmentDirectionData::Theirs,
        },
        status: match record.status {
            CommitmentStatus::Open => CommitmentStatusData::Open,
            CommitmentStatus::Resolved => CommitmentStatusData::Resolved,
            CommitmentStatus::Expired => CommitmentStatusData::Expired,
        },
        who_owes: record.who_owes,
        what: record.what,
        by_when: record.by_when,
        evidence_msg_id: record.evidence_msg_id,
        extracted_at: record.extracted_at,
    }
}
