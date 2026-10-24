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
