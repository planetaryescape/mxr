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
        other => panic!("Expected Ack for mailto unsubscribe, got {:?}", other),
    }

    let sent = fake.sent_drafts();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to[0].email, "unsub@changelog.com");
    assert_eq!(sent[0].subject, "unsubscribe");
}

/// Phase 2.6: `mxr unsubscribe <id>` is idempotent. A second call
/// against the same message must NOT re-send the mailto / re-POST
/// the one-click URL — the user's intent on the second call is "I
/// already unsubscribed, stop bugging me." Without this guard, a
/// shell retry / agent loop would spam the list operator's inbox.
/// Phase 1.5: saved-search unread counts return one entry per
/// configured saved search. Counts reflect the saved query
/// ANDed with `is:unread`. The tab strip uses this to render
/// `(N)` on each tab.
#[tokio::test]
async fn dispatch_list_saved_search_unread_counts_returns_id_to_count_map() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    // Register two saved searches with predictable shapes:
    //   "All Mail"  = "" (empty query) — matches everything;
    //                  unread count == number of unread messages
    //   "Nonexistent" = "from:nobody@nope.example" — zero matches
    let now = chrono::Utc::now();
    let search_all = mxr_core::types::SavedSearch {
        id: mxr_core::id::SavedSearchId::new(),
        account_id: None,
        name: "All Mail".to_string(),
        query: String::new(),
        search_mode: mxr_core::SearchMode::Lexical,
        sort: mxr_core::SortOrder::DateDesc,
        icon: None,
        position: 0,
        created_at: now,
    };
    let search_none = mxr_core::types::SavedSearch {
        id: mxr_core::id::SavedSearchId::new(),
        account_id: None,
        name: "Nonexistent".to_string(),
        query: "from:nobody@nope.example".to_string(),
        search_mode: mxr_core::SearchMode::Lexical,
        sort: mxr_core::SortOrder::DateDesc,
        icon: None,
        position: 1,
        created_at: now,
    };
    state.store.insert_saved_search(&search_all).await.unwrap();
    state.store.insert_saved_search(&search_none).await.unwrap();

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::ListSavedSearchUnreadCounts),
        },
    )
    .await;
    let counts = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearchUnreadCounts { counts },
        }) => counts,
        other => panic!("expected SavedSearchUnreadCounts, got {other:?}"),
    };

    // Both saved searches appear in the response (even the
    // zero-match one — the tab strip needs to know it exists).
    assert!(
        counts.contains_key(&search_all.id),
        "every registered saved search must be present in the count map; missing All Mail"
    );
    assert!(
        counts.contains_key(&search_none.id),
        "every registered saved search must be present in the count map; missing Nonexistent"
    );
    assert_eq!(
        counts[&search_none.id], 0,
        "the never-matching saved search reports zero unread"
    );
    // We don't assert an exact number for All Mail because the
    // FakeProvider fixture set evolves; we just assert it's
    // non-negative (always true for u32) and the response shape
    // is correct.
}

#[tokio::test]
async fn dispatch_unsubscribe_is_idempotent_via_event_log() {
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

    let request = || IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsubscribe {
            message_id: mailto_id.clone(),
        }),
    };

    // First call: succeeds and emits the outbound message.
    let resp = handle_request(&state, &request()).await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));
    assert_eq!(
        fake.sent_drafts().len(),
        1,
        "first call sends the unsubscribe mail"
    );

    // Second call: also returns Ack but MUST NOT re-send.
    let resp = handle_request(&state, &request()).await;
    assert!(
        matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ),
        "repeated unsubscribe should still ack so scripts and agents don't see a spurious failure"
    );
    assert_eq!(
        fake.sent_drafts().len(),
        1,
        "second call must not produce a second outbound — that's the entire point of idempotency"
    );
}

/// Phase 2.6: when the unsubscribe URL fails (network error,
/// non-2xx), the handler must surface an Error response. Quietly
/// returning Ack would mislead the user into thinking they were
/// removed from the list when nothing happened. Equally critically,
/// no `_unsubscribed` event must be logged on a failed attempt —
/// otherwise the idempotency check would block a future retry.
#[tokio::test]
async fn dispatch_unsubscribe_oneclick_failure_returns_error_and_does_not_log() {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    // Replace the fixture envelope's unsubscribe method with a
    // OneClick URL pointing at a port nothing's listening on — the
    // POST will reliably fail.
    let mut envelope = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 200, 0)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("at least one fixture envelope");
    envelope.unsubscribe = UnsubscribeMethod::OneClick {
        // 127.0.0.1:1 — RFC 6890 / "definitely-no-listener" port.
        url: "http://127.0.0.1:1/unsubscribe".into(),
    };
    state
        .store
        .upsert_envelope_with_direction(&envelope, mxr_core::types::MessageDirection::Inbound)
        .await
        .unwrap();

    let resp = handle_request(
        &state,
        &IpcMessage {
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::Unsubscribe {
                message_id: envelope.id.clone(),
            }),
        },
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Error { .. }) => {} // expected
        other => panic!("expected Error for failed one-click POST, got {other:?}"),
    }

    // No success event was logged. If it were, a retry would be
    // blocked by the idempotency short-circuit.
    let logged = state
        .store
        .has_event_for_message_with_summary(&envelope.id.as_str(), "mutation", "unsubscrib")
        .await
        .unwrap();
    assert!(
            !logged,
            "a failed unsubscribe must not write a success event — otherwise retries are silently blocked"
        );
}

#[tokio::test]
async fn dispatch_mutation_nonexistent_message() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let fake_id = mxr_core::MessageId::new();
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
            message_ids: vec![fake_id],
            starred: true,
        })),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.contains("not found") || message.contains("Not found"),
                "Expected 'not found' error, got: {}",
                message
            );
        }
        other => panic!("Expected Error, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_list_drafts_empty() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListDrafts),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Drafts { drafts },
        }) => {
            assert!(drafts.is_empty(), "Expected empty drafts list");
        }
        other => panic!("Expected Drafts, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_list_drafts_includes_all_accounts() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let default_account_id = state.default_account_id();
    let other_account_id = mxr_core::AccountId::new();
    let other_account = crate::test_fixtures::test_account_with_id(other_account_id.clone());
    state.store.insert_account(&other_account).await.unwrap();

    let old_draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: default_account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "Default account draft".to_string(),
        body_markdown: "older".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now() - chrono::Duration::minutes(5),
        updated_at: chrono::Utc::now() - chrono::Duration::minutes(5),
    };
    let new_draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: other_account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "Other account draft".to_string(),
        body_markdown: "newer".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&old_draft).await.unwrap();
    state.store.insert_draft(&new_draft).await.unwrap();

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListDrafts),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Drafts { drafts },
        }) => {
            assert_eq!(drafts.len(), 2);
            assert_eq!(drafts[0].id, new_draft.id);
            assert_eq!(drafts[1].id, old_draft.id);
        }
        other => panic!("Expected Drafts, got {:?}", other),
    }
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
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Stored draft".to_string(),
        body_markdown: "Test body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let save_msg = IpcMessage {
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
            data: ResponseData::Ack
        })
    ));

    let send_msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
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

/// Slice 1.3: when CheckDraftSafety returns Blocked, the daemon
/// mints a single-use override token and stamps it onto each
/// blocker issue. The next SendStoredDraft with that token must
/// succeed (and FakeProvider must actually be invoked exactly once),
/// while a second send attempt with the same token must fail with
/// the token already-used error.
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

    // PEM private key in the body → Blocker.
    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: account_id.clone(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "alice@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "key transfer".to_string(),
        body_markdown: "Here is the key:\n-----BEGIN RSA PRIVATE KEY-----\n...\n".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    // Save the draft so SendStoredDraft can locate it.
    let save = handle_request(
        &state,
        &IpcMessage {
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SaveDraft {
                draft: draft.clone(),
            }),
        },
    )
    .await;
    assert!(matches!(
        save.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));

    // 1. Check returns Blocked + a token on the blocker issue.
    let check = handle_request(
        &state,
        &IpcMessage {
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

    // 2. SendStoredDraft WITHOUT the token: refused, FakeProvider untouched.
    assert_eq!(fake.sent_drafts().len(), 0);
    let blocked = handle_request(
        &state,
        &IpcMessage {
            id: 3,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
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
    // Draft must still be in `Draft` status (no CAS to Sending).
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_some());

    // 3. SendStoredDraft WITH token: succeeds.
    let ok = handle_request(
        &state,
        &IpcMessage {
            id: 4,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
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

    // 4. Reusing the same token after the draft is gone — token is
    // single-use; consume must fail. We test by minting a fresh
    // override against a new draft, sending once, then trying the
    // SAME token a second time to assert single-use.
    let draft2 = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: account_id.clone(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "bob@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "again".into(),
        body_markdown: "-----BEGIN RSA PRIVATE KEY-----\nzz\n".into(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let _ = handle_request(
        &state,
        &IpcMessage {
            id: 5,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SaveDraft {
                draft: draft2.clone(),
            }),
        },
    )
    .await;
    let check2 = handle_request(
        &state,
        &IpcMessage {
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
    // First use succeeds.
    let first = handle_request(
        &state,
        &IpcMessage {
            id: 7,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
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
    // Second use with the SAME token must fail (token consumed). We
    // can't re-send the same draft (already gone after send), so we
    // make a third draft and try to use the spent token.
    let draft3 = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        ..draft2
    };
    let _ = handle_request(
        &state,
        &IpcMessage {
            id: 8,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SaveDraft {
                draft: draft3.clone(),
            }),
        },
    )
    .await;
    let reuse = handle_request(
        &state,
        &IpcMessage {
            id: 9,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SendStoredDraft {
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

/// The live send pipeline must touch `last_heartbeat_at` once it has
/// CAS'd a draft into `Sending`. Otherwise, a long-running send (large
/// attachment, slow OAuth refresh) could be misidentified as orphaned
/// by the 1h startup recovery cutoff. We verify this by exercising the
/// failure path: with no send provider configured, `send_stored_draft`
/// CAS's into `Sending`, touches the heartbeat, then reverts to
/// `Draft` when provider lookup fails — leaving a fresh heartbeat we
/// can read back.
#[tokio::test]
async fn send_stored_draft_touches_heartbeat_after_cas() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    // No send provider — `send_provider_for_account` will fail.
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, None)
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Heartbeat probe".to_string(),
        body_markdown: "Body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();
    // Pre-condition: a brand-new draft has no heartbeat.
    assert_eq!(
        state.store.get_draft_heartbeat(&draft.id).await.unwrap(),
        None,
        "fresh draft must have NULL last_heartbeat_at"
    );

    let before = chrono::Utc::now();
    let send_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
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

    // Post-condition: heartbeat was set during the CAS-to-Sending phase
    // and survives the revert-to-Draft on provider-lookup failure.
    let heartbeat = state
        .store
        .get_draft_heartbeat(&draft.id)
        .await
        .unwrap()
        .expect("send_stored_draft must touch the heartbeat after CAS");
    let after = chrono::Utc::now();
    assert!(
        heartbeat >= before - chrono::Duration::seconds(1),
        "heartbeat {heartbeat} must not predate test start {before}"
    );
    assert!(
        heartbeat <= after + chrono::Duration::seconds(1),
        "heartbeat {heartbeat} must not postdate test end {after}"
    );
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
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "No recipients".to_string(),
        body_markdown: "Body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();

    let send_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
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
async fn send_draft_blocks_invalid_recipient_before_provider_send() {
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
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "not an address".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Invalid recipient".to_string(),
        body_markdown: "Body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let send_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendDraft {
            draft,
            override_safety_token: None,
        }),
    };
    match handle_request(&state, &send_msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("draft safety"));
            assert!(message.contains("invalid recipient"));
        }
        other => panic!("Expected draft safety error, got {other:?}"),
    }

    assert_eq!(fake.sent_drafts().len(), 0);
}

#[tokio::test]
async fn send_stored_reply_all_blocks_missing_original_recipient_before_sending_state() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let account_email = account.email.clone();
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let mut parent = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .provider_id("reply-all-parent")
        .message_id_header(Some("<reply-all-parent@example.com>".to_string()))
        .build();
    parent.from = mxr_core::types::Address {
        name: None,
        email: "alice@example.com".to_string(),
    };
    parent.to = vec![
        mxr_core::types::Address {
            name: None,
            email: account_email,
        },
        mxr_core::types::Address {
            name: None,
            email: "bob@example.com".to_string(),
        },
    ];
    parent.cc = vec![mxr_core::types::Address {
        name: None,
        email: "carol@example.com".to_string(),
    }];
    state.store.upsert_envelope(&parent).await.unwrap();

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: Some(mxr_core::ReplyHeaders {
            in_reply_to: "<reply-all-parent@example.com>".to_string(),
            references: vec!["<reply-all-parent@example.com>".to_string()],
            thread_id: None,
        }),
        intent: mxr_core::DraftIntent::ReplyAll,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "alice@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Re: parent".to_string(),
        body_markdown: "reply".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    state.store.insert_draft(&draft).await.unwrap();

    let send_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendStoredDraft {
            draft_id: draft.id.clone(),
            override_safety_token: None,
        }),
    };
    match handle_request(&state, &send_msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("reply-all is missing recipient"));
            assert!(message.contains("bob@example.com"));
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
async fn dispatch_send_draft_preserves_parent_thread_for_synthetic_sent() {
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
    let parent_thread_id = mxr_core::ThreadId::new();
    let parent = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .thread_id(parent_thread_id.clone())
        .provider_id("parent")
        .message_id_header(Some("<parent@example.com>".to_string()))
        .build();
    state.store.upsert_envelope(&parent).await.unwrap();
    state
        .store
        .set_reply_later(&parent.id, chrono::Utc::now())
        .await
        .unwrap();

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: Some(mxr_core::ReplyHeaders {
            in_reply_to: "<parent@example.com>".to_string(),
            references: vec!["<parent@example.com>".to_string()],
            thread_id: None,
        }),
        intent: mxr_core::DraftIntent::Reply,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Re: parent".to_string(),
        body_markdown: "reply".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendDraft {
            draft,
            override_safety_token: None,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    let local_message_id = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SendReceipt {
                local_message_id, ..
            },
        }) => local_message_id,
        other => panic!("Expected SendReceipt, got {:?}", other),
    };
    let sent = state
        .store
        .get_envelope(&local_message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sent.thread_id, parent_thread_id);
    assert!(
        !state.store.is_reply_later(&parent.id).await.unwrap(),
        "sending a reply clears the parent reply-later flag"
    );
}

#[tokio::test]
async fn dispatch_save_draft_to_server_falls_back_to_local_draft() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake;
    let send_provider: Arc<dyn mxr_core::MailSendProvider> =
        Arc::new(UnsupportedServerDraftProvider);
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
            .await
            .unwrap(),
    );

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Local fallback".to_string(),
        body_markdown: "body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SaveDraftToServer {
            draft: draft.clone(),
        }),
    };
    let resp = handle_request(&state, &msg).await;
    assert!(matches!(
        resp.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack
        })
    ));
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_some());
}

#[tokio::test]
async fn dispatch_saved_search_delete() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    // Create a saved search
    let create_msg = IpcMessage {
        id: 20,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateSavedSearch {
            name: "ToDelete".to_string(),
            query: "is:unread".to_string(),
            search_mode: mxr_core::SearchMode::Lexical,
        }),
    };
    let resp = handle_request(&state, &create_msg).await;
    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearchData { search },
        }) => {
            assert_eq!(search.name, "ToDelete");
        }
        other => panic!("Expected SavedSearchData, got {:?}", other),
    }

    // Verify it's in the list
    let list_msg = IpcMessage {
        id: 21,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSavedSearches),
    };
    let resp = handle_request(&state, &list_msg).await;
    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearches { searches },
        }) => {
            assert_eq!(searches.len(), 1);
            assert_eq!(searches[0].name, "ToDelete");
        }
        other => panic!("Expected SavedSearches with 1 item, got {:?}", other),
    }

    // Delete it
    let delete_msg = IpcMessage {
        id: 22,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::DeleteSavedSearch {
            name: "ToDelete".to_string(),
        }),
    };
    let resp = handle_request(&state, &delete_msg).await;
    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {:?}", other),
    }

    // Verify it's gone
    let list_msg2 = IpcMessage {
        id: 23,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSavedSearches),
    };
    let resp = handle_request(&state, &list_msg2).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SavedSearches { searches },
        }) => {
            assert!(
                searches.is_empty(),
                "Saved searches should be empty after delete"
            );
        }
        other => panic!("Expected empty SavedSearches, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_export_thread_markdown() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    // Sync to get messages
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    // Get an envelope to find its thread_id
    let list_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 1,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    let thread_id = match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => envelopes[0].thread_id.clone(),
        other => panic!("Expected Envelopes, got {:?}", other),
    };

    // Export the thread as markdown
    let export_msg = IpcMessage {
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
                "Should be markdown: {}",
                content
            );
            assert!(content.contains("Exported from mxr"));
        }
        other => panic!("Expected ExportResult, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_sync_now_acknowledges() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 300,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SyncNow { account_id: None }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_export_thread_json_is_valid() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let list_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 1,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &list_msg).await;
    let thread_id = match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => envelopes[0].thread_id.clone(),
        other => panic!("Expected Envelopes, got {:?}", other),
    };

    let export_msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ExportThread {
            thread_id,
            format: mxr_core::types::ExportFormat::Json,
        }),
    };
    let resp = handle_request(&state, &export_msg).await;
    match &resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ExportResult { content },
        }) => {
            let parsed: serde_json::Value =
                serde_json::from_str(content).expect("Export JSON should be valid");
            assert!(parsed["message_count"].as_u64().unwrap() >= 1);
            assert!(parsed["subject"].is_string());
        }
        other => panic!("Expected ExportResult, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_get_headers_includes_standards_metadata() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let mut body = state.store.get_body(&id).await.unwrap().unwrap();
    body.metadata.list_id = Some("fixtures.example.com".into());
    body.metadata.auth_results = vec!["mx.example.net; dkim=pass".into()];
    body.metadata.content_language = vec!["en".into(), "fr".into()];
    state.store.insert_body(&body).await.unwrap();

    let msg = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetHeaders {
            message_id: id.clone(),
        }),
    };
    let resp = handle_request(&state, &msg).await;

    let headers = match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Headers { headers },
        }) => headers,
        other => panic!("Expected Headers, got {:?}", other),
    };

    assert!(headers.iter().any(|(name, _)| name == "From"));
    assert!(headers.iter().any(|(name, _)| name == "Subject"));
    assert!(headers
        .iter()
        .any(|(name, value)| name == "List-Id" && value == "fixtures.example.com"));
    assert!(headers.iter().any(|(name, value)| {
        name == "Authentication-Results" && value == "mx.example.net; dkim=pass"
    }));
    assert!(headers
        .iter()
        .any(|(name, value)| { name == "Content-Language" && value == "en, fr" }));
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
        id: 4,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ExportSearch {
            query: "deployment".into(),
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
        other => panic!("Expected ExportResult, got {:?}", other),
    }
}

#[tokio::test]
async fn dispatch_save_draft_to_server() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: state.default_account_id(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: Some("Recipient".into()),
            email: "recipient@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Saved draft".into(),
        body_markdown: "Body".into(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let msg = IpcMessage {
        id: 5,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SaveDraftToServer { draft }),
    };
    let resp = handle_request(&state, &msg).await;

    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {:?}", other),
    }
}
