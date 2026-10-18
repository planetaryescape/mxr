use super::*;
use crate::handler::microsoft_todo_cli::test_support::FakeCli;

#[test]
fn task_transmission_requires_send_permission_but_previews_are_read_only() {
    use crate::handler::{request_is_read_only, request_requires_send_capability};
    use mxr_protocol::Request;
    for dry_run in [true, false] {
        for request in [
            Request::SendTodoToMicrosoft {
                account_id: None,
                todo_id: "todo_1".into(),
                dry_run,
                fingerprint: None,
            },
            Request::SyncMicrosoftTodos {
                account_id: None,
                todo_id: Some("todo_1".into()),
                dry_run,
            },
        ] {
            assert_eq!(request_is_read_only(&request), dry_run);
            assert_eq!(request_requires_send_capability(&request), !dry_run);
        }
    }
}

#[test]
fn microsoft_todo_activity_contains_only_the_target_id_and_omits_previews() {
    use crate::activity::mapper::map_request;
    use mxr_protocol::{ClientKind, Request};
    let request = Request::SendTodoToMicrosoft {
        account_id: None,
        todo_id: "todo_1".into(),
        dry_run: false,
        fingerprint: Some("not-an-activity-field".into()),
    };
    let entry = map_request(&request, ClientKind::Cli, None, true).unwrap();
    assert_eq!(entry.action, "todo.microsoft_link");
    assert_eq!(entry.target_id.as_deref(), Some("todo_1"));
    assert!(entry.context.is_none());
    let preview = Request::SendTodoToMicrosoft {
        account_id: None,
        todo_id: "todo_1".into(),
        dry_run: true,
        fingerprint: None,
    };
    assert!(map_request(&preview, ClientKind::Cli, None, true).is_none());
}

async fn fixture() -> (AppState, FakeCli, String) {
    let (state, _) = AppState::in_memory_with_fake().await.unwrap();
    let fake = FakeCli::new();
    let id = uuid::Uuid::now_v7().to_string();
    sqlx::query("INSERT INTO todos (id, account_id, kind, verb, title, origin, reason, rules_version, dedup_key, created_at, updated_at) VALUES (?, ?, 'bill', 'pay', 'Pay council tax', 'rule', 'Payment due', 1, ?, 1, 1)")
        .bind(&id).bind(state.default_account_id().as_str()).bind(&id)
        .execute(state.store.writer()).await.unwrap();
    (state, fake, id)
}

#[tokio::test]
async fn microsoft_selection_and_sync_never_cross_explicit_account_boundaries() {
    let (state, fake, own_id) = fixture().await;
    let own = state.default_account_id().clone();
    let other = mxr_core::AccountId::new();
    state
        .store
        .insert_account(&crate::test_fixtures::test_account_with_id(other.clone()))
        .await
        .unwrap();
    let other_id = uuid::Uuid::now_v7().to_string();
    sqlx::query("INSERT INTO todos (id,account_id,kind,verb,title,origin,reason,rules_version,dedup_key,created_at,updated_at) SELECT ?,?,kind,verb,title,origin,reason,rules_version,?,created_at,updated_at FROM todos WHERE id=?")
        .bind(&other_id).bind(other.as_str()).bind(&other_id).bind(&own_id).execute(state.store.writer()).await.unwrap();
    linked(&state, &fake, &own_id).await;
    linked(&state, &fake, &other_id).await;
    for (account, id, foreign) in [(&own, &own_id, &other_id), (&other, &other_id, &own_id)] {
        let accounts = std::slice::from_ref(account);
        let links = select_links(&state, None, Some(accounts)).await.unwrap();
        assert_eq!(
            links
                .iter()
                .map(|link| link.todo_id.as_str())
                .collect::<Vec<_>>(),
            [id.as_str()]
        );
        let response = sync_links(&state, &fake.cli(), links, true).await.unwrap();
        assert!(
            matches!(response, ResponseData::MicrosoftTodoSync { sync } if sync.results.len() == 1 && sync.results[0].todo_id == *id)
        );
        assert!(select_links(&state, Some(foreign), Some(accounts))
            .await
            .is_err());
        assert!(link(&state, foreign, Some(account), true, None)
            .await
            .is_err());
    }
    assert!(select_links(&state, None, Some(&[]))
        .await
        .unwrap()
        .is_empty());
    assert_eq!(fake.calls(), ["add", "add", "show", "show"]);
}

async fn preview(state: &AppState, fake: &FakeCli, id: &str) -> MicrosoftTodoSendData {
    match link_with_cli(state, id, true, None, Some(&fake.cli()))
        .await
        .unwrap()
    {
        ResponseData::MicrosoftTodoSend { result } => result,
        _ => panic!("expected link preview"),
    }
}

async fn linked(state: &AppState, fake: &FakeCli, id: &str) -> TodoMicrosoftExport {
    let preview = preview(state, fake, id).await;
    link_with_cli(
        state,
        id,
        false,
        Some(&preview.fingerprint),
        Some(&fake.cli()),
    )
    .await
    .unwrap();
    state
        .store
        .todo_microsoft_export(id)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn preview_does_not_spawn_or_claim_and_stale_confirmation_is_refused() {
    let (state, fake, id) = fixture().await;
    let preview = preview(&state, &fake, &id).await;
    assert_eq!(preview.title, "Pay council tax");
    assert!(fake.calls().is_empty());
    assert!(state
        .store
        .todo_microsoft_export(&id)
        .await
        .unwrap()
        .is_none());
    sqlx::query("UPDATE todos SET title = 'A newer title' WHERE id = ?")
        .bind(&id)
        .execute(state.store.writer())
        .await
        .unwrap();
    assert!(link_with_cli(
        &state,
        &id,
        false,
        Some(&preview.fingerprint),
        Some(&fake.cli())
    )
    .await
    .is_err());
    assert!(fake.calls().is_empty());
}

#[tokio::test]
async fn repeated_link_uses_durable_receipt_instead_of_creating_another_task() {
    let (state, fake, id) = fixture().await;
    let receipt = linked(&state, &fake, &id).await;
    assert_eq!(receipt.task_id.as_deref(), Some("remote-task"));
    let again = preview(&state, &fake, &id).await;
    assert_eq!(again.state, "linked");
    assert_eq!(fake.calls(), ["add"]);
    assert!(
        state
            .store
            .get_todo(&id)
            .await
            .unwrap()
            .unwrap()
            .user_edited
    );
}

#[tokio::test]
async fn lost_create_answer_never_allows_a_second_create() {
    let (state, fake, id) = fixture().await;
    fake.fail("add", 1);
    let preview = preview(&state, &fake, &id).await;
    let result = link_with_cli(
        &state,
        &id,
        false,
        Some(&preview.fingerprint),
        Some(&fake.cli()),
    )
    .await
    .unwrap();
    assert!(
        matches!(result, ResponseData::MicrosoftTodoSend { result } if result.state == "unknown")
    );
    link_with_cli(
        &state,
        &id,
        false,
        Some(&preview.fingerprint),
        Some(&fake.cli()),
    )
    .await
    .unwrap();
    assert_eq!(fake.calls(), ["add"]);
}

#[tokio::test]
async fn microsoft_completion_and_reopening_change_the_local_task() {
    let (state, fake, id) = fixture().await;
    let receipt = linked(&state, &fake, &id).await;
    let mut remote = fake.fields();
    remote.completed = true;
    fake.set_remote(&remote);
    let result = sync_one(&state, &fake.cli(), &receipt, true).await.unwrap();
    assert_eq!(result.state, "changes");
    assert_eq!(
        state.store.get_todo(&id).await.unwrap().unwrap().state,
        TodoState::Open
    );
    sync_one(&state, &fake.cli(), &receipt, false)
        .await
        .unwrap();
    assert_eq!(
        state.store.get_todo(&id).await.unwrap().unwrap().state,
        TodoState::Done
    );
    remote.completed = false;
    fake.set_remote(&remote);
    let receipt = state
        .store
        .todo_microsoft_export(&id)
        .await
        .unwrap()
        .unwrap();
    sync_one(&state, &fake.cli(), &receipt, false)
        .await
        .unwrap();
    assert_eq!(
        state.store.get_todo(&id).await.unwrap().unwrap().state,
        TodoState::Open
    );
    assert!(fake
        .calls()
        .iter()
        .all(|verb| verb == "add" || verb == "show"));
}

#[tokio::test]
async fn mixer_completion_and_reopening_are_sent_to_microsoft() {
    let (state, fake, id) = fixture().await;
    linked(&state, &fake, &id).await;
    for completed in [true, false] {
        let to = if completed {
            TodoState::Done
        } else {
            TodoState::Open
        };
        let from = if completed {
            TodoState::Open
        } else {
            TodoState::Done
        };
        state
            .store
            .set_todos_state(std::slice::from_ref(&id), &[from], to, Utc::now())
            .await
            .unwrap();
        let mut remote = fake.fields();
        remote.completed = completed;
        fake.set_next(&remote);
        let receipt = state
            .store
            .todo_microsoft_export(&id)
            .await
            .unwrap()
            .unwrap();
        let result = sync_one(&state, &fake.cli(), &receipt, false)
            .await
            .unwrap();
        assert_eq!(result.state, "synced");
    }
    assert_eq!(
        fake.calls()
            .iter()
            .filter(|verb| *verb == "complete" || *verb == "reopen")
            .cloned()
            .collect::<Vec<_>>(),
        ["complete", "reopen"]
    );
}

#[tokio::test]
async fn conflicting_edits_preserve_both_versions_and_disjoint_edits_merge() {
    let (state, fake, id) = fixture().await;
    let receipt = linked(&state, &fake, &id).await;
    sqlx::query("UPDATE todos SET title = 'Mixer edit' WHERE id = ?")
        .bind(&id)
        .execute(state.store.writer())
        .await
        .unwrap();
    let mut peer = fake.fields();
    peer.title = "Phone edit".into();
    fake.set_remote(&peer);
    let result = sync_one(&state, &fake.cli(), &receipt, false)
        .await
        .unwrap();
    assert_eq!(result.state, "conflict");
    assert_eq!(
        state.store.get_todo(&id).await.unwrap().unwrap().title,
        "Mixer edit"
    );
    assert_eq!(fake.calls(), ["add", "show"]);
    peer.title = fake.fields().title;
    peer.due = NaiveDate::from_ymd_opt(2026, 11, 5);
    fake.set_remote(&peer);
    peer.title = "Mixer edit".into();
    fake.set_next(&peer);
    let result = sync_one(&state, &fake.cli(), &receipt, false)
        .await
        .unwrap();
    assert_eq!(result.state, "synced");
    let record = state.store.get_todo(&id).await.unwrap().unwrap();
    assert_eq!(LinkedTodoFields::from_record(&record, &Local), peer);
}

#[tokio::test]
async fn recurring_unknown_and_failed_tasks_pause_without_mutations() {
    let (state, fake, id) = fixture().await;
    let receipt = linked(&state, &fake, &id).await;
    for extra in [
        serde_json::json!({"recurrence": {"pattern": {"type": "daily"}}}),
        serde_json::json!({"sync_state": "unknown"}),
        serde_json::json!({"sync_state": "failed"}),
    ] {
        // The CLI shape, not the normalized merge DTO.
        fake.set_remote(&fake.fields());
        let mut task: serde_json::Value =
            serde_json::from_slice(&std::fs::read(fake.path().join("remote.json")).unwrap())
                .unwrap();
        task.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        fake.set_remote_json(&task);
        let result = sync_one(&state, &fake.cli(), &receipt, false)
            .await
            .unwrap();
        assert_eq!(result.state, "paused");
    }
    assert!(fake
        .calls()
        .iter()
        .all(|verb| verb == "add" || verb == "show"));
}

#[tokio::test]
async fn uncertain_sync_is_not_replayed_from_its_persisted_receipt() {
    let (state, fake, id) = fixture().await;
    let receipt = linked(&state, &fake, &id).await;
    sqlx::query("UPDATE todos SET title = 'Mixer edit' WHERE id = ?")
        .bind(&id)
        .execute(state.store.writer())
        .await
        .unwrap();
    fake.fail("edit", 1);
    sync_one(&state, &fake.cli(), &receipt, false)
        .await
        .unwrap();
    let receipt = state
        .store
        .todo_microsoft_export(&id)
        .await
        .unwrap()
        .unwrap();
    assert!(receipt.uncertain_json.is_some());
    sync_one(&state, &fake.cli(), &receipt, false)
        .await
        .unwrap();
    assert_eq!(
        fake.calls().iter().filter(|verb| *verb == "edit").count(),
        1
    );
}

#[tokio::test]
async fn failed_read_before_an_outbound_write_does_not_leave_an_uncertain_receipt() {
    let (state, fake, id) = fixture().await;
    let receipt = linked(&state, &fake, &id).await;
    sqlx::query("UPDATE todos SET title = 'Mixer edit' WHERE id = ?")
        .bind(&id)
        .execute(state.store.writer())
        .await
        .unwrap();
    std::fs::write(fake.path().join("fail-after-show"), "1").unwrap();
    let result = sync_one(&state, &fake.cli(), &receipt, false)
        .await
        .unwrap();
    assert_eq!(result.state, "paused");
    assert!(state
        .store
        .todo_microsoft_export(&id)
        .await
        .unwrap()
        .unwrap()
        .uncertain_json
        .is_none());
    assert_eq!(fake.calls(), ["add", "show", "show"]);
}

#[tokio::test]
async fn concurrent_link_requests_can_launch_only_one_create() {
    let (state, fake, id) = fixture().await;
    let preview = preview(&state, &fake, &id).await;
    let cli = fake.cli();
    let (a, b) = tokio::join!(
        link_with_cli(&state, &id, false, Some(&preview.fingerprint), Some(&cli)),
        link_with_cli(&state, &id, false, Some(&preview.fingerprint), Some(&cli)),
    );
    assert!(a.is_ok() || b.is_ok());
    assert_eq!(fake.calls(), ["add"]);
}

#[tokio::test]
async fn stale_local_snapshot_cannot_overwrite_a_user_edit_or_claim_a_link() {
    let (state, _, id) = fixture().await;
    let snapshot = state.store.get_todo(&id).await.unwrap().unwrap();
    let mut desired = snapshot.clone();
    desired.title = "Phone edit".into();
    sqlx::query("UPDATE todos SET title = 'My newer edit' WHERE id = ?")
        .bind(&id)
        .execute(state.store.writer())
        .await
        .unwrap();
    assert!(!state
        .store
        .apply_linked_todo_fields(&snapshot, &desired)
        .await
        .unwrap());
    assert!(!state
        .store
        .claim_todo_microsoft_export(&snapshot, "{}")
        .await
        .unwrap());
    assert_eq!(
        state.store.get_todo(&id).await.unwrap().unwrap().title,
        "My newer edit"
    );
    assert!(state
        .store
        .todo_microsoft_export(&id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn matching_date_only_peer_does_not_erase_local_times() {
    let (state, fake, id) = fixture().await;
    let due = Local
        .with_ymd_and_hms(2026, 11, 5, 17, 43, 0)
        .single()
        .unwrap()
        .with_timezone(&Utc);
    let scheduled = Local
        .with_ymd_and_hms(2026, 11, 3, 12, 17, 0)
        .single()
        .unwrap()
        .with_timezone(&Utc);
    sqlx::query("UPDATE todos SET due_at = ?, scheduled_for = ? WHERE id = ?")
        .bind(due.timestamp())
        .bind(scheduled.timestamp())
        .bind(&id)
        .execute(state.store.writer())
        .await
        .unwrap();
    let mut fields = fake.fields();
    fields.due = NaiveDate::from_ymd_opt(2026, 11, 5);
    fields.scheduled = NaiveDate::from_ymd_opt(2026, 11, 3);
    fake.set_remote(&fields);
    let receipt = linked(&state, &fake, &id).await;
    let result = sync_one(&state, &fake.cli(), &receipt, false)
        .await
        .unwrap();
    assert_eq!(result.state, "synced");
    let saved = state.store.get_todo(&id).await.unwrap().unwrap();
    assert_eq!(
        (saved.due_at, saved.scheduled_for),
        (Some(due), Some(scheduled))
    );
}

#[tokio::test]
async fn inbound_dates_persist_timing_provenance_and_allow_explicit_clearing() {
    let (state, fake, id) = fixture().await;
    linked(&state, &fake, &id).await;
    let mut peer = fake.fields();
    peer.due = NaiveDate::from_ymd_opt(2026, 11, 5);
    peer.scheduled = NaiveDate::from_ymd_opt(2026, 11, 3);
    for fields in [peer, fake.fields()] {
        fake.set_remote(&fields);
        let receipt = state
            .store
            .todo_microsoft_export(&id)
            .await
            .unwrap()
            .unwrap();
        let result = sync_one(&state, &fake.cli(), &receipt, false)
            .await
            .unwrap();
        assert_eq!(result.state, "synced");
        let record = state.store.get_todo(&id).await.unwrap().unwrap();
        assert_eq!(LinkedTodoFields::from_record(&record, &Local), fields);
        assert_eq!(record.window_source.as_deref(), Some("user"));
        assert!(record.due_words.is_none());
    }
    assert!(fake
        .calls()
        .iter()
        .all(|verb| verb == "add" || verb == "show"));
}

#[tokio::test]
async fn older_cli_without_deferral_cannot_clear_an_existing_schedule() {
    let (state, fake, id) = fixture().await;
    let scheduled = Local
        .with_ymd_and_hms(2026, 11, 3, 12, 17, 0)
        .single()
        .unwrap()
        .with_timezone(&Utc);
    sqlx::query("UPDATE todos SET scheduled_for = ? WHERE id = ?")
        .bind(scheduled.timestamp())
        .bind(&id)
        .execute(state.store.writer())
        .await
        .unwrap();
    let mut fields = fake.fields();
    fields.scheduled = NaiveDate::from_ymd_opt(2026, 11, 3);
    fake.set_remote(&fields);
    let receipt = linked(&state, &fake, &id).await;
    let mut old: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fake.path().join("remote.json")).unwrap()).unwrap();
    old.as_object_mut().unwrap().remove("defer_until");
    fake.set_remote_json(&old);
    let result = sync_one(&state, &fake.cli(), &receipt, false)
        .await
        .unwrap();
    assert_eq!(result.state, "paused");
    assert_eq!(
        state
            .store
            .get_todo(&id)
            .await
            .unwrap()
            .unwrap()
            .scheduled_for,
        Some(scheduled)
    );
    assert_eq!(
        state
            .store
            .todo_microsoft_export(&id)
            .await
            .unwrap()
            .unwrap()
            .baseline_json,
        receipt.baseline_json
    );
    assert_eq!(fake.calls(), ["add", "show"]);
}

#[tokio::test]
async fn incompatible_cli_is_refused_before_creating_a_task_or_claiming_a_link() {
    let (state, fake, id) = fixture().await;
    std::fs::write(
        fake.path().join("schema.json"),
        r#"{"schema_version":2,"output":{"required":["id","title"]}}"#,
    )
    .unwrap();
    let preview = preview(&state, &fake, &id).await;
    let result = link_with_cli(
        &state,
        &id,
        false,
        Some(&preview.fingerprint),
        Some(&fake.cli()),
    )
    .await;
    assert!(result.is_err());
    assert!(fake.calls().is_empty());
    assert!(state
        .store
        .todo_microsoft_export(&id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn reopening_a_linked_task_restores_the_runway_not_its_old_expired_bucket() {
    let (state, fake, id) = fixture().await;
    linked(&state, &fake, &id).await;
    sqlx::query("UPDATE todos SET state = 'expired', expired_at = 1, expired_at_birth = 1, catchup = 'let_go' WHERE id = ?")
        .bind(&id).execute(state.store.writer()).await.unwrap();
    let mut completed = fake.fields();
    completed.completed = true;
    // Establish agreement on the terminal state before Microsoft reopens it.
    state
        .store
        .note_todo_microsoft_sync(&id, Some(&serde_json::to_string(&completed).unwrap()), None)
        .await
        .unwrap();
    let receipt = state
        .store
        .todo_microsoft_export(&id)
        .await
        .unwrap()
        .unwrap();
    let result = sync_one(&state, &fake.cli(), &receipt, false)
        .await
        .unwrap();
    assert_eq!(result.state, "synced");
    let record = state.store.get_todo(&id).await.unwrap().unwrap();
    assert_eq!(record.state, TodoState::Open);
    assert_eq!(record.catchup, Some(mxr_store::TodoCatchup::Kept));
    assert!(!record.expired_at_birth);
    assert!(record.expired_at.is_none());
    assert_eq!(
        state.store.expire_lapsed_todos(Utc::now()).await.unwrap(),
        0
    );
}

#[tokio::test]
async fn stale_timing_snapshot_cannot_erase_a_concurrent_window_update() {
    let (state, _, id) = fixture().await;
    let snapshot = state.store.get_todo(&id).await.unwrap().unwrap();
    let mut desired = snapshot.clone();
    desired.title = "Phone edit".into();
    sqlx::query(
        "UPDATE todos SET relevant_until = 2000000000, window_source = 'new evidence' WHERE id = ?",
    )
    .bind(&id)
    .execute(state.store.writer())
    .await
    .unwrap();
    assert!(!state
        .store
        .apply_linked_todo_fields(&snapshot, &desired)
        .await
        .unwrap());
    let record = state.store.get_todo(&id).await.unwrap().unwrap();
    assert_eq!(record.title, "Pay council tax");
    assert_eq!(record.window_source.as_deref(), Some("new evidence"));
    assert_eq!(record.relevant_until.unwrap().timestamp(), 2_000_000_000);
}

#[tokio::test]
async fn promise_completion_and_reopening_commit_atomically_with_the_linked_task() {
    use mxr_store::{CommitmentDirection, CommitmentStatus, ContactCommitmentRecord};
    let (state, fake, id) = fixture().await;
    state
        .store
        .upsert_contact_commitment(&ContactCommitmentRecord {
            id: "linked-promise".into(),
            account_id: state.default_account_id().clone(),
            email: "person@example.test".into(),
            thread_id: mxr_core::ThreadId::new(),
            direction: CommitmentDirection::Yours,
            status: CommitmentStatus::Open,
            who_owes: "you".into(),
            what: "Send the form".into(),
            by_when: None,
            evidence_msg_id: mxr_core::MessageId::new(),
            extracted_at: Utc::now(),
            resolved_at: None,
        })
        .await
        .unwrap();
    sqlx::query("UPDATE todos SET commitment_id = 'linked-promise' WHERE id = ?")
        .bind(&id)
        .execute(state.store.writer())
        .await
        .unwrap();
    let receipt = linked(&state, &fake, &id).await;
    let mut peer = fake.fields();
    peer.completed = true;
    fake.set_remote(&peer);
    sqlx::query("CREATE TRIGGER reject_promise_update BEFORE UPDATE ON contact_commitments BEGIN SELECT RAISE(ABORT, 'synthetic mirror failure'); END")
        .execute(state.store.writer()).await.unwrap();
    assert!(sync_one(&state, &fake.cli(), &receipt, false)
        .await
        .is_err());
    assert_eq!(
        state.store.get_todo(&id).await.unwrap().unwrap().state,
        TodoState::Open
    );
    assert_eq!(
        state
            .store
            .get_contact_commitment("linked-promise")
            .await
            .unwrap()
            .unwrap()
            .status,
        CommitmentStatus::Open
    );
    assert_eq!(
        state
            .store
            .todo_microsoft_export(&id)
            .await
            .unwrap()
            .unwrap()
            .baseline_json,
        receipt.baseline_json
    );
    sqlx::query("DROP TRIGGER reject_promise_update")
        .execute(state.store.writer())
        .await
        .unwrap();
    for (completed, todo_state, promise_state) in [
        (true, TodoState::Done, CommitmentStatus::Resolved),
        (false, TodoState::Open, CommitmentStatus::Open),
    ] {
        peer.completed = completed;
        fake.set_remote(&peer);
        let receipt = state
            .store
            .todo_microsoft_export(&id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            sync_one(&state, &fake.cli(), &receipt, false)
                .await
                .unwrap()
                .state,
            "synced"
        );
        assert_eq!(
            state.store.get_todo(&id).await.unwrap().unwrap().state,
            todo_state
        );
        assert_eq!(
            state
                .store
                .get_contact_commitment("linked-promise")
                .await
                .unwrap()
                .unwrap()
                .status,
            promise_state
        );
    }
}
