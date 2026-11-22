use super::desk::{request, Fixture};
use super::promise_projection::{chosen, settle, source};
use super::*;
use chrono::{Duration, Local};
use mxr_todo::provenance::{FieldSource, FieldSources};
use mxr_todo::timing::{lead_time, window, TimingInput};
use mxr_todo::TodoKind;

fn text() -> &'static str {
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        let by = chrono::Utc::now().date_naive() + Duration::days(120);
        format!(
            "I'll send the tenancy notice by {}.",
            by.format("%-d %B %Y")
        )
    })
}

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
    let mail = source(fx, text()).await;
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
    assert_eq!(answer.quote(promises[0]), Some(text()));
    let a = chosen() + Duration::days(120);
    let id = record(fx, &mail, text(), a).await;
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
            what: text().into(),
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
    assert_eq!(record(fx, &mail, text(), b).await, id);
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
    assert_eq!(actual.reason, text());
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
