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

#[tokio::test]
async fn review6_recorded_date_updates_an_accepted_task_after_title_only_correction() {
    let fx = Fixture::new().await;
    accepted_date_journey(&fx).await;
}

async fn accepted_date_journey(fx: &Fixture) -> (mxr_core::Envelope, mxr_store::TodoRecord) {
    const TEXT: &str = "I'll prepare the onboarding outline.";
    let mail = source(fx, TEXT).await;
    let a = chosen();
    let b = a + Duration::days(40);
    let record = |due_at| Request::RecordPromise {
        message_id: mail.id.clone(),
        what: TEXT.into(),
        due_at,
        dry_run: false,
    };
    let ResponseData::RecordedPromise {
        commitment: first, ..
    } = request(fx, record(a)).await
    else {
        panic!("record A");
    };
    settle(fx, &mail).await;
    let asks = fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    assert!(!asks[&mail.id]
        .units
        .iter()
        .any(|u| u.answer == mxr_core::asks::AskAnswer::YouPromised));
    let rows = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let original = &rows[0];
    let ResponseData::TodoChange { change } = request(
        fx,
        Request::CorrectTodo {
            todo_id: original.id.clone(),
            reason: mxr_protocol::TodoCorrectionData::WrongDetails,
            edits: vec![mxr_protocol::TodoEditData {
                field: "title".into(),
                value: "Prepare my onboarding plan".into(),
            }],
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    else {
        panic!("correct");
    };
    let corrected = fx
        .state
        .store
        .get_todo(&original.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(corrected.due_at, Some(a));
    let changes: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(fx.state.store.writer())
        .await
        .unwrap();
    request(
        fx,
        Request::RecordPromise {
            message_id: mail.id.clone(),
            what: TEXT.into(),
            due_at: b,
            dry_run: true,
        },
    )
    .await;
    assert_eq!(
        changes,
        sqlx::query_scalar::<_, i64>("SELECT total_changes()")
            .fetch_one(fx.state.store.writer())
            .await
            .unwrap()
    );
    let ResponseData::RecordedPromise {
        commitment: second, ..
    } = request(fx, record(b)).await
    else {
        panic!("record B");
    };
    let ResponseData::Todo { todo } = request(
        fx,
        Request::GetTodo {
            todo_id: original.id.clone(),
        },
    )
    .await
    else {
        panic!("get");
    };
    eprintln!("REVIEW6_ACCEPTED contact={second:?} corrected={corrected:?} public_task={todo:?} undo={:?}", change.correction_undo);
    assert_eq!(second.id, first.id);
    assert_eq!(second.by_when, Some(b));
    assert_eq!(todo.id, original.id);
    assert_eq!(todo.title, corrected.title);
    assert_eq!(todo.due_at, Some(b));
    assert_eq!(todo.why, format!("Here because: {TEXT}"));
    settle(fx, &mail).await;
    let tasks = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, original.id);
    assert_eq!(tasks[0].commitment_id.as_deref(), Some(first.id.as_str()));
    assert_eq!(tasks[0].due_at, Some(b));
    assert_eq!(tasks[0].title, corrected.title);
    assert_eq!(tasks[0].reason, TEXT);
    (mail, tasks.into_iter().next().unwrap())
}

#[tokio::test]
async fn review6_accepted_dates_keep_independent_fields_and_closed_decisions() {
    const TEXT: &str = "I'll prepare the onboarding outline.";
    for control in ["due", "done", "dismissed", "mixed", "legacy"] {
        let fx = Fixture::new().await;
        let mail = source(&fx, TEXT).await;
        let ResponseData::RecordedPromise { commitment, .. } = request(
            &fx,
            Request::RecordPromise {
                message_id: mail.id.clone(),
                what: TEXT.into(),
                due_at: chosen(),
                dry_run: false,
            },
        )
        .await
        else {
            panic!("record");
        };
        settle(&fx, &mail).await;
        let original = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap()
            .remove(0);
        let fields = if control == "due" {
            vec![("due", "1 April 2027")]
        } else if control == "mixed" {
            vec![
                ("title", "My outline"),
                ("amount", "£25"),
                ("kind", "sign"),
                ("counterparty", "Priya"),
            ]
        } else {
            vec![("title", "My outline")]
        };
        request(
            &fx,
            Request::CorrectTodo {
                todo_id: original.id.clone(),
                reason: mxr_protocol::TodoCorrectionData::WrongDetails,
                edits: fields
                    .into_iter()
                    .map(|(field, value)| mxr_protocol::TodoEditData {
                        field: field.into(),
                        value: value.into(),
                    })
                    .collect(),
                time_zone: None,
                dry_run: false,
            },
        )
        .await;
        if control == "mixed" {
            request(
                &fx,
                Request::ScheduleTodo {
                    todo_id: original.id.clone(),
                    when: Some("in 2d".into()),
                    time_zone: None,
                    dry_run: false,
                },
            )
            .await;
        }
        if control == "done" || control == "dismissed" {
            request(
                &fx,
                Request::SetTodoState {
                    todo_ids: vec![original.id.clone()],
                    action: if control == "done" {
                        mxr_protocol::TodoStateActionData::Done
                    } else {
                        mxr_protocol::TodoStateActionData::Dismiss
                    },
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
        let cfg = super::super::todos::pass_config(&fx.state, chrono::Utc::now());
        let stale = fx
            .state
            .store
            .promise_for_todo(&fx.account, &commitment.id)
            .await
            .unwrap()
            .unwrap();
        if control == "legacy" {
            sqlx::query("UPDATE contact_commitments SET by_when_source='model_date' WHERE id=?")
                .bind(&commitment.id)
                .execute(fx.state.store.writer())
                .await
                .unwrap();
            mxr_todo::accepted_promises::sync(&fx.state.store, &cfg, &fx.account, false)
                .await
                .unwrap();
        } else {
            request(
                &fx,
                Request::RecordPromise {
                    message_id: mail.id.clone(),
                    what: TEXT.into(),
                    due_at: chosen() + Duration::days(40),
                    dry_run: false,
                },
            )
            .await;
        }
        let after = fx
            .state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap();
        if control == "mixed" {
            assert_eq!(after.due_at, Some(chosen() + Duration::days(40)));
            assert_eq!(
                (
                    &after.title,
                    &after.kind,
                    &after.verb,
                    &after.counterparty,
                    &after.amount_minor,
                    &after.currency,
                    &after.scheduled_for,
                    &after.reason,
                    &after.dedup_key
                ),
                (
                    &before.title,
                    &before.kind,
                    &before.verb,
                    &before.counterparty,
                    &before.amount_minor,
                    &before.currency,
                    &before.scheduled_for,
                    &before.reason,
                    &before.dedup_key
                )
            );
            assert_eq!(after.commitment_id, before.commitment_id);
            assert_eq!(after.id, before.id);
        } else {
            assert_eq!(after, before, "{control}");
        }
        if control != "legacy" {
            let outcome = fx
                .state
                .store
                .upsert_accepted_promise_todo(&before, &stale)
                .await
                .unwrap();
            assert!(
                matches!(outcome, mxr_store::TodoUpsert::Unchanged),
                "stale snapshot must not overwrite the acknowledged date"
            );
            assert_eq!(
                fx.state
                    .store
                    .get_todo(&original.id)
                    .await
                    .unwrap()
                    .unwrap(),
                after
            );
            let mut foreign = stale.clone();
            foreign.account_id = mxr_core::AccountId::new();
            assert!(fx
                .state
                .store
                .upsert_accepted_promise_todo(&before, &foreign)
                .await
                .is_err());
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
        assert_eq!(
            fx.state
                .store
                .list_promise_todos(&fx.account)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn review6_accepted_date_and_independent_title_survive_real_reopen() {
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
                let (mail, before) = accepted_date_journey(&fx).await;
                let commitments = state
                    .store
                    .list_contact_commitments(&fx.account, None, None)
                    .await
                    .unwrap();
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
                    state.store.get_todo(&before.id).await.unwrap(),
                    Some(before)
                );
                assert_eq!(
                    state
                        .store
                        .list_contact_commitments(&fx.account, None, None)
                        .await
                        .unwrap(),
                    commitments
                );
                state
                    .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
                    .await;
            });
        },
    );
}

#[tokio::test]
async fn review6_legacy_model_or_missing_date_source_stays_unverified_until_acceptance() {
    const TEXT: &str = "I'll prepare the onboarding outline.";
    for source in [Some("model_date"), None] {
        let fx = Fixture::new().await;
        let mail = super::promise_projection::source(&fx, TEXT).await;
        let ResponseData::RecordedPromise { commitment, .. } = request(
            &fx,
            Request::RecordPromise {
                message_id: mail.id.clone(),
                what: TEXT.into(),
                due_at: chosen(),
                dry_run: false,
            },
        )
        .await
        else {
            panic!("record");
        };
        // Seed the persisted shape of an old extractor row; remove the unedited seed task.
        sqlx::query("DELETE FROM todos WHERE commitment_id=?")
            .bind(&commitment.id)
            .execute(fx.state.store.writer())
            .await
            .unwrap();
        let legacy_id = format!("{}::{}", mail.id, uuid::Uuid::new_v4());
        sqlx::query("UPDATE contact_commitments SET id=?, by_when_source=? WHERE id=?")
            .bind(&legacy_id)
            .bind(source)
            .bind(&commitment.id)
            .execute(fx.state.store.writer())
            .await
            .unwrap();
        let cfg = super::super::todos::pass_config(&fx.state, chrono::Utc::now());
        mxr_todo::accepted_promises::sync(&fx.state.store, &cfg, &fx.account, false)
            .await
            .unwrap();
        let rows = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].due_at, None);
        assert_eq!(rows[0].reason, TEXT);
        assert!(fx
            .state
            .store
            .get_contact_commitment(&legacy_id)
            .await
            .unwrap()
            .unwrap()
            .date_unverified());
        let id = rows[0].id.clone();
        let ResponseData::RecordedPromise {
            commitment: accepted,
            ..
        } = request(
            &fx,
            Request::RecordPromise {
                message_id: mail.id.clone(),
                what: TEXT.into(),
                due_at: chosen() + Duration::days(40),
                dry_run: false,
            },
        )
        .await
        else {
            panic!("accept");
        };
        assert_eq!(accepted.id, legacy_id);
        assert!(!accepted.date_unverified);
        let after = fx.state.store.get_todo(&id).await.unwrap().unwrap();
        assert_eq!(after.due_at, accepted.by_when);
        assert_eq!(after.reason, TEXT);
        assert_eq!(after.commitment_id.as_deref(), Some(legacy_id.as_str()));
        settle(&fx, &mail).await;
        assert_eq!(
            fx.state
                .store
                .list_promise_todos(&fx.account)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
