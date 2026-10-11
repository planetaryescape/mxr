use super::*;
use crate::handler::archive_recovery;

#[tokio::test]
async fn archive_history_failure_rolls_back_completion_without_repeating_provider() {
    let (state, provider, operation) = fixture().await;
    sqlx::query("CREATE TRIGGER archive_history_failure BEFORE INSERT ON event_log WHEN NEW.category = 'mutation' BEGIN SELECT RAISE(ABORT, 'injected history failure'); END")
        .execute(state.store.writer()).await.unwrap();
    submit(&state, &operation).await.unwrap();
    let failed = wait(&state, &operation).await;
    assert_eq!(failed.items[0].state, ItemState::ProviderSucceeded);
    assert_eq!(failed.items[1].state, ItemState::Pending);
    assert_completed_history(&state, &failed).await;
    assert_eq!(provider.mutations().len(), 1);
    sqlx::query("DROP TRIGGER archive_history_failure")
        .execute(state.store.writer())
        .await
        .unwrap();
    submit(&state, &operation).await.unwrap();
    let resumed = wait(&state, &operation).await;
    assert_eq!(resumed.items[0].state, ItemState::ProviderSucceeded);
    assert_eq!(resumed.items[1].state, ItemState::Completed);
    assert_completed_history(&state, &resumed).await;
    assert_eq!(provider.mutations().len(), 2);
}

#[tokio::test]
async fn archive_completion_batch_count_is_zero_on_replay() {
    let (state, provider, mut operation) = fixture().await;
    operation.accepted = true;
    overwrite(&state, &operation).await;
    assert_eq!(
        archive_recovery::run_for_test(&state, &operation.identity)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        archive_recovery::run_for_test(&state, &operation.identity)
            .await
            .unwrap(),
        0
    );
    assert_eq!(provider.mutations().len(), 2);
    assert_completed_history(&state, &query(&state, &operation).await).await;
}

#[tokio::test]
async fn archive_already_outside_inbox_records_success_without_provider_effect() {
    let (state, provider, reviewed) = fixture().await;
    for item in &reviewed.items {
        state
            .store
            .set_message_labels(&item.message_id, &[], mxr_core::EventSource::User)
            .await
            .unwrap();
    }
    let mut identity = reviewed.identity.clone();
    identity.operation_id = uuid::Uuid::now_v7().to_string();
    let ResponseData::ArchiveOperation { operation } =
        archive_recovery::preview(&state, &identity, &mutation(&reviewed))
            .await
            .unwrap()
    else {
        panic!("preview")
    };
    assert!(operation.items.iter().all(|item| !item.was_inbox));
    submit(&state, &operation).await.unwrap();
    let outcome = wait(&state, &operation).await;
    assert!(outcome
        .items
        .iter()
        .all(|item| item.state == ItemState::Completed));
    assert_completed_history(&state, &outcome).await;
    assert_eq!(provider.mutations().len(), 0);
}

#[tokio::test]
async fn archive_stopped_runner_queries_have_current_outcomes() {
    let (state, _, operation) = fixture().await;
    submit(&state, &operation).await.unwrap();
    let readers = (0..4)
        .map(|_| {
            let state = state.clone();
            let operation = operation.clone();
            tokio::spawn(async move {
                loop {
                    let receipt = query(&state, &operation).await;
                    if !receipt.running {
                        assert!(receipt
                            .items
                            .iter()
                            .all(|item| item.state == ItemState::Completed));
                        return;
                    }
                    tokio::task::yield_now().await;
                }
            })
        })
        .collect::<Vec<_>>();
    for reader in readers {
        tokio::time::timeout(std::time::Duration::from_secs(5), reader)
            .await
            .unwrap()
            .unwrap();
    }
}
