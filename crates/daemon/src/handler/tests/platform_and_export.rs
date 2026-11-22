use super::*;

#[tokio::test]
async fn dispatch_unsubscribe_mailto_sends_via_provider() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let mailto_id = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 200, 0)
        .await
        .unwrap()
        .into_iter()
        .find(|envelope| matches!(envelope.unsubscribe, UnsubscribeMethod::Mailto { .. }))
        .map(|envelope| envelope.id)
        .expect("mailto fixture");

    let msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsubscribe {
            message_id: mailto_id,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack for mailto unsubscribe, got {other:?}"),
    }

    let sent = fake.sent_drafts();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to[0].email, "unsub@changelog.com");
    assert_eq!(sent[0].subject, "unsubscribe");
}

#[tokio::test]
async fn dispatch_one_click_post_fires_once_after_event_log_is_purged() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let posts = Arc::new(AtomicUsize::new(0));
    let counter = posts.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut buf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
            counter.fetch_add(1, Ordering::SeqCst);
            let _ = tokio::io::AsyncWriteExt::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await;
        }
    });

    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let mut envelope = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 200, 0)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("at least one fixture envelope");
    envelope.unsubscribe = UnsubscribeMethod::OneClick {
        url: format!("http://{addr}/unsubscribe"),
    };
    state
        .store
        .upsert_envelope_with_direction(&envelope, mxr_core::types::MessageDirection::Inbound)
        .await
        .unwrap();

    let request = || IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsubscribe {
            message_id: envelope.id.clone(),
        }),
    };

    handle_request(&state, &request()).await;
    assert_eq!(posts.load(Ordering::SeqCst), 1);

    state.store.prune_events_before(i64::MAX).await.unwrap();
    handle_request(&state, &request()).await;
    assert_eq!(
        posts.load(Ordering::SeqCst),
        1,
        "a purged log must not cause a second one-click POST"
    );
}

async fn unsubscribe_request(state: &Arc<AppState>, message_id: &mxr_core::MessageId) -> Response {
    match handle_request(
        state,
        &IpcMessage {
            expected_workspace: None,
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::Unsubscribe {
                message_id: message_id.clone(),
            }),
        },
    )
    .await
    .payload
    {
        IpcPayload::Response(response) => response,
        other => panic!("expected a response, got {other:?}"),
    }
}

async fn mailto_fixture(state: &Arc<AppState>) -> mxr_core::MessageId {
    state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 200, 0)
        .await
        .unwrap()
        .into_iter()
        .find(|envelope| matches!(envelope.unsubscribe, UnsubscribeMethod::Mailto { .. }))
        .map(|envelope| envelope.id)
        .expect("mailto fixture")
}

fn expect_error(response: &Response) -> (&str, bool) {
    match response {
        Response::Error {
            code, retryable, ..
        } => (code.as_str(), *retryable),
        other => panic!("expected an error response, got {other:?}"),
    }
}

async fn synced_state() -> (Arc<AppState>, Arc<mxr_provider_fake::FakeProvider>) {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();
    (Arc::new(state), fake)
}

fn error_message(response: &Response) -> &str {
    match response {
        Response::Error { message, .. } => message.as_str(),
        other => panic!("expected an error response, got {other:?}"),
    }
}

fn is_ack(response: &Response) -> bool {
    matches!(
        response,
        Response::Ok {
            data: ResponseData::Ack
        }
    )
}

#[tokio::test]
async fn concurrent_unsubscribes_send_exactly_once() {
    let (state, fake) = synced_state().await;
    let id = mailto_fixture(&state).await;
    let (first, second) = tokio::join!(
        unsubscribe_request(&state, &id),
        unsubscribe_request(&state, &id),
    );
    for response in [&first, &second] {
        assert!(
            is_ack(response) || expect_error(response) == ("unsubscribe_in_progress", true),
            "{response:?}"
        );
    }
    assert_eq!(fake.sent_drafts().len(), 1, "only the claimant sends");
}

#[tokio::test]
async fn a_send_that_cannot_be_recorded_is_not_retryable_and_never_resent() {
    let (state, fake) = synced_state().await;
    let id = mailto_fixture(&state).await;
    sqlx::query(
        "CREATE TRIGGER fail_confirm BEFORE UPDATE ON unsubscribe_receipts
         BEGIN SELECT RAISE(ABORT, 'injected receipt failure'); END",
    )
    .execute(state.store.writer())
    .await
    .unwrap();

    let response = unsubscribe_request(&state, &id).await;
    assert_eq!(
        expect_error(&response),
        ("unsubscribe_sent_not_recorded", false),
        "{response:?}"
    );
    assert_eq!(fake.sent_drafts().len(), 1);

    sqlx::query("DROP TRIGGER fail_confirm")
        .execute(state.store.writer())
        .await
        .unwrap();
    let retry = unsubscribe_request(&state, &id).await;
    assert_eq!(
        expect_error(&retry),
        ("unsubscribe_in_progress", true),
        "{retry:?}"
    );
    assert_eq!(fake.sent_drafts().len(), 1, "a retry must not send again");
}

#[tokio::test]
async fn a_failed_send_releases_the_claim_and_a_retry_may_send_again() {
    let (state, fake) = synced_state().await;
    let id = mailto_fixture(&state).await;
    let address = mailto_address(&state, &id).await;
    fake.fail_sends_after_accepting(true);

    let response = unsubscribe_request(&state, &id).await;
    assert_eq!(
        expect_error(&response),
        ("unsubscribe_not_confirmed", true),
        "{response:?}"
    );
    let message = error_message(&response);
    assert!(
        !message.contains(&address),
        "the recipient must not reach the error: {message}"
    );
    assert!(
        !message.contains("PROVIDER-TEXT"),
        "provider text must not reach the error: {message}"
    );
    assert!(!message.contains("mailto"), "{message}");

    let receipts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM unsubscribe_receipts WHERE message_id = ?")
            .bind(id.as_str())
            .fetch_one(state.store.reader())
            .await
            .unwrap();
    assert_eq!(receipts, 0, "the claim is released");

    fake.fail_sends_after_accepting(false);
    let before_retry = fake.sent_drafts().len();
    let retry = unsubscribe_request(&state, &id).await;
    assert!(is_ack(&retry), "{retry:?}");
    assert_eq!(
        fake.sent_drafts().len(),
        before_retry + 1,
        "the retry sends once"
    );
}

async fn mailto_address(state: &Arc<AppState>, id: &mxr_core::MessageId) -> String {
    state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 200, 0)
        .await
        .unwrap()
        .into_iter()
        .find(|envelope| &envelope.id == id)
        .and_then(|envelope| match envelope.unsubscribe {
            UnsubscribeMethod::Mailto { address, .. } => Some(address),
            _ => None,
        })
        .expect("mailto address")
}

#[tokio::test]
async fn dispatch_save_and_send_stored_draft() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id,
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Stored draft".to_string(),
        content: mxr_core::types::DraftContent::markdown("Test body".to_string()),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let save_msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SaveDraft {
            draft: draft.clone(),
        }),
    };
    let save_resp = handle_request(&state, &save_msg).await;
    assert!(matches!(
        save_resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { .. }
        })
    ));

    let send_msg = IpcMessage {
        expected_workspace: None,
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
            managed_attachments_reviewed: true,
            send_identity: Some(mxr_core::SendIdentity::for_revision(&draft.id, 1)),
            draft_id: draft.id.clone(),
            override_safety_token: None,
        }),
    };
    let send_resp = handle_request(&state, &send_msg).await;
    assert!(
        matches!(
            send_resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SendReceipt { .. }
            })
        ),
        "send_stored_draft should return SendReceipt, got {:?}",
        send_resp.payload
    );

    assert_eq!(fake.sent_drafts().len(), 1);
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_none());
}

#[tokio::test]
async fn dispatch_send_stored_draft_replay_returns_original_receipt_without_resending() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id,
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Stored replay-safe draft".to_string(),
        content: mxr_core::types::DraftContent::markdown("Test body".to_string()),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();

    let send = || IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
            managed_attachments_reviewed: true,
            send_identity: Some(mxr_core::SendIdentity::for_revision(&draft.id, 1)),
            draft_id: draft.id.clone(),
            override_safety_token: None,
        }),
    };
    let receipt = |resp: IpcMessage| match resp.payload {
        IpcPayload::Response(Response::Ok {
            data:
                ResponseData::SendReceipt {
                    local_message_id,
                    provider_message_id,
                    rfc2822_message_id,
                },
        }) => (local_message_id, provider_message_id, rfc2822_message_id),
        other => panic!("Expected SendReceipt, got {other:?}"),
    };

    let first = receipt(handle_request(&state, &send()).await);
    assert_eq!(fake.sent_drafts().len(), 1);
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_none());

    let second = receipt(handle_request(&state, &send()).await);
    assert_eq!(
        fake.sent_drafts().len(),
        1,
        "replaying the same SendStoredDraft id must not invoke the provider twice"
    );
    assert_eq!(
        second, first,
        "a replay should return the original send receipt so clients can safely retry"
    );
}

#[tokio::test]
async fn override_token_unblocks_send_exactly_once() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        revision: None,
        id: mxr_core::DraftId::new(),
        account_id: account_id.clone(),
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "alice@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "key transfer".to_string(),
        content: mxr_core::types::DraftContent::markdown(
            concat!(
                "Here is the key:\n",
                "-----BEGIN RSA ",
                "PRIVATE KEY-----\n...\n"
            )
            .to_string(),
        ),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let saved_draft = |response: IpcMessage, expected_id: &mxr_core::DraftId| match response.payload
    {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { draft },
        }) => {
            assert_eq!(&draft.id, expected_id);
            assert!(
                draft.revision.is_some(),
                "saved draft must carry its canonical revision"
            );
            draft
        }
        other => panic!("expected confirmed Draft, got {other:?}"),
    };

    let save = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SaveDraft {
                draft: draft.clone(),
            }),
        },
    )
    .await;
    let draft = saved_draft(save, &draft.id);

    let check = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 2,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::CheckDraftSafety {
                draft: draft.clone(),
                context: Default::default(),
            }),
        },
    )
    .await;
    let token = match check.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::DraftSafetyReportResponse { report },
        }) => {
            assert!(matches!(
                report.verdict,
                mxr_core::DraftSafetyVerdict::Blocked
            ));
            let blocker = report
                .issues
                .iter()
                .find(|i| i.severity == mxr_core::DraftSafetySeverity::Blocker)
                .expect("at least one blocker");
            blocker
                .override_token
                .clone()
                .expect("blocker should carry override token")
        }
        other => panic!("expected DraftSafetyReportResponse, got {other:?}"),
    };

    assert_eq!(fake.sent_drafts().len(), 0);
    let blocked = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 3,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
                managed_attachments_reviewed: true,
                send_identity: Some(mxr_core::SendIdentity::for_revision(&draft.id, 1)),
                draft_id: draft.id.clone(),
                override_safety_token: None,
            }),
        },
    )
    .await;
    match blocked.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("blocked"), "{message}");
        }
        other => panic!("expected Error, got {other:?}"),
    }
    assert_eq!(
        fake.sent_drafts().len(),
        0,
        "provider must NOT be called when blocked"
    );
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_some());

    let ok = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 4,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
                managed_attachments_reviewed: true,
                send_identity: Some(mxr_core::SendIdentity::for_revision(&draft.id, 1)),
                draft_id: draft.id.clone(),
                override_safety_token: Some(token.clone()),
            }),
        },
    )
    .await;
    assert!(
        matches!(
            ok.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SendReceipt { .. }
            })
        ),
        "expected SendReceipt with override, got {:?}",
        ok.payload
    );
    assert_eq!(fake.sent_drafts().len(), 1);

    let draft2 = mxr_core::types::Draft {
        revision: None,
        id: mxr_core::DraftId::new(),
        account_id: account_id.clone(),
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "bob@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "again".into(),
        content: mxr_core::types::DraftContent::markdown(concat!(
            "-----BEGIN RSA ",
            "PRIVATE KEY-----\nzz\n"
        )),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let save2 = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 5,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SaveDraft {
                draft: draft2.clone(),
            }),
        },
    )
    .await;
    let draft2 = saved_draft(save2, &draft2.id);
    let check2 = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 6,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::CheckDraftSafety {
                draft: draft2.clone(),
                context: Default::default(),
            }),
        },
    )
    .await;
    let token2 = match check2.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::DraftSafetyReportResponse { report },
        }) => report
            .issues
            .iter()
            .find(|i| i.severity == mxr_core::DraftSafetySeverity::Blocker)
            .and_then(|i| i.override_token.clone())
            .expect("blocker token"),
        other => panic!("unexpected: {other:?}"),
    };
    let first = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 7,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
                managed_attachments_reviewed: true,
                send_identity: Some(mxr_core::SendIdentity::for_revision(&draft2.id, 1)),
                draft_id: draft2.id.clone(),
                override_safety_token: Some(token2.clone()),
            }),
        },
    )
    .await;
    assert!(matches!(
        first.payload,
        IpcPayload::Response(Response::Ok { .. })
    ));
    let draft3 = mxr_core::types::Draft {
        revision: None,
        id: mxr_core::DraftId::new(),
        ..draft2
    };
    let save3 = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 8,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SaveDraft {
                draft: draft3.clone(),
            }),
        },
    )
    .await;
    let draft3 = saved_draft(save3, &draft3.id);
    let reuse = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 9,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
                managed_attachments_reviewed: true,
                send_identity: Some(mxr_core::SendIdentity::for_revision(&draft3.id, 1)),
                draft_id: draft3.id.clone(),
                override_safety_token: Some(token2),
            }),
        },
    )
    .await;
    match reuse.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.contains("override token unknown or already used")
                    || message.contains("does not cover blocker"),
                "got {message}"
            );
        }
        other => panic!("expected error on token reuse, got {other:?}"),
    }
}

#[tokio::test]
async fn send_stored_draft_without_provider_does_not_claim_submission() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, None)
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id,
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Heartbeat probe".to_string(),
        content: mxr_core::types::DraftContent::markdown("Body".to_string()),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();
    assert_eq!(
        state.store.get_draft_heartbeat(&draft.id).await.unwrap(),
        None,
        "fresh draft must have NULL last_heartbeat_at"
    );

    let send_msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
            managed_attachments_reviewed: true,
            send_identity: Some(mxr_core::SendIdentity::for_revision(&draft.id, 1)),
            draft_id: draft.id.clone(),
            override_safety_token: None,
        }),
    };
    let send_resp = handle_request(&state, &send_msg).await;
    assert!(
        matches!(
            send_resp.payload,
            IpcPayload::Response(Response::Error { .. })
        ),
        "send_stored_draft without a send provider must error, got {:?}",
        send_resp.payload
    );

    assert_eq!(
        state.store.get_draft_heartbeat(&draft.id).await.unwrap(),
        None
    );
    assert_eq!(
        state.store.get_draft_status(&draft.id).await.unwrap(),
        Some(mxr_core::DraftStatus::Draft)
    );
    assert!(state
        .store
        .get_draft_send_outcome(&draft.id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn send_stored_draft_blocks_empty_recipient_before_sending_state() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id,
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "No recipients".to_string(),
        content: mxr_core::types::DraftContent::markdown("Body".to_string()),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();

    let send_msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
            managed_attachments_reviewed: true,
            send_identity: Some(mxr_core::SendIdentity::for_revision(&draft.id, 1)),
            draft_id: draft.id.clone(),
            override_safety_token: None,
        }),
    };
    match handle_request(&state, &send_msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("draft safety"));
            assert!(message.contains("recipient"));
        }
        other => panic!("Expected draft safety error, got {other:?}"),
    }

    assert_eq!(
        state.store.get_draft_status(&draft.id).await.unwrap(),
        Some(mxr_core::DraftStatus::Draft)
    );
    assert_eq!(
        state.store.get_draft_heartbeat(&draft.id).await.unwrap(),
        None
    );
    assert_eq!(fake.sent_drafts().len(), 0);
}

#[tokio::test]
async fn dispatch_export_thread_markdown() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let list_msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 1,
            offset: 0,
            label_role: None,
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    let thread_id = match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => envelopes[0].thread_id.clone(),
        other => panic!("Expected Envelopes, got {other:?}"),
    };

    let export_msg = IpcMessage {
        expected_workspace: None,
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ExportThread {
            thread_id,
            format: mxr_core::types::ExportFormat::Markdown,
        }),
    };
    let resp = handle_request(&state, &export_msg).await;
    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ExportResult { content },
        }) => {
            assert!(
                content.starts_with("# Thread:"),
                "Should be markdown: {content}"
            );
            assert!(content.contains("Exported from mxr"));
        }
        other => panic!("Expected ExportResult, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_export_search_json_is_valid() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        expected_workspace: None,
        id: 4,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ExportSearch {
            query: "deployment".into(),
            account_id: None,
            format: mxr_core::types::ExportFormat::Json,
        }),
    };
    let resp = handle_request(&state, &msg).await;

    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ExportResult { content },
        }) => {
            let parsed: serde_json::Value =
                serde_json::from_str(content).expect("Export JSON should be valid");
            let messages = parsed["messages"]
                .as_array()
                .expect("export search should include messages");
            assert!(!messages.is_empty(), "export search should return results");
            assert!(messages[0].as_object().is_some());
        }
        other => panic!("Expected ExportResult, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_save_draft_to_server_updates_linked_provider_draft_in_place() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );
    let mut draft = mxr_core::types::Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id,
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "recipient@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "First version".into(),
        content: mxr_core::types::DraftContent::markdown("Body"),
        inline_assets: vec![],
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();

    let first = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 51,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SaveDraftToServer {
                draft: draft.clone(),
            }),
        },
    )
    .await;
    assert!(matches!(
        first.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { .. }
        })
    ));
    let provider_draft_id = state
        .store
        .get_provider_draft_id(&draft.id)
        .await
        .unwrap()
        .expect("first push should retain the provider draft id");

    draft.subject = "Edited version".into();
    draft.updated_at = chrono::Utc::now();
    let second = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 52,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::UpdateDraft {
                draft: draft.clone(),
            }),
        },
    )
    .await;
    assert!(matches!(
        second.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { .. }
        })
    ));
    assert_eq!(
        state.store.get_provider_draft_id(&draft.id).await.unwrap(),
        Some(provider_draft_id.clone())
    );
    let server_drafts = fake.server_drafts();
    assert_eq!(server_drafts.len(), 1, "second push must not create a copy");
    assert_eq!(server_drafts[&provider_draft_id].subject, "Edited version");

    let remote_edit = mxr_core::Draft {
        subject: "Edited in Gmail".into(),
        content: mxr_core::DraftContent::markdown("Remote body"),
        updated_at: chrono::Utc::now(),
        ..draft.clone()
    };
    assert!(fake.replace_server_draft(&provider_draft_id, remote_edit));
    assert_eq!(
        reconcile_provider_drafts(&state, &draft.account_id)
            .await
            .unwrap(),
        0
    );
    let pulled = state.store.get_draft(&draft.id).await.unwrap().unwrap();
    assert_eq!(pulled.subject, "Edited in Gmail");
    assert_eq!(pulled.content.analysis_text(), "Remote body");

    let locally_deleted = mxr_core::types::Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        subject: "Delete from mxr".into(),
        ..draft.clone()
    };
    state.store.insert_draft(&locally_deleted).await.unwrap();
    let save_local_delete = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 53,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SaveDraftToServer {
                draft: locally_deleted.clone(),
            }),
        },
    )
    .await;
    assert!(matches!(
        save_local_delete.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { .. }
        })
    ));
    let locally_deleted_provider_id = state
        .store
        .get_provider_draft_id(&locally_deleted.id)
        .await
        .unwrap()
        .unwrap();
    let delete_local = handle_request(
        &state,
        &IpcMessage {
            expected_workspace: None,
            id: 54,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::DeleteDraft {
                expected_revision: Some(1),
                draft_id: locally_deleted.id.clone(),
            }),
        },
    )
    .await;
    assert!(matches!(
        delete_local.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));
    assert!(state
        .store
        .get_draft(&locally_deleted.id)
        .await
        .unwrap()
        .is_none());
    assert!(!fake
        .server_drafts()
        .contains_key(&locally_deleted_provider_id));

    mxr_core::MailSendProvider::delete_draft(fake.as_ref(), &provider_draft_id)
        .await
        .unwrap();
    assert_eq!(
        reconcile_provider_drafts(&state, &draft.account_id)
            .await
            .unwrap(),
        1
    );
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_none());
}

#[tokio::test]
async fn dispatch_sync_now_wakes_the_sync_loop_while_the_backfill_has_more() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_provider().account_id().clone();
    state.add_sync_provider_for_test(Arc::new(
        mxr_provider_fake::FakeProvider::with_demo_dataset(account_id.clone(), 40)
            .with_page_size(10),
    ));
    let notify = state.idle_notify_for_account(&account_id);

    let msg = IpcMessage {
        expected_workspace: None,
        id: 301,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SyncNow {
            account_id: Some(account_id.clone()),
            background: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    assert!(
        matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ),
        "expected Ack, got {:?}",
        resp.payload
    );

    let woken = tokio::time::timeout(std::time::Duration::from_secs(2), notify.notified())
        .await
        .is_ok();
    assert!(
        woken,
        "a mid-backfill sync must wake the account's sync loop"
    );
}

#[tokio::test]
async fn dispatch_background_sync_now_runs_the_full_fan_out() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_provider().account_id().clone();
    let mut events = state.event_tx.subscribe();

    let msg = IpcMessage {
        expected_workspace: None,
        id: 311,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SyncNow {
            account_id: Some(account_id.clone()),
            background: true,
        }),
    };
    handle_request(&state, &msg).await;

    let mut saw_new_messages = false;
    let mut saw_completed = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while !(saw_new_messages && saw_completed) {
        let event = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("background sync must emit its events")
            .expect("event stream stays open");
        match event.payload {
            IpcPayload::Event(::mxr_protocol::DaemonEvent::NewMessages { .. }) => {
                saw_new_messages = true;
            }
            IpcPayload::Event(::mxr_protocol::DaemonEvent::SyncCompleted { .. }) => {
                saw_completed = true;
            }
            _ => {}
        }
    }

    let status = state
        .store
        .get_sync_runtime_status(&account_id)
        .await
        .unwrap()
        .expect("sync status row");
    assert!(
        !status.sync_in_progress,
        "the account must be idle once the drained background sync finishes"
    );
    assert!(status.last_success_at.is_some());
}

#[tokio::test]
async fn resolve_time_answers_with_choices_or_a_specific_error() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let now = chrono::Utc.with_ymd_and_hms(2030, 5, 7, 12, 0, 0).unwrap();
    let resolve = |input: &str| IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ResolveTime {
            input: input.into(),
            now: Some(now),
            time_zone: None,
        }),
    };

    let resp = handle_request(&state, &resolve("fri 3")).await;
    let IpcPayload::Response(Response::Ok {
        data:
            ResponseData::ResolvedTime {
                input,
                resolution: Some(resolution),
                error: None,
            },
    }) = resp.payload
    else {
        panic!("expected a resolution, got {:?}", resp.payload);
    };
    assert_eq!(input, "fri 3");
    let labels: Vec<&str> = resolution
        .choices
        .iter()
        .map(|c| c.label.as_str())
        .collect();
    assert_eq!(labels, ["15:00", "03:00"]);
    assert_eq!(resolution.at, resolution.choices[0].at);
    let local = resolution.at.with_timezone(&chrono::Local);
    assert_eq!(chrono::Datelike::weekday(&local), chrono::Weekday::Fri);
    assert_eq!(chrono::Timelike::hour(&local), 15);

    let resp = handle_request(&state, &resolve("frday")).await;
    let IpcPayload::Response(Response::Ok {
        data:
            ResponseData::ResolvedTime {
                resolution: None,
                error: Some(error),
                ..
            },
    }) = resp.payload
    else {
        panic!("expected an error answer, got {:?}", resp.payload);
    };
    assert_eq!(
        error.kind,
        mxr_core::natural_time::TimeResolveErrorKind::Unrecognized
    );
    assert_eq!(error.token.as_deref(), Some("frday"));
}

#[tokio::test]
async fn saved_search_names_stay_unique() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let create = |name: &str| IpcMessage {
        expected_workspace: None,
        id: 30,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateSavedSearch {
            name: name.to_string(),
            query: "is:unread".to_string(),
            account_id: None,
            search_mode: mxr_core::SearchMode::Lexical,
        }),
    };
    let ok =
        |message: &IpcMessage| matches!(message.payload, IpcPayload::Response(Response::Ok { .. }));
    assert!(ok(&handle_request(&state, &create("Unread")).await));
    assert!(ok(&handle_request(&state, &create("Later")).await));
    assert!(!ok(&handle_request(&state, &create("Unread")).await));

    let rename = IpcMessage {
        expected_workspace: None,
        id: 31,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UpdateSavedSearch {
            name: "Later".to_string(),
            new_name: Some("Unread".to_string()),
            query: None,
            search_mode: None,
            sort: None,
            icon: None,
            position: None,
        }),
    };
    assert!(!ok(&handle_request(&state, &rename).await));
    assert_eq!(state.store.list_saved_searches().await.unwrap().len(), 2);
}
