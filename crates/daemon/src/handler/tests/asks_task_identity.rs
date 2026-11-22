use super::desk::{request, Fixture, ME};
use super::promise_projection::{replace, settle};
use super::*;
use chrono::Duration;
use mxr_core::ThreadId;
use mxr_store::TodoState;

fn currency_deadline() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive() + Duration::days(7)
}

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
        let due = currency_deadline();
        let date = due.format("%-d %B %Y");
        let eur = format!("Account #A123 amount due €40 by {date}.");
        replace(&fx, &mail, &eur).await;
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
            "asks|payment|{}|due:{}",
            mxr_store::assessment_fingerprint("billing@example.com|payment|account:a123"),
            due.format("%Y-%m")
        );
        sqlx::query("UPDATE todos SET dedup_key=?, field_sources=replace(field_sources, 'asks-r2c-13:', 'asks-r2c-12:') WHERE id=?").bind(&legacy_key).bind(&original.id).execute(fx.state.store.writer()).await.unwrap();
        replace(
            &fx,
            &mail,
            &format!("Account #A123 amount due £25 by {date}.\n\n{eur}"),
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
