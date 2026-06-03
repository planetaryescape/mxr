use super::*;

#[tokio::test]
async fn snooze_on_folder_provider_reanchors_to_reconciled_message_copy() {
    let state = folder_copy_state().await;
    let original_id = sync_and_get_first_id(&state).await;

    let snooze = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Snooze {
            message_id: original_id.clone(),
            wake_at: chrono::Utc::now() + chrono::Duration::hours(4),
        }),
    };
    match handle_request(&state, &snooze).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack for Snooze, got {other:?}"),
    }

    let snoozed = state.store.list_snoozed().await.unwrap();
    assert_eq!(snoozed.len(), 1, "expected one snoozed message");
    assert_ne!(
        snoozed[0].message_id, original_id,
        "folder-backed snooze should track the reconciled message copy"
    );

    let archived = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 20, 0)
        .await
        .unwrap();
    assert_eq!(
        archived.len(),
        1,
        "expected exactly one archived copy after snooze: {archived:?}"
    );
    assert!(
        archived
            .iter()
            .all(|envelope| envelope.label_provider_ids == vec!["Archive".to_string()]),
        "expected only archived copy after snooze: {archived:?}"
    );

    let unsnooze = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsnooze {
            message_id: snoozed[0].message_id.clone(),
        }),
    };
    match handle_request(&state, &unsnooze).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack for Unsnooze, got {other:?}"),
    }

    let inbox = state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 20, 0)
        .await
        .unwrap();
    assert_eq!(
        inbox.len(),
        1,
        "expected exactly one inbox copy after unsnooze: {inbox:?}"
    );
    assert!(
        inbox
            .iter()
            .all(|envelope| envelope.label_provider_ids == vec!["INBOX".to_string()]),
        "expected only inbox copy after unsnooze: {inbox:?}"
    );
    assert!(
        state.store.list_snoozed().await.unwrap().is_empty(),
        "expected snooze row to be cleared after unsnooze"
    );
}

#[tokio::test]
async fn snooze_on_folder_provider_errors_when_reconciled_copy_is_missing() {
    let state = folder_copy_state_with_mode(FolderCopyReanchorMode::MissingAfterArchive).await;
    let original_id = sync_and_get_first_id(&state).await;

    let snooze = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Snooze {
            message_id: original_id,
            wake_at: chrono::Utc::now() + chrono::Duration::hours(4),
        }),
    };
    match handle_request(&state, &snooze).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.contains("Reconciled message not found"),
                "expected missing reanchor error, got: {message}"
            );
        }
        other => panic!("Expected Error for missing reconciled snooze copy, got {other:?}"),
    }

    assert!(
        state.store.list_snoozed().await.unwrap().is_empty(),
        "expected no snooze row after failed reanchor"
    );
    assert!(
        state
            .store
            .list_envelopes_by_account(&state.default_account_id(), 20, 0)
            .await
            .unwrap()
            .is_empty(),
        "expected provider sync to reflect the missing reconciled copy"
    );
}

#[tokio::test]
async fn dispatch_mutation_set_read() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::SetRead {
            message_ids: vec![id.clone()],
            read: true,
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);

    let get_msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
    };
    let resp = handle_request(&state, &get_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelope { envelope },
        }) => {
            assert!(
                envelope.flags.contains(mxr_core::types::MessageFlags::READ),
                "Expected READ flag to be set, got {:?}",
                envelope.flags
            );
        }
        other => panic!("Expected Envelope, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_mutation_archive() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
            message_ids: vec![id.clone()],
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);

    let events = state
        .store
        .list_events(10, None, Some("mutation"))
        .await
        .unwrap();
    let id_str = id.as_str();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].message_id.as_deref(), Some(id_str.as_str()));
    assert!(events[0].summary.contains("Archived"));
}

/// Phase 1.4 / Behaviors 1+2+3+8: archive a message, observe the new
/// `mutation_id` in the response, undo it within the window, and
/// verify the message is back under the INBOX label both locally and
/// on the (fake) provider. Proves the snapshot capture, write,
/// reverse-op dispatch, and local restoration all line up.
#[tokio::test]
async fn undo_archive_restores_inbox_label() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    // Pre-condition: the message has the INBOX label.
    let pre = state.store.get_envelope(&id).await.unwrap().unwrap();
    assert!(
        pre.label_provider_ids.iter().any(|l| l == "INBOX"),
        "fixture must start in INBOX; got {:?}",
        pre.label_provider_ids
    );

    // Archive — captures snapshot, writes undo entry, returns mutation_id.
    let archive = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
            message_ids: vec![id.clone()],
        })),
    };
    let result = assert_mutation_succeeded(handle_request(&state, &archive).await.payload);
    let mutation_id = result
        .mutation_id
        .clone()
        .expect("Archive must return a mutation_id");

    let post_archive = state.store.get_envelope(&id).await.unwrap().unwrap();
    assert!(
        !post_archive.label_provider_ids.iter().any(|l| l == "INBOX"),
        "INBOX must be removed by Archive; got {:?}",
        post_archive.label_provider_ids
    );

    // Undo — restores INBOX both locally and via the fake provider.
    let undo = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UndoMutation {
            mutation_id: mutation_id.clone(),
        }),
    };
    let resp = handle_request(&state, &undo).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("expected Ack from UndoMutation; got {other:?}"),
    }

    let restored = state.store.get_envelope(&id).await.unwrap().unwrap();
    assert!(
        restored.label_provider_ids.iter().any(|l| l == "INBOX"),
        "INBOX must be restored after Undo; got {:?}",
        restored.label_provider_ids
    );

    // The undo entry is consumed — replaying the same id is now a no-op
    // (regression test for "user mashes u and double-undoes").
    let replay = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UndoMutation { mutation_id }),
    };
    match handle_request(&state, &replay).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.to_lowercase().contains("not found"),
                "second undo must return not-found; got {message}"
            );
        }
        other => panic!("expected Error on replay; got {other:?}"),
    }
}

/// Phase 1.4 / Behavior 4: Undo for an unknown id returns Error
/// with "not found" so the TUI can render the right message instead
/// of silently succeeding or panicking.
#[tokio::test]
async fn undo_unknown_mutation_id_returns_not_found() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UndoMutation {
            mutation_id: "01HVTOTALLYBOGUSID0000000".into(),
        }),
    };
    match handle_request(&state, &msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.to_lowercase().contains("not found"),
                "expected not-found error; got {message}"
            );
        }
        other => panic!("expected Error; got {other:?}"),
    }
}

/// Phase 1.4 / Behavior 6: a bulk Archive of multiple messages
/// produces a single mutation_id and a single Undo restores all of
/// them. Catches regressions where snapshots are dropped or only the
/// first envelope is restored.
#[tokio::test]
async fn undo_bulk_archive_restores_all_messages() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    // Sync first to populate the fixture.
    let _ = sync_and_get_first_id(&state).await;

    // Pull three INBOX-tagged messages by listing envelopes.
    let list_msg = IpcMessage {
        id: 100,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 3,
            offset: 0,
        }),
    };
    let envelopes = match handle_request(&state, &list_msg).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => envelopes,
        other => panic!("expected Envelopes; got {other:?}"),
    };
    let ids: Vec<mxr_core::MessageId> = envelopes.iter().take(3).map(|e| e.id.clone()).collect();
    assert!(ids.len() >= 2, "fixture must contain >=2 messages");

    let archive = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
            message_ids: ids.clone(),
        })),
    };
    let result = assert_mutation_succeeded(handle_request(&state, &archive).await.payload);
    let mutation_id = result.mutation_id.clone().expect("mutation_id required");
    assert_eq!(result.succeeded, ids.len() as u32);

    let undo = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UndoMutation { mutation_id }),
    };
    match handle_request(&state, &undo).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("expected Ack; got {other:?}"),
    }

    // Every archived message should now have INBOX again.
    for id in &ids {
        let env = state.store.get_envelope(id).await.unwrap().unwrap();
        assert!(
            env.label_provider_ids.iter().any(|l| l == "INBOX"),
            "{id} must have INBOX restored; got {:?}",
            env.label_provider_ids
        );
    }
}

/// Phase 1.4: Star is not undoable — the response carries no
/// mutation_id so clients know not to render the undo affordance.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_mutation_job_reports_progress_and_undo_ids_for_large_batch() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let first_id = sync_and_get_first_id(&state).await;
    let account_id = state
        .store
        .get_envelope(&first_id)
        .await
        .unwrap()
        .unwrap()
        .account_id;

    let mut ids = Vec::new();
    for i in 0..405 {
        let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .provider_id(format!("large-job-{i}"))
            .subject(format!("large batch {i}"))
            .label_provider_ids(vec!["INBOX".to_string()])
            .build();
        ids.push(envelope.id.clone());
        state.store.upsert_envelope(&envelope).await.unwrap();
    }

    let start = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::StartMutationJob {
            mutation: MutationCommand::Archive {
                message_ids: ids.clone(),
            },
            client_correlation_id: None,
        }),
    };
    let job_id = match handle_request(&state, &start).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::JobStarted { job },
        }) => {
            assert_eq!(job.progress.total, 405);
            job.job_id
        }
        other => panic!("expected JobStarted; got {other:?}"),
    };

    let mut completed_job = None;
    for attempt in 0..800 {
        let inspect = IpcMessage {
            id: 2 + attempt,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::GetJob {
                job_id: job_id.clone(),
            }),
        };
        let job = match handle_request(&state, &inspect).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Job { job },
            }) => job,
            other => panic!("expected Job; got {other:?}"),
        };
        if matches!(job.status, JobStatusData::Succeeded | JobStatusData::Failed) {
            completed_job = Some(job);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    let job = completed_job.expect("job should finish within test timeout");
    assert_eq!(job.status, JobStatusData::Succeeded);
    assert_eq!(job.progress.completed, 405);
    assert_eq!(job.progress.succeeded, 405);
    assert_eq!(job.progress.skipped, 0);
    assert!(
        job.undo_ids.len() >= 2,
        "large jobs should surface all chunk undo ids; got {:?}",
        job.undo_ids
    );

    for id in ids.iter().take(5) {
        let envelope = state.store.get_envelope(id).await.unwrap().unwrap();
        assert!(
            !envelope
                .label_provider_ids
                .iter()
                .any(|label| label == "INBOX"),
            "archived job message should leave inbox"
        );
    }
}

#[tokio::test]
async fn star_mutation_omits_mutation_id() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
            message_ids: vec![id],
            starred: true,
        })),
    };
    let result = assert_mutation_succeeded(handle_request(&state, &msg).await.payload);
    assert!(
        result.mutation_id.is_none(),
        "Star must not return a mutation_id; got {:?}",
        result.mutation_id
    );
}

#[tokio::test]
async fn mutation_archives_healthy_account_when_other_account_provider_fails() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let healthy_id = sync_and_get_first_id(&state).await;
    let failing_calls = Arc::new(AtomicUsize::new(0));
    add_failing_sync_account(&state, failing_calls.clone()).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
            message_ids: vec![healthy_id],
        })),
    };
    let result = assert_mutation_succeeded(handle_request(&state, &msg).await.payload);

    assert_eq!(result.requested, 1);
    assert_eq!(result.succeeded, 1);
    assert_eq!(result.skipped, 0);
    assert_eq!(failing_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn mixed_account_mutation_returns_partial_success() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let healthy_id = sync_and_get_first_id(&state).await;
    let failing_calls = Arc::new(AtomicUsize::new(0));
    let (bad_account_id, bad_id) = add_failing_sync_account(&state, failing_calls.clone()).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
            message_ids: vec![healthy_id, bad_id],
        })),
    };
    let result = assert_mutation_succeeded(handle_request(&state, &msg).await.payload);

    assert_eq!(result.requested, 2);
    assert_eq!(result.succeeded, 1);
    assert_eq!(result.skipped, 1);
    assert_eq!(failing_calls.load(Ordering::SeqCst), 1);
    let bad_account = result
        .accounts
        .iter()
        .find(|account| account.account_id == bad_account_id)
        .expect("bad account result");
    assert_eq!(bad_account.succeeded, 0);
    assert_eq!(bad_account.skipped, 1);
    assert!(bad_account
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("keychain"));
}

#[tokio::test]
async fn dispatch_mutation_read_and_archive() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::ReadAndArchive {
            message_ids: vec![id.clone()],
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);

    let envelope = state
        .store
        .get_envelope(&id)
        .await
        .unwrap()
        .expect("message should still exist");
    assert!(envelope.flags.contains(mxr_core::types::MessageFlags::READ));

    let label_ids = state.store.get_message_label_ids(&id).await.unwrap();
    assert!(!label_ids
        .iter()
        .any(|label_id| label_id.as_str() == "INBOX"));

    let events = state
        .store
        .list_events(10, None, Some("mutation"))
        .await
        .unwrap();
    assert!(events[0].summary.contains("read and archived"));
}

#[tokio::test]
async fn dispatch_mutation_trash() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Trash {
            message_ids: vec![id],
        })),
    };
    let resp = handle_request(&state, &msg).await;
    assert_mutation_succeeded(resp.payload);
}

#[tokio::test]
async fn dispatch_prepare_reply() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let expected_subject = state
        .store
        .get_envelope(&id)
        .await
        .unwrap()
        .unwrap()
        .subject;

    // Fetch body first so it's cached
    let body_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    handle_request(&state, &body_msg).await;

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::PrepareReply {
            message_id: id,
            reply_all: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyContext { context },
        }) => {
            assert!(context.reply_to.contains('@'));
            assert_eq!(context.subject, expected_subject);
        }
        other => panic!("Expected ReplyContext, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_prepare_reply_all() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let expected_subject = state
        .store
        .get_envelope(&id)
        .await
        .unwrap()
        .unwrap()
        .subject;

    // Fetch body first
    let body_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    handle_request(&state, &body_msg).await;

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::PrepareReply {
            message_id: id,
            reply_all: true,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyContext { context },
        }) => {
            assert!(context.reply_to.contains('@'));
            assert_eq!(context.subject, expected_subject);
            // cc may or may not be empty depending on the message, but the field should exist
        }
        other => panic!("Expected ReplyContext, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_prepare_reply_renders_html_context() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: None,
            text_html: Some("<p>Hello <b>world</b></p>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        })
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::PrepareReply {
            message_id: id,
            reply_all: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyContext { context },
        }) => {
            assert!(context.thread_context.contains("Hello world"));
            assert!(!context.thread_context.contains("<p>"));
        }
        other => panic!("Expected ReplyContext, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_prepare_forward() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let expected_subject = state
        .store
        .get_envelope(&id)
        .await
        .unwrap()
        .unwrap()
        .subject;

    // Fetch body first
    let body_msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    handle_request(&state, &body_msg).await;

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::PrepareForward { message_id: id }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ForwardContext { context },
        }) => {
            assert_eq!(context.subject, expected_subject);
            assert!(
                !context.forwarded_content.is_empty(),
                "forwarded_content should be non-empty"
            );
        }
        other => panic!("Expected ForwardContext, got {other:?}"),
    }
}

#[tokio::test]
async fn modify_labels_persists_to_store_immediately() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let create = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Follow Up".into(),
            color: None,
            account_id: None,
        }),
    };
    let label = match handle_request(&state, &create).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Label { label },
        }) => label,
        other => panic!("Expected Label response, got {other:?}"),
    };

    let modify = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::ModifyLabels {
            message_ids: vec![id.clone()],
            add: vec![label.name.clone()],
            remove: vec![],
        })),
    };
    assert_mutation_succeeded(handle_request(&state, &modify).await.payload);

    let label_ids = state.store.get_message_label_ids(&id).await.unwrap();
    assert!(label_ids.iter().any(|label_id| label_id == &label.id));
}

#[tokio::test]
async fn get_thread_includes_message_label_provider_ids() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let envelope = state.store.get_envelope(&id).await.unwrap().unwrap();

    let create = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Recruiters".into(),
            color: None,
            account_id: None,
        }),
    };
    let label = match handle_request(&state, &create).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Label { label },
        }) => label,
        other => panic!("Expected Label response, got {other:?}"),
    };

    state
        .store
        .add_message_label(&id, &label.id, mxr_core::EventSource::User)
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetThread {
            thread_id: envelope.thread_id,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Thread { messages, .. },
        }) => {
            let message = messages
                .into_iter()
                .find(|message| message.id == id)
                .unwrap();
            assert!(message
                .label_provider_ids
                .iter()
                .any(|provider_id| provider_id == &label.provider_id));
        }
        other => panic!("Expected Thread response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_envelopes_includes_message_label_provider_ids() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let create = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::CreateLabel {
            name: "Recruiters".into(),
            color: None,
            account_id: None,
        }),
    };
    let label = match handle_request(&state, &create).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Label { label },
        }) => label,
        other => panic!("Expected Label response, got {other:?}"),
    };

    state
        .store
        .add_message_label(&id, &label.id, mxr_core::EventSource::User)
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 200,
            offset: 0,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        }) => {
            let envelope = envelopes
                .into_iter()
                .find(|envelope| envelope.id == id)
                .unwrap();
            assert!(envelope
                .label_provider_ids
                .iter()
                .any(|provider_id| provider_id == &label.provider_id));
        }
        other => panic!("Expected Envelopes response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_accounts_surfaces_runtime_accounts_without_config_entries() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListAccounts),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Accounts { accounts },
        }) => {
            assert_eq!(accounts.len(), 1);
            assert_eq!(accounts[0].email, "user@example.com");
            assert_eq!(accounts[0].source, AccountSourceData::Runtime);
            assert_eq!(accounts[0].editable, AccountEditModeData::RuntimeOnly);
            assert!(accounts[0].is_default);
        }
        other => panic!("Expected Accounts response, got {other:?}"),
    }
}

#[tokio::test]
async fn get_llm_status_reports_noop_provider_by_default() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetLlmStatus),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::LlmStatus { snapshot },
        }) => {
            assert!(!snapshot.enabled);
            assert_eq!(snapshot.provider, "noop");
            assert_eq!(snapshot.model, "noop");
            assert_eq!(snapshot.configured_model, "qwen2.5:3b-instruct");
            assert_eq!(snapshot.base_url, None);
            assert_eq!(snapshot.context_window, 0);
        }
        other => panic!("Expected LlmStatus response, got {other:?}"),
    }
}

#[tokio::test]
async fn config_reload_rebuilds_llm_provider_for_status() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.llm.enabled = true;
    config.llm.model = "local-test-model".to_string();
    config.llm.base_url = "http://127.0.0.1:11434/v1".to_string();
    config.llm.context_window = 4096;
    config.llm.request_timeout_secs = 30;
    state.set_config_for_test(config).await;

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetLlmStatus),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::LlmStatus { snapshot },
        }) => {
            assert!(snapshot.enabled);
            assert_eq!(snapshot.provider, "openai_compatible");
            assert_eq!(snapshot.model, "local-test-model");
            assert_eq!(snapshot.configured_model, "local-test-model");
            assert_eq!(
                snapshot.base_url.as_deref(),
                Some("http://127.0.0.1:11434/v1")
            );
            assert_eq!(snapshot.context_window, 4096);
            assert_eq!(snapshot.request_timeout_secs, 30);
        }
        other => panic!("Expected LlmStatus response, got {other:?}"),
    }
}

#[test]
fn update_llm_config_persists_and_rebuilds_provider_status() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let config_dir = temp_dir.path().join("config");
    let data_dir = temp_dir.path().join("data");
    let socket_path = temp_dir.path().join("mxr.sock");
    std::fs::create_dir_all(&config_dir).expect("config dir");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    temp_env::with_vars(
        [
            ("MXR_CONFIG_DIR", Some(config_dir)),
            ("MXR_DATA_DIR", Some(data_dir)),
            ("MXR_SOCKET_PATH", Some(socket_path)),
        ],
        || {
            runtime.block_on(async {
                mxr_config::save_config(&mxr_config::MxrConfig::default())
                    .expect("save default config");
                let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
                let msg = IpcMessage {
                    id: 1,
                    source: ::mxr_protocol::ClientKind::default(),
                    payload: IpcPayload::Request(Request::UpdateLlmConfig {
                        config: Box::new(mxr_protocol::LlmConfigData {
                            enabled: true,
                            base_url: "http://127.0.0.1:11434/v1".into(),
                            model: "local-test-model".into(),
                            api_key_env: "MXR_TEST_LLM_KEY".into(),
                            context_window: 4096,
                            request_timeout_secs: 30,
                            allow_cloud_relationship_data: true,
                            overrides: None,
                        }),
                    }),
                };

                let resp = handle_request(&state, &msg).await;
                match resp.payload {
                    IpcPayload::Response(Response::Ok {
                        data: ResponseData::LlmConfig { config },
                    }) => {
                        assert!(config.enabled);
                        assert_eq!(config.model, "local-test-model");
                        assert!(config.allow_cloud_relationship_data);
                    }
                    other => panic!("Expected LlmConfig response, got {other:?}"),
                }

                let saved = mxr_config::load_config().expect("load saved config");
                assert!(saved.llm.enabled);
                assert_eq!(saved.llm.model, "local-test-model");
                assert_eq!(saved.llm.api_key_env, "MXR_TEST_LLM_KEY");
                assert!(saved.llm.allow_cloud_relationship_data);

                let status_msg = IpcMessage {
                    id: 2,
                    source: ::mxr_protocol::ClientKind::default(),
                    payload: IpcPayload::Request(Request::GetLlmStatus),
                };
                let status_resp = handle_request(&state, &status_msg).await;
                match status_resp.payload {
                    IpcPayload::Response(Response::Ok {
                        data: ResponseData::LlmStatus { snapshot },
                    }) => {
                        assert!(snapshot.enabled);
                        assert_eq!(snapshot.provider, "openai_compatible");
                        assert_eq!(snapshot.model, "local-test-model");
                        assert_eq!(snapshot.context_window, 4096);
                        assert_eq!(snapshot.request_timeout_secs, 30);
                    }
                    other => panic!("Expected LlmStatus response, got {other:?}"),
                }
            });
        },
    );
}

#[test]
fn update_notification_chimes_persists_and_updates_daemon_snapshot() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let config_dir = temp_dir.path().join("config");
    let data_dir = temp_dir.path().join("data");
    let socket_path = temp_dir.path().join("mxr.sock");
    std::fs::create_dir_all(&config_dir).expect("config dir");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    temp_env::with_vars(
        [
            ("MXR_CONFIG_DIR", Some(config_dir)),
            ("MXR_DATA_DIR", Some(data_dir)),
            ("MXR_SOCKET_PATH", Some(socket_path)),
        ],
        || {
            runtime.block_on(async {
                mxr_config::save_config(&mxr_config::MxrConfig::default())
                    .expect("save default config");
                let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
                let desired = mxr_protocol::NotificationChimesData {
                    enabled: true,
                    volume: 0.5,
                    new_mail: mxr_protocol::NotificationChimeSoundData::Glass,
                    sent: mxr_protocol::NotificationChimeSoundData::Sent,
                    archived: mxr_protocol::NotificationChimeSoundData::Archive,
                    trashed: mxr_protocol::NotificationChimeSoundData::Thud,
                    spam: mxr_protocol::NotificationChimeSoundData::Alert,
                    snoozed: mxr_protocol::NotificationChimeSoundData::Pop,
                    unsnoozed: mxr_protocol::NotificationChimeSoundData::Glass,
                    reminder: mxr_protocol::NotificationChimeSoundData::Bell,
                    error: mxr_protocol::NotificationChimeSoundData::Alert,
                };
                let msg = IpcMessage {
                    id: 1,
                    source: ::mxr_protocol::ClientKind::default(),
                    payload: IpcPayload::Request(Request::UpdateNotificationChimes {
                        config: Box::new(desired),
                    }),
                };

                let resp = handle_request(&state, &msg).await;
                match resp.payload {
                    IpcPayload::Response(Response::Ok {
                        data: ResponseData::NotificationChimes { config },
                    }) => {
                        assert!(config.enabled);
                        assert_eq!(config.volume, 0.5);
                        assert_eq!(
                            config.new_mail,
                            mxr_protocol::NotificationChimeSoundData::Glass
                        );
                    }
                    other => panic!("Expected NotificationChimes response, got {other:?}"),
                }

                let saved = mxr_config::load_config().expect("load saved config");
                assert!(saved.notifications.chimes.enabled);
                assert_eq!(saved.notifications.chimes.volume, 0.5);
                assert_eq!(
                    saved.notifications.chimes.new_mail,
                    mxr_config::ChimeSound::Glass
                );
                assert!(state.config_snapshot().notifications.chimes.enabled);
            });
        },
    );
}

#[tokio::test]
async fn update_llm_config_rejects_blank_model() {
    let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UpdateLlmConfig {
            config: Box::new(mxr_protocol::LlmConfigData {
                enabled: true,
                base_url: "http://127.0.0.1:11434/v1".into(),
                model: "  ".into(),
                api_key_env: String::new(),
                context_window: 4096,
                request_timeout_secs: 30,
                allow_cloud_relationship_data: false,
                overrides: None,
            }),
        }),
    };

    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("llm.model must not be empty"));
        }
        other => panic!("Expected error response, got {other:?}"),
    }
    assert_eq!(
        state.config_snapshot().llm.model,
        mxr_config::LlmConfig::default().model
    );
}

#[tokio::test]
async fn dispatch_send_draft() {
    let state = Arc::new(AppState::in_memory().await.unwrap());

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: state.default_account_id(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Test subject".to_string(),
        body_markdown: "Test body".to_string(),
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
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SendReceipt { .. },
        }) => {}
        other => panic!("Expected SendReceipt, got {other:?}"),
    }
}

#[tokio::test]
async fn draft_only_safety_policy_blocks_send_but_allows_local_draft() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.general.safety_policy = mxr_config::SafetyPolicy::DraftOnly;
    state.set_config_for_test(config).await;

    let draft = mxr_core::types::Draft {
        id: mxr_core::DraftId::new(),
        account_id: state.default_account_id(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Draft-only policy".to_string(),
        body_markdown: "Test body".to_string(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let send = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendDraft {
            draft: draft.clone(),
            override_safety_token: None,
        }),
    };
    match handle_request(&state, &send).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("draft-only safety policy"));
        }
        other => panic!("Expected safety policy error, got {other:?}"),
    }

    let save = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SaveDraft { draft }),
    };
    match handle_request(&state, &save).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected SaveDraft Ack, got {other:?}"),
    }
}

#[tokio::test]
async fn read_only_safety_policy_blocks_mutations_but_allows_search() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.general.safety_policy = mxr_config::SafetyPolicy::ReadOnly;
    state.set_config_for_test(config).await;

    let mutation = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
            message_ids: vec![mxr_core::MessageId::new()],
            starred: true,
        })),
    };
    match handle_request(&state, &mutation).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("read-only safety policy"));
        }
        other => panic!("Expected safety policy error, got {other:?}"),
    }

    let search = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            query: "hello".into(),
            limit: 10,
            offset: 0,
            account_id: None,
            mode: None,
            sort: None,
            explain: false,
        }),
    };
    match handle_request(&state, &search).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SearchResults { .. },
        }) => {}
        other => panic!("Expected SearchResults, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_send_draft_preserves_keychain_repair_error() {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let sync_provider = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let send_provider: Arc<dyn mxr_core::MailSendProvider> = Arc::new(FailingSendProvider {
            message: "Keyring error: Password for mxr/consulting-smtp/hello@bhekani.com requires interactive macOS keychain approval. Re-save that account password once with `mxr accounts repair`.",
        });
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
        subject: "Test subject".to_string(),
        body_markdown: "Test body".to_string(),
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
    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("consulting-smtp"));
            assert!(message.contains("mxr accounts repair"));
        }
        other => panic!("Expected send error, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_snooze_and_list() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    // Snooze
    let wake_at = chrono::Utc::now() + chrono::Duration::hours(24);
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Snooze {
            message_id: id.clone(),
            wake_at,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack for Snooze, got {other:?}"),
    }

    // List snoozed - should have 1
    let msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSnoozed),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SnoozedMessages { snoozed },
        }) => {
            assert_eq!(snoozed.len(), 1, "Expected 1 snoozed message");
        }
        other => panic!("Expected SnoozedMessages, got {other:?}"),
    }

    // Unsnooze
    let msg = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsnooze { message_id: id }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack for Unsnooze, got {other:?}"),
    }

    // List snoozed - should have 0
    let msg = IpcMessage {
        id: 4,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::ListSnoozed),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SnoozedMessages { snoozed },
        }) => {
            assert_eq!(
                snoozed.len(),
                0,
                "Expected 0 snoozed messages after unsnooze"
            );
        }
        other => panic!("Expected SnoozedMessages, got {other:?}"),
    }
}

#[tokio::test]
async fn snooze_removes_inbox_and_unsnooze_restores_it() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    let envelope = state.store.get_envelope(&id).await.unwrap().unwrap();
    let inbox = state
        .store
        .list_labels_by_account(&envelope.account_id)
        .await
        .unwrap()
        .into_iter()
        .find(|label| label.provider_id == "INBOX")
        .unwrap();

    let before = state.store.get_message_label_ids(&id).await.unwrap();
    assert!(before.iter().any(|label_id| label_id == &inbox.id));

    let snooze = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Snooze {
            message_id: id.clone(),
            wake_at: chrono::Utc::now() + chrono::Duration::hours(4),
        }),
    };
    match handle_request(&state, &snooze).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {other:?}"),
    }

    let snoozed_labels = state.store.get_message_label_ids(&id).await.unwrap();
    assert!(!snoozed_labels.iter().any(|label_id| label_id == &inbox.id));

    let unsnooze = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsnooze {
            message_id: id.clone(),
        }),
    };
    match handle_request(&state, &unsnooze).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {other:?}"),
    }

    let restored_labels = state.store.get_message_label_ids(&id).await.unwrap();
    assert!(restored_labels.iter().any(|label_id| label_id == &inbox.id));
}

#[tokio::test]
async fn dispatch_set_flags() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    use mxr_core::types::MessageFlags;
    let flags = MessageFlags::READ | MessageFlags::STARRED;
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SetFlags {
            message_id: id.clone(),
            flags,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack, got {other:?}"),
    }

    // Verify flags
    let get_msg = IpcMessage {
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
    };
    let resp = handle_request(&state, &get_msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Envelope { envelope },
        }) => {
            assert_eq!(
                envelope.flags, flags,
                "Expected flags {:?}, got {:?}",
                flags, envelope.flags
            );
        }
        other => panic!("Expected Envelope, got {other:?}"),
    }
}

#[tokio::test]
async fn unsubscribe_purge_dry_run_reports_method_and_sender_count() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UnsubscribePurge {
            address: "noreply@rust-lang.org".into(),
            account_id: None,
            dry_run: true,
            archive_on_no_method: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::UnsubscribePurgeResult { result },
        }) => {
            assert!(result.dry_run);
            assert_eq!(result.message_count, 1);
            assert!(matches!(
                result.method,
                mxr_core::types::UnsubscribeMethod::OneClick { .. }
            ));
            assert_eq!(result.archived_count, 0);
            assert!(result.mutation_id.is_none());
        }
        other => panic!("Expected unsubscribe purge preview, got {other:?}"),
    }
}

#[tokio::test]
async fn unsubscribe_purge_without_method_does_not_archive_unless_allowed() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UnsubscribePurge {
            address: "alice@work.com".into(),
            account_id: None,
            dry_run: false,
            archive_on_no_method: false,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::UnsubscribePurgeResult { result },
        }) => {
            assert_eq!(
                result.status,
                mxr_protocol::UnsubscribePurgeStatusData::NoMethod
            );
            assert_eq!(result.archived_count, 0);
            assert!(result.mutation_id.is_none());
            assert!(result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("No unsubscribe"));
        }
        other => panic!("Expected unsubscribe purge no-method result, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_unsubscribe_no_method() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    // The first envelope from FakeProvider fixtures uses UnsubscribeMethod::None
    let msg = IpcMessage {
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Unsubscribe { message_id: id }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.contains("unsubscribe"),
                "Expected error about unsubscribe, got: {message}"
            );
        }
        other => panic!("Expected Error for no unsubscribe method, got {other:?}"),
    }
}

/// Regression: phishing that spoofs carriers (fake "DHL"/tracking) lands in
/// Spam, and the delivery detector used to classify it as a real shipment.
/// Spam/trash mail must be skipped — even when its content would otherwise
/// create a delivery.
#[tokio::test]
async fn spam_or_trash_mail_is_never_detected_as_a_delivery() {
    let state = folder_copy_state().await;
    let account_id = state.default_account_id_opt().unwrap();

    // Carrier sender + checksum-valid UPS tracking number: exactly the shape
    // that creates a delivery. Flagged SPAM, it must be skipped.
    let spam_id = seed_carrier_shipment(
        &state,
        &account_id,
        "spam-ups",
        mxr_core::MessageFlags::SPAM,
    )
    .await;
    let summary =
        crate::handler::deliveries::scan_messages(&state, std::slice::from_ref(&spam_id)).await;
    assert_eq!(summary.scanned, 1);
    assert_eq!(summary.created, 0, "spam mail must not create a delivery");

    // Same in TRASH — also skipped.
    let trash_id = seed_carrier_shipment(
        &state,
        &account_id,
        "trash-ups",
        mxr_core::MessageFlags::TRASH,
    )
    .await;
    let summary =
        crate::handler::deliveries::scan_messages(&state, std::slice::from_ref(&trash_id)).await;
    assert_eq!(
        summary.created, 0,
        "trashed mail must not create a delivery"
    );

    // Identical content without the flag DOES create — proves the guard, not
    // the content, is what suppresses the spam/trash cases.
    let clean_id = seed_carrier_shipment(
        &state,
        &account_id,
        "clean-ups",
        mxr_core::MessageFlags::empty(),
    )
    .await;
    let summary =
        crate::handler::deliveries::scan_messages(&state, std::slice::from_ref(&clean_id)).await;
    assert_eq!(
        summary.created, 1,
        "a genuine carrier shipment should create a delivery"
    );
}

async fn seed_carrier_shipment(
    state: &AppState,
    account_id: &mxr_core::AccountId,
    provider_id: &str,
    flags: mxr_core::MessageFlags,
) -> mxr_core::MessageId {
    let mut envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .provider_id(provider_id)
        .subject("Your package has shipped")
        .sender_address("UPS", "ship@ups.com")
        .recipient_address(Some("User"), "user@example.com")
        .flags(flags)
        .build();
    envelope.body_word_count = 80;
    envelope.link_count = 1;
    let id = envelope.id.clone();
    state.store.upsert_envelope(&envelope).await.unwrap();
    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: Some("Your package has shipped. Tracking: 1Z5R89390357567127".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata::default(),
        })
        .await
        .unwrap();
    id
}
