use super::*;

fn draft(state: &AppState) -> mxr_core::Draft {
    let now = chrono::Utc::now();
    mxr_core::Draft {
        revision: None,
        id: mxr_core::DraftId::new(),
        account_id: state.default_account_id(),
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::Address {
            name: None,
            email: "alice@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Revision test".into(),
        content: mxr_core::DraftContent::markdown("original"),
        attachments: vec![],
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    }
}

async fn call(state: &Arc<AppState>, request: Request) -> Response {
    match handle_request(
        state,
        &IpcMessage {
            id: 1,
            source: ClientKind::Cli,
            payload: IpcPayload::Request(request),
        },
    )
    .await
    .payload
    {
        IpcPayload::Response(response) => response,
        other => panic!("unexpected response {other:?}"),
    }
}

fn saved(response: Response) -> mxr_core::Draft {
    match response {
        Response::Ok {
            data: ResponseData::Draft { draft },
        } => draft,
        other => panic!("expected canonical draft: {other:?}"),
    }
}

fn conflict(response: Response, revision: i64) {
    match response {
        Response::Error {
            code,
            retryable,
            details,
            ..
        } => {
            assert_eq!(code, "draft_revision_conflict");
            assert!(!retryable);
            assert_eq!(details.unwrap()["current_revision"], revision);
        }
        other => panic!("expected conflict: {other:?}"),
    }
}

#[tokio::test]
async fn draft_revisions_repeated_create_and_concurrent_edits_preserve_both_versions() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let original = draft(&state);
    let stored = saved(
        call(
            &state,
            Request::SaveDraft {
                draft: original.clone(),
            },
        )
        .await,
    );
    assert_eq!(stored.revision, Some(1));
    assert_eq!(
        saved(
            call(
                &state,
                Request::SaveDraft {
                    draft: original.clone()
                }
            )
            .await
        )
        .revision,
        Some(1)
    );
    let mut different = original.clone();
    different.content = mxr_core::DraftContent::markdown("different create");
    conflict(
        call(&state, Request::SaveDraft { draft: different }).await,
        1,
    );
    let mut a = stored.clone();
    a.content = mxr_core::DraftContent::markdown("editor A");
    let mut b = stored.clone();
    b.content = mxr_core::DraftContent::markdown("editor B");
    let (ra, rb) = tokio::join!(
        call(&state, Request::UpdateDraft { draft: a.clone() }),
        call(&state, Request::UpdateDraft { draft: b.clone() })
    );
    let accepted = match (ra, rb) {
        (ok @ Response::Ok { .. }, stale) => {
            conflict(stale, 2);
            saved(ok)
        }
        (stale, ok @ Response::Ok { .. }) => {
            conflict(stale, 2);
            saved(ok)
        }
        other => panic!("one edit must succeed: {other:?}"),
    };
    assert_eq!(accepted.revision, Some(2));
    assert_eq!(a.content.analysis_text(), "editor A");
    assert_eq!(b.content.analysis_text(), "editor B");
    let mut legacy = accepted.clone();
    legacy.revision = None;
    conflict(
        call(&state, Request::UpdateDraft { draft: legacy }).await,
        2,
    );
    conflict(
        call(
            &state,
            Request::DeleteDraft {
                draft_id: stored.id.clone(),
                expected_revision: None,
            },
        )
        .await,
        2,
    );
    conflict(
        call(
            &state,
            Request::DeleteDraft {
                draft_id: stored.id.clone(),
                expected_revision: Some(1),
            },
        )
        .await,
        2,
    );
    assert_eq!(
        state
            .store
            .get_draft(&stored.id)
            .await
            .unwrap()
            .unwrap()
            .content
            .analysis_text(),
        accepted.content.analysis_text()
    );
}

#[tokio::test]
async fn draft_revisions_stale_linked_edit_has_no_provider_effect_and_sync_uses_canonical_content()
{
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let original = saved(
        call(
            &state,
            Request::SaveDraftToServer {
                draft: draft(&state),
            },
        )
        .await,
    );
    let provider_id = state
        .store
        .get_provider_draft_id(&original.id)
        .await
        .unwrap()
        .unwrap();
    let mut edited = original.clone();
    edited.content = mxr_core::DraftContent::markdown("accepted edit");
    let updated = saved(call(&state, Request::UpdateDraft { draft: edited }).await);
    let before = mxr_core::MailSendProvider::fetch_draft(fake.as_ref(), &provider_id)
        .await
        .unwrap()
        .unwrap()
        .revision;
    let mut stale = original;
    stale.content = mxr_core::DraftContent::markdown("stale unsent text");
    conflict(
        call(
            &state,
            Request::UpdateDraft {
                draft: stale.clone(),
            },
        )
        .await,
        2,
    );
    conflict(
        call(
            &state,
            Request::SaveDraftToServer {
                draft: stale.clone(),
            },
        )
        .await,
        2,
    );
    conflict(
        call(
            &state,
            Request::DeleteDraft {
                draft_id: stale.id.clone(),
                expected_revision: Some(1),
            },
        )
        .await,
        2,
    );
    assert_eq!(
        mxr_core::MailSendProvider::fetch_draft(fake.as_ref(), &provider_id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        before
    );
    let mut unpersisted = updated.clone();
    unpersisted.content = mxr_core::DraftContent::markdown("never saved");
    saved(call(&state, Request::SaveDraftToServer { draft: unpersisted }).await);
    assert_eq!(
        fake.server_drafts()[&provider_id].content.analysis_text(),
        "accepted edit"
    );
    assert_eq!(stale.content.analysis_text(), "stale unsent text");
}

#[tokio::test]
async fn draft_revisions_reconciliation_advances_revision_before_stale_editor_or_delete() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let stored = saved(
        call(
            &state,
            Request::SaveDraftToServer {
                draft: draft(&state),
            },
        )
        .await,
    );
    let provider_id = state
        .store
        .get_provider_draft_id(&stored.id)
        .await
        .unwrap()
        .unwrap();
    let mut remote = stored.clone();
    remote.content = mxr_core::DraftContent::markdown("provider edit");
    assert!(fake.replace_server_draft(&provider_id, remote));
    let provider_guard = state.acquire_provider_operation(&stored.account_id).await;
    let reconciling_state = state.clone();
    let account_id = stored.account_id.clone();
    let reconcile =
        tokio::spawn(
            async move { reconcile_provider_drafts(&reconciling_state, &account_id).await },
        );
    // The store read before reconciliation's lock can yield; wait for the
    // lock to be held rather than assuming one task yield reached it.
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if tokio::time::timeout(
                std::time::Duration::from_millis(10),
                state.acquire_draft_operation(&stored.id),
            )
            .await
            .is_err()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("draft writer must acquire its lock before waiting on provider");
    let deleting_state = state.clone();
    let draft_id = stored.id.clone();
    let expected_revision = stored.revision;
    let deletion = tokio::spawn(async move {
        call(
            &deleting_state,
            Request::DeleteDraft {
                draft_id,
                expected_revision,
            },
        )
        .await
    });
    assert!(!deletion.is_finished());
    drop(provider_guard);
    reconcile.await.unwrap().unwrap();
    conflict(deletion.await.unwrap(), 2);
    assert!(fake.server_drafts().contains_key(&provider_id));
    let current = state.store.get_draft(&stored.id).await.unwrap().unwrap();
    assert_eq!(current.revision, Some(2));
    assert!(current.content.analysis_text().contains("provider edit"));
    conflict(
        call(
            &state,
            Request::UpdateDraft {
                draft: stored.clone(),
            },
        )
        .await,
        2,
    );
    conflict(
        call(
            &state,
            Request::DeleteDraft {
                draft_id: stored.id.clone(),
                expected_revision: stored.revision,
            },
        )
        .await,
        2,
    );
}

#[tokio::test]
async fn draft_revisions_edit_holds_lock_while_waiting_for_provider_and_send_uses_saved_edit() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let stored = saved(
        call(
            &state,
            Request::SaveDraftToServer {
                draft: draft(&state),
            },
        )
        .await,
    );
    let provider_guard = state.acquire_provider_operation(&stored.account_id).await;
    let mut edited = stored.clone();
    edited.content = mxr_core::DraftContent::markdown("send this accepted edit");
    let editing_state = state.clone();
    let edit =
        tokio::spawn(
            async move { call(&editing_state, Request::UpdateDraft { draft: edited }).await },
        );
    // The store read before reconciliation's lock can yield; wait for the
    // lock to be held rather than assuming one task yield reached it.
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if tokio::time::timeout(
                std::time::Duration::from_millis(10),
                state.acquire_draft_operation(&stored.id),
            )
            .await
            .is_err()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("draft writer must acquire its lock before waiting on provider");
    let send_state = state.clone();
    let id = stored.id.clone();
    let send = tokio::spawn(async move { send_stored_draft(&send_state, &id, None).await });
    assert!(!edit.is_finished());
    assert!(!send.is_finished());
    drop(provider_guard);
    assert_eq!(saved(edit.await.unwrap()).revision, Some(2));
    assert!(send.await.unwrap().is_ok());
    assert_eq!(
        fake.sent_drafts().last().unwrap().content.analysis_text(),
        "send this accepted edit"
    );
}
