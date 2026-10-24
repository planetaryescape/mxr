//! Explicit promise dates use the retained task kind's delivery window.
use super::desk::{request, Fixture, ME};
use super::promise_projection::{chosen, settle, source};
use super::*;
use chrono::{Duration, Local};
use mxr_todo::provenance::{FieldSource, FieldSources};
use mxr_todo::timing::{lead_time, window, TimingInput};
use mxr_todo::TodoKind;

const TEXT: &str = "I'll send the tenancy notice by 28 February 2027.";

async fn record(
    fx: &Fixture,
    mail: &mxr_core::Envelope,
    what: &str,
    due: chrono::DateTime<chrono::Utc>,
) -> String {
    let ResponseData::RecordedPromise { commitment, .. } = request(
        fx,
        Request::RecordPromise {
            message_id: mail.id.clone(),
            what: what.into(),
            due_at: due,
            dry_run: false,
        },
    )
    .await
    else {
        panic!("record promise")
    };
    commitment.id
}

async fn corrected_kind_journey(fx: &Fixture) -> (mxr_core::Envelope, mxr_store::TodoRecord) {
    let mail = source(fx, TEXT).await;
    settle(fx, &mail).await;
    let tasks = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    let original = &tasks[0];
    assert_eq!(original.kind, "promise");
    assert!(original.has_stored_asks());
    let answers = fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    let answer = &answers[&mail.id];
    eprintln!("REVIEW8_STORED_ASSESSMENT {answer:?}");
    let promises: Vec<_> = answer
        .units
        .iter()
        .filter(|unit| unit.answer == mxr_core::asks::AskAnswer::YouPromised)
        .collect();
    assert_eq!(promises.len(), 1);
    assert_eq!(answer.quote(promises[0]), Some(TEXT));
    let a = chosen() + Duration::days(120);
    let id = record(fx, &mail, TEXT, a).await;
    let ResponseData::TodoChange { change } = request(
        fx,
        Request::CorrectTodo {
            todo_id: original.id.clone(),
            reason: mxr_protocol::TodoCorrectionData::WrongDetails,
            edits: vec![mxr_protocol::TodoEditData {
                field: "kind".into(),
                value: "lease".into(),
            }],
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    else {
        panic!("correct kind")
    };
    assert!(change.correction_undo.is_some());
    let corrected = fx
        .state
        .store
        .get_todo(&original.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(corrected.due_at, Some(a));
    assert_eq!(corrected.scheduled_for, None);
    let b = a + Duration::days(40);
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
        sqlx::query_scalar::<_, i64>("SELECT total_changes()")
            .fetch_one(fx.state.store.writer())
            .await
            .unwrap(),
        changes
    );
    assert_eq!(record(fx, &mail, TEXT, b).await, id);
    let ResponseData::Todo { todo } = request(
        fx,
        Request::GetTodo {
            todo_id: original.id.clone(),
        },
    )
    .await
    else {
        panic!("get todo")
    };
    let actual = fx
        .state
        .store
        .get_todo(&original.id)
        .await
        .unwrap()
        .unwrap();
    let input = TimingInput {
        kind: TodoKind::Lease,
        doc_type: None,
        arrived: mail.date,
        due: Some(b),
        event_start: None,
    };
    let cfg = super::super::todos::pass_config(&fx.state, chrono::Utc::now());
    let timing = lead_time(&input, &Local, cfg.morning_hour);
    let relevant = window(&input);
    eprintln!("REVIEW8_IMMEDIATE actual={actual:?} expected={timing:?} window={relevant:?}");
    assert_eq!(actual.id, original.id);
    assert_eq!(actual.commitment_id.as_deref(), Some(id.as_str()));
    assert_eq!(actual.reason, TEXT);
    assert_eq!(todo.kind, "lease");
    assert_eq!(todo.due_at, Some(b));
    assert_eq!(todo.act_by_at, timing.act_by);
    assert_eq!(actual.surface_at, timing.surface_at);
    assert_eq!(actual.relevant_until, relevant.until);
    let fields = FieldSources::from_json(&actual.field_sources);
    assert_eq!(fields.get("kind").unwrap().source, FieldSource::User);
    assert_eq!(fields.get("due_at").unwrap().source, FieldSource::User);
    for (field, rule) in [
        ("act_by_at", timing.act_by_rule),
        ("surface_at", timing.surface_rule),
        ("relevant_until", relevant.rule),
    ] {
        assert_eq!(fields.get(field).unwrap().evidence.as_deref(), Some(rule));
    }
    let promise = lead_time(
        &TimingInput {
            kind: TodoKind::Promise,
            ..input
        },
        &Local,
        cfg.morning_hour,
    );
    let at =
        timing.surface_at.unwrap() + (promise.surface_at.unwrap() - timing.surface_at.unwrap()) / 2;
    let runway = super::super::todos::runway_at(&fx.state, Some(&fx.account), true, at)
        .await
        .unwrap();
    assert!(
        runway.now.iter().any(|row| row.id == actual.id),
        "{runway:?}"
    );
    let now = super::super::now::now_at(&fx.state, Some(&fx.account), at, &Local)
        .await
        .unwrap();
    assert!(
        now.due_soon
            .todos
            .iter()
            .any(|row| row.todo.id == actual.id),
        "{now:?}"
    );
    settle(fx, &mail).await;
    assert_eq!(
        fx.state.store.get_todo(&actual.id).await.unwrap(),
        Some(actual.clone())
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
    (mail, actual)
}

#[tokio::test]
async fn canonical_reacceptance_retimes_the_corrected_kind() {
    corrected_kind_journey(&Fixture::new().await).await;
}

#[tokio::test]
async fn corrected_kind_timing_survives_source_retraction_and_exact_undo() {
    let fx = Fixture::new().await;
    let (mail, before) = corrected_kind_journey(&fx).await;
    let ResponseData::TodoChange { change } = request(
        &fx,
        Request::CorrectTodo {
            todo_id: before.id.clone(),
            reason: mxr_protocol::TodoCorrectionData::WrongDetails,
            edits: vec![mxr_protocol::TodoEditData {
                field: "title".into(),
                value: "Give tenancy notice".into(),
            }],
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    else {
        panic!("correct title")
    };
    request(
        &fx,
        Request::UndoTodoCorrection {
            todo_id: before.id.clone(),
            correction_id: change.correction_undo.unwrap(),
            dry_run: false,
        },
    )
    .await;
    assert_eq!(
        fx.state.store.get_todo(&before.id).await.unwrap(),
        Some(before.clone())
    );
    super::promise_projection::replace(&fx, &mail, "The tenancy notice has been sent.").await;
    settle(&fx, &mail).await;
    assert_eq!(
        fx.state.store.get_todo(&before.id).await.unwrap(),
        Some(before)
    );
}

#[test]
fn corrected_kind_timing_survives_real_reopen() {
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
                let (mail, before) = corrected_kind_journey(&fx).await;
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
async fn unchanged_canonical_and_corrected_accepted_only_kinds_keep_their_own_timing() {
    for (text, kind, canonical) in [
        (TEXT, TodoKind::Promise, true),
        ("I'll prepare the tenancy outline.", TodoKind::Lease, false),
    ] {
        let fx = Fixture::new().await;
        let mail = source(&fx, text).await;
        settle(&fx, &mail).await;
        let answers = fx
            .state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
            .await
            .unwrap();
        assert_eq!(
            answers[&mail.id]
                .units
                .iter()
                .any(|unit| unit.answer == mxr_core::asks::AskAnswer::YouPromised),
            canonical
        );
        let a = chosen() + Duration::days(120);
        let id = record(&fx, &mail, text, a).await;
        let mut rows = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        let original = rows.remove(0);
        if kind == TodoKind::Lease {
            request(
                &fx,
                Request::CorrectTodo {
                    todo_id: original.id.clone(),
                    reason: mxr_protocol::TodoCorrectionData::WrongDetails,
                    edits: vec![mxr_protocol::TodoEditData {
                        field: "kind".into(),
                        value: "lease".into(),
                    }],
                    time_zone: None,
                    dry_run: false,
                },
            )
            .await;
        }
        let b = a + Duration::days(40);
        assert_eq!(record(&fx, &mail, text, b).await, id);
        let ResponseData::Todo { todo } = request(
            &fx,
            Request::GetTodo {
                todo_id: original.id.clone(),
            },
        )
        .await
        else {
            panic!("get control")
        };
        let input = TimingInput {
            kind,
            doc_type: None,
            arrived: mail.date,
            due: Some(b),
            event_start: None,
        };
        let cfg = super::super::todos::pass_config(&fx.state, chrono::Utc::now());
        let timing = lead_time(&input, &Local, cfg.morning_hour);
        assert_eq!(todo.kind, kind.as_str());
        assert_eq!(todo.due_at, Some(b));
        assert_eq!(todo.act_by_at, timing.act_by);
        assert_eq!(todo.surface_at, timing.surface_at);
        assert_eq!(todo.relevant_until, window(&input).until);
        let before = fx
            .state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before.reason, text);
        settle(&fx, &mail).await;
        assert_eq!(
            fx.state.store.get_todo(&original.id).await.unwrap(),
            Some(before)
        );
    }
}

#[tokio::test]
async fn replay_repairs_previously_persisted_promise_timing_without_changing_the_accepted_date() {
    let fx = Fixture::new().await;
    let (mail, expected) = corrected_kind_journey(&fx).await;
    let cfg = super::super::todos::pass_config(&fx.state, chrono::Utc::now());
    let input = TimingInput {
        kind: TodoKind::Promise,
        doc_type: None,
        arrived: mail.date,
        due: expected.due_at,
        event_start: None,
    };
    let old = lead_time(&input, &Local, cfg.morning_hour);
    // Faithful published row: Lease fields/provenance, but Promise-derived dates from reacceptance.
    sqlx::query("UPDATE todos SET act_by_at=?, surface_at=?, relevant_until=? WHERE id=?")
        .bind(old.act_by.map(|at| at.timestamp()))
        .bind(old.surface_at.map(|at| at.timestamp()))
        .bind(window(&input).until.map(|at| at.timestamp()))
        .bind(&expected.id)
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    let broken = fx
        .state
        .store
        .get_todo(&expected.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(broken.kind, "lease");
    assert_eq!(broken.due_at, expected.due_at);
    assert_ne!(broken.act_by_at, expected.act_by_at);
    settle(&fx, &mail).await;
    let actual = fx
        .state
        .store
        .get_todo(&expected.id)
        .await
        .unwrap()
        .unwrap();
    eprintln!("REVIEW8_RETAINED_ROW broken={broken:?} replayed={actual:?}");
    assert_eq!(actual.act_by_at, expected.act_by_at);
    assert_eq!(actual.surface_at, expected.surface_at);
    assert_eq!(actual.relevant_until, expected.relevant_until);
    assert_eq!(actual.field_sources, expected.field_sources);
    assert_eq!(actual.due_at, expected.due_at);
    assert_eq!(actual.kind, expected.kind);
    assert_eq!(actual.reason, expected.reason);
    assert_eq!(actual.commitment_id, expected.commitment_id);
    assert_eq!(actual.dedup_key, expected.dedup_key);
    // A projector that read the old kind cannot replace the corrected kind's timing.
    let mut stale = actual.clone();
    stale.kind = "promise".into();
    stale.act_by_at = old.act_by;
    stale.surface_at = old.surface_at;
    stale.relevant_until = window(&input).until;
    let answers = fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    assert!(matches!(
        fx.state
            .store
            .upsert_asks_todo(&stale, &answers[&mail.id])
            .await
            .unwrap(),
        mxr_store::TodoUpsert::Unchanged
    ));
    assert_eq!(
        fx.state.store.get_todo(&expected.id).await.unwrap(),
        Some(actual.clone())
    );
    settle(&fx, &mail).await;
    assert_eq!(
        fx.state.store.get_todo(&expected.id).await.unwrap(),
        Some(actual)
    );
}
