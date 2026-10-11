use super::*;

async fn transition(
    state: &AppState,
    identity: &ArchiveIdentityData,
    id: &mxr_core::MessageId,
    expected: ItemState,
    next: ItemState,
    reason: Option<String>,
) -> Result<bool, HandlerError> {
    transition_with_history(state, identity, id, expected, next, reason, None).await
}

async fn transition_with_history(
    state: &AppState,
    identity: &ArchiveIdentityData,
    id: &mxr_core::MessageId,
    expected: ItemState,
    next: ItemState,
    reason: Option<String>,
    completed_envelope: Option<&Envelope>,
) -> Result<bool, HandlerError> {
    loop {
        let (json, mut operation) = load(state, identity).await?;
        let Some(item) = operation
            .items
            .iter_mut()
            .find(|item| item.message_id == *id)
        else {
            return Ok(false);
        };
        if item.state != expected {
            return Ok(false);
        }
        item.state = next;
        item.reason.clone_from(&reason);
        let next_json = serde_json::to_string(&operation)?;
        let won = if let Some(envelope) = completed_envelope {
            let (summary, details) = super::super::mutations::mutation_log_entry(
                &MutationCommand::Archive {
                    message_ids: vec![id.clone()],
                },
                envelope,
            );
            let message_id = envelope.id.as_str();
            state
                .store
                .compare_exchange_archive_job_with_event(
                    &job_key(identity),
                    &json,
                    &next_json,
                    &summary,
                    mxr_store::EventLogRefs {
                        account_id: Some(&envelope.account_id),
                        message_id: Some(&message_id),
                        rule_id: None,
                    },
                    details.as_deref(),
                )
                .await?
        } else {
            state
                .store
                .compare_exchange_archive_job(&job_key(identity), &json, &next_json)
                .await?
        };
        if won {
            return Ok(true);
        }
    }
}

async fn dispatch_intent(
    state: &AppState,
    identity: &ArchiveIdentityData,
    item: &ArchiveItemData,
    current: &Envelope,
) -> Result<bool, HandlerError> {
    loop {
        let (json, mut operation) = load(state, identity).await?;
        let Some(retained) = operation
            .items
            .iter_mut()
            .find(|entry| entry.message_id == item.message_id)
        else {
            return Ok(false);
        };
        if retained.state != ItemState::Pending {
            return Ok(false);
        }
        retained.state = ItemState::Unknown;
        retained.reason = Some("Dispatch intent retained; provider completion is unproved. Query this operation; do not submit a new one".into());
        // Undo must restore the labels present at dispatch, including changes made after review.
        retained.prior_flags_bits = current.flags.bits();
        retained
            .prior_label_provider_ids
            .clone_from(&current.label_provider_ids);
        if state
            .store
            .compare_exchange_archive_job(
                &job_key(identity),
                &json,
                &serde_json::to_string(&operation)?,
            )
            .await?
        {
            return Ok(true);
        }
    }
}

pub(in crate::handler) async fn run(
    state: &AppState,
    identity: &ArchiveIdentityData,
) -> Result<u32, HandlerError> {
    let (_, operation) = load(state, identity).await?;
    // A runner started after completion must not restore an Undo entry the
    // user has already consumed.
    if !operation
        .items
        .iter()
        .any(|item| item.state == ItemState::Pending)
    {
        return Ok(0);
    }
    let mut completed = 0;
    for item in operation.items {
        if item.state != ItemState::Pending {
            continue;
        }
        let _guard = state.acquire_provider_operation(&item.account_id).await;
        validate_identity(state, identity).await?;
        // Check again under the same guard as dispatch: two concurrent workers
        // may have read Pending, but only the durable CAS winner calls the provider.
        let current = state.store.get_envelope(&item.message_id).await?;
        if !current.as_ref().is_some_and(|env| binding(env, &item)) {
            transition(state, identity, &item.message_id, ItemState::Pending, ItemState::NotExecuted,
                Some("Review is stale: message ownership, provider identity or Inbox membership changed; review again".into())).await?;
            continue;
        }
        let Some(provider) = state.sync_provider_for_account(&item.account_id) else {
            transition(
                state,
                identity,
                &item.message_id,
                ItemState::Pending,
                ItemState::NotExecuted,
                Some(
                    "No provider configured for the reviewed account; no effect was dispatched"
                        .into(),
                ),
            )
            .await?;
            continue;
        };
        let Some(current) = current else {
            continue;
        };
        if !item.was_inbox {
            transition_with_history(
                state,
                identity,
                &item.message_id,
                ItemState::Pending,
                ItemState::Completed,
                Some("Already outside Inbox at review; no provider effect needed".into()),
                Some(&current),
            )
            .await?;
            continue;
        }
        if !dispatch_intent(state, identity, &item, &current).await? {
            continue;
        }
        let mutation = Mutation::ModifyLabels {
            provider_message_id: item.provider_id.clone(),
            add: vec![],
            remove: vec!["INBOX".into()],
        };
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            provider.apply_mutation_classified(&operation.mutation_id, &mutation),
        )
        .await;
        match result {
            Ok(Ok(())) => {
                // Receipt first. Failure here leaves durable Unknown, never permission to repeat.
                transition(
                    state,
                    identity,
                    &item.message_id,
                    ItemState::Unknown,
                    ItemState::ProviderSucceeded,
                    Some("Provider completed Archive; local reconciliation remains".into()),
                )
                .await?;
                match crate::handler::reconcile_label_mutation(
                    state,
                    provider.as_ref(),
                    &item.message_id,
                    &[],
                    &["INBOX".into()],
                )
                .await
                {
                    Ok(()) => {
                        if transition_with_history(
                            state,
                            identity,
                            &item.message_id,
                            ItemState::ProviderSucceeded,
                            ItemState::Completed,
                            None,
                            Some(&current),
                        )
                        .await?
                        {
                            completed += 1;
                        }
                        if let Err(error) = crate::handler::mutations::reindex_messages_in_search(
                            state,
                            std::slice::from_ref(&item.message_id),
                        )
                        .await
                        {
                            tracing::warn!(%error, "Archive completed but search refresh failed");
                        }
                    }
                    Err(error) => {
                        transition(state, identity, &item.message_id, ItemState::ProviderSucceeded, ItemState::ProviderSucceeded, Some(format!("Provider completed Archive; local reconciliation failed: {error}. Do not repeat the provider effect"))).await?;
                    }
                }
            }
            Ok(Err(MutationFailure::NotAccepted(error))) => {
                transition(
                    state,
                    identity,
                    &item.message_id,
                    ItemState::Unknown,
                    ItemState::NotExecuted,
                    Some(format!("Provider proved non-acceptance: {error}")),
                )
                .await?;
            }
            Ok(Err(MutationFailure::Unknown(error))) => {
                transition(state, identity, &item.message_id, ItemState::Unknown, ItemState::Unknown, Some(format!("Provider outcome unknown: {error}. Reconciliation required; never resubmit"))).await?;
            }
            Err(_) => {
                transition(state, identity, &item.message_id, ItemState::Unknown, ItemState::Unknown, Some("Provider timed out; outcome unknown. Reconciliation required; never resubmit".into())).await?;
            }
        }
    }
    let (_, final_operation) = load(state, identity).await?;
    let snapshots = final_operation
        .items
        .iter()
        .filter(|item| {
            item.was_inbox
                && matches!(
                    item.state,
                    ItemState::Completed | ItemState::ProviderSucceeded | ItemState::Unknown
                )
        })
        .map(|item| mxr_store::UndoEntrySnapshot {
            message_id: item.message_id.clone(),
            account_id: item.account_id.clone(),
            provider_id: item.provider_id.clone(),
            prior_flags_bits: item.prior_flags_bits,
            prior_label_provider_ids: item.prior_label_provider_ids.clone(),
            uncertain: item.state != ItemState::Completed,
        })
        .collect::<Vec<_>>();
    if !snapshots.is_empty()
        && state
            .store
            .read_undo_entry(&final_operation.mutation_id)
            .await?
            .is_none()
    {
        let now = chrono::Utc::now().timestamp();
        let entry = mxr_store::UndoEntry {
            mutation_id: final_operation.mutation_id,
            kind: mxr_store::UndoableMutationKind::Archive,
            snapshots,
            desk: None,
            applied_at: now,
            expires_at: now + crate::handler::mutations::UNDO_WINDOW_SECS,
        };
        if let Err(error) = state.store.write_undo_entry(&entry).await {
            tracing::warn!(%error, "Archive provider outcomes retained but undo unavailable");
        }
    }
    Ok(completed)
}
