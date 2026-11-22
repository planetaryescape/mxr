use super::desk::{request, Fixture};
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
