use super::desk::{request, Fixture};
use super::promise_projection::{month_start, settle, source, written};
use super::*;
use mxr_store::{CommitmentStatus, TodoState};

fn gbp_text() -> String {
    format!(
        "I'll pay invoice #A123 amount due £25 by {}.",
        written(month_start(1))
    )
}

fn eur_text() -> String {
    format!(
        "I'll pay invoice #A123 amount due €40 by {}.",
        written(month_start(1))
    )
}

#[tokio::test]
async fn review4_currency_distinct_promises_keep_independent_public_lifecycles() {
    for accepted_before_assessment in [false, true] {
        let fx = Fixture::new().await;
        let mail = source(&fx, &format!("{}\n\n{}", gbp_text(), eur_text())).await;
        let accepted = if accepted_before_assessment {
            let ResponseData::RecordedPromise { commitment, .. } = request(
                &fx,
                Request::RecordPromise {
                    message_id: mail.id.clone(),
                    what: gbp_text(),
                    due_at: super::promise_projection::chosen(),
                    dry_run: false,
                },
            )
            .await
            else {
                panic!("accepted promise")
            };
            assert_eq!(commitment.what, gbp_text());
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
            let gbp = commitments.iter().find(|c| c.what == gbp_text()).unwrap();
            assert_eq!(gbp.id, id);
            assert_eq!(gbp.by_when, Some(super::promise_projection::chosen()));
            assert_eq!(gbp.by_when_source, Some(mxr_store::ByWhenSource::User));
            assert_eq!(
                tasks
                    .iter()
                    .find(|t| t.reason == gbp_text())
                    .unwrap()
                    .due_at,
                gbp.by_when
            );
        }
        for quote in [gbp_text(), eur_text()] {
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
