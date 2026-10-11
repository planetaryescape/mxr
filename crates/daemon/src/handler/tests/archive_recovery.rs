use super::*;
use mxr_protocol::{ArchiveIdentityData, ArchiveItemStateData as ItemState, ArchiveOperationData};

async fn fixture() -> (
    Arc<AppState>,
    Arc<mxr_provider_fake::FakeProvider>,
    ArchiveOperationData,
) {
    let (state, provider) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    sync_and_get_first_id(&state).await;
    let ids = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 100, 0)
        .await
        .unwrap()
        .into_iter()
        .filter(|env| env.label_provider_ids.iter().any(|label| label == "INBOX"))
        .take(2)
        .map(|env| env.id)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    let ResponseData::ArchiveWorkspace { workspace } =
        super::super::archive_recovery::workspace(&state)
            .await
            .unwrap()
    else {
        panic!("workspace");
    };
    let identity = ArchiveIdentityData {
        workspace,
        caller_id: uuid::Uuid::now_v7().to_string(),
        operation_id: uuid::Uuid::now_v7().to_string(),
    };
    let ResponseData::ArchiveOperation { operation } = super::super::archive_recovery::preview(
        &state,
        &identity,
        &MutationCommand::Archive { message_ids: ids },
    )
    .await
    .unwrap() else {
        panic!("preview");
    };
    assert!(!operation.accepted);
    (state, provider, operation)
}
fn mutation(operation: &ArchiveOperationData) -> MutationCommand {
    MutationCommand::Archive {
        message_ids: operation
            .items
            .iter()
            .map(|item| item.message_id.clone())
            .collect(),
    }
}
async fn submit(state: &Arc<AppState>, operation: &ArchiveOperationData) -> HandlerResult {
    super::super::archive_recovery::submit(
        state.clone(),
        mxr_protocol::ClientKind::Cli,
        &operation.identity,
        &operation.review_id,
        &mutation(operation),
    )
    .await
}
async fn query(state: &AppState, operation: &ArchiveOperationData) -> ArchiveOperationData {
    let ResponseData::ArchiveOperation { operation } =
        super::super::archive_recovery::get(state, &operation.identity)
            .await
            .unwrap()
    else {
        panic!("query");
    };
    operation
}
async fn wait(state: &AppState, operation: &ArchiveOperationData) -> ArchiveOperationData {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let outcome = query(state, operation).await;
            if !outcome.running {
                return outcome;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}
fn key(operation: &ArchiveOperationData) -> String {
    format!(
        "recoverable_archive:{}:{}",
        operation.identity.caller_id, operation.identity.operation_id
    )
}
async fn overwrite(state: &AppState, operation: &ArchiveOperationData) {
    let key = key(operation);
    let expected = state.store.get_mutation_job(&key).await.unwrap().unwrap();
    assert!(state
        .store
        .compare_exchange_archive_job(&key, &expected, &serde_json::to_string(operation).unwrap())
        .await
        .unwrap());
}

#[tokio::test]
async fn archive_lost_acceptance_and_completion_replies_concurrent_replays_never_repeat() {
    let (state, provider, operation) = fixture().await;
    let (one, two) = tokio::join!(submit(&state, &operation), submit(&state, &operation));
    assert!(one.is_ok() && two.is_ok()); // Drop both acknowledgements.
    let outcome = wait(&state, &operation).await;
    assert!(outcome
        .items
        .iter()
        .all(|item| item.state == ItemState::Completed));
    assert_eq!(provider.mutations().len(), 2);
    submit(&state, &operation).await.unwrap(); // Completion response was lost too.
    let replay = wait(&state, &operation).await;
    assert_eq!(replay.items, outcome.items);
    assert_eq!(provider.mutations().len(), 2);
    assert_eq!(replay.mutation_id, operation.mutation_id);
}

#[tokio::test]
async fn archive_payload_workspace_generation_and_caller_conflicts_are_refused_without_effects() {
    let (state, provider, operation) = fixture().await;
    let mut changed = operation.clone();
    changed.items.pop();
    assert!(submit(&state, &changed).await.is_err());
    assert!(super::super::archive_recovery::submit(
        state.clone(),
        mxr_protocol::ClientKind::Cli,
        &operation.identity,
        &operation.review_id,
        &MutationCommand::Trash {
            message_ids: operation
                .items
                .iter()
                .map(|i| i.message_id.clone())
                .collect()
        }
    )
    .await
    .is_err());
    for change in 0..3 {
        let mut changed = operation.clone();
        match change {
            0 => changed.identity.workspace.workspace_id = uuid::Uuid::now_v7().to_string(),
            1 => changed.identity.workspace.recovery_generation = uuid::Uuid::now_v7().to_string(),
            _ => changed.identity.caller_id = uuid::Uuid::now_v7().to_string(),
        }
        assert!(submit(&state, &changed).await.is_err());
    }
    assert_eq!(provider.mutations().len(), 0);
    state.store.rotate_archive_generation().await.unwrap();
    assert!(
        super::super::archive_recovery::get(&state, &operation.identity)
            .await
            .is_err()
    );
    assert_eq!(provider.mutations().len(), 0);
}

#[tokio::test]
async fn archive_stale_item_is_not_dispatched_but_harmless_flags_do_not_invalidate_review() {
    let (state, provider, operation) = fixture().await;
    let mut first = state
        .store
        .get_envelope(&operation.items[0].message_id)
        .await
        .unwrap()
        .unwrap();
    first.provider_id = "changed-provider-ownership".into();
    sqlx::query("UPDATE messages SET provider_id = ? WHERE id = ?")
        .bind(&first.provider_id)
        .bind(first.id.as_str())
        .execute(state.store.writer())
        .await
        .unwrap();
    state
        .store
        .set_read(
            &operation.items[1].message_id,
            true,
            mxr_core::EventSource::User,
        )
        .await
        .unwrap();
    submit(&state, &operation).await.unwrap();
    let outcome = wait(&state, &operation).await;
    assert_eq!(outcome.items[0].state, ItemState::NotExecuted);
    assert_eq!(outcome.items[1].state, ItemState::Completed);
    assert_eq!(provider.mutations().len(), 1);
}

#[tokio::test]
async fn archive_provider_acceptance_response_loss_stays_unknown_after_replay() {
    let (state, provider, operation) = fixture().await;
    provider.fail_mutations_after_accepting(true);
    submit(&state, &operation).await.unwrap();
    let outcome = wait(&state, &operation).await;
    assert!(outcome
        .items
        .iter()
        .all(|item| item.state == ItemState::Unknown));
    assert_eq!(provider.mutations().len(), 2);
    provider.fail_mutations_after_accepting(false);
    submit(&state, &operation).await.unwrap();
    assert!(wait(&state, &operation)
        .await
        .items
        .iter()
        .all(|item| item.state == ItemState::Unknown));
    assert_eq!(provider.mutations().len(), 2);
}

#[tokio::test]
async fn archive_partial_batch_known_rejection_is_separate_from_uncertainty() {
    let (state, provider, operation) = fixture().await;
    provider.fail_mutations_of(Some(&operation.items[0].provider_id));
    submit(&state, &operation).await.unwrap();
    let outcome = wait(&state, &operation).await;
    assert_eq!(outcome.items[0].state, ItemState::NotExecuted);
    assert_eq!(outcome.items[1].state, ItemState::Completed);
    assert_eq!(provider.mutations().len(), 1);
    provider.fail_mutations_of(None);
    submit(&state, &operation).await.unwrap();
    wait(&state, &operation).await;
    assert_eq!(provider.mutations().len(), 1);
}

#[tokio::test]
async fn archive_acceptance_and_dispatch_persistence_failures_prevent_effects() {
    for boundary in ["$.accepted", "$.items[0].state"] {
        let (state, provider, operation) = fixture().await;
        let sql = if boundary == "$.accepted" {
            "CREATE TRIGGER archive_failure BEFORE UPDATE ON mutation_jobs WHEN json_extract(NEW.data_json, '$.accepted') = 1 BEGIN SELECT RAISE(FAIL, 'injected persistence failure'); END"
        } else {
            "CREATE TRIGGER archive_failure BEFORE UPDATE ON mutation_jobs WHEN json_extract(NEW.data_json, '$.items[0].state') = 'unknown' BEGIN SELECT RAISE(FAIL, 'injected persistence failure'); END"
        };
        sqlx::query(sql)
            .execute(state.store.writer())
            .await
            .unwrap();
        let result = submit(&state, &operation).await;
        if boundary == "$.accepted" {
            assert!(result.is_err());
        } else {
            result.unwrap();
        }
        let outcome = wait(&state, &operation).await;
        assert!(outcome
            .items
            .iter()
            .all(|item| item.state == ItemState::Pending));
        assert_eq!(provider.mutations().len(), 0);
    }
}

#[tokio::test]
async fn archive_failed_success_receipt_never_repeats_accepted_provider_effect() {
    let (state, provider, operation) = fixture().await;
    sqlx::query("CREATE TRIGGER archive_receipt_failure BEFORE UPDATE ON mutation_jobs WHEN json_extract(NEW.data_json, '$.items[0].state') = 'provider_succeeded' BEGIN SELECT RAISE(FAIL, 'injected receipt failure'); END")
        .execute(state.store.writer()).await.unwrap();
    submit(&state, &operation).await.unwrap();
    let outcome = wait(&state, &operation).await;
    assert_eq!(outcome.items[0].state, ItemState::Unknown);
    assert_eq!(outcome.items[1].state, ItemState::Pending);
    assert_eq!(provider.mutations().len(), 1);
    sqlx::query("DROP TRIGGER archive_receipt_failure")
        .execute(state.store.writer())
        .await
        .unwrap();
    submit(&state, &operation).await.unwrap();
    let outcome = wait(&state, &operation).await;
    assert_eq!(outcome.items[0].state, ItemState::Unknown);
    assert_eq!(outcome.items[1].state, ItemState::Completed);
    assert_eq!(provider.mutations().len(), 2);
}

#[tokio::test]
async fn archive_interrupted_dispatch_is_unknown_and_query_never_resumes_pending_work() {
    let (state, provider, mut operation) = fixture().await;
    operation.accepted = true;
    operation.items[0].state = ItemState::Unknown;
    overwrite(&state, &operation).await; // Crash after dispatch claim; second item never claimed.
    let queried = query(&state, &operation).await;
    assert_eq!(queried.items[0].state, ItemState::Unknown);
    assert_eq!(queried.items[1].state, ItemState::Pending);
    assert_eq!(provider.mutations().len(), 0);
    submit(&state, &operation).await.unwrap();
    let outcome = wait(&state, &operation).await;
    assert_eq!(outcome.items[0].state, ItemState::Unknown);
    assert_eq!(outcome.items[1].state, ItemState::Completed);
    assert_eq!(provider.mutations().len(), 1);
    state.store.prune_mutation_jobs(0).await.unwrap();
    state
        .store
        .prune_expired_mutation_dedup(chrono::Utc::now().timestamp() + 100_000)
        .await
        .unwrap();
    submit(&state, &operation).await.unwrap();
    wait(&state, &operation).await;
    assert_eq!(provider.mutations().len(), 1);
}

#[tokio::test]
async fn archive_known_provider_success_failed_local_reconciliation_is_not_retryable() {
    let (state, provider, operation) = fixture().await;
    sqlx::query("CREATE TRIGGER archive_local_failure BEFORE DELETE ON message_labels BEGIN SELECT RAISE(FAIL, 'injected local persistence failure'); END")
        .execute(state.store.writer()).await.unwrap();
    submit(&state, &operation).await.unwrap();
    let outcome = wait(&state, &operation).await;
    assert!(outcome
        .items
        .iter()
        .all(|item| item.state == ItemState::ProviderSucceeded));
    assert_eq!(provider.mutations().len(), 2);
    submit(&state, &operation).await.unwrap();
    wait(&state, &operation).await;
    assert_eq!(provider.mutations().len(), 2);
}

#[tokio::test]
async fn archive_undo_snapshot_keeps_labels_added_after_preview() {
    let (state, _, operation) = fixture().await;
    let id = &operation.items[0].message_id;
    let mut envelope = state.store.get_envelope(id).await.unwrap().unwrap();
    envelope
        .label_provider_ids
        .push("intervening-user-label".into());
    let label = mxr_core::Label {
        id: mxr_core::LabelId::new(),
        account_id: envelope.account_id.clone(),
        name: "Intervening".into(),
        provider_id: "intervening-user-label".into(),
        kind: mxr_core::LabelKind::User,
        color: None,
        unread_count: 0,
        total_count: 0,
        role: None,
    };
    state.store.upsert_label(&label).await.unwrap();
    state
        .store
        .add_message_label(id, &label.id, mxr_core::EventSource::User)
        .await
        .unwrap();
    assert!(state
        .store
        .get_envelope(id)
        .await
        .unwrap()
        .unwrap()
        .label_provider_ids
        .contains(&label.provider_id));
    submit(&state, &operation).await.unwrap();
    let outcome = wait(&state, &operation).await;
    assert!(outcome.items[0]
        .prior_label_provider_ids
        .contains(&"intervening-user-label".into()));
    let undo = state
        .store
        .read_undo_entry(&operation.mutation_id)
        .await
        .unwrap()
        .unwrap();
    assert!(undo
        .snapshots
        .iter()
        .find(|snapshot| &snapshot.message_id == id)
        .unwrap()
        .prior_label_provider_ids
        .contains(&"intervening-user-label".into()));
}

#[tokio::test]
async fn archive_terminal_resubmission_does_not_rearm_consumed_undo() {
    let (state, provider, operation) = fixture().await;
    submit(&state, &operation).await.unwrap();
    let completed = wait(&state, &operation).await;
    assert!(completed.undo_available);
    assert_eq!(provider.mutations().len(), 2);
    state
        .store
        .delete_undo_entry(&operation.mutation_id)
        .await
        .unwrap();
    submit(&state, &operation).await.unwrap();
    let repeated = wait(&state, &operation).await;
    assert!(!repeated.undo_available);
    assert!(state
        .store
        .read_undo_entry(&operation.mutation_id)
        .await
        .unwrap()
        .is_none());
    assert_eq!(provider.mutations().len(), 2);
}
