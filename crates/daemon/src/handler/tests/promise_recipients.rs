use super::desk::{request, Fixture, ME};
use super::promise_projection::{accept, chosen, settle, source, QUOTE};
use super::*;
use mxr_core::MessageDirection;

#[tokio::test]
async fn review2_owned_first_recipient_keeps_one_promise_and_task() {
    let fx = Fixture::new().await;
    let mut mail = source(&fx, QUOTE).await;
    mail.to.insert(
        0,
        mxr_core::Address {
            name: None,
            email: ME.into(),
        },
    );
    fx.store_envelope(&mail, MessageDirection::Outbound).await;
    settle(&fx, &mail).await;
    let original = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap()
        .remove(0);
    let id = accept(&fx, &mail).await;
    let rows = fx
        .state
        .store
        .list_contact_commitments(&fx.account, None, None)
        .await
        .unwrap();
    eprintln!("REVIEW2_RECIPIENT {rows:?}");
    assert_eq!(rows.len(), 1);
    assert_eq!(original.commitment_id.as_deref(), Some(id.as_str()));
    settle(&fx, &mail).await;
    let tasks = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, original.id);
    assert_eq!(tasks[0].due_at, Some(chosen()));
}

#[tokio::test]
async fn owned_alias_and_cc_fallback_share_acceptance_identity() {
    for cc_only in [false, true] {
        let fx = Fixture::new().await;
        fx.state
            .store
            .add_account_address(&fx.account, "alias@example.com", false)
            .await
            .unwrap();
        let mut mail = source(&fx, QUOTE).await;
        let recipient = mail.to.remove(0);
        mail.to.push(mxr_core::Address {
            name: None,
            email: "ALIAS@example.com".into(),
        });
        if cc_only {
            mail.cc.push(recipient);
        } else {
            mail.to.push(recipient);
        }
        fx.store_envelope(&mail, MessageDirection::Outbound).await;
        settle(&fx, &mail).await;
        let original = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap()
            .remove(0);
        for what in [QUOTE, "send the report"] {
            request(
                &fx,
                Request::RecordPromise {
                    message_id: mail.id.clone(),
                    what: what.into(),
                    due_at: chosen(),
                    dry_run: false,
                },
            )
            .await;
            settle(&fx, &mail).await;
        }
        let rows = fx
            .state
            .store
            .list_contact_commitments(&fx.account, None, None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].email, "nora@example.com");
        let tasks = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, original.id);
        assert_eq!(tasks[0].reason, QUOTE);
        assert_eq!(tasks[0].due_at, Some(chosen()));
    }
}
