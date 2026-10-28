//! Accepted draft promises retain authority independently of date confidence.
use super::desk::{request, Fixture, ME};
use super::*;
use chrono::{Duration, Utc};
use mxr_core::{Address, Draft, DraftContent, DraftId, DraftIntent, ThreadId};
use mxr_store::{ByWhenSource, CommitmentDirection, CommitmentStatus, DraftCommitmentCandidate};

#[tokio::test]
async fn promoted_and_recorded_promises_stay_visible_without_reviving_model_history() {
    let fx = Fixture::new().await;
    let sent = fx
        .message(
            &ThreadId::new(),
            ME,
            "person@example.com",
            Duration::days(2),
            None,
        )
        .await;
    let now = Utc::now();
    fx.state
        .store
        .insert_body(&mxr_core::MessageBody {
            message_id: sent.id.clone(),
            text_plain: Some("Following up on our conversation.".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: now,
            metadata: Default::default(),
        })
        .await
        .unwrap();
    let draft = Draft {
        revision: Some(1),
        id: DraftId::new(),
        account_id: fx.account.clone(),
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![Address {
            email: "person@example.com".into(),
            name: None,
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Promise".into(),
        content: DraftContent::markdown("I'll send the report."),
        inline_assets: vec![],
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    };
    for source in [None, Some(ByWhenSource::ModelDate)] {
        fx.state
            .store
            .upsert_draft_commitment_candidate(&DraftCommitmentCandidate {
                id: mxr_store::new_candidate_id(),
                draft_id: draft.id.clone(),
                account_id: fx.account.clone(),
                email: "person@example.com".into(),
                direction: CommitmentDirection::Yours,
                who_owes: "you".into(),
                what: format!("report {source:?}"),
                by_when: Some(now - Duration::days(1)),
                by_when_source: source,
                extracted_at: now,
            })
            .await
            .unwrap();
    }
    crate::handler::commitments_extract::promote_after_send(&fx.state, &draft, &sent.id)
        .await
        .unwrap();
    let promoted = fx
        .state
        .store
        .list_contact_commitments(&fx.account, None, Some(CommitmentStatus::Open))
        .await
        .unwrap();
    assert_eq!(promoted.len(), 2);
    let other_account = mxr_core::AccountId::new();
    fx.state
        .store
        .insert_account(&crate::test_fixtures::test_account_with_id(
            other_account.clone(),
        ))
        .await
        .unwrap();
    let mut cross_account = promoted[0].clone();
    cross_account.account_id = other_account.clone();
    cross_account.id = format!("{}::{}", sent.id, mxr_store::new_candidate_id());
    fx.state
        .store
        .upsert_contact_commitment(&cross_account)
        .await
        .unwrap();
    assert!(
        fx.state
            .store
            .list_contact_commitments(&other_account, None, Some(CommitmentStatus::Open))
            .await
            .unwrap()
            .is_empty(),
        "a matching promotion namespace cannot borrow another account's source"
    );
    assert_eq!(
        fx.state
            .store
            .list_contact_commitments(&other_account, None, None)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(promoted
        .iter()
        .all(|p| p.date_unverified() && p.actionable_by_when().is_none()));
    for id in [
        "commitment-retired-model".to_string(),
        format!("{}::promise::undated", mxr_core::MessageId::new()),
        format!("{}::unknown", sent.id),
        format!(
            "{}::{}",
            mxr_core::MessageId::new(),
            mxr_store::new_candidate_id()
        ),
    ] {
        let mut old = promoted[0].clone();
        old.id = id.clone();
        old.what = id;
        fx.state
            .store
            .upsert_contact_commitment(&old)
            .await
            .unwrap();
    }
    assert_eq!(
        fx.state
            .store
            .list_contact_commitments(&fx.account, None, Some(CommitmentStatus::Open))
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        fx.state
            .store
            .list_contact_commitments(&fx.account, None, None)
            .await
            .unwrap()
            .len(),
        6
    );
    let ResponseData::Desk { due, .. } = fx.desk(100).await else {
        panic!("desk")
    };
    assert!(
        due.rows.is_empty(),
        "unverified model dates never drive Due"
    );
    let data = crate::handler::now::now_at(
        &fx.state,
        None,
        now,
        &chrono::FixedOffset::east_opt(0).expect("UTC"),
    )
    .await
    .unwrap();
    assert!(
        data.due_soon.todos.is_empty(),
        "unverified model dates never drive Now"
    );
    request(
        &fx,
        Request::RecordPromise {
            message_id: sent.id.clone(),
            what: "send the final report".into(),
            due_at: now + Duration::days(1),
            dry_run: false,
        },
    )
    .await;
    let active = fx
        .state
        .store
        .list_contact_commitments(&fx.account, None, Some(CommitmentStatus::Open))
        .await
        .unwrap();
    assert_eq!(active.len(), 3);
    assert!(active
        .iter()
        .any(|p| p.id.contains("::promise::") && p.by_when_source == Some(ByWhenSource::User)));
    let mut fingerprints = std::collections::HashMap::new();
    while crate::handler::todos::tick(&fx.state, now, &mut fingerprints)
        .await
        .unwrap()
    {}
    let rows = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        3,
        "only the two promoted and one user-recorded promises project"
    );
    let user = active
        .iter()
        .find(|p| p.by_when_source == Some(ByWhenSource::User))
        .unwrap();
    let todo = rows
        .iter()
        .find(|row| row.commitment_id.as_deref() == Some(&user.id))
        .unwrap();
    assert_eq!(
        todo.due_at.map(|at| at.timestamp()),
        user.by_when.map(|at| at.timestamp())
    );
    assert!(rows
        .iter()
        .filter(|row| row.id != todo.id)
        .all(|row| row.due_at.is_none()));
    let before = fx
        .state
        .store
        .list_promises_for_todos(&fx.account)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.commitment_id == user.id)
        .unwrap();
    sqlx::query("UPDATE contact_commitments SET by_when_source='model_date' WHERE id=?")
        .bind(&user.id)
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    assert_eq!(
        fx.state
            .store
            .upsert_accepted_promise_todo(todo, &before)
            .await
            .unwrap(),
        mxr_store::TodoUpsert::Unchanged,
        "stale accepted-date snapshot cannot win its CAS"
    );
    fingerprints.clear();
    while crate::handler::todos::tick(&fx.state, now, &mut fingerprints)
        .await
        .unwrap()
    {}
    let undated = fx.state.store.get_todo(&todo.id).await.unwrap().unwrap();
    assert_eq!(
        undated.due_at, None,
        "downgraded model date stops driving To do"
    );
    assert_eq!(undated.state, mxr_store::TodoState::Open);
    let manual = now + Duration::days(42);
    sqlx::query("UPDATE todos SET user_edited=1, due_at=? WHERE id=?")
        .bind(manual.timestamp())
        .bind(&todo.id)
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    sqlx::query("UPDATE contact_commitments SET by_when_source='words' WHERE id=?")
        .bind(&user.id)
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    fingerprints.clear();
    while crate::handler::todos::tick(&fx.state, now, &mut fingerprints)
        .await
        .unwrap()
    {}
    assert_eq!(
        fx.state
            .store
            .get_todo(&todo.id)
            .await
            .unwrap()
            .unwrap()
            .due_at
            .map(|at| at.timestamp()),
        Some(manual.timestamp()),
        "manual edits survive restart and date-source changes"
    );
    assert_eq!(
        fx.state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap()
            .len(),
        3,
        "repeated projection has stable identities"
    );
}
