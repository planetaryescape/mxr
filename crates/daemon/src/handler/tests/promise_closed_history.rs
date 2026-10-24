//! Explicit resolution retains recovery for already-closed promise history.
use super::desk::{request, Fixture};
use super::promise_projection::{accept, settle, source, QUOTE};
use super::*;
use mxr_store::{CommitmentStatus, TodoState};

#[tokio::test]
async fn direct_resolve_repairs_linked_open_tasks_on_closed_commitments() {
    for status in [CommitmentStatus::Resolved, CommitmentStatus::Expired] {
        let fx = Fixture::new().await;
        let mail = source(&fx, QUOTE).await;
        settle(&fx, &mail).await;
        let id = accept(&fx, &mail).await;
        let task = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap()
            .remove(0);
        // Reproduce retained history from a pre-fix Desk resolve or expiration.
        fx.state
            .store
            .set_contact_commitment_status(&fx.account, &id, status, Some(chrono::Utc::now()))
            .await
            .unwrap();
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
        request(
            &fx,
            Request::ResolveCommitment {
                commitment_id: id.clone(),
            },
        )
        .await;
        let saved = fx.state.store.get_todo(&task.id).await.unwrap().unwrap();
        assert_eq!(saved.state, TodoState::Done);
        assert_eq!(saved.id, task.id);
        assert_eq!(saved.title, task.title);
        assert_eq!(saved.reason, task.reason);
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
    }
}
