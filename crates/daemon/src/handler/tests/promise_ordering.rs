//! Explicit acceptance and normal assessment share one source identity in either order.
use super::desk::{request, Fixture, ME};
use super::promise_projection::{chosen, settle, source};
use super::*;
use chrono::Duration;
use mxr_core::Envelope;
use mxr_store::{ByWhenSource, CommitmentStatus, TodoState};

#[tokio::test]
async fn review7_accepting_a_fragment_before_assessment_keeps_one_identity() {
    let fx = Fixture::new().await;
    fragment_before_assessment(&fx).await;
}

async fn fragment_before_assessment(fx: &Fixture) -> (Envelope, mxr_store::TodoRecord) {
    let quote = format!(
        "I'll send the report by {}.",
        (chosen() - Duration::days(2)).format("%-d %B %Y")
    );
    let mail = source(fx, &quote).await;
    assert!(fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
        .await
        .unwrap()
        .is_empty());
    let changes: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(fx.state.store.writer())
        .await
        .unwrap();
    let record = |dry_run| Request::RecordPromise {
        message_id: mail.id.clone(),
        what: "send the report".into(),
        due_at: chosen() + Duration::days(40),
        dry_run,
    };
    let ResponseData::RecordedPromise {
        commitment: preview,
        ..
    } = request(fx, record(true)).await
    else {
        panic!("preview")
    };
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT total_changes()")
            .fetch_one(fx.state.store.writer())
            .await
            .unwrap(),
        changes
    );
    let ResponseData::RecordedPromise {
        commitment: accepted,
        ..
    } = request(fx, record(false)).await
    else {
        panic!("record")
    };
    assert_eq!(preview.what, accepted.what);
    let immediate = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap()
        .remove(0);
    settle(fx, &mail).await;
    let commitments = fx
        .state
        .store
        .list_contact_commitments(&fx.account, None, None)
        .await
        .unwrap();
    let tasks = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap();
    let ResponseData::Todos { todos: active } = request(
        fx,
        Request::ListTodos {
            account_id: Some(fx.account.clone()),
            state: mxr_protocol::TodoStateData::Open,
            limit: 100,
        },
    )
    .await
    else {
        panic!("active")
    };
    eprintln!("REVIEW7_FRAGMENT preview={preview:?} accepted={accepted:?} immediate={immediate:?} commitments={commitments:?} tasks={tasks:?} public={active:?}");
    assert_eq!(
        commitments.len(),
        1,
        "one source obligation after reconciliation"
    );
    assert_eq!(tasks.len(), 1);
    assert_eq!(active.len(), 1);
    assert_eq!(commitments[0].id, accepted.id);
    assert_eq!(commitments[0].what, quote);
    assert_eq!(commitments[0].by_when_source, Some(ByWhenSource::User));
    assert_eq!(commitments[0].by_when, accepted.by_when);
    assert_eq!(tasks[0].id, immediate.id);
    assert_eq!(tasks[0].reason, quote);
    assert_eq!(tasks[0].due_at, accepted.by_when);
    assert_eq!(
        tasks[0].commitment_id.as_deref(),
        Some(accepted.id.as_str())
    );
    settle(fx, &mail).await;
    assert_eq!(
        fx.state.store.get_todo(&immediate.id).await.unwrap(),
        Some(tasks[0].clone())
    );
    let before = &tasks[0];
    let items = vec![mxr_protocol::DeskDoneItemData {
        account_id: None,
        thread_id: mail.thread_id.clone(),
        lane: Some(mxr_protocol::DeskLaneKind::Due),
        commitment_id: Some(accepted.id.clone()),
    }];
    let changes: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(fx.state.store.writer())
        .await
        .unwrap();
    request(
        fx,
        Request::ResolveDeskItems {
            items: items.clone(),
            dry_run: true,
        },
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT total_changes()")
            .fetch_one(fx.state.store.writer())
            .await
            .unwrap(),
        changes
    );
    let ResponseData::DeskItemsResolved {
        mutation_id: Some(mutation_id),
        ..
    } = request(
        fx,
        Request::ResolveDeskItems {
            items,
            dry_run: false,
        },
    )
    .await
    else {
        panic!("resolve")
    };
    settle(fx, &mail).await;
    assert_eq!(
        fx.state
            .store
            .get_todo(&before.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        TodoState::Done
    );
    assert_eq!(
        fx.state
            .store
            .get_contact_commitment(&accepted.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommitmentStatus::Resolved
    );
    request(fx, Request::UndoMutation { mutation_id }).await;
    assert_eq!(
        fx.state.store.get_todo(&before.id).await.unwrap().as_ref(),
        Some(before)
    );
    assert_eq!(
        fx.state
            .store
            .get_contact_commitment(&accepted.id)
            .await
            .unwrap()
            .as_ref(),
        Some(&commitments[0])
    );
    settle(fx, &mail).await;
    assert_eq!(
        fx.state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap()
            .len(),
        1
    );
    (mail, before.clone())
}

#[tokio::test]
async fn review7_pending_source_keeps_ambiguous_and_freeform_acceptance_distinct() {
    let fx = Fixture::new().await;
    let mail = source(
        &fx,
        "I'll send the report tomorrow. I'll send the report on Friday.",
    )
    .await;
    let changes: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(fx.state.store.writer())
        .await
        .unwrap();
    for dry_run in [true, false] {
        let error = super::super::promises::record_promise(
            &fx.state,
            &mail.id,
            "send the report",
            chosen(),
            dry_run,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("more than one promise"));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT total_changes()")
            .fetch_one(fx.state.store.writer())
            .await
            .unwrap(),
        changes
    );
    let ResponseData::RecordedPromise { commitment, .. } = request(
        &fx,
        Request::RecordPromise {
            message_id: mail.id.clone(),
            what: "prepare a separate outline".into(),
            due_at: chosen(),
            dry_run: false,
        },
    )
    .await
    else {
        panic!("freeform")
    };
    settle(&fx, &mail).await;
    let rows = fx
        .state
        .store
        .list_contact_commitments(&fx.account, None, None)
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        3,
        "two source promises and independent accepted work"
    );
    assert!(rows
        .iter()
        .any(|r| r.id == commitment.id && r.what == "prepare a separate outline"));
}

#[tokio::test]
async fn review7_pending_fragment_keeps_manual_and_closed_task_decisions() {
    for control in ["title", "done", "dismissed"] {
        let fx = Fixture::new().await;
        let mail = source(&fx, "I'll send the report tomorrow.").await;
        request(
            &fx,
            Request::RecordPromise {
                message_id: mail.id.clone(),
                what: "send the report".into(),
                due_at: chosen(),
                dry_run: false,
            },
        )
        .await;
        let immediate = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap()
            .remove(0);
        let undo = if control == "title" {
            let ResponseData::TodoChange { change } = request(
                &fx,
                Request::CorrectTodo {
                    todo_id: immediate.id.clone(),
                    reason: mxr_protocol::TodoCorrectionData::WrongDetails,
                    edits: vec![mxr_protocol::TodoEditData {
                        field: "title".into(),
                        value: "Send my final report".into(),
                    }],
                    time_zone: None,
                    dry_run: false,
                },
            )
            .await
            else {
                panic!("correct")
            };
            change.correction_undo
        } else {
            request(
                &fx,
                Request::SetTodoState {
                    todo_ids: vec![immediate.id.clone()],
                    action: if control == "done" {
                        mxr_protocol::TodoStateActionData::Done
                    } else {
                        mxr_protocol::TodoStateActionData::Dismiss
                    },
                    dry_run: false,
                },
            )
            .await;
            None
        };
        let before = fx
            .state
            .store
            .get_todo(&immediate.id)
            .await
            .unwrap()
            .unwrap();
        settle(&fx, &mail).await;
        assert_eq!(
            fx.state
                .store
                .list_promise_todos(&fx.account)
                .await
                .unwrap(),
            vec![before.clone()],
            "{control}"
        );
        assert_eq!(
            fx.state
                .store
                .list_contact_commitments(&fx.account, None, None)
                .await
                .unwrap()
                .len(),
            1
        );
        if let Some(correction_id) = undo {
            request(
                &fx,
                Request::UndoTodoCorrection {
                    todo_id: immediate.id.clone(),
                    correction_id,
                    dry_run: false,
                },
            )
            .await;
            assert_eq!(
                fx.state.store.get_todo(&immediate.id).await.unwrap(),
                Some(immediate)
            );
        }
    }
}

#[test]
fn review7_fragment_identity_date_and_history_survive_real_reopen() {
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
                let state = std::sync::Arc::new(crate::state::AppState::new().await.unwrap());
                let fx = Fixture::with_persistent_state(state.clone());
                let (mail, before) = fragment_before_assessment(&fx).await;
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
                let state = std::sync::Arc::new(crate::state::AppState::new().await.unwrap());
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
async fn review7_uncached_source_keeps_freeform_acceptance_without_fetching() {
    let fx = Fixture::new().await;
    let mut mail = fx
        .message(
            &mxr_core::ThreadId::new(),
            ME,
            "nora@example.com",
            Duration::minutes(5),
            None,
        )
        .await;
    mail.flags |= mxr_core::MessageFlags::SENT;
    fx.store_envelope(&mail, mxr_core::MessageDirection::Outbound)
        .await;
    assert!(fx.state.store.get_body(&mail.id).await.unwrap().is_none());
    for dry_run in [true, false] {
        let ResponseData::RecordedPromise { commitment, .. } = request(
            &fx,
            Request::RecordPromise {
                message_id: mail.id.clone(),
                what: "**prepare #outline**\nnext".into(),
                due_at: chosen(),
                dry_run,
            },
        )
        .await
        else {
            panic!("freeform")
        };
        assert_eq!(commitment.what, "prepare outline next");
        assert!(fx.state.store.get_body(&mail.id).await.unwrap().is_none());
    }
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
