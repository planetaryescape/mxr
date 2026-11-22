use super::*;

async fn assert_request_waits_for_provider_lock(
    state: Arc<AppState>,
    request: Request,
    action: &str,
) -> IpcMessage {
    let provider_guard = state
        .acquire_provider_operation(&state.default_account_id())
        .await;
    let task_state = state.clone();
    let mut task = tokio::spawn(async move {
        let msg = IpcMessage {
            expected_workspace: None,
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(request),
        };
        handle_request(&task_state, &msg).await
    });

    tokio::select! {
        result = &mut task => {
            panic!("{action} completed before provider lock was released: {result:?}");
        }
        _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
    }

    drop(provider_guard);
    task.await.expect("request task should complete")
}

#[tokio::test]
async fn snooze_on_folder_provider_reanchors_to_reconciled_message_copy() {
    let state = folder_copy_state().await;
    let original_id = sync_and_get_first_id(&state).await;

    let snooze = IpcMessage {
        expected_workspace: None,
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
        expected_workspace: None,
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
async fn snooze_waits_for_account_provider_lock() {
    let state = folder_copy_state().await;
    let original_id = sync_and_get_first_id(&state).await;

    let response = assert_request_waits_for_provider_lock(
        state.clone(),
        Request::Snooze {
            message_id: original_id,
            wake_at: chrono::Utc::now() + chrono::Duration::hours(4),
        },
        "snooze",
    )
    .await;

    match response.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("Expected Ack for Snooze, got {other:?}"),
    }
    assert_eq!(state.store.list_snoozed().await.unwrap().len(), 1);
}

#[tokio::test]
async fn dispatch_mutation_set_read() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let msg = IpcMessage {
        expected_workspace: None,
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
        expected_workspace: None,
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

async fn add_test_label_to_message(state: &AppState, id: &mxr_core::MessageId, label: &str) {
    let account_id = state.default_account_id();
    let label_record = upsert_test_label(state, label).await;
    let mut label_ids = state
        .store
        .find_labels_by_provider_ids(&account_id, &["INBOX".to_string()])
        .await
        .unwrap();
    label_ids.push(label_record.id);
    state
        .store
        .set_message_labels(id, &label_ids, mxr_core::EventSource::User)
        .await
        .unwrap();
}

async fn upsert_test_label(state: &AppState, label: &str) -> mxr_core::Label {
    let account_id = state.default_account_id();
    let label_record = mxr_core::Label {
        id: mxr_core::LabelId::from_provider_id("test", label),
        account_id: account_id.clone(),
        name: label.to_string(),
        kind: mxr_core::LabelKind::User,
        color: None,
        provider_id: label.to_string(),
        unread_count: 0,
        total_count: 0,
        role: None,
    };
    state.store.upsert_label(&label_record).await.unwrap();
    label_record
}

#[tokio::test]
async fn route_adds_target_removes_queue_archives_read_and_uses_single_undo_id() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    add_test_label_to_message(&state, &id, "Notto").await;
    upsert_test_label(&state, "Follow Up").await;

    let route = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Route {
            message_ids: vec![id.clone()],
            to_label: "Follow Up".to_string(),
            from_queue_label: "Notto".to_string(),
            archive: true,
            dry_run: false,
        })),
    };
    let result = assert_mutation_succeeded(handle_request(&state, &route).await.payload);
    let mutation_id = result
        .mutation_id
        .clone()
        .expect("route with archive must return one undo id");

    let routed = state.store.get_envelope(&id).await.unwrap().unwrap();
    assert!(
        routed
            .label_provider_ids
            .iter()
            .any(|label| label == "Follow Up"),
        "target label must be applied: {:?}",
        routed.label_provider_ids
    );
    assert!(
        !routed
            .label_provider_ids
            .iter()
            .any(|label| label == "Notto"),
        "queue label must be removed: {:?}",
        routed.label_provider_ids
    );
    assert!(
        !routed
            .label_provider_ids
            .iter()
            .any(|label| label == "INBOX"),
        "archive must remove INBOX: {:?}",
        routed.label_provider_ids
    );
    assert!(routed.flags.contains(mxr_core::MessageFlags::READ));

    let undo = IpcMessage {
        expected_workspace: None,
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UndoMutation { mutation_id }),
    };
    match handle_request(&state, &undo).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Ack,
        }) => {}
        other => panic!("expected route undo Ack; got {other:?}"),
    }

    let restored = state.store.get_envelope(&id).await.unwrap().unwrap();
    assert!(restored
        .label_provider_ids
        .iter()
        .any(|label| label == "Notto"));
    assert!(restored
        .label_provider_ids
        .iter()
        .any(|label| label == "INBOX"));
    assert!(!restored
        .label_provider_ids
        .iter()
        .any(|label| label == "Follow Up"));
}

#[tokio::test]
async fn route_dry_run_uses_same_selection_but_does_not_mutate() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;
    add_test_label_to_message(&state, &id, "Notto").await;
    upsert_test_label(&state, "Follow Up").await;
    let before = state.store.get_envelope(&id).await.unwrap().unwrap();

    let route = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Route {
            message_ids: vec![id.clone()],
            to_label: "Follow Up".to_string(),
            from_queue_label: "Notto".to_string(),
            archive: true,
            dry_run: true,
        })),
    };
    let result = match handle_request(&state, &route).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::MutationResult { result },
        }) => result,
        other => panic!("expected dry-run MutationResult; got {other:?}"),
    };
    assert_eq!(result.requested, 1);
    assert_eq!(result.succeeded, 0);
    assert_eq!(result.skipped, 1);
    assert!(result.mutation_id.is_none());

    let after = state.store.get_envelope(&id).await.unwrap().unwrap();
    assert_eq!(after.label_provider_ids, before.label_provider_ids);
    assert_eq!(after.flags, before.flags);
}

#[tokio::test]
async fn undo_archive_restores_inbox_label() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let pre = state.store.get_envelope(&id).await.unwrap().unwrap();
    assert!(
        pre.label_provider_ids.iter().any(|l| l == "INBOX"),
        "fixture must start in INBOX; got {:?}",
        pre.label_provider_ids
    );

    let archive = IpcMessage {
        expected_workspace: None,
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

    let undo = IpcMessage {
        expected_workspace: None,
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

    let replay = IpcMessage {
        expected_workspace: None,
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
        expected_workspace: None,
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
            expected_workspace: None,
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

async fn indexed_inbox_messages(
    state: &Arc<AppState>,
    term: &str,
    count: usize,
) -> Vec<mxr_core::MessageId> {
    let first_id = sync_and_get_first_id(state).await;
    let account_id = state
        .store
        .get_envelope(&first_id)
        .await
        .unwrap()
        .unwrap()
        .account_id;
    let inbox = state
        .store
        .list_labels_by_account(&account_id)
        .await
        .unwrap()
        .into_iter()
        .find(|label| label.provider_id == "INBOX")
        .expect("fake account has an INBOX label");

    let mut ids = Vec::new();
    let mut entries = Vec::new();
    for i in 0..count {
        let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .provider_id(format!("{term}-{i}"))
            .subject(format!("{term} {i}"))
            .label_provider_ids(vec!["INBOX".to_string()])
            .build();
        state.store.upsert_envelope(&envelope).await.unwrap();
        state
            .store
            .set_message_labels(
                &envelope.id,
                std::slice::from_ref(&inbox.id),
                mxr_core::EventSource::Sync,
            )
            .await
            .unwrap();
        entries.push(mxr_search::SearchIndexEntry {
            envelope: state
                .store
                .get_envelope(&envelope.id)
                .await
                .unwrap()
                .unwrap(),
            body: None,
            reply_later: false,
        });
        ids.push(envelope.id);
    }
    state
        .search
        .apply_batch(mxr_search::SearchUpdateBatch {
            entries,
            removed_message_ids: Vec::new(),
        })
        .await
        .unwrap();
    ids
}

async fn lexical_hits(state: &Arc<AppState>, query: &str) -> usize {
    let search = IpcMessage {
        expected_workspace: None,
        id: 9,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            limit_from_config: false,
            query: query.to_string(),
            limit: 10,
            offset: 0,
            account_id: None,
            mode: None,
            sort: None,
            explain: false,
        }),
    };
    match handle_request(state, &search).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SearchResults { results, .. },
        }) => results.len(),
        other => panic!("expected SearchResults; got {other:?}"),
    }
}

fn archive_request(message_ids: Vec<mxr_core::MessageId>) -> IpcMessage {
    IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive { message_ids })),
    }
}

#[tokio::test]
async fn undo_reindexes_every_restored_message_in_search() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let ids = indexed_inbox_messages(&state, "undoreindex", 3).await;
    let result =
        assert_mutation_succeeded(handle_request(&state, &archive_request(ids)).await.payload);
    let mutation_id = result.mutation_id.expect("archive is undoable");
    assert_eq!(lexical_hits(&state, "undoreindex in:inbox").await, 0);

    let undo = IpcMessage {
        expected_workspace: None,
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

    assert_eq!(lexical_hits(&state, "undoreindex in:inbox").await, 3);
}

#[tokio::test]
async fn a_failed_search_read_skips_only_that_message_and_marks_it_for_repair() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let ids = indexed_inbox_messages(&state, "unreadablereindex", 3).await;
    let unreadable = ids[1].clone();
    sqlx::query(
        "INSERT INTO bodies (message_id, text_plain, text_html, fetched_at, metadata_json)
         VALUES (?, 'body', NULL, 0, '{}')",
    )
    .bind(unreadable.as_str())
    .execute(state.store.writer())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO attachments (id, message_id, filename, mime_type, size_bytes, provider_id)
         VALUES ('not-an-id', ?, 'a.txt', 'text/plain', 1, 'p')",
    )
    .bind(unreadable.as_str())
    .execute(state.store.writer())
    .await
    .unwrap();

    let result = assert_mutation_succeeded(
        handle_request(&state, &archive_request(ids.clone()))
            .await
            .payload,
    );
    assert_eq!(result.succeeded, 3);

    assert_eq!(lexical_hits(&state, "unreadablereindex in:inbox").await, 1);
    assert_eq!(
        state.store.list_search_reindex_pending(10).await.unwrap(),
        vec![unreadable.clone()]
    );

    sqlx::query("DELETE FROM attachments WHERE id = 'not-an-id'")
        .execute(state.store.writer())
        .await
        .unwrap();
    let other = indexed_inbox_messages(&state, "laterreindex", 1).await;
    assert_mutation_succeeded(
        handle_request(&state, &archive_request(other))
            .await
            .payload,
    );

    assert_eq!(lexical_hits(&state, "unreadablereindex in:inbox").await, 0);
    assert!(state
        .store
        .list_search_reindex_pending(10)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mutation_reindex_commits_the_store_state_at_commit_time() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let ids = indexed_inbox_messages(&state, "racereindex", 1).await;
    let id = ids[0].clone();

    let held = state.search_reindex.lock().await;
    let archive = {
        let state = state.clone();
        let request = archive_request(ids);
        tokio::spawn(async move { handle_request(&state, &request).await.payload })
    };
    while !state
        .store
        .get_message_label_ids(&id)
        .await
        .unwrap()
        .is_empty()
    {
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    state
        .store
        .set_starred(&id, true, mxr_core::EventSource::User)
        .await
        .unwrap();
    drop(held);
    assert_mutation_succeeded(archive.await.unwrap());

    assert_eq!(lexical_hits(&state, "racereindex is:starred").await, 1);
    assert_eq!(lexical_hits(&state, "racereindex in:inbox").await, 0);
}

#[tokio::test]
async fn mixed_account_mutation_returns_partial_success() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let healthy_id = sync_and_get_first_id(&state).await;
    let failing_calls = Arc::new(AtomicUsize::new(0));
    let (bad_account_id, bad_id) = add_failing_sync_account(&state, failing_calls.clone()).await;

    let msg = IpcMessage {
        expected_workspace: None,
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

    let body_msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    handle_request(&state, &body_msg).await;

    let msg = IpcMessage {
        expected_workspace: None,
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

    let body_msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::GetBody {
            message_id: id.clone(),
        }),
    };
    handle_request(&state, &body_msg).await;

    let msg = IpcMessage {
        expected_workspace: None,
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
            assert!(
                context.forwarded_content.contains("From:")
                    && context.forwarded_content.contains("Date:")
                    && context.forwarded_content.contains("Subject:")
                    && context.forwarded_content.contains("To:"),
                "forward must include original header context, got {:?}",
                context.forwarded_content
            );
        }
        other => panic!("Expected ForwardContext, got {other:?}"),
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
                    expected_workspace: None,
                    id: 1,
                    source: ::mxr_protocol::ClientKind::default(),
                    payload: IpcPayload::Request(Request::UpdateLlmConfig {
                        config: Box::new(mxr_protocol::LlmConfigData {
                            enabled: true,
                            base_url: "http://127.0.0.1:11434/v1".into(),
                            model: "local-test-model".into(),
                            compose_model: None,
                            models: Some(vec![]),
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
                    expected_workspace: None,
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
                    expected_workspace: None,
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
async fn draft_only_safety_policy_blocks_send_but_allows_local_draft() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.general.safety_policy = mxr_config::SafetyPolicy::DraftOnly;
    state.set_config_for_test(config).await;

    let draft = mxr_core::types::Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id: state.default_account_id(),
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "test@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Draft-only policy".to_string(),
        content: mxr_core::types::DraftContent::markdown("Test body".to_string()),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let send = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendDraft {
            send_identity: Some(mxr_core::SendIdentity::for_draft(&draft)),
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
        expected_workspace: None,
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SaveDraft { draft }),
    };
    match handle_request(&state, &save).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { .. },
        }) => {}
        other => panic!("Expected confirmed SaveDraft response, got {other:?}"),
    }
}

#[tokio::test]
async fn read_only_safety_policy_blocks_mutations_but_allows_search() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.general.safety_policy = mxr_config::SafetyPolicy::ReadOnly;
    state.set_config_for_test(config).await;

    let mutation = IpcMessage {
        expected_workspace: None,
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
        expected_workspace: None,
        id: 2,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::Search {
            limit_from_config: false,
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
async fn agent_profile_enforces_account_allowlist_in_dispatch() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();
    let _ = sync_and_get_first_id(&state).await;

    let mut config = state.config_snapshot();
    config.agent_surfaces.profiles.insert(
        "agent".into(),
        mxr_config::AgentProfileConfig {
            safety_policy: mxr_config::SafetyPolicy::ReadOnly,
            allowed_accounts: vec![account_id.as_str()],
            allow_send: false,
            allow_destructive: false,
            allowed_destructive_actions: vec![],
        },
    );
    state.set_config_for_test(config).await;

    let scoped_search = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::Agent,
        payload: IpcPayload::Request(Request::Search {
            limit_from_config: false,
            query: "hello".into(),
            limit: 10,
            offset: 0,
            account_id: Some(account_id),
            mode: None,
            sort: None,
            explain: false,
        }),
    };
    match handle_request(&state, &scoped_search).await.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::SearchResults { .. },
        }) => {}
        other => panic!("Expected scoped agent search to succeed, got {other:?}"),
    }

    let unscoped_search = IpcMessage {
        expected_workspace: None,
        id: 2,
        source: ::mxr_protocol::ClientKind::Agent,
        payload: IpcPayload::Request(Request::Search {
            limit_from_config: false,
            query: "hello".into(),
            limit: 10,
            offset: 0,
            account_id: None,
            mode: None,
            sort: None,
            explain: false,
        }),
    };
    match handle_request(&state, &unscoped_search).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("account allowlist"));
        }
        other => panic!("Expected account allowlist error, got {other:?}"),
    }
}

#[tokio::test]
async fn agent_profile_destructive_action_allowlist_scopes_per_action() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let message_id = sync_and_get_first_id(&state).await;
    let account_id = state.default_account_id();

    let mut config = state.config_snapshot();
    config.agent_surfaces.profiles.insert(
        "agent".into(),
        mxr_config::AgentProfileConfig {
            safety_policy: mxr_config::SafetyPolicy::Full,
            allowed_accounts: vec![account_id.as_str()],
            allow_send: false,
            allow_destructive: true,
            allowed_destructive_actions: vec![mxr_config::DestructiveAction::Archive],
        },
    );
    state.set_config_for_test(config).await;

    let archive = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::Agent,
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
            message_ids: vec![message_id.clone()],
        })),
    };
    match handle_request(&state, &archive).await.payload {
        IpcPayload::Response(Response::Ok { .. }) => {}
        other => panic!("expected archive to be allowed, got {other:?}"),
    }

    let trash = IpcMessage {
        expected_workspace: None,
        id: 2,
        source: ::mxr_protocol::ClientKind::Agent,
        payload: IpcPayload::Request(Request::mutation(MutationCommand::Trash {
            message_ids: vec![message_id],
        })),
    };
    match handle_request(&state, &trash).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.contains("destructive-action allowlist"),
                "expected per-action rejection, got {message}"
            );
        }
        other => panic!("expected trash to be rejected, got {other:?}"),
    }
}

#[tokio::test]
async fn mcp_profile_send_gate_blocks_provider_send() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let account_id = state.default_account_id();

    let mut config = state.config_snapshot();
    config.agent_surfaces.profiles.insert(
        "mcp".into(),
        mxr_config::AgentProfileConfig {
            safety_policy: mxr_config::SafetyPolicy::Full,
            allowed_accounts: vec![account_id.as_str()],
            allow_send: false,
            allow_destructive: true,
            allowed_destructive_actions: vec![],
        },
    );
    state.set_config_for_test(config).await;

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
        subject: "MCP send gate".to_string(),
        content: mxr_core::types::DraftContent::markdown("Test body".to_string()),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::Mcp,
        payload: IpcPayload::Request(Request::SendDraft {
            send_identity: Some(mxr_core::SendIdentity::for_draft(&draft)),
            draft,
            override_safety_token: None,
        }),
    };
    match handle_request(&state, &msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(message.contains("send gate"));
        }
        other => panic!("Expected send gate error, got {other:?}"),
    }
    assert_eq!(fake.sent_drafts().len(), 0);
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
        subject: "Test subject".to_string(),
        content: mxr_core::types::DraftContent::markdown("Test body".to_string()),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::SendDraft {
            send_identity: Some(mxr_core::SendIdentity::for_draft(&draft)),
            draft,
            override_safety_token: None,
        }),
    };
    let resp = handle_request(&state, &msg).await;
    match resp.payload {
        IpcPayload::Response(Response::Error {
            message,
            code,
            retryable,
            details,
            ..
        }) => {
            assert!(message.contains("consulting-smtp"), "{message}");
            assert!(message.contains("mxr accounts repair"), "{message}");
            assert!(message.contains("do not resend"), "{message}");
            assert_eq!(code, "send_outcome_unknown");
            assert!(!retryable);
            assert_eq!(details.unwrap()["outcome"]["state"], "unknown");
        }
        other => panic!("Expected send error, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_snooze_and_list() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let id = sync_and_get_first_id(&state).await;

    let wake_at = chrono::Utc::now() + chrono::Duration::hours(24);
    let msg = IpcMessage {
        expected_workspace: None,
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

    let msg = IpcMessage {
        expected_workspace: None,
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

    let msg = IpcMessage {
        expected_workspace: None,
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

    let msg = IpcMessage {
        expected_workspace: None,
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
        expected_workspace: None,
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
        expected_workspace: None,
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

fn from_request(request: Request) -> IpcMessage {
    IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(request),
    }
}

fn draft_from(account_id: mxr_core::AccountId, from: Option<&str>) -> mxr_core::types::Draft {
    mxr_core::types::Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id,
        from: from.map(|email| mxr_core::types::Address {
            name: None,
            email: email.to_string(),
        }),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "client@example.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "From selection".to_string(),
        content: mxr_core::types::DraftContent::markdown("Body".to_string()),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn send_uses_registered_alias_from_override() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    state
        .store
        .add_account_address(&account_id, "hello@example.com", false)
        .await
        .unwrap();
    let state = Arc::new(state);

    let draft = draft_from(account_id, Some("hello@example.com"));
    let resp = handle_request(
        &state,
        &from_request(Request::SendDraft {
            send_identity: Some(mxr_core::SendIdentity::for_draft(&draft)),
            draft,
            override_safety_token: None,
        }),
    )
    .await;
    assert!(
        matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SendReceipt { .. }
            })
        ),
        "expected SendReceipt, got {:?}",
        resp.payload
    );

    let froms = fake.sent_from_addresses();
    assert_eq!(froms.len(), 1);
    assert_eq!(froms[0].email, "hello@example.com");
    assert_eq!(froms[0].name.as_deref(), Some("Fake Account"));
    assert_eq!(
        fake.sent_drafts()[0].from.as_ref().unwrap().email,
        "hello@example.com"
    );
}

#[tokio::test]
async fn send_rejects_unowned_from_override() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    let state = Arc::new(state);

    let draft = draft_from(account_id, Some("stranger@evil.com"));
    let draft_id = draft.id.clone();
    let resp = handle_request(
        &state,
        &from_request(Request::SendDraft {
            send_identity: Some(mxr_core::SendIdentity::for_draft(&draft)),
            draft,
            override_safety_token: None,
        }),
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.to_lowercase().contains("invalid"),
                "error should be an InvalidRequest: {message}"
            );
            assert!(
                message.contains("user@example.com"),
                "error should list the registered addresses: {message}"
            );
        }
        other => panic!("expected an error, got {other:?}"),
    }
    assert!(fake.sent_drafts().is_empty());
    assert!(matches!(
        state.store.get_draft_status(&draft_id).await.unwrap(),
        None | Some(mxr_core::types::DraftStatus::Draft)
    ));
}

#[tokio::test]
async fn save_draft_to_server_caches_the_provider_thread_id_on_reply_drafts() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    let state = Arc::new(state);

    let mut draft = draft_from(account_id, None);
    draft.intent = mxr_core::DraftIntent::Reply;
    draft.reply_headers = Some(mxr_core::ReplyHeaders {
        in_reply_to: "<parent@example.com>".to_string(),
        references: vec!["<parent@example.com>".to_string()],
        thread_id: None,
    });

    let resp = handle_request(
        &state,
        &from_request(Request::SaveDraftToServer {
            draft: draft.clone(),
        }),
    )
    .await;
    assert!(
        matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Draft { .. }
            })
        ),
        "expected Ack, got {:?}",
        resp.payload
    );

    let stored = state.store.get_draft(&draft.id).await.unwrap().unwrap();
    let headers = stored
        .reply_headers
        .expect("reply headers survive the push");
    assert_eq!(headers.in_reply_to, "<parent@example.com>");
    assert_eq!(
        headers.thread_id.as_deref(),
        Some("fake-thread-<parent@example.com>"),
        "the provider thread id is cached onto the local draft"
    );

    let mut known = draft_from(stored.account_id.clone(), None);
    known.intent = mxr_core::DraftIntent::Reply;
    known.reply_headers = Some(mxr_core::ReplyHeaders {
        in_reply_to: "<parent@example.com>".to_string(),
        references: vec![],
        thread_id: Some("thread-known".to_string()),
    });
    handle_request(
        &state,
        &from_request(Request::SaveDraftToServer {
            draft: known.clone(),
        }),
    )
    .await;
    let stored = state.store.get_draft(&known.id).await.unwrap().unwrap();
    assert_eq!(
        stored.reply_headers.unwrap().thread_id.as_deref(),
        Some("thread-known")
    );
}

#[tokio::test]
async fn prepare_reply_defaults_from_to_delivered_alias() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    state
        .store
        .add_account_address(&account_id, "hello@example.com", false)
        .await
        .unwrap();

    let to_alias = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .provider_id("reply-alias")
        .subject("Delivered to alias")
        .sender_address("Client", "client@example.com")
        .recipient_address(Some("Hello"), "hello@example.com")
        .build();
    let alias_msg_id = to_alias.id.clone();
    state.store.upsert_envelope(&to_alias).await.unwrap();

    let to_other = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .provider_id("reply-other")
        .subject("Delivered elsewhere")
        .sender_address("Client", "client@example.com")
        .recipient_address(Some("List"), "list@example.com")
        .build();
    let other_msg_id = to_other.id.clone();
    state.store.upsert_envelope(&to_other).await.unwrap();
    let state = Arc::new(state);

    let resp = handle_request(
        &state,
        &from_request(Request::PrepareReply {
            message_id: alias_msg_id,
            reply_all: false,
        }),
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyContext { context },
        }) => assert_eq!(context.from, "hello@example.com"),
        other => panic!("expected ReplyContext, got {other:?}"),
    }

    let resp = handle_request(
        &state,
        &from_request(Request::PrepareReply {
            message_id: other_msg_id,
            reply_all: false,
        }),
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyContext { context },
        }) => assert_eq!(context.from, "user@example.com"),
        other => panic!("expected ReplyContext, got {other:?}"),
    }
}

async fn seed_reply_source_with_raw_headers(
    state: &AppState,
    account_id: &mxr_core::AccountId,
    provider_id: &str,
    header_pairs: &[(String, String)],
) -> mxr_core::MessageId {
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .provider_id(provider_id)
        .subject("Steer me")
        .sender_address("Sender", "sender@example.com")
        .recipient_address(Some("External"), "external@example.com")
        .build();
    let message_id = envelope.id.clone();
    state.store.upsert_envelope(&envelope).await.unwrap();
    let raw_headers = mxr_mail_parse::raw_headers_from_pairs(header_pairs);
    assert!(
        !raw_headers.contains("\r\n\r\n"),
        "producer is headers-only"
    );
    state
        .store
        .insert_body(&mxr_core::types::MessageBody {
            message_id: message_id.clone(),
            text_plain: Some("hi".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                raw_headers: Some(raw_headers),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    message_id
}

async fn reply_context_from(state: &Arc<AppState>, message_id: mxr_core::MessageId) -> String {
    let resp = handle_request(
        state,
        &from_request(Request::PrepareReply {
            message_id,
            reply_all: false,
        }),
    )
    .await;
    match resp.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::ReplyContext { context },
        }) => context.from,
        other => panic!("expected ReplyContext, got {other:?}"),
    }
}

#[tokio::test]
async fn prepare_reply_neutralizes_a_value_injected_delivered_to_and_defaults_to_primary() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    state
        .store
        .add_account_address(&account_id, "hello@example.com", false)
        .await
        .unwrap();
    let message_id = seed_reply_source_with_raw_headers(
        &state,
        &account_id,
        "value-injected-delivered-to",
        &[(
            "Subject".to_string(),
            "steer\r\nDelivered-To: hello@example.com".to_string(),
        )],
    )
    .await;
    let state = Arc::new(state);
    assert_eq!(
        reply_context_from(&state, message_id).await,
        "user@example.com",
        "a value-injected Delivered-To must not steer the From"
    );
}

#[tokio::test]
async fn ensure_reply_draft_reuses_one_canonical_draft_concurrently_and_preserves_edits() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    let message_id =
        seed_reply_source_with_raw_headers(&state, &account_id, "ensure-reply", &[]).await;
    let original = state
        .store
        .get_envelope(&message_id)
        .await
        .unwrap()
        .unwrap();
    let other = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .thread_id(original.thread_id)
        .provider_id("ensure-reply-later")
        .subject("Re: Steer me")
        .sender_address("Sender", "sender@example.com")
        .recipient_address(Some("External"), "external@example.com")
        .build();
    let other_message_id = other.id.clone();
    state.store.upsert_envelope(&other).await.unwrap();
    let state = Arc::new(state);
    let preview = handle_request(
        &state,
        &from_request(Request::EnsureReplyDraft {
            message_id: message_id.clone(),
            reply_all: false,
            dry_run: true,
        }),
    )
    .await;
    assert!(matches!(
        preview.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { .. }
        })
    ));
    assert!(state
        .store
        .list_drafts(&account_id)
        .await
        .unwrap()
        .is_empty());
    let mut joins = Vec::new();
    for index in 0..8 {
        let state = state.clone();
        let message_id = if index % 2 == 0 {
            message_id.clone()
        } else {
            other_message_id.clone()
        };
        joins.push(tokio::spawn(async move {
            let result = handle_request(
                &state,
                &from_request(Request::EnsureReplyDraft {
                    message_id,
                    reply_all: false,
                    dry_run: false,
                }),
            )
            .await;
            match result.payload {
                IpcPayload::Response(Response::Ok {
                    data: ResponseData::Draft { draft },
                }) => draft.id,
                other => panic!("expected reply draft, got {other:?}"),
            }
        }));
    }
    let mut ids = Vec::new();
    for join in joins {
        ids.push(join.await.unwrap());
    }
    assert!(ids.iter().all(|id| *id == ids[0]));
    let drafts = state.store.list_drafts(&account_id).await.unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].content.analysis_text(), "");
    let mut edited = drafts[0].clone();
    edited.content = mxr_core::types::DraftContent::markdown("preserve existing reply text");
    edited
        .attachments
        .push(std::path::PathBuf::from("reply-attachment.txt"));
    edited.to.push(mxr_core::types::Address {
        name: Some("Custom".into()),
        email: "custom@example.com".into(),
    });
    let saved = handle_request(
        &state,
        &from_request(Request::UpdateDraft { draft: edited }),
    )
    .await;
    assert!(matches!(
        saved.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { .. }
        })
    ));
    let resumed = handle_request(
        &state,
        &from_request(Request::EnsureReplyDraft {
            message_id: other_message_id,
            reply_all: true,
            dry_run: false,
        }),
    )
    .await;
    let resumed = match resumed.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { draft },
        }) => draft,
        other => panic!("expected resumed reply, got {other:?}"),
    };
    assert_eq!(resumed.id, drafts[0].id);
    assert_eq!(
        resumed.content.analysis_text(),
        "preserve existing reply text"
    );
    assert_eq!(
        resumed.attachments,
        [std::path::PathBuf::from("reply-attachment.txt")]
    );
    assert!(resumed.to.iter().any(|a| a.email == "custom@example.com"));
}

#[tokio::test]
async fn reply_audience_preview_is_read_only_and_apply_preserves_body_custom_recipients_and_bcc() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    let message_id =
        seed_reply_source_with_raw_headers(&state, &account_id, "audience-reply", &[]).await;
    let mut duplicate = state
        .store
        .get_envelope(&message_id)
        .await
        .unwrap()
        .unwrap();
    duplicate.id = mxr_core::MessageId::new();
    duplicate.provider_id = "duplicate-audience-source".into();
    state.store.upsert_envelope(&duplicate).await.unwrap();
    let state = Arc::new(state);
    let created = handle_request(
        &state,
        &from_request(Request::EnsureReplyDraft {
            message_id,
            reply_all: false,
            dry_run: false,
        }),
    )
    .await;
    let mut draft = match created.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { draft },
        }) => draft,
        other => panic!("expected reply draft, got {other:?}"),
    };
    draft.content = mxr_core::types::DraftContent::markdown("keep this body byte-for-byte");
    draft.to.push(mxr_core::types::Address {
        name: Some("Custom".into()),
        email: "custom@example.com".into(),
    });
    draft.bcc.push(mxr_core::types::Address {
        name: None,
        email: "blind@example.com".into(),
    });
    draft.bcc.push(mxr_core::types::Address {
        name: None,
        email: "sender@example.com".into(),
    });
    let saved = handle_request(
        &state,
        &from_request(Request::UpdateDraft {
            draft: draft.clone(),
        }),
    )
    .await;
    let draft = match saved.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { draft },
        }) => draft,
        other => panic!("expected saved draft, got {other:?}"),
    };
    let revision = draft.revision.unwrap();
    let preview = handle_request(
        &state,
        &from_request(Request::SetReplyDraftAudience {
            draft_id: draft.id.clone(),
            reply_all: true,
            expected_revision: revision,
            dry_run: true,
        }),
    )
    .await;
    let preview = match preview.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { draft },
        }) => draft,
        other => panic!("expected audience preview, got {other:?}"),
    };
    assert_eq!(
        preview.content.analysis_text(),
        "keep this body byte-for-byte"
    );
    assert!(preview.to.iter().any(|a| a.email == "custom@example.com"));
    assert_eq!(preview.bcc[0].email, "blind@example.com");
    assert!(!preview.to.iter().any(|a| a.email == "sender@example.com"));
    assert!(preview.bcc.iter().any(|a| a.email == "sender@example.com"));
    let unchanged = state.store.get_draft(&draft.id).await.unwrap().unwrap();
    assert_eq!(unchanged.revision, Some(revision));
    assert_eq!(unchanged.intent, mxr_core::DraftIntent::Reply);

    let applied = handle_request(
        &state,
        &from_request(Request::SetReplyDraftAudience {
            draft_id: draft.id.clone(),
            reply_all: true,
            expected_revision: revision,
            dry_run: false,
        }),
    )
    .await;
    let applied = match applied.payload {
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Draft { draft },
        }) => draft,
        other => panic!("expected audience update, got {other:?}"),
    };
    assert_eq!(
        applied.content.analysis_text(),
        "keep this body byte-for-byte"
    );
    assert!(applied.to.iter().any(|a| a.email == "custom@example.com"));
    assert_eq!(applied.bcc[0].email, "blind@example.com");
    assert!(!applied.to.iter().any(|a| a.email == "sender@example.com"));
    assert!(applied.bcc.iter().any(|a| a.email == "sender@example.com"));
    assert_eq!(applied.intent, mxr_core::DraftIntent::ReplyAll);
    assert!(applied.revision.unwrap() > revision);
    duplicate.thread_id = mxr_core::ThreadId::new();
    state.store.upsert_envelope(&duplicate).await.unwrap();
    let ambiguous = handle_request(
        &state,
        &from_request(Request::SetReplyDraftAudience {
            draft_id: applied.id,
            reply_all: false,
            expected_revision: applied.revision.unwrap(),
            dry_run: false,
        }),
    )
    .await;
    assert!(matches!(
        ambiguous.payload,
        IpcPayload::Response(Response::Error { .. })
    ));
}

#[tokio::test]
async fn ensure_reply_draft_never_reuses_sent_scheduled_or_sending_replies() {
    let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
    let account_id = state.default_account_id_opt().unwrap();
    let message_id =
        seed_reply_source_with_raw_headers(&state, &account_id, "reply-lifecycle-status", &[])
            .await;
    let state = Arc::new(state);
    let ensure = |state: Arc<AppState>, message_id: mxr_core::MessageId| async move {
        match handle_request(
            &state,
            &from_request(Request::EnsureReplyDraft {
                message_id,
                reply_all: false,
                dry_run: false,
            }),
        )
        .await
        .payload
        {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Draft { draft },
            }) => draft,
            other => panic!("expected reply draft, got {other:?}"),
        }
    };
    let sent = ensure(state.clone(), message_id.clone()).await;
    state
        .store
        .update_draft_status(&sent.id, mxr_core::DraftStatus::Sent)
        .await
        .unwrap();
    let scheduled = ensure(state.clone(), message_id.clone()).await;
    assert_ne!(sent.id, scheduled.id);
    state
        .store
        .schedule_send(
            &scheduled.id,
            chrono::Utc::now() + chrono::Duration::hours(1),
        )
        .await
        .unwrap();
    let sending = ensure(state.clone(), message_id.clone()).await;
    assert_ne!(scheduled.id, sending.id);
    state
        .store
        .update_draft_status(&sending.id, mxr_core::DraftStatus::Sending)
        .await
        .unwrap();
    let fresh = ensure(state, message_id).await;
    assert_ne!(sending.id, fresh.id);
}

async fn purge(
    state: &Arc<AppState>,
    address: &str,
    dry_run: bool,
    preview_token: Option<String>,
) -> Response {
    let msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::UnsubscribePurge {
            address: address.into(),
            account_id: None,
            dry_run,
            archive_on_no_method: true,
            preview_token,
        }),
    };
    match handle_request(state, &msg).await.payload {
        IpcPayload::Response(response) => response,
        other => panic!("expected a response, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unsubscribe_committed_with_its_preview_token_clears_only_what_was_previewed() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();
    let Response::Ok {
        data: ResponseData::UnsubscribePurgeResult { result: preview },
    } = purge(&state, "alice@work.com", true, None).await
    else {
        panic!("expected a preview");
    };
    let token = preview
        .preview_token
        .clone()
        .expect("a dry run issues a token");
    assert!(preview.message_count > 0);

    let wrong = purge(&state, "noreply@rust-lang.org", false, Some(token.clone())).await;
    assert!(matches!(wrong, Response::Error { .. }), "{wrong:?}");
    let made_up = purge(&state, "alice@work.com", false, Some("nope".into())).await;
    assert!(matches!(made_up, Response::Error { .. }), "{made_up:?}");

    let mut later = state
        .store
        .get_envelope(&preview.message_ids[0])
        .await
        .unwrap()
        .unwrap();
    later.id = mxr_core::MessageId::new();
    later.provider_id = format!("after-preview-{}", later.id);
    later.message_id_header = Some(format!("<{}@after>", later.id));
    state.store.upsert_envelope(&later).await.unwrap();
    state
        .search
        .apply_batch(mxr_search::SearchUpdateBatch {
            entries: vec![mxr_search::SearchIndexEntry {
                envelope: later.clone(),
                body: None,
                reply_later: false,
            }],
            ..Default::default()
        })
        .await
        .unwrap();
    state.search.commit().await.unwrap();

    let Response::Ok {
        data: ResponseData::UnsubscribePurgeResult { result },
    } = purge(&state, "alice@work.com", false, Some(token.clone())).await
    else {
        panic!("expected the purge");
    };
    assert_eq!(result.message_ids, preview.message_ids);
    assert!(!result.message_ids.contains(&later.id));
    let again = purge(&state, "alice@work.com", false, Some(token)).await;
    assert!(matches!(again, Response::Error { .. }), "{again:?}");
}

async fn sender_unsubscribe(
    state: &Arc<AppState>,
    dry_run: bool,
    archive: bool,
    preview_token: Option<String>,
) -> Response {
    let request = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(if dry_run {
            Request::UnsubscribePurge {
                address: "alice@work.com".into(),
                account_id: state.default_account_id_opt(),
                dry_run: true,
                archive_on_no_method: false,
                preview_token: None,
            }
        } else {
            Request::CommitUnsubscribePreview {
                address: "alice@work.com".into(),
                account_id: state.default_account_id_opt(),
                archive,
                preview_token: preview_token.unwrap_or_default(),
                archive_on_no_method: false,
            }
        }),
    };
    match handle_request(state, &request).await.payload {
        IpcPayload::Response(response) => response,
        other => panic!("expected response, got {other:?}"),
    }
}

fn sender_unsubscribe_result(response: Response) -> mxr_protocol::UnsubscribePurgeResultData {
    match response {
        Response::Ok {
            data: ResponseData::UnsubscribePurgeResult { result },
        } => result,
        other => panic!("expected unsubscribe result, got {other:?}"),
    }
}

#[tokio::test]
async fn unsubscribe_keep_or_clear_uses_previewed_method_even_if_selected_issue_has_another_or_none(
) {
    use mxr_core::types::UnsubscribeMethod;
    for archive in [false, true] {
        for selected_method in [
            UnsubscribeMethod::None,
            UnsubscribeMethod::Mailto {
                address: "wrong@example.test".into(),
                subject: None,
            },
        ] {
            let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
            state
                .sync_engine
                .sync_account(state.default_provider().as_ref())
                .await
                .unwrap();
            let state = Arc::new(state);
            let initial =
                sender_unsubscribe_result(sender_unsubscribe(&state, true, archive, None).await);
            let selected_id = initial.message_ids[0].clone();
            let mut selected = state
                .store
                .get_envelope(&selected_id)
                .await
                .unwrap()
                .unwrap();
            selected.unsubscribe = selected_method;
            state.store.upsert_envelope(&selected).await.unwrap();
            let method = UnsubscribeMethod::Mailto {
                address: "previewed@example.test".into(),
                subject: Some("cancel the previewed list".into()),
            };
            let mut previewed_issue = selected.clone();
            previewed_issue.id = mxr_core::MessageId::new();
            previewed_issue.provider_id = format!("previewed-{}", previewed_issue.id);
            previewed_issue.message_id_header = Some(format!("<{}@preview>", previewed_issue.id));
            previewed_issue.date = chrono::Utc::now();
            previewed_issue.unsubscribe = method.clone();
            state.store.upsert_envelope(&previewed_issue).await.unwrap();
            state
                .search
                .apply_batch(mxr_search::SearchUpdateBatch {
                    entries: vec![mxr_search::SearchIndexEntry {
                        envelope: previewed_issue.clone(),
                        body: None,
                        reply_later: false,
                    }],
                    ..Default::default()
                })
                .await
                .unwrap();
            state.search.commit().await.unwrap();
            let preview =
                sender_unsubscribe_result(sender_unsubscribe(&state, true, archive, None).await);
            assert_eq!(preview.method, method);
            assert!(preview.message_ids.contains(&selected_id));
            assert!(preview.message_ids.contains(&previewed_issue.id));
            let mut before = Vec::new();
            for id in &preview.message_ids {
                before.push((
                    state.store.get_envelope(id).await.unwrap().unwrap(),
                    state.store.get_message_label_ids(id).await.unwrap(),
                ));
            }
            let result = sender_unsubscribe_result(
                sender_unsubscribe(&state, false, archive, preview.preview_token).await,
            );
            assert_eq!(result.method, method);
            assert_eq!(result.message_ids, preview.message_ids);
            assert_eq!(
                result.status,
                mxr_protocol::UnsubscribePurgeStatusData::Unsubscribed
            );
            let sent = fake.sent_drafts();
            assert_eq!(sent.len(), 1);
            assert_eq!(sent[0].to[0].email, "previewed@example.test");
            assert_eq!(sent[0].subject, "cancel the previewed list");
            if archive {
                assert_eq!(result.archived_count, preview.message_count);
                let expected_provider_ids: Vec<_> = before
                    .iter()
                    .map(|(envelope, _)| envelope.provider_id.as_str())
                    .collect();
                let calls = fake.mutations();
                assert_eq!(calls.len(), expected_provider_ids.len() * 2);
                for call in calls {
                    match call {
                        mxr_provider_fake::RecordedMutation::ReadSet { provider_id, read } => {
                            assert!(read);
                            assert!(expected_provider_ids.contains(&provider_id.as_str()));
                        }
                        mxr_provider_fake::RecordedMutation::LabelsModified {
                            provider_id,
                            added,
                            removed,
                        } => {
                            assert!(added.is_empty());
                            assert_eq!(removed, vec!["INBOX"]);
                            assert!(expected_provider_ids.contains(&provider_id.as_str()));
                        }
                        other => panic!("unexpected archive mutation: {other:?}"),
                    }
                }
                for (original, _labels) in before {
                    let after = state
                        .store
                        .get_envelope(&original.id)
                        .await
                        .unwrap()
                        .unwrap();
                    assert!(after.flags.contains(mxr_core::MessageFlags::READ));
                    let inbox = state
                        .store
                        .list_labels_by_account(&after.account_id)
                        .await
                        .unwrap()
                        .into_iter()
                        .find(|label| label.provider_id == "INBOX")
                        .unwrap();
                    assert!(!state
                        .store
                        .get_message_label_ids(&after.id)
                        .await
                        .unwrap()
                        .contains(&inbox.id));
                }
            } else {
                assert_eq!(result.archived_count, 0);
                assert!(result.mutation_id.is_none());
                assert!(fake.mutations().is_empty());
                for (original, labels) in before {
                    let after = state
                        .store
                        .get_envelope(&original.id)
                        .await
                        .unwrap()
                        .unwrap();
                    assert_eq!(after.flags, original.flags);
                    assert_eq!(
                        state
                            .store
                            .get_message_label_ids(&original.id)
                            .await
                            .unwrap(),
                        labels
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn unsubscribe_preview_token_allows_only_one_parallel_commit() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();
    let state = Arc::new(state);
    let initial = sender_unsubscribe_result(sender_unsubscribe(&state, true, false, None).await);
    for id in &initial.message_ids {
        let mut envelope = state.store.get_envelope(id).await.unwrap().unwrap();
        envelope.unsubscribe = mxr_core::types::UnsubscribeMethod::Mailto {
            address: "once@example.test".into(),
            subject: None,
        };
        state.store.upsert_envelope(&envelope).await.unwrap();
    }
    let preview = sender_unsubscribe_result(sender_unsubscribe(&state, true, false, None).await);
    let token = preview.preview_token.unwrap();
    let (first, second) = tokio::join!(
        sender_unsubscribe(&state, false, false, Some(token.clone())),
        sender_unsubscribe(&state, false, false, Some(token)),
    );
    assert_eq!(
        [first, second]
            .iter()
            .filter(|response| matches!(response, Response::Ok { .. }))
            .count(),
        1
    );
    assert_eq!(fake.sent_drafts().len(), 1);
}

fn config_account(key: &str, name: &str, is_default: bool) -> AccountConfigData {
    AccountConfigData {
        key: key.into(),
        name: name.into(),
        email: format!("{key}@example.com"),
        enabled: true,
        sync: None,
        send: None,
        is_default,
        color: None,
        tag: None,
    }
}

const THREE_ACCOUNTS: &str = "[general]\ndefault_account = \"personal\"\n\
[accounts.work]\nname = \"Work\"\nemail = \"work@example.com\"\nenabled = true\n\
[accounts.personal]\nname = \"Personal\"\nemail = \"personal@example.com\"\nenabled = true\n\
[accounts.side]\nname = \"Side\"\nemail = \"side@example.com\"\nenabled = true\n";

fn with_config_file<F>(config: &str, env_default: Option<&str>, body: F)
where
    F: FnOnce(std::path::PathBuf) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
{
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let config_dir = temp_dir.path().join("config");
    let data_dir = temp_dir.path().join("data");
    let socket_path = temp_dir.path().join("mxr.sock");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    let config_file = config_dir.join("config.toml");
    std::fs::write(&config_file, config).expect("write config");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    temp_env::with_vars(
        [
            ("MXR_CONFIG_DIR", Some(config_dir.into_os_string())),
            ("MXR_DATA_DIR", Some(data_dir.into_os_string())),
            ("MXR_SOCKET_PATH", Some(socket_path.into_os_string())),
            (
                "MXR_DEFAULT_ACCOUNT",
                env_default.map(std::ffi::OsString::from),
            ),
        ],
        || runtime.block_on(body(config_file)),
    );
}

#[test]
fn an_env_default_override_is_not_written_by_an_untouched_toggle() {
    with_config_file(THREE_ACCOUNTS, Some("work"), |config_file| {
        Box::pin(async move {
            let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
            let result = crate::handler::account_config::upsert_account_config(
                &state,
                config_account("work", "Work Mail", true),
                true,
            )
            .await;
            assert!(result.ok, "{}", result.summary);
            let written =
                mxr_config::load_config_file_as_written(&config_file).expect("load as written");
            assert_eq!(written.general.default_account.as_deref(), Some("personal"));

            let result = crate::handler::account_config::upsert_account_config(
                &state,
                config_account("side", "Side", true),
                true,
            )
            .await;
            assert!(result.ok, "{}", result.summary);
            let written =
                mxr_config::load_config_file_as_written(&config_file).expect("load as written");
            assert_eq!(written.general.default_account.as_deref(), Some("side"));
        })
    });
}
