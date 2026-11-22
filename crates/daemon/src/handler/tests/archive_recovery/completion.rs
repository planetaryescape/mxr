use super::*;

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
