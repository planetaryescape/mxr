use super::*;

#[tokio::test]
async fn disabled_account_keeps_pairs_until_reenabled() {
    let (fx, model, ids) = ambiguous_pairs(5, 1).await;
    todos::scan_messages(&fx.state, &ids).await;
    let now = Utc::now();
    fx.state
        .store
        .set_account_enabled(&fx.account, false)
        .await
        .unwrap();
    assert!(!drain(&fx, now).await);
    assert_eq!(model.calls.load(Ordering::SeqCst), 0);
    fx.state
        .store
        .set_account_enabled(&fx.account, true)
        .await
        .unwrap();
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(now)
            .await
            .unwrap(),
        (5, 5)
    );
    assert!(drain(&fx, now).await);
    assert_eq!(model.calls.load(Ordering::SeqCst), 4);
    assert!(!drain(&fx, now).await);
    assert_eq!(model.calls.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn disabling_account_during_model_call_preserves_retry_after_reenable() {
    let (fx, model, ids) = ambiguous_pairs(1, 1).await;
    gate(&model);
    todos::scan_messages(&fx.state, &ids).await;
    let now = Utc::now();
    let state = fx.state.clone();
    let worker =
        tokio::spawn(async move { crate::handler::todo_resolution::drain(&state, now).await });
    wait_for_call(&model).await;
    fx.state
        .store
        .set_account_enabled(&fx.account, false)
        .await
        .unwrap();
    model.gate.add_permits(1);
    assert!(!worker.await.unwrap().unwrap());
    fx.state
        .store
        .set_account_enabled(&fx.account, true)
        .await
        .unwrap();
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(now)
            .await
            .unwrap(),
        (1, 0)
    );
    model.gate.add_permits(1);
    assert!(!drain(&fx, now + Duration::hours(1)).await);
    assert_eq!(model.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(now + Duration::hours(1))
            .await
            .unwrap(),
        (0, 0)
    );
}

#[tokio::test]
async fn invalid_model_outputs_retry_each_pair_five_times_independently() {
    let (fx, model, ids) = ambiguous_pairs(2, 1).await;
    *model.response.lock().unwrap() = "invalid JSON".into();
    todos::scan_messages(&fx.state, &ids).await;
    let mut now = Utc::now();
    for hours in [1, 4, 16, 24, 24] {
        assert!(!drain(&fx, now).await);
        todos::scan_messages(&fx.state, &ids).await;
        now += Duration::hours(hours);
    }
    assert!(!drain(&fx, now + Duration::days(10)).await);
    assert_eq!(model.calls.load(Ordering::SeqCst), 10);
    assert_eq!(
        fx.state
            .store
            .todo_resolution_queue_counts(now)
            .await
            .unwrap(),
        (0, 0)
    );
}

#[tokio::test]
async fn breaker_skips_preserve_attempt_count_and_return_idle_cadence() {
    let (fx, model, ids) = ambiguous_pairs(6, 1).await;
    model.fail.store(true, Ordering::SeqCst);
    todos::scan_messages(&fx.state, &ids).await;
    let now = Utc::now();
    assert!(drain(&fx, now).await);
    assert!(!drain(&fx, now).await);
    assert_eq!(model.calls.load(Ordering::SeqCst), 5);
    let deferred = fx
        .state
        .store
        .list_due_todo_resolutions(now + Duration::minutes(5), 10)
        .await
        .unwrap();
    assert_eq!(deferred.len(), 1);
    assert_eq!(deferred[0].attempts, 0);
    assert!(!drain(&fx, now + Duration::minutes(5)).await);
    assert_eq!(model.calls.load(Ordering::SeqCst), 5);
}
