use super::desk::{request, Fixture};
use super::promise_projection::{settle, source};
use super::*;
use mxr_store::{CommitmentStatus, TodoState};

const GBP: &str = "I'll pay invoice #A123 amount due £25 by 1 November 2026.";
const EUR: &str = "I'll pay invoice #A123 amount due €40 by 1 November 2026.";

#[tokio::test]
async fn review4_currency_distinct_promises_keep_independent_public_lifecycles() {
    for accepted_before_assessment in [false, true] {
        let fx = Fixture::new().await;
        let mail = source(&fx, &format!("{GBP}\n\n{EUR}")).await;
        let accepted = if accepted_before_assessment {
            let ResponseData::RecordedPromise { commitment, .. } = request(
                &fx,
                Request::RecordPromise {
                    message_id: mail.id.clone(),
                    what: GBP.into(),
                    due_at: super::promise_projection::chosen(),
                    dry_run: false,
                },
            )
            .await
            else {
                panic!("accepted promise")
            };
            assert_eq!(commitment.what, GBP);
            Some(commitment.id)
        } else {
            None
        };
        settle(&fx, &mail).await;
        let ResponseData::CommitmentList {
            commitments: public,
        } = request(
            &fx,
            Request::ListCommitments {
                account_id: fx.account.clone(),
                email: None,
                status: None,
            },
        )
        .await
        else {
            panic!("public commitments")
        };
        eprintln!(
            "REVIEW4_PUBLIC accepted_before_assessment={accepted_before_assessment} {public:?}"
        );
        assert_eq!(public.len(), 2);
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
        let answer = fx
            .state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
            .await
            .unwrap();
        eprintln!("REVIEW4_CURRENCY answer={answer:?} commitments={commitments:?} tasks={tasks:?}");
        assert_eq!(commitments.len(), 2);
        assert_eq!(tasks.len(), 2);
        let ResponseData::Todos { todos: active } = request(
            &fx,
            Request::ListTodos {
                account_id: Some(fx.account.clone()),
                state: mxr_protocol::TodoStateData::Open,
                limit: 100,
            },
        )
        .await
        else {
            panic!("public active tasks")
        };
        assert_eq!(active.len(), 2);
        for task in &tasks {
            assert!(active.iter().any(|row| row.id == task.id));
        }
        if let Some(id) = accepted {
            let gbp = commitments.iter().find(|c| c.what == GBP).unwrap();
            assert_eq!(gbp.id, id);
            assert_eq!(gbp.by_when, Some(super::promise_projection::chosen()));
            assert_eq!(gbp.by_when_source, Some(mxr_store::ByWhenSource::User));
            assert_eq!(
                tasks.iter().find(|t| t.reason == GBP).unwrap().due_at,
                gbp.by_when
            );
        }
        for quote in [GBP, EUR] {
            let commitment = commitments.iter().find(|c| c.what == quote).unwrap();
            let task = tasks.iter().find(|t| t.reason == quote).unwrap();
            assert_eq!(task.commitment_id.as_deref(), Some(commitment.id.as_str()));
        }
        settle(&fx, &mail).await;
        assert_eq!(
            fx.state
                .store
                .list_contact_commitments(&fx.account, None, None)
                .await
                .unwrap(),
            commitments
        );
        for task in &tasks {
            let id = task.commitment_id.clone().unwrap();
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
                TodoState::Done
            );
            let other = tasks.iter().find(|t| t.id != task.id).unwrap();
            assert_eq!(
                fx.state
                    .store
                    .get_todo(&other.id)
                    .await
                    .unwrap()
                    .unwrap()
                    .state,
                TodoState::Open
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
            settle(&fx, &mail).await;
            assert_eq!(
                fx.state
                    .store
                    .get_todo(&task.id)
                    .await
                    .unwrap()
                    .unwrap()
                    .state,
                TodoState::Open
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
        }
    }
}

#[tokio::test]
async fn review4_payment_commitments_separate_references_and_periods_but_reuse_reminders() {
    for (other, expected) in [
        (
            "I will pay invoice #A123 amount due £25 by 1 November 2026.",
            1,
        ),
        (
            "I'll pay invoice #B456 amount due £25 by 1 November 2026.",
            2,
        ),
        (
            "I'll pay invoice #A123 amount due £25 by 1 December 2026.",
            2,
        ),
    ] {
        let fx = Fixture::new().await;
        let mail = source(&fx, &format!("{GBP}\n\n{other}")).await;
        settle(&fx, &mail).await;
        let rows = fx
            .state
            .store
            .list_contact_commitments(&fx.account, None, None)
            .await
            .unwrap();
        assert_eq!(rows.len(), expected, "{other}");
        assert_eq!(
            fx.state
                .store
                .list_promise_todos(&fx.account)
                .await
                .unwrap()
                .len(),
            expected
        );
    }
}

#[tokio::test]
async fn review4_policy14_currency_collision_keeps_closed_user_history_and_task_ids() {
    for dismissed in [false, true] {
        let fx = Fixture::new().await;
        let mail = source(&fx, &format!("{GBP}\n\n{EUR}")).await;
        settle(&fx, &mail).await;
        let tasks = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap();
        let gbp = tasks.iter().find(|t| t.reason == GBP).unwrap();
        let eur = tasks.iter().find(|t| t.reason == EUR).unwrap();
        let old_id = format!(
            "ask_{}",
            mxr_store::assessment_fingerprint(&format!(
                "{}|nora@example.com|yours|Pay|invoice:a123|2026-11",
                fx.account
            ))
        );
        // Reproduce policy14's one surviving EUR commitment and unlinked GBP task.
        sqlx::query("DELETE FROM contact_commitments WHERE id=?")
            .bind(&gbp.commitment_id)
            .execute(fx.state.store.writer())
            .await
            .unwrap();
        sqlx::query("UPDATE contact_commitments SET id=? WHERE id=?")
            .bind(&old_id)
            .bind(&eur.commitment_id)
            .execute(fx.state.store.writer())
            .await
            .unwrap();
        sqlx::query("UPDATE todos SET commitment_id=CASE WHEN id=? THEN ? ELSE NULL END WHERE source_message_id=?").bind(&eur.id).bind(&old_id).bind(mail.id.as_str()).execute(fx.state.store.writer()).await.unwrap();
        request(
            &fx,
            Request::RecordPromise {
                message_id: mail.id.clone(),
                what: EUR.into(),
                due_at: super::promise_projection::chosen(),
                dry_run: false,
            },
        )
        .await;
        request(
            &fx,
            Request::ResolveCommitment {
                commitment_id: old_id.clone(),
            },
        )
        .await;
        if dismissed {
            request(
                &fx,
                Request::SetTodoState {
                    todo_ids: vec![eur.id.clone()],
                    action: mxr_protocol::TodoStateActionData::Dismiss,
                    dry_run: false,
                },
            )
            .await;
        }
        assert_eq!(
            fx.state
                .store
                .list_contact_commitments(&fx.account, None, None)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            fx.state
                .store
                .get_todo(&gbp.id)
                .await
                .unwrap()
                .unwrap()
                .commitment_id,
            None
        );
        // Retained tasks carry the old policy's unit provenance. Keeping the
        // seed's current provenance would falsely model an already-applied replay.
        sqlx::query("UPDATE todos SET field_sources=replace(field_sources, ?, 'asks-r2c-14:') WHERE source_message_id=?")
            .bind(format!("{}:", mxr_core::asks::ASKS_POLICY_VERSION))
            .bind(mail.id.as_str()).execute(fx.state.store.writer()).await.unwrap();
        let saved = fx.state.store.get_todo(&eur.id).await.unwrap().unwrap();
        let commitment = fx
            .state
            .store
            .get_contact_commitment(&old_id)
            .await
            .unwrap()
            .unwrap();
        sqlx::query("UPDATE mail_assessments SET policy_version='asks-r2c-14', assessment_json=json_set(assessment_json,'$.policy_version','asks-r2c-14') WHERE message_id=? AND policy_version=?")
            .bind(mail.id.as_str()).bind(mxr_core::asks::ASKS_POLICY_VERSION).execute(fx.state.store.writer()).await.unwrap();
        assert!(fx
            .state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
            .await
            .unwrap()
            .is_empty());
        settle(&fx, &mail).await;
        assert_eq!(
            fx.state.store.get_todo(&eur.id).await.unwrap().unwrap(),
            saved
        );
        assert_eq!(
            fx.state
                .store
                .get_contact_commitment(&old_id)
                .await
                .unwrap()
                .unwrap(),
            commitment
        );
        let now_gbp = fx.state.store.get_todo(&gbp.id).await.unwrap().unwrap();
        assert_eq!(now_gbp.state, TodoState::Open);
        assert!(now_gbp.commitment_id.is_some());
        assert_ne!(now_gbp.commitment_id.as_deref(), Some(old_id.as_str()));
        let rows = fx
            .state
            .store
            .list_contact_commitments(&fx.account, None, None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        settle(&fx, &mail).await;
        assert_eq!(
            fx.state
                .store
                .list_contact_commitments(&fx.account, None, None)
                .await
                .unwrap(),
            rows
        );
    }
}
