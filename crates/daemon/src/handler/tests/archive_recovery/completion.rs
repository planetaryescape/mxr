use super::*;
use crate::handler::archive_recovery;

#[tokio::test]
async fn archive_terminal_replay_does_not_rearm_undo_or_repeat_effects() {
    let (state, provider, mut operation) = fixture().await;
    operation.accepted = true;
    overwrite(&state, &operation).await;
    assert_eq!(
        archive_recovery::run_for_test(&state, &operation.identity)
            .await
            .unwrap(),
        2
    );
    assert!(state
        .store
        .read_undo_entry(&operation.mutation_id)
        .await
        .unwrap()
        .is_some());
    let mutations_before_replay = provider.mutations().len();
    state
        .store
        .delete_undo_entry(&operation.mutation_id)
        .await
        .unwrap();

    assert_eq!(
        archive_recovery::run_for_test(&state, &operation.identity)
            .await
            .unwrap(),
        0
    );
    assert!(state
        .store
        .read_undo_entry(&operation.mutation_id)
        .await
        .unwrap()
        .is_none());
    assert_eq!(provider.mutations().len(), mutations_before_replay);
    assert_completed_history(&state, &query(&state, &operation).await).await;
}

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
