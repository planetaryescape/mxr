use super::desk::{request, Fixture, ME};
use super::promise_projection::{accept, chosen, settle, source, QUOTE};
use super::*;
use chrono::Duration;
use mxr_store::{CommitmentStatus, TodoState};

#[tokio::test]
async fn review2_latest_explicit_date_updates_the_same_task_immediately() {
    let fx = Fixture::new().await;
    let mail = source(&fx, QUOTE).await;
    settle(&fx, &mail).await;
    let original = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap()
        .remove(0);
    for due in [chosen(), chosen() + Duration::days(40)] {
        let ResponseData::RecordedPromise { commitment, .. } = request(
            &fx,
            Request::RecordPromise {
                message_id: mail.id.clone(),
                what: QUOTE.into(),
                due_at: due,
                dry_run: false,
            },
        )
        .await
        else {
            panic!("record promise")
        };
        let ResponseData::Todo { todo } = request(
            &fx,
            Request::GetTodo {
                todo_id: original.id.clone(),
            },
        )
        .await
        else {
            panic!("get todo")
        };
        eprintln!(
            "REVIEW2_DATE contact={:?} task={:?}",
            commitment.by_when, todo.due_at
        );
        assert_eq!(todo.due_at, Some(due));
        assert_eq!(todo.id, original.id);
        settle(&fx, &mail).await;
        assert_eq!(
            fx.state
                .store
                .get_todo(&original.id)
                .await
                .unwrap()
                .unwrap()
                .due_at,
            Some(due)
        );
    }
}

#[tokio::test]
async fn review2_desk_done_and_undo_restore_both_promise_and_task() {
    let fx = Fixture::new().await;
    let mail = source(&fx, QUOTE).await;
    settle(&fx, &mail).await;
    let id = accept(&fx, &mail).await;
    let original = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap()
        .remove(0);
    let items = vec![mxr_protocol::DeskDoneItemData {
        thread_id: mail.thread_id.clone(),
        lane: Some(mxr_protocol::DeskLaneKind::Due),
        commitment_id: Some(id.clone()),
    }];
    request(
        &fx,
        Request::ResolveDeskItems {
            items: items.clone(),
            dry_run: true,
        },
    )
    .await;
    assert_eq!(
        fx.state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap(),
        original
    );
    let ResponseData::DeskItemsResolved {
        mutation_id: Some(mutation_id),
        items,
        ..
    } = request(
        &fx,
        Request::ResolveDeskItems {
            items,
            dry_run: false,
        },
    )
    .await
    else {
        panic!("desk done")
    };
    assert!(items[0].error.is_none(), "{items:?}");
    let done = fx
        .state
        .store
        .get_todo(&original.id)
        .await
        .unwrap()
        .unwrap();
    eprintln!("REVIEW2_DESK task={done:?}");
    assert_eq!(done.state, TodoState::Done);
    settle(&fx, &mail).await;
    assert_eq!(
        fx.state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        TodoState::Done
    );
    request(&fx, Request::UndoMutation { mutation_id }).await;
    assert_eq!(
        fx.state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap(),
        original
    );
    assert_eq!(
        fx.state
            .store
            .get_contact_commitment(&id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommitmentStatus::Open
    );
    settle(&fx, &mail).await;
    assert_eq!(
        fx.state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap(),
        original
    );
}

#[tokio::test]
async fn explicit_dates_preserve_independent_fields_and_closed_tasks() {
    for closed in [
        None,
        Some(mxr_protocol::TodoStateActionData::Done),
        Some(mxr_protocol::TodoStateActionData::Dismiss),
    ] {
        let fx = Fixture::new().await;
        let mail = source(&fx, QUOTE).await;
        settle(&fx, &mail).await;
        accept(&fx, &mail).await;
        let original = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap()
            .remove(0);
        request(
            &fx,
            Request::CorrectTodo {
                todo_id: original.id.clone(),
                reason: mxr_protocol::TodoCorrectionData::WrongDetails,
                time_zone: None,
                edits: vec![mxr_protocol::TodoEditData {
                    field: "title".into(),
                    value: "My report plan".into(),
                }],
                dry_run: false,
            },
        )
        .await;
        request(
            &fx,
            Request::ScheduleTodo {
                todo_id: original.id.clone(),
                when: Some((chosen() + Duration::days(2)).to_rfc3339()),
                time_zone: None,
                dry_run: false,
            },
        )
        .await;
        if let Some(action) = closed {
            request(
                &fx,
                Request::SetTodoState {
                    todo_ids: vec![original.id.clone()],
                    action,
                    dry_run: false,
                },
            )
            .await;
        }
        let before = fx
            .state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap();
        let due = chosen() + Duration::days(40);
        request(
            &fx,
            Request::RecordPromise {
                message_id: mail.id.clone(),
                what: QUOTE.into(),
                due_at: due,
                dry_run: false,
            },
        )
        .await;
        let after = fx
            .state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.title, before.title);
        assert_eq!(after.scheduled_for, before.scheduled_for);
        assert_eq!(after.reason, QUOTE);
        assert_eq!(after.state, before.state);
        if closed.is_some() {
            assert_eq!(after, before);
        } else {
            assert_eq!(after.due_at, Some(due));
        }
        settle(&fx, &mail).await;
        assert_eq!(
            fx.state
                .store
                .get_todo(&original.id)
                .await
                .unwrap()
                .unwrap(),
            after
        );
    }
}

#[tokio::test]
async fn desk_undo_keeps_later_task_decisions_and_other_promises() {
    for dismissed in [false, true] {
        let fx = Fixture::new().await;
        let mail = source(&fx, QUOTE).await;
        settle(&fx, &mail).await;
        let id = accept(&fx, &mail).await;
        let original = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap()
            .remove(0);
        let other = source(&fx, "I'll upload the plan tomorrow.").await;
        settle(&fx, &other).await;
        let other_task = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.id != original.id)
            .unwrap();
        request(
            &fx,
            Request::CorrectTodo {
                todo_id: original.id.clone(),
                reason: mxr_protocol::TodoCorrectionData::WrongDetails,
                time_zone: None,
                edits: vec![mxr_protocol::TodoEditData {
                    field: "title".into(),
                    value: "My report plan".into(),
                }],
                dry_run: false,
            },
        )
        .await;
        if dismissed {
            request(
                &fx,
                Request::SetTodoState {
                    todo_ids: vec![original.id.clone()],
                    action: mxr_protocol::TodoStateActionData::Dismiss,
                    dry_run: false,
                },
            )
            .await;
        }
        let before = fx
            .state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap();
        let ResponseData::DeskItemsResolved {
            mutation_id: Some(mutation_id),
            ..
        } = request(
            &fx,
            Request::ResolveDeskItems {
                items: vec![mxr_protocol::DeskDoneItemData {
                    thread_id: mail.thread_id.clone(),
                    lane: Some(mxr_protocol::DeskLaneKind::Due),
                    commitment_id: Some(id.clone()),
                }],
                dry_run: false,
            },
        )
        .await
        else {
            panic!("desk done")
        };
        let after = fx
            .state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap();
        if dismissed {
            assert_eq!(after, before);
        } else {
            assert_eq!(after.title, before.title);
            assert_eq!(after.state, TodoState::Done);
        }
        request(
            &fx,
            Request::SetTodoState {
                todo_ids: vec![original.id.clone()],
                action: mxr_protocol::TodoStateActionData::Dismiss,
                dry_run: false,
            },
        )
        .await;
        let later = fx
            .state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap();
        request(&fx, Request::UndoMutation { mutation_id }).await;
        assert_eq!(
            fx.state
                .store
                .get_todo(&original.id)
                .await
                .unwrap()
                .unwrap(),
            later
        );
        assert_eq!(
            fx.state
                .store
                .get_todo(&other_task.id)
                .await
                .unwrap()
                .unwrap(),
            other_task
        );
        assert_eq!(
            fx.state
                .store
                .get_contact_commitment(&id)
                .await
                .unwrap()
                .unwrap()
                .status,
            if dismissed {
                CommitmentStatus::Open
            } else {
                CommitmentStatus::Resolved
            }
        );
    }
}

#[test]
fn accepted_date_and_desk_undo_survive_a_real_store_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
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
                    .replace("fake@example.com", ME),
            )
            .unwrap();
            let mut config = mxr_config::load_config().unwrap();
            config.llm.enabled = false;
            config.search.semantic.enabled = false;
            config.search.semantic.auto_download_models = false;
            config.activity.enabled = false;
            mxr_config::save_config(&config).unwrap();
            runtime.block_on(async {
                let state = Arc::new(AppState::new().await.unwrap());
                let fx = Fixture::with_persistent_state(state.clone());
                let mail = source(&fx, QUOTE).await;
                settle(&fx, &mail).await;
                let id = accept(&fx, &mail).await;
                let due = chosen() + Duration::days(40);
                request(
                    &fx,
                    Request::RecordPromise {
                        message_id: mail.id.clone(),
                        what: QUOTE.into(),
                        due_at: due,
                        dry_run: false,
                    },
                )
                .await;
                let original = state
                    .store
                    .list_promise_todos(&fx.account)
                    .await
                    .unwrap()
                    .remove(0);
                assert_eq!(original.due_at, Some(due));
                let ResponseData::DeskItemsResolved {
                    mutation_id: Some(mutation_id),
                    ..
                } = request(
                    &fx,
                    Request::ResolveDeskItems {
                        items: vec![mxr_protocol::DeskDoneItemData {
                            thread_id: mail.thread_id.clone(),
                            lane: Some(mxr_protocol::DeskLaneKind::Due),
                            commitment_id: Some(id.clone()),
                        }],
                        dry_run: false,
                    },
                )
                .await
                else {
                    panic!("desk done")
                };
                assert_eq!(
                    state
                        .store
                        .get_todo(&original.id)
                        .await
                        .unwrap()
                        .unwrap()
                        .state,
                    TodoState::Done
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
                settle(&fx, &mail).await;
                assert_eq!(
                    state
                        .store
                        .get_todo(&original.id)
                        .await
                        .unwrap()
                        .unwrap()
                        .state,
                    TodoState::Done
                );
                request(&fx, Request::UndoMutation { mutation_id }).await;
                assert_eq!(
                    state.store.get_todo(&original.id).await.unwrap().unwrap(),
                    original
                );
                assert_eq!(
                    state
                        .store
                        .get_contact_commitment(&id)
                        .await
                        .unwrap()
                        .unwrap()
                        .status,
                    CommitmentStatus::Open
                );
                settle(&fx, &mail).await;
                assert_eq!(
                    state.store.get_todo(&original.id).await.unwrap().unwrap(),
                    original
                );
                state
                    .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
                    .await;
                state.store.reader().close().await;
                state.store.writer().close().await;
            });
        },
    );
}
