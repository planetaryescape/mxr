//! Replaying a completed projection after a real store reopen does not rejudge mail.
use super::*;

#[test]
fn published_projection_replays_once_after_reopen() {
    isolated_utc(|| {
        let dir = tempfile::tempdir().unwrap();
        temp_env::with_vars(
            [
                ("MXR_CONFIG_DIR", Some(dir.path().join("config"))),
                ("MXR_DATA_DIR", Some(dir.path().join("data"))),
                ("MXR_SOCKET_PATH", Some(dir.path().join("socket"))),
            ],
            || {
                let config_dir = dir.path().join("config");
                std::fs::create_dir_all(&config_dir).unwrap();
                mxr_test_support::daemon::write_fake_account_config(&config_dir);
                let path = config_dir.join("config.toml");
                std::fs::write(
                    &path,
                    std::fs::read_to_string(&path)
                        .unwrap()
                        .replace("fake@example.com", super::super::super::desk::ME),
                )
                .unwrap();
                let mut config = mxr_config::load_config().unwrap();
                config.llm.enabled = false;
                config.search.semantic.enabled = false;
                config.search.semantic.auto_download_models = false;
                config.activity.enabled = false;
                mxr_config::save_config(&config).unwrap();
                utc_test(async {
                    let state = Arc::new(AppState::new().await.unwrap());
                    let fx = Fixture::with_persistent_state(state.clone());
                    state
                        .store
                        .upsert_label(&mxr_core::types::Label {
                            id: fx.inbox.clone(),
                            account_id: fx.account.clone(),
                            name: "Inbox".into(),
                            kind: mxr_core::types::LabelKind::System,
                            color: None,
                            provider_id: "INBOX".into(),
                            unread_count: 0,
                            total_count: 0,
                            role: None,
                        })
                        .await
                        .unwrap();
                    let (id, expected) = bank_journey(&fx).await;
                    seed_published_row(&fx, &expected).await;
                    let answers = state
                        .store
                        .asks_for_messages(&fx.account, std::slice::from_ref(&id))
                        .await
                        .unwrap();
                    assert!(answers.values().all(|a| a.policy_version == "asks-r2c-16"));
                    state
                        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
                        .await;
                    state.store.reader().close().await;
                    state.store.writer().close().await;
                    drop(fx);
                    drop(state);
                    let state = Arc::new(AppState::new().await.unwrap());
                    let fx = Fixture::with_persistent_state(state.clone());
                    finish_first_run(&fx, expected.source_date.unwrap()).await;
                    let repaired = stored(&fx, &id).await;
                    assert_eq!(repaired.id, expected.id);
                    assert_eq!(repaired.dedup_key, expected.dedup_key);
                    assert_eq!(repaired.doc_type, expected.doc_type);
                    assert_eq!(repaired.act_by_at, expected.act_by_at);
                    assert_eq!(repaired.surface_at, expected.surface_at);
                    assert_eq!(
                        state
                            .store
                            .asks_for_messages(&fx.account, std::slice::from_ref(&id))
                            .await
                            .unwrap(),
                        answers
                    );
                    assert_eq!(
                        state
                            .store
                            .get_todo_run(&fx.account)
                            .await
                            .unwrap()
                            .unwrap()
                            .rules_version,
                        3
                    );
                    state
                        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
                        .await;
                    state.store.reader().close().await;
                    state.store.writer().close().await;
                    drop(fx);
                    drop(state);
                    let state = Arc::new(AppState::new().await.unwrap());
                    let fx = Fixture::with_persistent_state(state.clone());
                    let changes: i64 = sqlx::query_scalar("SELECT total_changes()")
                        .fetch_one(state.store.writer())
                        .await
                        .unwrap();
                    finish_first_run(&fx, expected.source_date.unwrap()).await;
                    assert_eq!(stored(&fx, &id).await, repaired);
                    assert_eq!(
                        state
                            .store
                            .asks_for_messages(&fx.account, std::slice::from_ref(&id))
                            .await
                            .unwrap(),
                        answers
                    );
                    assert_eq!(
                        sqlx::query_scalar::<_, i64>("SELECT total_changes()")
                            .fetch_one(state.store.writer())
                            .await
                            .unwrap(),
                        changes
                    );
                    state
                        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
                        .await;
                });
            },
        );
    });
}
