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
        managed_attachments: None,
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
            expected_workspace: None,
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

async fn gmail_linked_draft(
    server: &wiremock::MockServer,
    revision: Option<&str>,
) -> (Arc<AppState>, mxr_core::Draft) {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let sync = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let client = mxr_provider_gmail::client::GmailClient::new(
        mxr_provider_gmail::auth::GmailAuth::for_test_token("synthetic-token"),
    )
    .with_base_url(server.uri());
    let sender = Arc::new(mxr_provider_gmail::GmailProvider::new(account_id, client));
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync, Some(sender))
            .await
            .unwrap(),
    );
    let stored = saved(
        call(
            &state,
            Request::SaveDraft {
                draft: draft(&state),
            },
        )
        .await,
    );
    state
        .store
        .set_provider_draft_link(&stored.id, "linked-draft", revision)
        .await
        .unwrap();
    (state, stored)
}

async fn sync_linked_draft(state: &Arc<AppState>, draft: &mxr_core::Draft) {
    let response = call(
        state,
        Request::SyncNow {
            account_id: Some(draft.account_id.clone()),
            background: false,
        },
    )
    .await;
    assert!(
        matches!(
            response,
            Response::Ok {
                data: ResponseData::Ack
            }
        ),
        "{response:?}"
    );
}

fn gmail_raw_draft(revision: &str, subject: &str, body: &str) -> wiremock::ResponseTemplate {
    use base64::Engine;
    let mime = format!(
        "From: sender@example.com\r\nTo: alice@example.com\r\nSubject: {subject}\r\nContent-Type: text/plain\r\n\r\n{body}"
    );
    wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "message": {
            "id": revision,
            "raw": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mime)
        }
    }))
}

#[tokio::test]
async fn gmail_draft_sync_does_not_download_unchanged_contents() {
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/drafts/linked-draft"))
        .and(query_param("format", "minimal"))
        .and(query_param("fields", "message/id"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": { "id": "revision-1" }
        })))
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drafts/linked-draft"))
        .and(query_param("format", "raw"))
        .respond_with(gmail_raw_draft("revision-1", "Revision test", "original"))
        .expect(0)
        .mount(&server)
        .await;
    let (state, stored) = gmail_linked_draft(&server, Some("revision-1")).await;
    for _ in 0..3 {
        sync_linked_draft(&state, &stored).await;
    }
    let current = saved(
        call(
            &state,
            Request::GetDraft {
                draft_id: stored.id.clone(),
            },
        )
        .await,
    );
    assert_eq!(current.revision, stored.revision);
    assert_eq!(current.content.analysis_text(), "original");
    server.verify().await;
}

#[tokio::test]
async fn gmail_draft_sync_downloads_an_edit_once_and_tracks_the_downloaded_revision() {
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/drafts/linked-draft"))
        .and(query_param("format", "minimal"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": { "id": "revision-2" }
        })))
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drafts/linked-draft"))
        .and(query_param("format", "minimal"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": { "id": "revision-3" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    // A second Gmail edit lands between the metadata and raw requests.
    Mock::given(method("GET"))
        .and(path("/drafts/linked-draft"))
        .and(query_param("format", "raw"))
        .respond_with(gmail_raw_draft(
            "revision-3",
            "Edited in Gmail",
            "provider edit",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let (state, stored) = gmail_linked_draft(&server, Some("revision-1")).await;
    for _ in 0..2 {
        sync_linked_draft(&state, &stored).await;
        let current = saved(
            call(
                &state,
                Request::GetDraft {
                    draft_id: stored.id.clone(),
                },
            )
            .await,
        );
        assert_eq!(current.id, stored.id);
        assert_eq!(current.subject, "Edited in Gmail");
        assert_eq!(current.content.analysis_text(), "provider edit");
        assert_eq!(current.revision, Some(2));
    }
    server.verify().await;
}

#[tokio::test]
async fn gmail_draft_sync_establishes_a_legacy_baseline_without_downloading_contents() {
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/drafts/linked-draft"))
        .and(query_param("format", "minimal"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": { "id": "revision-1" }
        })))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(query_param("format", "raw"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let (state, stored) = gmail_linked_draft(&server, None).await;
    for _ in 0..2 {
        sync_linked_draft(&state, &stored).await;
    }
    let current = saved(
        call(
            &state,
            Request::GetDraft {
                draft_id: stored.id.clone(),
            },
        )
        .await,
    );
    assert_eq!(current.revision, Some(1));
    assert_eq!(current.content.analysis_text(), "original");
    server.verify().await;

    // The baseline must be remembered, so a later remote edit is pulled
    // instead of silently treated as another first sync.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/drafts/linked-draft"))
        .and(query_param("format", "minimal"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": { "id": "revision-2" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(query_param("format", "raw"))
        .respond_with(gmail_raw_draft(
            "revision-2",
            "Edited in Gmail",
            "provider edit",
        ))
        .expect(1)
        .mount(&server)
        .await;
    sync_linked_draft(&state, &stored).await;
    let current = saved(
        call(
            &state,
            Request::GetDraft {
                draft_id: stored.id.clone(),
            },
        )
        .await,
    );
    assert_eq!(current.revision, Some(2));
    assert_eq!(current.content.analysis_text(), "provider edit");
    server.verify().await;
}

#[tokio::test]
async fn gmail_draft_sync_preserves_local_contents_on_lookup_or_download_failure() {
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    for (metadata, raw_requests) in [
        (ResponseTemplate::new(403), 0),
        (
            ResponseTemplate::new(429).insert_header("retry-after", "900"),
            0,
        ),
        (ResponseTemplate::new(500), 0),
        (
            ResponseTemplate::new(200).set_body_json(serde_json::json!({})),
            0,
        ),
        (
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"message": {"id": "revision-2"}})),
            1,
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/drafts/linked-draft"))
            .and(query_param("format", "minimal"))
            .respond_with(metadata)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(query_param("format", "raw"))
            .respond_with(ResponseTemplate::new(500))
            .expect(raw_requests)
            .mount(&server)
            .await;
        let (state, stored) = gmail_linked_draft(&server, Some("revision-1")).await;
        sync_linked_draft(&state, &stored).await;
        let current = saved(
            call(
                &state,
                Request::GetDraft {
                    draft_id: stored.id.clone(),
                },
            )
            .await,
        );
        assert_eq!(current.revision, Some(1));
        assert_eq!(current.content.analysis_text(), "original");
        server.verify().await;

        // A failed content download must not advance the saved provider
        // revision and make the next successful response look unchanged.
        server.reset().await;
        Mock::given(method("GET"))
            .and(path("/drafts/linked-draft"))
            .and(query_param("format", "minimal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "id": "revision-2" }
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(query_param("format", "raw"))
            .respond_with(gmail_raw_draft(
                "revision-2",
                "Edited in Gmail",
                "provider edit",
            ))
            .expect(1)
            .mount(&server)
            .await;
        tokio::time::pause();
        tokio::time::advance(std::time::Duration::from_secs(910)).await;
        tokio::time::resume();
        sync_linked_draft(&state, &stored).await;
        let current = saved(
            call(
                &state,
                Request::GetDraft {
                    draft_id: stored.id.clone(),
                },
            )
            .await,
        );
        assert_eq!(current.revision, Some(2));
        assert_eq!(current.content.analysis_text(), "provider edit");
        server.verify().await;
    }
}

#[tokio::test]
async fn gmail_draft_sync_removes_local_drafts_only_after_explicit_remote_deletion() {
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    for deleted_after_metadata in [false, true] {
        let server = MockServer::start().await;
        let metadata = if deleted_after_metadata {
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"message": {"id": "revision-2"}}))
        } else {
            ResponseTemplate::new(404)
        };
        Mock::given(method("GET"))
            .and(path("/drafts/linked-draft"))
            .and(query_param("format", "minimal"))
            .respond_with(metadata)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(query_param("format", "raw"))
            .respond_with(ResponseTemplate::new(404))
            .expect(u64::from(deleted_after_metadata))
            .mount(&server)
            .await;
        let (state, stored) = gmail_linked_draft(&server, Some("revision-1")).await;
        sync_linked_draft(&state, &stored).await;
        assert!(matches!(call(&state, Request::ListDrafts).await,
            Response::Ok { data: ResponseData::Drafts { drafts } } if drafts.is_empty()));
        server.verify().await;
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

#[tokio::test]
async fn legacy_sent_without_receipt_is_owned_unknown_and_cannot_resend() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let stored = saved(
        call(
            &state,
            Request::SaveDraft {
                draft: draft(&state),
            },
        )
        .await,
    );
    state
        .store
        .update_draft_status(&stored.id, mxr_core::DraftStatus::Sent)
        .await
        .unwrap();
    let alias = format!("draft:{}", stored.id);
    let result = call(
        &state,
        Request::GetSendOutcome {
            account_id: stored.account_id.clone(),
            operation_id: alias.clone(),
        },
    )
    .await;
    assert!(matches!(
        result,
        Response::Ok {
            data: ResponseData::SendOutcome {
                outcome: Some(mxr_core::DraftSendOutcome {
                    state: mxr_core::SendOutcomeState::Unknown,
                    ..
                })
            }
        }
    ));
    assert!(matches!(
        call(
            &state,
            Request::GetSendOutcome {
                account_id: mxr_core::AccountId::new(),
                operation_id: alias
            }
        )
        .await,
        Response::Error { .. }
    ));
    assert!(matches!(
        call(
            &state,
            Request::SendDraft {
                draft: stored.clone(),
                send_identity: Some(mxr_core::SendIdentity::for_draft(&stored)),
                override_safety_token: None
            }
        )
        .await,
        Response::Error { .. }
    ));
    assert!(fake.sent_drafts().is_empty());
}
