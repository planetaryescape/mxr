use super::desk::{request, Fixture, ME};
use super::promise_projection::{replace, settle, source};
use super::*;
use chrono::Duration;
use mxr_core::ThreadId;
use mxr_store::TodoState;

#[tokio::test]
async fn review2_sign_verify_speech_acts_control_stored_task_admission() {
    for (text, expected) in [
        ("I can sign the contract.", 0),
        ("She will sign the contract.", 0),
        ("I can verify your identity.", 0),
        ("She will verify your identity.", 0),
        ("Please sign the contract.", 1),
        ("Please verify your identity.", 1),
        ("Your signature required.", 1),
        ("The contract must be signed.", 1),
        ("I'll sign the contract.", 0),
    ] {
        let fx = Fixture::new().await;
        let mail = fx
            .message(
                &ThreadId::new(),
                "nora@example.com",
                ME,
                Duration::minutes(5),
                None,
            )
            .await;
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
        eprintln!(
            "REVIEW2_ACT {text:?} answers={:?} tasks={tasks:?}",
            answers[&mail.id].units
        );
        assert_eq!(tasks.len(), expected, "{text}");
        assert_eq!(
            answers[&mail.id]
                .units
                .iter()
                .filter(|u| u.answer == mxr_core::asks::AskAnswer::AsksYou)
                .count(),
            expected,
            "{text}"
        );
    }
}

#[tokio::test]
async fn review2_currency_balances_keep_distinct_rows_and_unit_links() {
    for separate in [false, true] {
        let fx = Fixture::new().await;
        let a = "Account #A123 amount due £25 by 1 November 2026.";
        let b = "Account #A123 amount due €40 by 1 November 2026.";
        let first = fx
            .message(
                &ThreadId::new(),
                "billing@example.com",
                ME,
                Duration::minutes(5),
                None,
            )
            .await;
        replace(
            &fx,
            &first,
            &if separate {
                a.into()
            } else {
                format!("{a}\n\n{b}")
            },
        )
        .await;
        settle(&fx, &first).await;
        if separate {
            let second = fx
                .message(
                    &ThreadId::new(),
                    "billing@example.com",
                    ME,
                    Duration::minutes(4),
                    None,
                )
                .await;
            replace(&fx, &second, b).await;
            settle(&fx, &second).await;
            // A new correspondent message invalidates the earlier thread's context.
            // Exercise the normal account reconciler before asserting current links.
            super::super::assessments::reconcile_account(&fx.state, &fx.account)
                .await
                .unwrap();
        }
        let tasks = fx
            .state
            .store
            .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
            .await
            .unwrap();
        eprintln!("REVIEW2_CURRENCY separate={separate} tasks={tasks:?}");
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM messages WHERE account_id=?")
            .bind(fx.account.as_str())
            .fetch_all(fx.state.store.reader())
            .await
            .unwrap();
        let ids: Vec<mxr_core::MessageId> = ids.iter().map(|id| id.parse().unwrap()).collect();
        let packets = fx.state.store.classify_eval_packet(&ids).await.unwrap();
        assert_eq!(
            packets
                .iter()
                .map(|p| p.projected_todos.len())
                .sum::<usize>(),
            2
        );
        assert!(packets
            .iter()
            .all(|p| p.asks.is_some() && !p.projection_pending));
        let links: std::collections::HashSet<_> = packets
            .iter()
            .flat_map(|p| p.projected_todos.iter().map(|t| t.unit_provenance.as_str()))
            .collect();
        assert_eq!(links.len(), 2);
        assert_eq!(tasks.len(), 2);
        assert!(tasks
            .iter()
            .any(|t| t.currency.as_deref() == Some("GBP") && t.amount_minor == Some(2500)));
        assert!(tasks
            .iter()
            .any(|t| t.currency.as_deref() == Some("EUR") && t.amount_minor == Some(4000)));
        assert_ne!(tasks[0].dedup_key, tasks[1].dedup_key);
        assert_ne!(tasks[0].field_sources, tasks[1].field_sources);
    }
}

#[tokio::test]
async fn firm_sign_and_verify_promises_retain_speaker_and_exact_quote() {
    for outbound in [false, true] {
        for text in ["I'll sign the contract.", "I'll verify your identity."] {
            let fx = Fixture::new().await;
            let mail = if outbound {
                source(&fx, text).await
            } else {
                let mail = fx
                    .message(
                        &ThreadId::new(),
                        "nora@example.com",
                        ME,
                        Duration::minutes(5),
                        None,
                    )
                    .await;
                replace(&fx, &mail, text).await;
                mail
            };
            settle(&fx, &mail).await;
            let answers = fx
                .state
                .store
                .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
                .await
                .unwrap();
            let answer = &answers[&mail.id];
            let expected = if outbound {
                mxr_core::asks::AskAnswer::YouPromised
            } else {
                mxr_core::asks::AskAnswer::TheyPromisedYou
            };
            let unit = answer
                .units
                .iter()
                .find(|u| u.answer == expected)
                .expect("firm speaker promise");
            assert_eq!(answer.quote(unit), Some(text));
            let tasks = fx
                .state
                .store
                .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
                .await
                .unwrap();
            assert_eq!(tasks.len(), usize::from(outbound));
        }
    }
}

#[tokio::test]
async fn currency_identity_adopts_previous_rows_and_keeps_completed_decisions() {
    for closed in [false, true] {
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
        let eur = "Account #A123 amount due €40 by 1 November 2026.";
        replace(&fx, &mail, eur).await;
        settle(&fx, &mail).await;
        let original = fx
            .state
            .store
            .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
            .await
            .unwrap()
            .remove(0);
        if closed {
            request(
                &fx,
                Request::SetTodoState {
                    todo_ids: vec![original.id.clone()],
                    action: mxr_protocol::TodoStateActionData::Done,
                    dry_run: false,
                },
            )
            .await;
        }
        let legacy_key = format!(
            "asks|payment|{}|due:2026-11",
            mxr_store::assessment_fingerprint("billing@example.com|payment|account:a123")
        );
        sqlx::query("UPDATE todos SET dedup_key=?, field_sources=replace(field_sources, 'asks-r2c-13:', 'asks-r2c-12:') WHERE id=?").bind(&legacy_key).bind(&original.id).execute(fx.state.store.writer()).await.unwrap();
        replace(
            &fx,
            &mail,
            &format!("Account #A123 amount due £25 by 1 November 2026.\n\n{eur}"),
        )
        .await;
        settle(&fx, &mail).await;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM todos")
            .fetch_one(fx.state.store.reader())
            .await
            .unwrap();
        assert_eq!(count, 2, "no obsolete currency-free duplicate");
        let retained = fx
            .state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap();
        if closed {
            assert_eq!(retained.dedup_key, legacy_key);
        } else {
            assert_ne!(retained.dedup_key, legacy_key);
        }
        assert_eq!(retained.currency.as_deref(), Some("EUR"));
        assert_eq!(
            retained.state,
            if closed {
                TodoState::Done
            } else {
                TodoState::Open
            }
        );
        let rows = fx
            .state
            .store
            .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
            .await
            .unwrap();
        assert_eq!(rows.len(), if closed { 1 } else { 2 });
        let reminder = fx
            .message(
                &ThreadId::new(),
                "billing@example.com",
                ME,
                Duration::minutes(4),
                None,
            )
            .await;
        replace(&fx, &reminder, "Account #A123 amount due €40.").await;
        settle(&fx, &reminder).await;
        super::super::assessments::reconcile_account(&fx.state, &fx.account)
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM todos")
            .fetch_one(fx.state.store.reader())
            .await
            .unwrap();
        assert_eq!(
            count, 2,
            "an undated reminder respects the migrated identity"
        );
        assert_eq!(
            fx.state
                .store
                .get_todo(&original.id)
                .await
                .unwrap()
                .unwrap()
                .state,
            if closed {
                TodoState::Done
            } else {
                TodoState::Open
            }
        );
    }
}

#[tokio::test]
async fn same_currency_reminders_and_payment_escalation_reuse_one_identity() {
    let fx = Fixture::new().await;
    let mut first_id = None;
    for text in [
        "Account #A123 amount due £25 by 1 November 2026.",
        "Reminder: Account #A123 amount due £25 by 1 November 2026.",
        "Account #A123 payment of £25 failed. Update your payment method by 1 November 2026.",
        "Account #A123 amount due £25.",
    ] {
        let mail = fx
            .message(
                &ThreadId::new(),
                "billing@example.com",
                ME,
                Duration::minutes(5),
                None,
            )
            .await;
        replace(&fx, &mail, text).await;
        settle(&fx, &mail).await;
        super::super::assessments::reconcile_account(&fx.state, &fx.account)
            .await
            .unwrap();
        let rows = fx
            .state
            .store
            .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "{text}: {rows:?}");
        if let Some(id) = &first_id {
            assert_eq!(&rows[0].id, id);
        } else {
            first_id = Some(rows[0].id.clone());
        }
    }
}

#[tokio::test]
async fn currency_upgrade_preserves_the_exact_durable_correction_undo() {
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
    let quote = "Account #A123 amount due €40 by 1 November 2026.";
    replace(&fx, &mail, quote).await;
    settle(&fx, &mail).await;
    let original = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
        .await
        .unwrap()
        .remove(0);
    let legacy_key = format!(
        "asks|payment|{}|due:2026-11",
        mxr_store::assessment_fingerprint("billing@example.com|payment|account:a123")
    );
    sqlx::query("UPDATE todos SET dedup_key=?, field_sources=replace(field_sources, 'asks-r2c-13:', 'asks-r2c-12:') WHERE id=?").bind(&legacy_key).bind(&original.id).execute(fx.state.store.writer()).await.unwrap();
    let ResponseData::TodoChange { change } = request(
        &fx,
        Request::CorrectTodo {
            todo_id: original.id.clone(),
            reason: mxr_protocol::TodoCorrectionData::AlreadyDone,
            edits: vec![],
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    else {
        panic!("correct")
    };
    let token = change.correction_undo.unwrap();
    let corrected = fx
        .state
        .store
        .get_todo(&original.id)
        .await
        .unwrap()
        .unwrap();
    replace(
        &fx,
        &mail,
        &format!("{quote}\n\nAccount #A123 amount due £25 by 1 November 2026."),
    )
    .await;
    settle(&fx, &mail).await;
    assert_eq!(
        fx.state
            .store
            .get_todo(&original.id)
            .await
            .unwrap()
            .unwrap(),
        corrected
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
            .unwrap()
            .state,
        TodoState::Open
    );
    settle(&fx, &mail).await;
    let tasks = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), TodoState::Open, 100)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 2);
    assert!(tasks.iter().any(|t| t.id == original.id));
    let packets = fx
        .state
        .store
        .classify_eval_packet(std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    assert_eq!(packets[0].projected_todos.len(), 2);
}

#[tokio::test]
async fn review6_payment_documents_retain_the_observed_speaker() {
    use mxr_core::asks::{Actor, AskAnswer};
    let due = (chrono::Utc::now() + Duration::days(9))
        .format("%-d %B %Y")
        .to_string();
    let mut failures = vec![];
    for (prefix, outbound, expected, answer) in [
        ("I can pay", false, 0, None),
        ("She will pay", false, 0, None),
        ("Please pay", false, 1, Some(AskAnswer::AsksYou)),
        ("I'll pay", false, 0, Some(AskAnswer::TheyPromisedYou)),
        ("I'll pay", true, 1, Some(AskAnswer::YouPromised)),
        ("I can pay", true, 0, None),
        ("Amount due for", false, 1, Some(AskAnswer::AsksYou)),
    ] {
        let fx = Fixture::new().await;
        let text = format!("{prefix} invoice #K918 £25 by {due}.");
        let mail = if outbound {
            source(&fx, &text).await
        } else {
            let mail = fx
                .message(
                    &ThreadId::new(),
                    "nora@example.com",
                    ME,
                    Duration::minutes(5),
                    None,
                )
                .await;
            replace(&fx, &mail, &text).await;
            mail
        };
        settle(&fx, &mail).await;
        let assessments = fx
            .state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
            .await
            .unwrap();
        let stored = &assessments[&mail.id];
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
            panic!("list");
        };
        eprintln!(
            "REVIEW6_SPEAKER text={text:?} outbound={outbound} units={:?} todos={todos:?}",
            stored.units
        );
        if todos.len() != expected {
            failures.push(format!(
                "{prefix} outbound={outbound}: {} tasks vs {expected}",
                todos.len()
            ));
        }
        if let Some(answer) = answer {
            assert!(
                stored
                    .units
                    .iter()
                    .any(|u| u.answer == answer && stored.quote(u) == Some(text.as_str())),
                "{text}"
            );
        } else if stored
            .units
            .iter()
            .any(|u| u.answer == AskAnswer::AsksYou || u.answer == AskAnswer::YouPromised)
        {
            failures.push(format!("non-obligation confirmed: {text}"));
        }
        if prefix == "She will pay" {
            if !stored.units.iter().any(|u| {
                u.tags
                    .as_ref()
                    .is_some_and(|t| t.actor == Actor::ThirdParty)
            }) {
                failures.push("third-party subject lost".into());
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

#[tokio::test]
async fn review6_financial_speakers_keep_remediation_and_independent_owner_work() {
    for (text, expected) in [
        ("I can update your payment details.", 0),
        ("She will update your payment details.", 0),
        ("Please update your payment details.", 1),
        ("Your payment failed. Please update your payment details.", 1),
        ("We couldn't charge your card. Update your payment details.", 1),
        ("I can pay invoice #K918 £25 by 1 November 2026.\n\nPlease sign the contract.", 1),
        ("She will pay invoice #K918 £25 by 1 November 2026.\n\nPlease pay invoice #Q222 €40 by 1 December 2026.", 1),
    ] {
        let fx=Fixture::new().await;
        let mail=fx.message(&ThreadId::new(),"nora@example.com",ME,Duration::minutes(5),None).await;
        replace(&fx,&mail,text).await; settle(&fx,&mail).await;
        let rows=fx.state.store.list_todos_in_state(Some(&fx.account),TodoState::Open,100).await.unwrap();
        assert_eq!(rows.len(),expected,"{text}");
        for row in rows {assert!(text.contains(&row.reason));assert!(row.reason.starts_with("Please") || row.reason.starts_with("Your payment") || row.reason.starts_with("We couldn't"));}
    }
}
