use super::HandlerResult;
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_llm::LlmFeature;
use mxr_protocol::{
    CommitmentData, CommitmentDirectionData, CommitmentStatusData, ContactRelationshipSummaryData,
    ContactStyleData, RelationshipDriftData, RelationshipProfileData, RelationshipRebuildSummary,
    ResponseData,
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

/// Rebuild every contact with mail in the window, backfilled mail
/// included, one contact at a time through the relationship worker so
/// background refreshes keep flowing. A dry run counts the contacts and the
/// model calls the real run would make, and writes nothing.
pub(super) async fn rebuild_relationship_profiles(
    state: &AppState,
    account_id: Option<&AccountId>,
    since_days: u32,
    dry_run: bool,
) -> HandlerResult {
    let since = chrono::Utc::now() - chrono::Duration::days(i64::from(since_days));
    let message_ids = state
        .store
        .list_message_ids_since(since, account_id)
        .await?;
    let contacts = state
        .store
        .relationship_contacts_for_messages(&message_ids)
        .await?;
    let config = state.config_snapshot();
    let model_usable = config.llm.enabled
        && state
            .llm
            .feature_block_reason(LlmFeature::RelationshipSummary)
            .is_none();
    let mut summary = RelationshipRebuildSummary {
        contacts: u32::try_from(contacts.len()).unwrap_or(u32::MAX),
        dry_run,
        ..Default::default()
    };
    for (account_id, email) in contacts {
        if dry_run {
            let sends = model_usable
                && mxr_relationship::summary::rebuild_would_send(&state.store, &account_id, &email)
                    .await
                    .map_err(|error| error.to_string())?;
            summary.model_calls += u32::from(sends);
            continue;
        }
        match state.relationship.rebuild_contact(account_id, email).await {
            Ok(sent) => summary.model_calls += u32::from(sent),
            Err(error) => {
                tracing::warn!(%error, "relationship rebuild failed for one contact");
                summary.errors += 1;
            }
        }
    }
    Ok(ResponseData::RelationshipRebuild { summary })
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
    let mut drift = None;
    let style = store
        .get_contact_style(account_id, email)
        .await
        .map_err(|error| error.to_string())?
        .map(|style| {
            if style.drift_detected {
                if let (Some(detected_at), Some(reason)) =
                    (style.drift_detected_at, style.drift_reason.clone())
                {
                    drift = Some(RelationshipDriftData {
                        detected_at,
                        reason,
                    });
                }
            }
            ContactStyleData {
                formality_score: style.formality_score,
                formality_score_theirs: style.formality_score_theirs,
                avg_sentence_len: style.avg_sentence_len,
                avg_sentence_len_theirs: style.avg_sentence_len_theirs,
                msg_count_used: style.msg_count_used,
                msg_count_used_theirs: style.msg_count_used_theirs,
                computed_at: style.computed_at,
                source_hash: style.source_hash,
            }
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
        drift,
    }))
}

pub(crate) fn commitment_data(record: ContactCommitmentRecord) -> CommitmentData {
    let date_unverified = record.date_unverified();
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
        date_unverified,
        evidence_msg_id: record.evidence_msg_id,
        extracted_at: record.extracted_at,
    }
}
