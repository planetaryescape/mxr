//! Reviewed Archive receipts in the existing mutation job store.
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::{Envelope, Mutation, MutationFailure};
use mxr_protocol::{
    ArchiveIdentityData, ArchiveItemData, ArchiveItemStateData as ItemState, ArchiveOperationData,
    ArchiveWorkspaceData, MutationCommand, ResponseData,
};
use std::sync::Arc;

mod runner;

const KIND: &str = "recoverable_archive";

fn failure(
    identity: &ArchiveIdentityData,
    code: &'static str,
    message: impl Into<String>,
) -> HandlerError {
    HandlerError::Archive {
        identity: Box::new(identity.clone()),
        kind: mxr_protocol::IpcErrorKind::InvalidRequest,
        code,
        message: message.into(),
        state: "refused",
    }
}

pub(super) fn with_identity_error(
    identity: &ArchiveIdentityData,
    error: HandlerError,
) -> HandlerError {
    match error {
        existing @ HandlerError::Archive { .. } => existing,
        other => HandlerError::Archive {
            identity: Box::new(identity.clone()),
            kind: mxr_protocol::IpcErrorKind::Store,
            code: "archive_persistence_error",
            message: other.to_string(),
            state: "receipt_unavailable",
        },
    }
}

fn job_key(identity: &ArchiveIdentityData) -> String {
    format!("{KIND}:{}:{}", identity.caller_id, identity.operation_id)
}

pub(super) async fn workspace(state: &AppState) -> HandlerResult {
    let (workspace_id, recovery_generation) = state.store.archive_workspace().await?;
    Ok(ResponseData::ArchiveWorkspace {
        workspace: ArchiveWorkspaceData {
            workspace_id,
            recovery_generation,
        },
    })
}

async fn validate_identity(
    state: &AppState,
    identity: &ArchiveIdentityData,
) -> Result<(), HandlerError> {
    for id in [
        &identity.operation_id,
        &identity.caller_id,
        &identity.workspace.workspace_id,
        &identity.workspace.recovery_generation,
    ] {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err(failure(
                identity,
                "archive_invalid_identity",
                "Archive identity fields must be UUIDs",
            ));
        }
    }
    let current = state.store.read_archive_workspace().await?;
    if current
        .as_ref()
        .map(|(workspace, generation)| (workspace.as_str(), generation.as_str()))
        != Some((
            identity.workspace.workspace_id.as_str(),
            identity.workspace.recovery_generation.as_str(),
        ))
    {
        return Err(failure(identity, "archive_workspace_conflict", "Selected daemon workspace or recovery generation changed; use the original workspace to recover"));
    }
    Ok(())
}

fn canonical_ids(
    identity: &ArchiveIdentityData,
    mutation: &MutationCommand,
) -> Result<Vec<mxr_core::MessageId>, HandlerError> {
    let MutationCommand::Archive { message_ids } = mutation else {
        return Err(failure(
            identity,
            "archive_unsupported_command",
            "Durable recovery supports Archive only",
        ));
    };
    let mut ids = message_ids.clone();
    ids.sort_by_key(mxr_core::MessageId::as_str);
    ids.dedup();
    if ids.is_empty() {
        return Err(failure(
            identity,
            "archive_empty_selection",
            "Archive requires a nonempty reviewed selection",
        ));
    }
    Ok(ids)
}

async fn load(
    state: &AppState,
    identity: &ArchiveIdentityData,
) -> Result<(String, ArchiveOperationData), HandlerError> {
    let json = state
        .store
        .get_mutation_job(&job_key(identity))
        .await?
        .ok_or_else(|| {
            failure(
                identity,
                "archive_not_found",
                "No retained Archive review or acceptance for this identity",
            )
        })?;
    let operation: ArchiveOperationData = serde_json::from_str(&json)?;
    if operation.identity != *identity {
        return Err(failure(
            identity,
            "archive_identity_conflict",
            "Archive identity differs from the retained review",
        ));
    }
    Ok((json, operation))
}

fn check_selection(
    identity: &ArchiveIdentityData,
    operation: &ArchiveOperationData,
    ids: &[mxr_core::MessageId],
) -> Result<(), HandlerError> {
    if operation
        .items
        .iter()
        .map(|item| &item.message_id)
        .ne(ids.iter())
    {
        return Err(failure(
            identity,
            "archive_payload_conflict",
            "Archive selection differs from the immutable review",
        ));
    }
    Ok(())
}

fn binding(envelope: &Envelope, item: &ArchiveItemData) -> bool {
    envelope.account_id == item.account_id
        && envelope.provider_id == item.provider_id
        && envelope
            .label_provider_ids
            .iter()
            .any(|label| label.eq_ignore_ascii_case("INBOX"))
            == item.was_inbox
}

pub(super) async fn preview(
    state: &AppState,
    identity: &ArchiveIdentityData,
    mutation: &MutationCommand,
) -> HandlerResult {
    validate_identity(state, identity).await?;
    let ids = canonical_ids(identity, mutation)?;
    let key = job_key(identity);
    if state.store.get_mutation_job(&key).await?.is_some() {
        let (_, operation) = load(state, identity).await?;
        check_selection(identity, &operation, &ids)?;
        return Ok(ResponseData::ArchiveOperation { operation });
    }
    let mut items = Vec::with_capacity(ids.len());
    for message_id in ids.iter() {
        let envelope = state.store.get_envelope(message_id).await?.ok_or_else(|| {
            failure(
                identity,
                "archive_message_missing",
                format!("Message {message_id} no longer exists; review a new selection"),
            )
        })?;
        items.push(ArchiveItemData {
            message_id: message_id.clone(),
            account_id: envelope.account_id,
            provider_id: envelope.provider_id,
            prior_flags_bits: envelope.flags.bits(),
            prior_label_provider_ids: envelope.label_provider_ids.clone(),
            was_inbox: envelope
                .label_provider_ids
                .iter()
                .any(|label| label.eq_ignore_ascii_case("INBOX")),
            state: ItemState::Pending,
            reason: None,
        });
    }
    let operation = ArchiveOperationData {
        identity: identity.clone(),
        review_id: uuid::Uuid::now_v7().to_string(),
        mutation_id: uuid::Uuid::now_v7().to_string(),
        accepted: false,
        undo_available: false,
        running: false,
        started_at: chrono::Utc::now().timestamp_millis(),
        items,
    };
    state
        .store
        .insert_archive_job(
            &key,
            operation.started_at,
            &serde_json::to_string(&operation)?,
        )
        .await?;
    let (_, operation) = load(state, identity).await?;
    check_selection(identity, &operation, &ids)?;
    Ok(ResponseData::ArchiveOperation { operation })
}

pub(super) async fn get(state: &AppState, identity: &ArchiveIdentityData) -> HandlerResult {
    validate_identity(state, identity).await?;
    let (_, mut operation) = load(state, identity).await?;
    operation.running = state.archive_runners.lock().contains(&job_key(identity));
    operation.undo_available = state
        .store
        .read_undo_entry(&operation.mutation_id)
        .await?
        .is_some_and(|entry| entry.expires_at >= chrono::Utc::now().timestamp());
    Ok(ResponseData::ArchiveOperation { operation })
}

pub(super) async fn submit(
    state: Arc<AppState>,
    source: mxr_protocol::ClientKind,
    identity: &ArchiveIdentityData,
    review_id: &str,
    mutation: &MutationCommand,
) -> HandlerResult {
    validate_identity(&state, identity).await?;
    let ids = canonical_ids(identity, mutation)?;
    let operation = loop {
        let (json, mut operation) = load(&state, identity).await?;
        check_selection(identity, &operation, &ids)?;
        if operation.review_id != review_id {
            return Err(failure(
                identity,
                "archive_review_conflict",
                "Archive review token differs from the retained review",
            ));
        }
        if operation.accepted {
            break operation;
        }
        operation.accepted = true;
        if state
            .store
            .compare_exchange_archive_job(
                &job_key(identity),
                &json,
                &serde_json::to_string(&operation)?,
            )
            .await?
        {
            // Acceptance records user intent once; item receipts describe provider outcomes.
            state.activity.record(crate::activity::OwnedEntry {
                ts: crate::activity::current_unix_ms(),
                account_id: None,
                source,
                action: "mail.archive".into(),
                target_kind: Some("message".into()),
                target_id: operation.items.first().map(|item| item.message_id.as_str().clone()),
                tier: crate::activity::tier::tier_for("mail.archive"),
                context: Some(serde_json::json!({
                    "count": operation.items.len(),
                    "target_ids": operation.items.iter().map(|item| item.message_id.as_str()).collect::<Vec<_>>(),
                    "operation_id": identity.operation_id,
                    "phase": "accepted",
                })),
            });
            break operation;
        }
    };
    if !operation
        .items
        .iter()
        .any(|item| item.state == ItemState::Pending)
    {
        return get(&state, identity).await;
    }
    let mut operation = operation;
    if !state.archive_runners.lock().insert(job_key(identity)) {
        operation.running = true;
        return Ok(ResponseData::ArchiveOperation { operation });
    }
    operation.running = true;
    let identity = identity.clone();
    tokio::spawn(async move {
        if let Err(error) = runner::run(&state, &identity).await {
            tracing::warn!(operation_id = %identity.operation_id, %error, "Archive receipt processing stopped; query retained state before retry");
        }
        state.archive_runners.lock().remove(&job_key(&identity));
    });
    Ok(ResponseData::ArchiveOperation { operation })
}
