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

async fn assert_completed_history(state: &AppState, operation: &ArchiveOperationData) {
    let entries = state
        .store
        .list_events(100, None, Some("mutation"))
        .await
        .unwrap();
    for item in &operation.items {
        let matching = entries
            .iter()
            .filter(|event| event.message_id.as_deref() == Some(item.message_id.as_str().as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            matching.len(),
            usize::from(item.state == ItemState::Completed)
        );
        if let Some(event) = matching.first() {
            let envelope = state
                .store
                .get_envelope(&item.message_id)
                .await
                .unwrap()
                .unwrap();
            let (summary, details) =
                super::super::mutations::mutation_log_entry(&mutation(operation), &envelope);
            assert_eq!(event.summary, summary);
            assert_eq!(event.details, details);
            assert_eq!(event.account_id.as_ref(), Some(&item.account_id));
        }
    }
}

#[tokio::test]
async fn archive_lost_acceptance_and_completion_replies_concurrent_replays_never_repeat() {
    let (state, provider, operation) = fixture().await;
    let (one, two) = tokio::join!(submit(&state, &operation), submit(&state, &operation));
    assert!(one.is_ok() && two.is_ok());
    let outcome = wait(&state, &operation).await;
    assert!(outcome
        .items
        .iter()
        .all(|item| item.state == ItemState::Completed));
    assert_eq!(provider.mutations().len(), 2);
    submit(&state, &operation).await.unwrap();
    let replay = wait(&state, &operation).await;
    assert_eq!(replay.items, outcome.items);
    assert_eq!(provider.mutations().len(), 2);
    assert_eq!(replay.mutation_id, operation.mutation_id);
    assert_completed_history(&state, &replay).await;
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
    assert_completed_history(&state, &outcome).await;
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

mod completion;
