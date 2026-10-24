use super::desk::{request, Fixture, ME};
use super::promise_projection::{replace, settle};
use super::*;
use chrono::Duration;
use mxr_core::{Address, MessageDirection, ThreadId};
use mxr_store::TodoState;

#[tokio::test]
async fn review3_outgoing_cc_request_reaches_public_waiting_without_owner_task() {
    for external_to in [false, true] {
        let fx = Fixture::new().await;
        fx.state
            .store
            .add_account_address(&fx.account, "alias@example.com", false)
            .await
            .unwrap();
        let mut mail = fx
            .message(&ThreadId::new(), ME, ME, Duration::hours(48), None)
            .await;
        mail.cc = vec![
            Address {
                name: None,
                email: "ALIAS@example.com".into(),
            },
            Address {
                name: None,
                email: "nora@example.com".into(),
            },
        ];
        let expected = if external_to {
            "leo@example.com"
        } else {
            "nora@example.com"
        };
        if external_to {
            mail.to.push(Address {
                name: None,
                email: expected.into(),
            });
        }
        fx.store_envelope(&mail, MessageDirection::Outbound).await;
        replace(&fx, &mail, "Could you reply with your availability?").await;
        settle(&fx, &mail).await;
        let answers = fx
            .state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
            .await
            .unwrap();
        let commitments = fx
            .state
            .store
            .list_contact_commitments(&fx.account, None, None)
            .await
            .unwrap();
        let tasks = fx
            .state
            .store
            .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
            .await
            .unwrap();
        let ResponseData::Desk { waiting, .. } = fx.desk(25).await else {
            panic!("desk")
        };
        eprintln!("REVIEW3_CC answers={answers:?} commitments={commitments:?} tasks={tasks:?} waiting={waiting:?}");
        assert!(answers[&mail.id]
            .units
            .iter()
            .any(|u| u.answer == mxr_core::asks::AskAnswer::YouAskedThem));
        assert_eq!(commitments.len(), 1);
        assert_eq!(commitments[0].email, expected);
        assert!(tasks.is_empty());
        assert_eq!(waiting.rows.len(), 1);
        assert_eq!(waiting.rows[0].thread_id, mail.thread_id);
        assert_eq!(waiting.rows[0].counterparty_email, expected);
        let reply = fx
            .message(
                &mail.thread_id,
                expected,
                ME,
                Duration::minutes(5),
                mail.message_id_header.as_deref(),
            )
            .await;
        replace(&fx, &reply, "Tuesday afternoon works.").await;
        settle(&fx, &reply).await;
        let ResponseData::Desk { waiting, .. } = fx.desk(25).await else {
            panic!("desk")
        };
        assert!(waiting.rows.is_empty(), "a real answer clears waiting");
    }
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
    assert_eq!(tasks[0].reason, text);
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

#[tokio::test]
async fn review3_independent_payment_remediations_keep_their_own_tasks() {
    for (text, expected) in [
        ("Your payment failed. Please update your payment details. Please sign the contract.", 2),
        ("Your payment failed.\n\nPlease update your payment details.", 2),
        ("Your payment of £25 for account #A123 failed. Please update your payment details for account #B456, amount due €40 by 1 December 2026.", 2),
        ("Your payment of £25 due by 1 November 2026 failed. Please update your payment details for £25 due by 1 December 2026.", 2),
        ("Your payment failed last month but is now resolved.", 0),
    ] {
        let fx = Fixture::new().await;
        let mail = fx.message(&ThreadId::new(), "billing@example.com", ME, Duration::minutes(5), None).await;
        replace(&fx, &mail, text).await;
        settle(&fx, &mail).await;
        let tasks = fx.state.store.list_todos_in_state(Some(&fx.account), TodoState::Open, 100).await.unwrap();
        eprintln!("REVIEW3_INDEPENDENT {text:?} tasks={tasks:?}");
        assert_eq!(tasks.len(), expected, "{text}");
    }
}

#[tokio::test]
async fn review3_policy13_grouping_preserves_saved_decisions_and_exact_undo() {
    for reason in [
        mxr_protocol::TodoCorrectionData::AlreadyDone,
        mxr_protocol::TodoCorrectionData::NotATodo,
        mxr_protocol::TodoCorrectionData::WrongDetails,
    ] {
        for protected_index in [0, 1] {
            let fx = Fixture::new().await;
            let mail = fx
                .message(
                    &ThreadId::new(),
                    "billing@example.com",
                    ME,
                    Duration::minutes(5),
                    None,
                )
                .await;
            let text = "Your payment of £25 failed. Please update your payment details.";
            replace(&fx, &mail, text).await;
            settle(&fx, &mail).await;
            let mut old = fx
                .state
                .store
                .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
                .await
                .unwrap()
                .remove(&mail.id)
                .unwrap();
            // Reproduce the two policy13 units observed in the native RED receipt.
            let unit_index = old
                .units
                .iter()
                .position(|u| u.answer == mxr_core::asks::AskAnswer::AsksYou)
                .unwrap();
            let mut first = old.units[unit_index].clone();
            first.span = mxr_core::asks::AskSpan {
                start: 0,
                end: text.find(". ").unwrap() + 1,
            };
            let mut second = first.clone();
            second.span = mxr_core::asks::AskSpan {
                start: first.span.end + 1,
                end: text.len(),
            };
            // Policy13 selected amounts within each unit's exact byte span.
            second.tags.as_mut().unwrap().amount = None;
            for vote in &mut second.votes {
                vote.tags.amount = None;
            }
            old.units.splice(unit_index..=unit_index, [first, second]);
            old.policy_version = "asks-r2c-13".into();
            sqlx::query("DELETE FROM todos WHERE source_message_id=?")
                .bind(mail.id.as_str())
                .execute(fx.state.store.writer())
                .await
                .unwrap();
            let scan = fx
                .state
                .store
                .list_todo_scan_rows(std::slice::from_ref(&mail.id))
                .await
                .unwrap()
                .remove(0);
            let cfg = mxr_todo::pass::PassConfig {
                now: chrono::Utc::now(),
                tz: mxr_store::AsksTimezone::resolve().unwrap(),
                morning_hour: 9,
                catchup_days: 30,
                catchup_max: 100,
            };
            let mut rows = Vec::new();
            for (_, detection) in mxr_todo::asks::items::detections(&old, &mail) {
                let (row, _) = mxr_todo::pass::stored_record(&cfg, &scan, detection, false);
                fx.state.store.insert_todo(&row).await.unwrap();
                rows.push(row);
            }
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].currency.as_deref(), Some("GBP"));
            assert_eq!(rows[1].currency, None);
            eprintln!("REVIEW4_OLD_UNITS {old:?} rows={rows:?}");
            let original = fx
                .state
                .store
                .get_todo(&rows[protected_index].id)
                .await
                .unwrap()
                .unwrap();
            let edits = if reason == mxr_protocol::TodoCorrectionData::WrongDetails {
                vec![mxr_protocol::TodoEditData {
                    field: "title".into(),
                    value: "Call the bank myself".into(),
                }]
            } else {
                vec![]
            };
            let ResponseData::TodoChange { change } = request(
                &fx,
                Request::CorrectTodo {
                    todo_id: original.id.clone(),
                    reason,
                    edits,
                    time_zone: None,
                    dry_run: false,
                },
            )
            .await
            else {
                panic!("correction")
            };
            let token = change.correction_undo.unwrap();
            let corrected = fx
                .state
                .store
                .get_todo(&original.id)
                .await
                .unwrap()
                .unwrap();
            sqlx::query("UPDATE mail_assessments SET policy_version='asks-r2c-13', assessment_json=? WHERE message_id=? AND policy_version=?")
            .bind(serde_json::to_string(&old).unwrap()).bind(mail.id.as_str()).bind(mxr_core::asks::ASKS_POLICY_VERSION).execute(fx.state.store.writer()).await.unwrap();
            assert!(fx
                .state
                .store
                .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
                .await
                .unwrap()
                .is_empty());
            super::super::assessments::reconcile_account(&fx.state, &fx.account)
                .await
                .unwrap();
            assert_eq!(
                fx.state
                    .store
                    .get_todo(&original.id)
                    .await
                    .unwrap()
                    .unwrap(),
                corrected
            );
            let open = fx
                .state
                .store
                .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
                .await
                .unwrap();
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
                panic!("public active todos")
            };
            eprintln!("REVIEW4_GROUPED protected={protected_index} reason={reason:?} open={open:?} public={todos:?}");
            assert_eq!(
                todos.len(),
                usize::from(reason == mxr_protocol::TodoCorrectionData::WrongDetails)
            );
            if let Some(saved) = todos.first() {
                assert_eq!(saved.id, original.id);
                assert_eq!(saved.title, corrected.title);
            }
            assert_eq!(
                open.len(),
                usize::from(reason == mxr_protocol::TodoCorrectionData::WrongDetails)
            );
            request(
                &fx,
                Request::UndoTodoCorrection {
                    todo_id: original.id.clone(),
                    correction_id: token,
                    dry_run: false,
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
            settle(&fx, &mail).await;
            assert_eq!(
                fx.state
                    .store
                    .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
                    .await
                    .unwrap()
                    .len(),
                1
            );
        }
    }
}

#[tokio::test]
async fn review3_cc_waiting_preserves_conservative_gates_and_explicit_timer() {
    for (recipient, age, archived) in [
        ("nora@example.com", Duration::hours(8), false),
        ("no-reply@example.com", Duration::hours(48), false),
        ("nora@example.com", Duration::hours(48), true),
    ] {
        let fx = Fixture::new().await;
        let thread = ThreadId::new();
        if archived {
            let prior = fx
                .message(&thread, recipient, ME, Duration::hours(72), None)
                .await;
            fx.state
                .store
                .set_message_labels(&prior.id, &[], mxr_core::EventSource::User)
                .await
                .unwrap();
        }
        let mut mail = fx.message(&thread, ME, ME, age, None).await;
        mail.cc.push(Address {
            name: None,
            email: recipient.into(),
        });
        fx.store_envelope(&mail, MessageDirection::Outbound).await;
        replace(&fx, &mail, "Could you reply with your availability?").await;
        settle(&fx, &mail).await;
        let ResponseData::Desk { waiting, .. } = fx.desk(25).await else {
            panic!("desk")
        };
        assert!(
            waiting.rows.is_empty(),
            "{recipient} {age} archived={archived}"
        );
        let due = chrono::Utc::now() - Duration::minutes(1);
        fx.state
            .store
            .set_auto_reminder(&mail.id, &fx.account, due, mail.date)
            .await
            .unwrap();
        let ResponseData::Desk { waiting, .. } = fx.desk(25).await else {
            panic!("desk")
        };
        assert_eq!(
            waiting.rows.len(),
            1,
            "explicit timer remains authoritative"
        );
        assert_eq!(waiting.rows[0].counterparty_email, recipient);
        assert!(waiting.rows[0].back_at.is_some());
    }
}

#[tokio::test]
async fn review3_incoming_correspondent_promise_still_waits_on_the_sender() {
    let fx = Fixture::new().await;
    let mail = fx
        .message(
            &ThreadId::new(),
            "nora@example.com",
            ME,
            Duration::hours(48),
            None,
        )
        .await;
    replace(&fx, &mail, "I'll send the report tomorrow.").await;
    settle(&fx, &mail).await;
    let answers = fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    assert!(answers[&mail.id]
        .units
        .iter()
        .any(|u| u.answer == mxr_core::asks::AskAnswer::TheyPromisedYou));
    let ResponseData::Desk { waiting, .. } = fx.desk(25).await else {
        panic!("desk")
    };
    assert_eq!(waiting.rows.len(), 1);
    assert_eq!(waiting.rows[0].counterparty_email, "nora@example.com");
    assert!(fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
        .await
        .unwrap()
        .is_empty());
}
