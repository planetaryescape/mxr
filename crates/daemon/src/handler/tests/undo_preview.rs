//! `UndoPreview` (the dry run of `mxr undo`) takes the undo's own lookup: it
//! refuses what the undo refuses with the same message, names the targets the
//! undo then restores, and changes nothing.

use super::selection_jobs::Fixture;
use super::*;
use mxr_core::id::{AccountId, MessageId};
use mxr_protocol::{MutationCommand, UndoPreview};
use mxr_store::{DeskUndo, HighlightPrior, ReadingHighlightRow, UndoEntry};
use std::collections::HashSet;

async fn archive(fx: &Fixture, ids: &[MessageId]) -> String {
    let ResponseData::MutationResult { result } = fx
        .send(Request::mutation(MutationCommand::Archive {
            message_ids: ids.to_vec(),
        }))
        .await
    else {
        panic!("expected a mutation result");
    };
    result.mutation_id.expect("an archive keeps an undo")
}

async fn preview(fx: &Fixture, mutation_id: &str) -> Result<UndoPreview, String> {
    match fx
        .send_raw(Request::UndoPreview {
            mutation_id: mutation_id.into(),
        })
        .await
    {
        Response::Ok {
            data: ResponseData::UndoPreview { preview },
        } => Ok(preview),
        Response::Error { message, .. } => Err(message),
        other => panic!("expected a preview or a refusal, got {other:?}"),
    }
}

async fn undo_refusal(fx: &Fixture, mutation_id: &str) -> String {
    fx.refused(Request::UndoMutation {
        mutation_id: mutation_id.into(),
    })
    .await
}

#[tokio::test]
async fn an_unknown_id_is_refused_with_the_undos_own_message() {
    let fx = Fixture::new().await;
    let previewed = fx
        .refused(Request::UndoPreview {
            mutation_id: "nope".into(),
        })
        .await;
    assert_eq!(previewed, undo_refusal(&fx, "nope").await);
    assert!(previewed.contains("not found"), "{previewed}");
}

#[tokio::test]
async fn an_undo_already_spent_is_refused_as_the_undo_refuses_it() {
    let fx = Fixture::new().await;
    let mail = fx.many(1, "letter").await;
    let mutation_id = archive(&fx, &[mail[0].id.clone()]).await;
    let ResponseData::Ack = fx
        .send(Request::UndoMutation {
            mutation_id: mutation_id.clone(),
        })
        .await
    else {
        panic!("the first undo should succeed");
    };

    let previewed = preview(&fx, &mutation_id).await.unwrap_err();
    let undone = undo_refusal(&fx, &mutation_id).await;
    assert_eq!(previewed, undone);
    assert!(previewed.contains("already undone"), "{previewed}");
}

#[tokio::test]
async fn an_expired_undo_is_refused_and_kept_by_the_preview_only() {
    let fx = Fixture::new().await;
    let mail = fx.many(1, "letter").await;
    let mutation_id = archive(&fx, &[mail[0].id.clone()]).await;
    let mut entry: UndoEntry = fx
        .state
        .store
        .read_undo_entry(&mutation_id)
        .await
        .unwrap()
        .expect("the archive left an undo entry");
    entry.expires_at = 0;
    fx.state.store.write_undo_entry(&entry).await.unwrap();

    let previewed = preview(&fx, &mutation_id).await.unwrap_err();
    assert!(previewed.contains("window expired"), "{previewed}");
    assert_eq!(fx.undo_entries().await, 1, "the preview removes nothing");

    let undone = undo_refusal(&fx, &mutation_id).await;
    assert_eq!(previewed, undone);
    assert_eq!(
        fx.undo_entries().await,
        0,
        "the undo removes the expired entry"
    );
}

#[tokio::test]
async fn an_all_job_is_refused_as_the_undo_refuses_it() {
    let fx = Fixture::new().await;
    fx.many(2, "invoice").await;
    let job = fx.archive_job("invoice").await;

    let previewed = preview(&fx, &job.job_id).await.unwrap_err();
    assert_eq!(previewed, undo_refusal(&fx, &job.job_id).await);
    assert!(previewed.contains("cannot be undone"), "{previewed}");
}

#[tokio::test]
async fn an_undo_whose_messages_are_gone_locally_is_refused_as_the_undo_refuses_it() {
    let fx = Fixture::new().await;
    let mail = fx.many(1, "letter").await;
    let mutation_id = archive(&fx, &[mail[0].id.clone()]).await;
    fx.state
        .store
        .delete_messages_and_derived(&fx.account, &[mail[0].provider_id.clone()])
        .await
        .unwrap();

    let previewed = preview(&fx, &mutation_id).await.unwrap_err();
    let undone = undo_refusal(&fx, &mutation_id).await;
    assert_eq!(previewed, undone);
    assert!(previewed.contains("irreversible"), "{previewed}");
}

#[tokio::test]
async fn a_highlight_undo_with_its_message_deleted_is_refused_as_the_undo_refuses_it() {
    let fx = Fixture::new().await;
    let mail = fx.many(1, "letter").await;
    let row = ReadingHighlightRow {
        id: "highlight-1".into(),
        account_id: fx.account.clone(),
        message_id: mail[0].id.clone(),
        idx: 0,
        view: "quote".into(),
        quote: "letter".into(),
        note: None,
        created_at: chrono::Utc::now(),
        origin: mxr_store::LOCAL_HIGHLIGHT_ORIGIN.into(),
        origin_id: None,
        paragraph: None,
    };
    fx.state
        .store
        .delete_messages_and_derived(&fx.account, &[mail[0].provider_id.clone()])
        .await
        .unwrap();
    let mutation_id = "removed-highlight";
    fx.state
        .store
        .write_undo_entry(&UndoEntry {
            mutation_id: mutation_id.into(),
            kind: mxr_store::UndoableMutationKind::Highlight,
            snapshots: vec![],
            desk: Some(DeskUndo {
                highlights: vec![HighlightPrior { row, removed: true }],
                ..DeskUndo::default()
            }),
            applied_at: 0,
            expires_at: chrono::Utc::now().timestamp() + 3600,
        })
        .await
        .unwrap();

    let previewed = preview(&fx, mutation_id).await.unwrap_err();
    assert_eq!(previewed, undo_refusal(&fx, mutation_id).await);
    assert!(previewed.contains("irreversible"), "{previewed}");
}

#[tokio::test]
async fn a_valid_preview_names_the_targets_the_undo_restores_and_changes_nothing() {
    let fx = Fixture::new().await;
    let mail = fx.many(2, "letter").await;
    let ids: Vec<MessageId> = mail.iter().map(|m| m.id.clone()).collect();
    let mutation_id = archive(&fx, &ids).await;
    for id in &ids {
        assert!(!fx.in_inbox(id).await, "archived before the preview");
    }

    let previewed = preview(&fx, &mutation_id).await.unwrap();
    assert_eq!(previewed.mutation_id, mutation_id);
    assert_eq!(previewed.kind, "archive");
    assert!(!previewed.restores_desk_state);
    let targets: HashSet<MessageId> = previewed
        .targets
        .iter()
        .map(|t| t.message_id.clone())
        .collect();
    assert_eq!(targets, ids.iter().cloned().collect::<HashSet<_>>());
    assert!(previewed.targets.iter().all(|t| t.in_local_store));

    for id in &ids {
        assert!(!fx.in_inbox(id).await, "the preview left the mail archived");
    }
    assert_eq!(fx.undo_entries().await, 1, "the preview keeps the undo");

    let ResponseData::Ack = fx
        .send(Request::UndoMutation {
            mutation_id: mutation_id.clone(),
        })
        .await
    else {
        panic!("the undo should succeed");
    };
    for target in &previewed.targets {
        assert!(
            fx.in_inbox(&target.message_id).await,
            "the undo restores what the preview named"
        );
    }
}

async fn activity_rows(fx: &Fixture) -> usize {
    crate::activity::settle(&fx.state.activity).await;
    fx.state
        .store
        .list_activity(&mxr_store::ActivityFilter::default(), 1000, None)
        .await
        .unwrap()
        .rows
        .len()
}

#[tokio::test]
async fn a_preview_calls_no_provider_records_no_activity_and_keeps_the_entry() {
    let fx = Fixture::new().await;
    let mail = fx.many(2, "letter").await;
    let ids: Vec<MessageId> = mail.iter().map(|m| m.id.clone()).collect();
    let mutation_id = archive(&fx, &ids).await;
    let entry_before = fx
        .state
        .store
        .read_undo_entry(&mutation_id)
        .await
        .unwrap()
        .expect("the archive left an undo entry");
    let provider_calls = fx.fake.mutations().len();
    let activity = activity_rows(&fx).await;

    preview(&fx, &mutation_id).await.unwrap();
    preview(&fx, &mutation_id).await.unwrap();
    assert!(preview(&fx, "nope").await.is_err());

    assert_eq!(
        fx.fake.mutations().len(),
        provider_calls,
        "no provider call"
    );
    assert_eq!(activity_rows(&fx).await, activity, "no activity row");
    let entry_after = fx
        .state
        .store
        .read_undo_entry(&mutation_id)
        .await
        .unwrap()
        .expect("the preview keeps the entry");
    assert_eq!(format!("{entry_after:?}"), format!("{entry_before:?}"));
}

/// Runs `request` as an MCP caller that may only touch `allowed`.
async fn as_mcp_limited_to(fx: &Fixture, allowed: AccountId, request: Request) -> Response {
    use mxr_config::SafetyPolicy;
    let mut config = fx.state.config_snapshot();
    config.agent_surfaces.profiles.insert(
        "mcp".into(),
        mxr_config::AgentProfileConfig {
            safety_policy: SafetyPolicy::Full,
            allowed_accounts: vec![allowed.as_str()],
            allow_send: false,
            allow_destructive: true,
            allowed_destructive_actions: vec![],
        },
    );
    fx.state.set_config_for_test(config).await;
    let msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::Mcp,
        payload: IpcPayload::Request(request),
    };
    match handle_request(&fx.state, &msg).await.payload {
        IpcPayload::Response(response) => response,
        other => panic!("expected a response, got {other:?}"),
    }
}

#[tokio::test]
async fn a_scoped_caller_is_refused_the_preview_exactly_as_the_undo() {
    let fx = Fixture::new().await;
    let other = fx.other_account(true).await;
    let mail = fx.many(1, "letter").await;
    let mutation_id = archive(&fx, &[mail[0].id.clone()]).await;

    let refused_preview = as_mcp_limited_to(
        &fx,
        other.clone(),
        Request::UndoPreview {
            mutation_id: mutation_id.clone(),
        },
    )
    .await;
    let refused_undo = as_mcp_limited_to(
        &fx,
        other,
        Request::UndoMutation {
            mutation_id: mutation_id.clone(),
        },
    )
    .await;
    let (
        Response::Error {
            message: previewed, ..
        },
        Response::Error {
            message: undone, ..
        },
    ) = (refused_preview, refused_undo)
    else {
        panic!("a caller outside the undo's account must be refused both");
    };
    // The allowlist refusal names the request it refused, so only that kind
    // differs; the reason and everything else must match.
    assert_eq!(
        previewed.replace("undo_preview", "undo_mutation"),
        undone,
        "{previewed}"
    );
    assert!(
        undone.contains("rejected by mcp profile account allowlist"),
        "{undone}"
    );

    let allowed = as_mcp_limited_to(
        &fx,
        fx.account.clone(),
        Request::UndoPreview {
            mutation_id: mutation_id.clone(),
        },
    )
    .await;
    assert!(
        matches!(
            allowed,
            Response::Ok {
                data: ResponseData::UndoPreview { .. }
            }
        ),
        "a caller in the undo's account gets the preview"
    );
    assert_eq!(
        fx.undo_entries().await,
        1,
        "the scoped previews keep the undo"
    );
}
