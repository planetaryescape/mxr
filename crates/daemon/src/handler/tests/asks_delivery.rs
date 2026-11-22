use super::desk::{request, Fixture, ME};
use super::promise_projection::{replace, settle};
use super::*;
use chrono::Duration;
use mxr_core::ThreadId;
use mxr_store::TodoState;

#[tokio::test]
async fn a_greeted_sole_recipient_without_a_name_gets_the_public_task() {
    let fx = Fixture::new().await;
    let mail = fx
        .message(
            &ThreadId::new(),
            "sam@example.com",
            ME,
            Duration::minutes(5),
            None,
        )
        .await;
    replace(
        &fx,
        &mail,
        "Hi Alex,\nThe renewal is attached. Please sign and send it back by tomorrow.\nThanks, Sam",
    )
    .await;
    settle(&fx, &mail).await;
    let ResponseData::Todos { todos } = request(
        &fx,
        Request::ListTodos {
            account_id: Some(fx.account.clone()),
            state: mxr_protocol::TodoStateData::Open,
            limit: 100,
        },
    )
    .await
    else {
        panic!("public todos")
    };
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].source_message_id.as_ref(), Some(&mail.id));
    assert_eq!(todos[0].why, "Please sign and send it back by tomorrow.");
    assert!(todos[0].due_at.is_some());
}

#[tokio::test]
async fn review3_failed_payment_and_polite_remediation_have_one_stored_task() {
    let fx = Fixture::new().await;
    fx.state
        .store
        .start_todo_run(&fx.account, mxr_todo::RULES_VERSION, chrono::Utc::now())
        .await
        .unwrap();
    fx.state
        .store
        .complete_todo_run(&fx.account, chrono::Utc::now())
        .await
        .unwrap();
    let mail = fx
        .message(
            &ThreadId::new(),
            "billing@example.com",
            ME,
            Duration::minutes(5),
            None,
        )
        .await;
    let text = "Your payment failed. Please update your payment details.";
    super::alert_todos::authenticated_known_source(&fx, &mail).await;
    replace(&fx, &mail, text).await;
    settle(&fx, &mail).await;
    let answers = fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    let tasks = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
        .await
        .unwrap();
    let packets = fx
        .state
        .store
        .classify_eval_packet(std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    eprintln!("REVIEW3_PAYMENT answers={answers:?} tasks={tasks:?} packets={packets:?}");
    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].reason.contains("verified origin example.com"));
    assert_eq!(
        mxr_todo::provenance::FieldSources::from_json(&tasks[0].field_sources)
            .get("quote")
            .and_then(|f| f.evidence.as_deref()),
        Some(text)
    );
    let ResponseData::Todos { todos } = request(
        &fx,
        Request::ListTodos {
            account_id: Some(fx.account.clone()),
            state: mxr_protocol::TodoStateData::Open,
            limit: 100,
        },
    )
    .await
    else {
        panic!("public todos")
    };
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].id, tasks[0].id);
    assert_eq!(
        answers[&mail.id]
            .units
            .iter()
            .filter(|u| u.answer == mxr_core::asks::AskAnswer::AsksYou)
            .count(),
        1
    );
    let (unit_index, unit) = answers[&mail.id]
        .units
        .iter()
        .enumerate()
        .find(|(_, u)| u.answer == mxr_core::asks::AskAnswer::AsksYou)
        .unwrap();
    assert_eq!(answers[&mail.id].quote(unit), Some(text));
    assert_eq!(
        unit.span,
        mxr_core::asks::AskSpan {
            start: 0,
            end: text.len()
        }
    );
    assert_eq!(packets[0].projected_todos.len(), 1);
    assert!(!packets[0].projection_pending);
    assert_eq!(
        packets[0].projected_todos[0].unit_provenance,
        mxr_todo::asks::items::unit_provenance(&answers[&mail.id], unit_index, unit)
    );
    settle(&fx, &mail).await;
    let again = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
        .await
        .unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].id, tasks[0].id);
}
