//! Accepted authority and canonical projections retain one identity and lifecycle.
use super::desk::{request, Fixture, ME};
use super::*;
use chrono::{Duration, Utc};
use mxr_core::{Envelope, MessageBody, MessageDirection, MessageFlags, ThreadId};
use mxr_store::{ByWhenSource, CommitmentStatus, TodoState};

const QUOTE: &str = "I'll send the report tomorrow.";

async fn source(fx: &Fixture, text: &str) -> Envelope {
    let mut mail = fx
        .message(
            &ThreadId::new(),
            ME,
            "nora@example.com",
            Duration::minutes(5),
            None,
        )
        .await;
    mail.flags |= MessageFlags::SENT;
    fx.store_envelope(&mail, MessageDirection::Outbound).await;
    replace(fx, &mail, text).await;
    mail
}

async fn replace(fx: &Fixture, mail: &Envelope, text: &str) {
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: mail.id.clone(),
            text_plain: Some(text.into()),
            text_html: None,
            attachments: vec![],
            fetched_at: Utc::now(),
            metadata: Default::default(),
        })
        .await
        .unwrap();
}

fn chosen() -> chrono::DateTime<Utc> {
    (Utc::now().date_naive() + Duration::days(9))
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc()
}

async fn accept(fx: &Fixture, mail: &Envelope) -> String {
    let ResponseData::RecordedPromise { commitment, .. } = request(
        fx,
        Request::RecordPromise {
            message_id: mail.id.clone(),
            what: QUOTE.into(),
            due_at: chosen(),
            dry_run: false,
        },
    )
    .await
    else {
        panic!("recorded promise");
    };
    commitment.id
}

async fn settle(fx: &Fixture, mail: &Envelope) {
    super::super::assessments::reconcile_messages(
        &fx.state,
        &fx.account,
        std::slice::from_ref(&mail.id),
    )
    .await
    .unwrap();
    super::super::todos::scan_messages(&fx.state, std::slice::from_ref(&mail.id)).await;
    let mut fingerprints = std::collections::HashMap::new();
    while super::super::todos::tick(&fx.state, Utc::now(), &mut fingerprints)
        .await
        .unwrap()
    {}
}

#[tokio::test]
async fn accepting_before_assessment_reuses_the_accepted_identity_and_date() {
    let fx = Fixture::new().await;
    let mail = source(&fx, QUOTE).await;
    let accepted = accept(&fx, &mail).await;
    settle(&fx, &mail).await;
    let rows = fx
        .state
        .store
        .list_contact_commitments(&fx.account, None, None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, accepted);
    assert_eq!(rows[0].by_when, Some(chosen()));
    assert_eq!(rows[0].by_when_source, Some(ByWhenSource::User));
    let tasks = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].commitment_id.as_deref(), Some(accepted.as_str()));
    assert_eq!(tasks[0].due_at, rows[0].by_when);
    assert_eq!(tasks[0].reason, QUOTE);
}

#[test]
fn adopting_a_canonical_promise_protects_its_date_and_task_after_retraction() {
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
                let mail = source(&fx, QUOTE).await;
                settle(&fx, &mail).await;
                let original = fx
                    .state
                    .store
                    .list_promise_todos(&fx.account)
                    .await
                    .unwrap()
                    .remove(0);
                let accepted = accept(&fx, &mail).await;
                assert_eq!(original.commitment_id.as_deref(), Some(accepted.as_str()));
                replace(&fx, &mail, &format!("{QUOTE}\n\nThanks.")).await;
                settle(&fx, &mail).await;
                let promise = fx
                    .state
                    .store
                    .get_contact_commitment(&accepted)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(promise.by_when_source, Some(ByWhenSource::User));
                assert_eq!(promise.by_when, Some(chosen()));
                let task = fx
                    .state
                    .store
                    .get_todo(&original.id)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    fx.state
                        .store
                        .list_promise_todos(&fx.account)
                        .await
                        .unwrap()
                        .len(),
                    1
                );
                assert_eq!(task.due_at, promise.by_when);
                assert_eq!(task.reason, QUOTE);
                replace(&fx, &mail, "The report is finished.").await;
                settle(&fx, &mail).await;
                assert_eq!(
                    fx.state
                        .store
                        .get_contact_commitment(&accepted)
                        .await
                        .unwrap()
                        .unwrap()
                        .status,
                    CommitmentStatus::Open
                );
                let retained = fx
                    .state
                    .store
                    .get_todo(&original.id)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(retained.state, TodoState::Open);
                assert_eq!(retained.due_at, promise.by_when);
                assert_eq!(retained.reason, QUOTE);
                assert_eq!(
                    fx.state
                        .store
                        .list_promise_todos(&fx.account)
                        .await
                        .unwrap()
                        .len(),
                    1
                );

                let before = fx
                    .state
                    .store
                    .get_contact_commitment(&accepted)
                    .await
                    .unwrap()
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
                    state.store.get_todo(&original.id).await.unwrap().unwrap(),
                    retained
                );
                assert_eq!(
                    state
                        .store
                        .list_promise_todos(&fx.account)
                        .await
                        .unwrap()
                        .len(),
                    1
                );
                assert_eq!(
                    state
                        .store
                        .get_contact_commitment(&accepted)
                        .await
                        .unwrap()
                        .unwrap(),
                    before
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
async fn resolving_an_accepted_or_canonical_promise_closes_its_task_without_reopening() {
    for canonical in [false, true] {
        let fx = Fixture::new().await;
        let mail = source(
            &fx,
            if canonical {
                QUOTE
            } else {
                "The report details are attached."
            },
        )
        .await;
        if !canonical {
            accept(&fx, &mail).await;
        }
        settle(&fx, &mail).await;
        let task = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap()
            .remove(0);
        let id = task.commitment_id.clone().unwrap();
        request(
            &fx,
            Request::ResolveCommitment {
                commitment_id: id.clone(),
            },
        )
        .await;
        assert_eq!(
            fx.state
                .store
                .get_todo(&task.id)
                .await
                .unwrap()
                .unwrap()
                .state,
            TodoState::Done
        );
        settle(&fx, &mail).await;
        assert_eq!(
            fx.state
                .store
                .get_todo(&task.id)
                .await
                .unwrap()
                .unwrap()
                .state,
            TodoState::Done
        );
        assert_eq!(
            fx.state
                .store
                .get_contact_commitment(&id)
                .await
                .unwrap()
                .unwrap()
                .status,
            CommitmentStatus::Resolved
        );
        request(
            &fx,
            Request::SetTodoState {
                todo_ids: vec![task.id.clone()],
                action: mxr_protocol::TodoStateActionData::Undo,
                dry_run: false,
            },
        )
        .await;
        let undone = fx.state.store.get_todo(&task.id).await.unwrap().unwrap();
        assert_eq!(undone.state, TodoState::Open);
        assert_eq!(undone.due_at, task.due_at);
        assert_eq!(undone.reason, task.reason);
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
        request(
            &fx,
            Request::SetTodoState {
                todo_ids: vec![task.id.clone()],
                action: mxr_protocol::TodoStateActionData::Dismiss,
                dry_run: false,
            },
        )
        .await;
        request(
            &fx,
            Request::ResolveCommitment {
                commitment_id: id.clone(),
            },
        )
        .await;
        settle(&fx, &mail).await;
        assert_eq!(
            fx.state
                .store
                .get_todo(&task.id)
                .await
                .unwrap()
                .unwrap()
                .state,
            TodoState::Dismissed
        );
    }
}

#[tokio::test]
async fn public_record_promise_updates_the_existing_task_without_a_settle_helper() {
    let mut config = mxr_config::MxrConfig::default();
    config.llm.enabled = false;
    config.search.semantic.enabled = false;
    config.search.semantic.auto_download_models = false;
    config.activity.enabled = false;
    let fx = Fixture::with_config(config).await;
    let mail = source(&fx, "I'll send the report.").await;
    settle(&fx, &mail).await;
    let original = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(original.due_at, None);
    let before = fx
        .state
        .store
        .promise_fingerprint(&fx.account)
        .await
        .unwrap();
    let response = request(
        &fx,
        Request::RecordPromise {
            message_id: mail.id.clone(),
            what: original.reason.clone(),
            due_at: chosen(),
            dry_run: true,
        },
    )
    .await;
    assert!(matches!(
        response,
        ResponseData::RecordedPromise { dry_run: true, .. }
    ));
    assert_eq!(
        fx.state
            .store
            .promise_fingerprint(&fx.account)
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        fx.state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap(),
        original
    );
    let ResponseData::RecordedPromise { commitment, .. } = request(
        &fx,
        Request::RecordPromise {
            message_id: mail.id.clone(),
            what: original.reason.clone(),
            due_at: chosen(),
            dry_run: false,
        },
    )
    .await
    else {
        panic!("recorded promise")
    };
    assert_eq!(
        original.commitment_id.as_deref(),
        Some(commitment.id.as_str())
    );
    // No scan, assessment reconciliation or tick between these public requests.
    let ResponseData::Todo { todo } = request(
        &fx,
        Request::GetTodo {
            todo_id: original.id.clone(),
        },
    )
    .await
    else {
        panic!("public todo")
    };
    let stored = fx
        .state
        .store
        .get_todo(&original.id)
        .await
        .unwrap()
        .unwrap();
    eprintln!(
        "PUBLIC_PROMISE_PACKET {}",
        serde_json::json!({"task_id": original.id, "commitment_id": commitment.id, "original_due": original.due_at, "contact_due": commitment.by_when, "public_due": todo.due_at, "stored_due": stored.due_at, "user_edited": stored.user_edited, "field_sources": stored.field_sources, "reason": stored.reason})
    );
    assert_eq!(todo.due_at, Some(chosen()));
    assert_eq!(stored.due_at, Some(chosen()));
    assert!(stored.user_edited);
    assert_eq!(
        mxr_todo::provenance::FieldSources::from_json(&stored.field_sources)
            .get("due_at")
            .unwrap()
            .source,
        mxr_todo::provenance::FieldSource::User
    );
    assert_eq!(stored.reason, original.reason);
    assert_eq!(stored.title, original.title);
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

#[tokio::test]
async fn public_record_promise_preserves_closed_and_user_edited_task_decisions() {
    for decision in ["done", "dismissed", "edited"] {
        let mut config = mxr_config::MxrConfig::default();
        config.llm.enabled = false;
        config.search.semantic.enabled = false;
        config.search.semantic.auto_download_models = false;
        config.activity.enabled = false;
        let fx = Fixture::with_config(config).await;
        let mail = source(&fx, "I'll send the report.").await;
        settle(&fx, &mail).await;
        let original = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap()
            .remove(0);
        if decision == "edited" {
            request(
                &fx,
                Request::UpdateTodo {
                    todo_id: original.id.clone(),
                    edits: vec![
                        mxr_protocol::TodoEditData {
                            field: "title".into(),
                            value: "My chosen report title".into(),
                        },
                        mxr_protocol::TodoEditData {
                            field: "due".into(),
                            value: "2026-11-06T12:00:00Z".into(),
                        },
                    ],
                    time_zone: Some("UTC".into()),
                    dry_run: false,
                },
            )
            .await;
        } else {
            request(
                &fx,
                Request::SetTodoState {
                    todo_ids: vec![original.id.clone()],
                    action: if decision == "done" {
                        mxr_protocol::TodoStateActionData::Done
                    } else {
                        mxr_protocol::TodoStateActionData::Dismiss
                    },
                    dry_run: false,
                },
            )
            .await;
        }
        let protected = fx
            .state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap();
        let ResponseData::RecordedPromise { commitment, .. } = request(
            &fx,
            Request::RecordPromise {
                message_id: mail.id.clone(),
                what: original.reason.clone(),
                due_at: chosen(),
                dry_run: false,
            },
        )
        .await
        else {
            panic!("recorded promise")
        };
        assert_eq!(
            original.commitment_id.as_deref(),
            Some(commitment.id.as_str())
        );
        let ResponseData::Todo { todo } = request(
            &fx,
            Request::GetTodo {
                todo_id: original.id.clone(),
            },
        )
        .await
        else {
            panic!("public todo")
        };
        assert_eq!(todo.id, original.id);
        assert_eq!(
            fx.state
                .store
                .get_todo(&original.id)
                .await
                .unwrap()
                .unwrap(),
            protected,
            "{decision}"
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
