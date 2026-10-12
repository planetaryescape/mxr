//! Bridge tests for send receipts, scheduled sends, compose validation
//! status and per-account thread labels.

use super::*;
use std::sync::{Arc, Mutex};

async fn serve(socket_path: PathBuf) -> std::net::SocketAddr {
    bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap()
}

fn ok(data: ResponseData) -> Option<Response> {
    Some(Response::Ok { data })
}

fn sample_scheduled_send(account_id: &AccountId) -> mxr_protocol::ScheduledSendData {
    mxr_protocol::ScheduledSendData {
        draft_id: DraftId::new(),
        account_id: account_id.clone(),
        send_at: Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap(),
        subject: "Quarterly plan".into(),
        to: vec![Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        last_attempt_at: None,
        last_attempt_outcome: None,
    }
}

/// Start a compose session, write recipients and a body into it, and return
/// `(draft_path, account_id, accepted_revision)`.
async fn prepared_session(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
    start: serde_json::Value,
    to: &str,
) -> (String, String, Option<i64>) {
    let started: serde_json::Value = client
        .post(format!("http://{addr}/api/v1/mail/compose/session"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&start)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let draft_path = compose_draft_path(&started);
    let account_id = started["session"]["accountId"]
        .as_str()
        .unwrap()
        .to_string();
    let subject = started["session"]["frontmatter"]["subject"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let updated = client
        .post(format!("http://{addr}/api/v1/mail/compose/session/update"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "to": to,
            "cc": "",
            "bcc": "",
            "subject": if subject.is_empty() { "Hello".to_string() } else { subject },
            "from": "me@example.com",
            "attach": [],
            "body": "Body text",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), reqwest::StatusCode::OK);
    let accepted: serde_json::Value = updated.json().await.unwrap();
    (
        draft_path,
        account_id,
        accepted["session"]["revision"].as_i64(),
    )
}

#[tokio::test]
async fn scheduled_sends_route_lists_sends_and_forwards_account_scope() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account_id = AccountId::new();
    let send = sample_scheduled_send(&account_id);
    let expected_draft_id = send.draft_id.to_string();
    let scopes = Arc::new(Mutex::new(Vec::new()));
    let scopes_seen = scopes.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListScheduledSends { account_id } => {
                scopes_seen.lock().unwrap().push(account_id);
                ok(ResponseData::ScheduledSends {
                    sends: vec![send.clone()],
                })
            }
            _ => None,
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let client = reqwest::Client::new();

    let all: serde_json::Value = client
        .get(format!("http://{addr}/api/v1/mail/scheduled-sends"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(all["sends"][0]["draft_id"], expected_draft_id);
    assert_eq!(all["sends"][0]["send_at"], "2026-10-01T09:00:00Z");
    assert_eq!(all["sends"][0]["subject"], "Quarterly plan");
    assert_eq!(all["sends"][0]["to"][0]["email"], "alice@example.com");

    let scoped = client
        .get(format!(
            "http://{addr}/api/v1/mail/scheduled-sends?account={account_id}"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(scoped.status(), reqwest::StatusCode::OK);
    assert_eq!(
        *scopes.lock().unwrap(),
        vec![None, Some(account_id.clone())]
    );

    let bad = client
        .get(format!(
            "http://{addr}/api/v1/mail/scheduled-sends?account=not-a-uuid"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(
        scopes.lock().unwrap().len(),
        2,
        "bad input never reaches the daemon"
    );
}

#[tokio::test]
async fn drafts_list_rows_carry_send_at_only_for_scheduled_drafts() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account_id = AccountId::new();
    let mut scheduled = sample_scheduled_send(&account_id);
    let draft = |id: DraftId| -> Draft {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "account_id": account_id,
            "intent": "new",
            "to": [{"email": "alice@example.com"}],
            "cc": [],
            "bcc": [],
            "subject": "Quarterly plan",
            "body_markdown": "Body",
            "attachments": [],
            "created_at": "2026-09-01T12:00:00Z",
            "updated_at": "2026-09-01T12:00:00Z"
        }))
        .unwrap()
    };
    let scheduled_draft = draft(DraftId::new());
    let plain_draft = draft(DraftId::new());
    scheduled.draft_id = scheduled_draft.id.clone();
    let drafts = vec![scheduled_draft.clone(), plain_draft.clone()];
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListDrafts => ok(ResponseData::Drafts {
                drafts: drafts.clone(),
            }),
            Request::ListScheduledSends { account_id: None } => ok(ResponseData::ScheduledSends {
                sends: vec![scheduled.clone()],
            }),
            _ => None,
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;

    let json: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/mail/drafts"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let rows = json["drafts"].as_array().unwrap();
    assert_eq!(rows[0]["id"], scheduled_draft.id.to_string());
    assert_eq!(rows[0]["send_at"], "2026-10-01T09:00:00Z");
    assert_eq!(rows[1]["id"], plain_draft.id.to_string());
    assert!(rows[1]["send_at"].is_null());
}

#[tokio::test]
async fn compose_send_returns_the_sent_message_id_for_send_and_remind() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account = sample_account(&AccountId::new());
    let sent_message_id = MessageId::new();
    let receipt_id = sent_message_id.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListAccounts => ok(ResponseData::Accounts {
                accounts: vec![account.clone()],
            }),
            Request::SendDraft { .. } => ok(ResponseData::SendReceipt {
                local_message_id: receipt_id.clone(),
                provider_message_id: Some("provider-1".into()),
                rfc2822_message_id: "<sent-1@example.com>".into(),
            }),
            _ => None,
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let client = reqwest::Client::new();
    let (draft_path, account_id, expected_revision) = prepared_session(
        &client,
        addr,
        serde_json::json!({ "kind": "new" }),
        "alice@example.com",
    )
    .await;

    let response = client
        .post(format!("http://{addr}/api/v1/mail/compose/session/send"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "draft_path": draft_path, "account_id": account_id, "expected_revision": expected_revision }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["draft_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(json["message_id"], sent_message_id.to_string());
}

#[tokio::test]
async fn send_stored_adds_message_id_and_draft_id_to_the_receipt() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let draft_id = DraftId::new();
    let sent_message_id = MessageId::new();
    let receipt_id = sent_message_id.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::SendStoredDraft { .. } => ok(ResponseData::SendReceipt {
                local_message_id: receipt_id.clone(),
                provider_message_id: None,
                rfc2822_message_id: "<sent-2@example.com>".into(),
            }),
            _ => None,
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;

    let json: serde_json::Value = reqwest::Client::new()
        .post(format!(
            "http://{addr}/api/v1/mail/drafts/{draft_id}/send-stored?expected_revision=1&operation_id={draft_id}:1"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(json["message_id"], sent_message_id.to_string());
    assert_eq!(json["draft_id"], draft_id.to_string());
    // Existing readers of the raw receipt keep working.
    assert_eq!(json["kind"], "SendReceipt");
    assert_eq!(json["local_message_id"], sent_message_id.to_string());
}

#[tokio::test]
async fn scheduling_a_reply_session_stores_it_with_reply_headers_then_schedules_it() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let envelope = sample_envelope();
    let account = sample_account(&envelope.account_id);
    let source_message_id = envelope.id.clone();
    let requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen = requests.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            let response = match &request {
                Request::ListAccounts => ok(ResponseData::Accounts {
                    accounts: vec![account.clone()],
                }),
                Request::GetEnvelope { .. } => ok(ResponseData::Envelope {
                    envelope: envelope.clone(),
                }),
                Request::PrepareReply { .. } => ok(ResponseData::ReplyContext {
                    context: mxr_protocol::ReplyContext {
                        account_id: envelope.account_id.clone(),
                        in_reply_to: "<msg-1@example.com>".into(),
                        references: vec!["<root@example.com>".into()],
                        reply_to: "sender@example.com".into(),
                        cc: String::new(),
                        subject: "Mailroom".into(),
                        from: "sender@example.com".into(),
                        thread_context: "Original thread context".into(),
                        thread_id: None,
                    },
                }),
                Request::SaveDraft { .. } | Request::ScheduleSend { .. } => ok(ResponseData::Ack),
                _ => None,
            };
            seen.lock().unwrap().push(request);
            response
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let client = reqwest::Client::new();
    let (draft_path, account_id, expected_revision) = prepared_session(
        &client,
        addr,
        serde_json::json!({ "kind": "reply", "message_id": source_message_id.to_string() }),
        "sender@example.com",
    )
    .await;

    let response = client
        .post(format!(
            "http://{addr}/api/v1/mail/compose/session/schedule"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "account_id": account_id,
            "expected_revision": expected_revision,
            "send_at": "2026-10-01T09:00:00Z",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["send_at"], "2026-10-01T09:00:00Z");
    let draft_id = json["draft_id"].as_str().unwrap().to_string();

    let requests = requests.lock().unwrap();
    let stored = requests
        .iter()
        .find_map(|request| match request {
            Request::SaveDraft { draft } => Some(draft.clone()),
            _ => None,
        })
        .expect("session stored as a local draft");
    assert_eq!(stored.id.to_string(), draft_id);
    let headers = stored
        .reply_headers
        .as_ref()
        .expect("reply headers survive into the stored draft");
    assert_eq!(headers.in_reply_to, "<msg-1@example.com>");
    assert_eq!(headers.references, vec!["<root@example.com>".to_string()]);
    assert_eq!(stored.to[0].email, "sender@example.com");
    assert!(
        !requests
            .iter()
            .any(|request| matches!(request, Request::SaveDraftToServer { .. })),
        "scheduling must not push a provider copy"
    );
    let scheduled = requests
        .iter()
        .find_map(|request| match request {
            Request::ScheduleSend {
                managed_attachments_reviewed: true,
                draft_id,
                send_at,
                ..
            } => Some((draft_id.clone(), *send_at)),
            _ => None,
        })
        .expect("stored draft scheduled");
    assert_eq!(scheduled.0.to_string(), draft_id);
    assert_eq!(scheduled.1.to_rfc3339(), "2026-10-01T09:00:00+00:00");
    assert!(
        !Path::new(&draft_path).exists(),
        "the stored draft owns the content once scheduled"
    );
}

#[tokio::test]
async fn scheduling_a_restored_session_updates_the_stored_draft_in_place() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account = sample_account(&AccountId::new());
    let stored_id = DraftId::new();
    let stored = markdown_draft(
        &stored_id,
        &account.account_id,
        "Hello",
        "original",
        Utc::now(),
    );
    let requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen = requests.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            let response = match &request {
                Request::ListDrafts => ok(ResponseData::Drafts {
                    drafts: vec![stored.clone()],
                }),
                Request::ListAccounts => ok(ResponseData::Accounts {
                    accounts: vec![account.clone()],
                }),
                Request::UpdateDraft { .. } | Request::ScheduleSend { .. } => ok(ResponseData::Ack),
                _ => None,
            };
            seen.lock().unwrap().push(request);
            response
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let client = reqwest::Client::new();
    let draft_path = restore_session_path(&client, addr, &stored_id).await;
    edit_session(&client, addr, &draft_path, "Hello", "Body text").await;
    let metadata = read_draft_session_metadata(Path::new(&draft_path))
        .await
        .unwrap()
        .unwrap();
    let account_id = metadata.account_id.to_string();

    let response = client
        .post(format!(
            "http://{addr}/api/v1/mail/compose/session/schedule"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "account_id": account_id,
            "draft_id": stored_id.to_string(),
            "expected_revision": metadata.revision,
            "send_at": "2026-10-01T09:00:00Z",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["draft_id"], stored_id.to_string());

    let requests = requests.lock().unwrap();
    assert!(requests
        .iter()
        .any(|request| matches!(request, Request::UpdateDraft { draft } if draft.id == stored_id)));
    assert!(!requests
        .iter()
        .any(|request| matches!(request, Request::SaveDraft { .. })));
    assert!(requests.iter().any(
        |request| matches!(request, Request::ScheduleSend { draft_id, .. } if *draft_id == stored_id)
    ));
}

#[tokio::test]
async fn failed_scheduling_keeps_the_accepted_revision_for_the_next_edit() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account_id = AccountId::new();
    let draft_id = DraftId::new();
    let store = FakeDraftStore::with(vec![markdown_draft(
        &draft_id,
        &account_id,
        "Schedule retry",
        " original whitespace \n",
        Utc::now(),
    )]);
    let responder = draft_store_responder(store.clone(), sample_account(&account_id));
    let scheduled = Arc::new(Mutex::new(0_u32));
    let scheduled_seen = scheduled.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ScheduleSend { .. } => {
                *scheduled_seen.lock().unwrap() += 1;
                Some(Response::error_kinded(
                    "Scheduling unavailable",
                    mxr_protocol::IpcErrorKind::Internal,
                ))
            }
            Request::SendDraft { .. } => panic!("scheduling must never send mail"),
            other => responder(other),
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let client = reqwest::Client::new();
    let path = restore_session_path(&client, addr, &draft_id).await;
    let response = client
        .post(format!(
            "http://{addr}/api/v1/mail/compose/session/schedule"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(
            &json!({ "draft_path": path, "account_id": account_id, "draft_id": draft_id,
            "expected_revision": 1, "send_at": "2027-01-01T09:00:00Z" }),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        reqwest::StatusCode::INTERNAL_SERVER_ERROR
    );
    let failure: serde_json::Value = response.json().await.unwrap();
    assert_eq!(failure["code"], "internal");
    assert_eq!(failure["accepted_draft"]["draft_id"], draft_id.to_string());
    assert_eq!(failure["accepted_draft"]["revision"], 2);
    assert!(
        Path::new(&path).exists(),
        "failed scheduling keeps the compose file"
    );
    assert_eq!(
        read_draft_session_metadata(Path::new(&path))
            .await
            .unwrap()
            .unwrap()
            .revision,
        Some(2)
    );
    let accepted = store.snapshot();
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].revision, Some(2));
    edit_session(&client, addr, &path, "Schedule retry", "retry text").await;
    let edited = store.snapshot();
    assert_eq!(edited.len(), 1);
    assert_eq!(edited[0].revision, Some(3));
    assert_eq!(edited[0].content.analysis_text(), "retry text");
    assert_eq!(*scheduled.lock().unwrap(), 1);
    remove_compose_file(Path::new(&path)).await.unwrap();
}

#[tokio::test]
async fn stale_discard_preserves_the_saved_draft_and_compose_file() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account_id = AccountId::new();
    let draft_id = DraftId::new();
    let store = FakeDraftStore::with(vec![markdown_draft(
        &draft_id,
        &account_id,
        "Retained",
        "original",
        Utc::now(),
    )]);
    let responder = draft_store_responder(store.clone(), sample_account(&account_id));
    let deletes = Arc::new(Mutex::new(0_u32));
    let deletes_seen = deletes.clone();
    let deletion_store = store.clone();
    let _ipc = spawn_fake_ipc_server(&socket_path, move |request| match request {
        Request::DeleteDraft { expected_revision, .. } => {
            let current_revision = deletion_store.snapshot()[0].revision;
            if expected_revision != current_revision {
                Some(Response::Error { message: "Draft revision conflict".into(),
                    kind: mxr_protocol::IpcErrorKind::InvalidRequest, code: "draft_revision_conflict".into(),
                    retryable: false, details: Some(json!({ "expected_revision": expected_revision, "current_revision": current_revision })) })
            } else {
                *deletes_seen.lock().unwrap() += 1;
                deletion_store.clear();
                ok(ResponseData::Ack)
            }
        }
        other => responder(other),
    }, None).await;
    let addr = serve(socket_path).await;
    let client = reqwest::Client::new();
    let path = restore_session_path(&client, addr, &draft_id).await;
    let mut other_editor = store.snapshot()[0].clone();
    other_editor.content = DraftContent::markdown("accepted elsewhere");
    store.update(other_editor).unwrap();
    let response = client
        .post(format!("http://{addr}/api/v1/mail/compose/session/discard"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&json!({ "draft_path": path, "expected_revision": 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
    assert!(Path::new(&path).exists());
    assert_eq!(
        read_draft_session_metadata(Path::new(&path))
            .await
            .unwrap()
            .unwrap()
            .revision,
        Some(1)
    );
    assert_eq!(
        store.snapshot()[0].content.analysis_text(),
        "accepted elsewhere"
    );
    assert_eq!(*deletes.lock().unwrap(), 0);
    remove_compose_file(Path::new(&path)).await.unwrap();
}

#[tokio::test]
async fn compose_validation_errors_are_422_and_never_reach_the_daemon() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account = sample_account(&AccountId::new());
    let daemon_sends = Arc::new(Mutex::new(0_u32));
    let sends_seen = daemon_sends.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListAccounts => ok(ResponseData::Accounts {
                accounts: vec![account.clone()],
            }),
            Request::SendDraft { .. }
            | Request::SaveDraft { .. }
            | Request::SaveDraftToServer { .. }
            | Request::ScheduleSend { .. } => {
                *sends_seen.lock().unwrap() += 1;
                ok(ResponseData::Ack)
            }
            _ => None,
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let client = reqwest::Client::new();
    // No recipients: a draft error the user has to fix.
    let (draft_path, account_id, expected_revision) =
        prepared_session(&client, addr, serde_json::json!({ "kind": "new" }), "").await;

    *daemon_sends.lock().unwrap() = 0;
    for (route, extra) in [
        ("send", serde_json::json!({})),
        (
            "schedule",
            serde_json::json!({ "send_at": "2026-10-01T09:00:00Z" }),
        ),
    ] {
        let mut body = serde_json::json!({ "draft_path": draft_path, "account_id": account_id, "expected_revision": expected_revision });
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let response = client
            .post(format!("http://{addr}/api/v1/mail/compose/session/{route}"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
            "route {route}"
        );
        let json: serde_json::Value = response.json().await.unwrap();
        assert!(
            json["error"]
                .as_str()
                .is_some_and(|error| error.starts_with("Draft errors:")),
            "route {route}: {json}"
        );
        assert_eq!(json["code"], "invalid_request", "route {route}");
    }
    assert_eq!(*daemon_sends.lock().unwrap(), 0);
    assert!(Path::new(&draft_path).exists(), "a rejected draft is kept");
    let _ = std::fs::remove_file(draft_path);
}

#[tokio::test]
async fn thread_reader_resolves_labels_from_the_threads_own_account() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let default_account = AccountId::new();
    let thread_account = AccountId::new();
    let mut envelope = sample_envelope();
    envelope.account_id = thread_account.clone();
    envelope.label_provider_ids = vec!["follow-up".into()];
    let thread = sample_thread(&envelope);
    let thread_id = thread.id.clone();
    let body = sample_body(&envelope);
    let mut thread_labels = sample_labels(&thread_account);
    thread_labels[2].name = "Other account follow up".into();
    // The default account has no label with the thread's provider id, so a
    // lookup against it leaves the message unlabelled.
    let default_labels = vec![sample_labels(&default_account)[0].clone()];
    let label_scopes = Arc::new(Mutex::new(Vec::new()));
    let scopes_seen = label_scopes.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::GetThread { .. } => ok(ResponseData::Thread {
                thread: thread.clone(),
                messages: vec![envelope.clone()],
                summary: None,
            }),
            Request::ListLabels { account_id } => {
                scopes_seen.lock().unwrap().push(account_id.clone());
                let labels = if account_id.as_ref() == Some(&thread_account) {
                    thread_labels.clone()
                } else {
                    default_labels.clone()
                };
                ok(ResponseData::Labels { labels })
            }
            Request::ListBodies { .. } => ok(ResponseData::Bodies {
                bodies: vec![body.clone()],
                failures: Vec::new(),
            }),
            _ => None,
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;

    let json: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/mail/threads/{thread_id}"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        json["messages"][0]["labels"][0]["name"], "Other account follow up",
        "{json}"
    );
    let thread_account_id = json["thread"]["account_id"].as_str().unwrap().to_string();
    let scopes = label_scopes.lock().unwrap();
    assert_eq!(scopes.len(), 1);
    assert_eq!(
        scopes[0].as_ref().map(ToString::to_string),
        Some(thread_account_id)
    );
}

#[tokio::test]
async fn compose_promises_send_the_session_draft_and_the_browser_zone() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account = sample_account(&AccountId::new());
    let seen = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen_for_ipc = seen.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListAccounts => ok(ResponseData::Accounts {
                accounts: vec![account.clone()],
            }),
            Request::DetectPromises { .. } => {
                seen_for_ipc.lock().unwrap().push(request);
                ok(ResponseData::Promises {
                    detection: mxr_protocol::PromiseDetectionData {
                        status: mxr_protocol::PromiseDetectionStatusData::Ready,
                        promises: Vec::new(),
                        provenance: None,
                        message: None,
                    },
                })
            }
            _ => None,
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let client = reqwest::Client::new();
    let (draft_path, account_id, _expected_revision) = prepared_session(
        &client,
        addr,
        serde_json::json!({ "kind": "new" }),
        "alice@example.com",
    )
    .await;

    let json: serde_json::Value = client
        .post(format!(
            "http://{addr}/api/v1/mail/compose/session/promises"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "account_id": account_id,
            "time_zone": "Europe/London",
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(json["kind"], "Promises");
    assert_eq!(json["detection"]["status"], "ready");

    let seen = seen.lock().unwrap();
    let Request::DetectPromises {
        source: mxr_protocol::PromiseSourceData::Draft { draft },
        time_zone,
        ..
    } = &seen[0]
    else {
        panic!("expected a draft source, got {:?}", seen[0]);
    };
    assert_eq!(time_zone.as_deref(), Some("Europe/London"));
    assert_eq!(draft.to[0].email, "alice@example.com");
    assert!(draft.content.analysis_text().contains("Body text"));
}

#[tokio::test]
async fn record_promise_forwards_the_chosen_instant_and_dry_run() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let seen = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen_for_ipc = seen.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::RecordPromise { .. } => {
                seen_for_ipc.lock().unwrap().push(request);
                Some(Response::error("stop here"))
            }
            _ => None,
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let message_id = MessageId::new();

    let response = reqwest::Client::new()
        .post(format!("http://{addr}/api/v1/mail/commitments"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "message_id": message_id.to_string(),
            "what": "send the deck",
            "due_at": "2026-10-02T08:00:00Z",
            "dry_run": true,
        }))
        .send()
        .await
        .unwrap();
    assert!(!response.status().is_success(), "daemon errors surface");

    let seen = seen.lock().unwrap();
    let Request::RecordPromise {
        message_id: forwarded,
        what,
        due_at,
        dry_run,
    } = &seen[0]
    else {
        panic!("expected RecordPromise");
    };
    assert_eq!(forwarded, &message_id);
    assert_eq!(what, "send the deck");
    assert_eq!(*due_at, Utc.with_ymd_and_hms(2026, 10, 2, 8, 0, 0).unwrap());
    assert!(*dry_run);
}

#[tokio::test]
async fn a_sent_message_reports_success_even_when_session_cleanup_fails() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account = sample_account(&AccountId::new());
    let sent_message_id = MessageId::new();
    let receipt_id = sent_message_id.clone();
    let session_path = Arc::new(Mutex::new(None::<PathBuf>));
    let session_for_ipc = session_path.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListAccounts => ok(ResponseData::Accounts {
                accounts: vec![account.clone()],
            }),
            Request::SendDraft { .. } => {
                // After the send, the session file can't be removed: it has
                // become a non-empty directory.
                let path = session_for_ipc.lock().unwrap().clone().unwrap();
                std::fs::remove_file(&path).unwrap();
                std::fs::create_dir_all(path.join("blocker")).unwrap();
                ok(ResponseData::SendReceipt {
                    local_message_id: receipt_id.clone(),
                    provider_message_id: None,
                    rfc2822_message_id: "<sent-cleanup@example.com>".into(),
                })
            }
            _ => None,
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let client = reqwest::Client::new();
    let (draft_path, account_id, expected_revision) = prepared_session(
        &client,
        addr,
        serde_json::json!({ "kind": "new" }),
        "alice@example.com",
    )
    .await;
    *session_path.lock().unwrap() = Some(PathBuf::from(&draft_path));

    let response = client
        .post(format!("http://{addr}/api/v1/mail/compose/session/send"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "draft_path": draft_path, "account_id": account_id, "expected_revision": expected_revision }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        reqwest::StatusCode::OK,
        "a sent message is never reported as failed"
    );
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["message_id"], sent_message_id.to_string());
    let warnings = json["cleanup_warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0]
        .as_str()
        .unwrap()
        .starts_with("Couldn't remove the session file"));
    std::fs::remove_dir_all(&draft_path).unwrap();
}

#[tokio::test]
async fn overlapping_shared_session_updates_keep_each_snapshot_with_its_observed_token() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let account = AccountId::new();
    let id = DraftId::new();
    let store = FakeDraftStore::with(vec![markdown_draft(
        &id,
        &account,
        "Shared",
        "original",
        Utc::now(),
    )]);
    let effects = Arc::new(Mutex::new(0));
    let seen = effects.clone();
    let responder = draft_store_responder(store.clone(), sample_account(&account));
    let _ipc = spawn_fake_ipc_server(
        &socket,
        move |request| {
            if matches!(
                &request,
                Request::SendDraft { .. }
                    | Request::SaveDraftToServer { .. }
                    | Request::ScheduleSend { .. }
                    | Request::DeleteDraft { .. }
            ) {
                *seen.lock().unwrap() += 1;
            }
            responder(request)
        },
        None,
    )
    .await;
    let addr = serve(socket).await;
    let client = reqwest::Client::new();
    let path = restore_session_path(&client, addr, &id).await;
    let submit = |body: &'static str| {
        client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&json!({ "draft_path": path, "account_id": account, "expected_revision": 1,
                "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Shared", "from": "me@example.com", "body": body }))
            .send()
    };
    let (left, right) = tokio::join!(submit("left snapshot"), submit("right snapshot"));
    let left = left.unwrap();
    let right = right.unwrap();
    let (accepted, rejected, winning, losing) = if left.status() == reqwest::StatusCode::OK {
        (left, right, "left snapshot", "right snapshot")
    } else {
        (right, left, "right snapshot", "left snapshot")
    };
    assert_eq!(accepted.status(), reqwest::StatusCode::OK);
    assert_eq!(rejected.status(), reqwest::StatusCode::CONFLICT);
    let success: serde_json::Value = accepted.json().await.unwrap();
    assert_eq!(success["session"]["bodyMarkdown"], winning);
    assert_eq!(success["session"]["revision"], 2);
    assert_eq!(store.snapshot()[0].content.analysis_text(), winning);
    let failure: serde_json::Value = rejected.json().await.unwrap();
    assert_eq!(failure["code"], "draft_revision_conflict");
    assert_eq!(failure["details"]["current_revision"], 2);
    assert!(failure.get("accepted_draft").is_none());
    let recovered = failure["recovery_session"]["draft_path"].as_str().unwrap();
    let refreshed = load_compose_session(Path::new(&path)).await.unwrap();
    assert_eq!(refreshed["bodyMarkdown"], winning);
    assert_eq!(refreshed["revision"], 2);
    let losing_session = load_compose_session(Path::new(recovered)).await.unwrap();
    assert_eq!(losing_session["bodyMarkdown"], losing);
    assert_eq!(losing_session["revision"], 1);
    assert_eq!(losing_session["recoverySourcePath"], path);
    for body in ["latest before close", "latest before close"] {
        let response = client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&json!({"draft_path": recovered, "account_id": account, "expected_revision": 1,
                "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Shared", "from": "me@example.com", "body": body}))
            .send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let local: serde_json::Value = response.json().await.unwrap();
        assert_eq!(local["session"]["draftPath"], recovered);
        assert_eq!(local["session"]["revision"], 1);
        assert_eq!(local["session"]["bodyMarkdown"], body);
        assert_eq!(store.snapshot()[0].revision, Some(2));
        assert_eq!(store.snapshot()[0].content.analysis_text(), winning);
    }
    // Recovery cannot cause effects, including requests from null-token clients.
    for endpoint in ["send", "save", "schedule", "safety-check"] {
        let blocked = client.post(format!("http://{addr}/api/v1/mail/compose/session/{endpoint}"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&json!({"draft_path": recovered, "account_id": account, "draft_id": id,
                "expected_revision": null, "save_to_server": true, "send_at": "2026-12-01T12:00:00Z"}))
            .send().await.unwrap();
        assert_eq!(
            blocked.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "{endpoint}"
        );
    }
    // Explicit canonical restore does not erase the rejected editor's text.
    let reopened = restore_session_path(&client, addr, &id).await;
    let canonical = load_compose_session(Path::new(&reopened)).await.unwrap();
    assert_eq!(canonical["bodyMarkdown"], winning);
    assert_eq!(canonical["revision"], 2);
    assert_eq!(
        load_compose_session(Path::new(recovered)).await.unwrap()["bodyMarkdown"],
        "latest before close"
    );
    // Two explicit reopens own independent editable copies, even when they
    // began with the same remembered recovery path.
    let mut opened = Vec::new();
    for _ in 0..2 {
        let response: serde_json::Value = client
            .post(format!("http://{addr}/api/v1/mail/compose/session/refresh"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&json!({"draft_path": recovered}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let copy = response["session"]["draftPath"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(copy, recovered);
        opened.push(copy);
    }
    assert_ne!(opened[0], opened[1]);
    for (copy, body) in opened
        .iter()
        .zip(["first reopened editor", "second reopened editor"])
    {
        let response = client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&json!({"draft_path": copy, "account_id": account, "expected_revision": 1,
                "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Shared", "from": "me@example.com", "body": body}))
            .send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }
    assert_eq!(
        load_compose_session(Path::new(recovered)).await.unwrap()["bodyMarkdown"],
        "latest before close"
    );
    let deleted_copy = client
        .post(format!("http://{addr}/api/v1/mail/compose/session/discard"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&json!({"draft_path": opened[0], "expected_revision": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted_copy.status(), reqwest::StatusCode::OK);
    assert_eq!(
        load_compose_session(Path::new(&opened[1])).await.unwrap()["bodyMarkdown"],
        "second reopened editor"
    );
    assert_eq!(store.snapshot()[0].revision, Some(2));
    remove_compose_file(Path::new(&opened[1])).await.unwrap();
    let discarded = client
        .post(format!("http://{addr}/api/v1/mail/compose/session/discard"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&json!({"draft_path": recovered, "expected_revision": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(discarded.status(), reqwest::StatusCode::OK);
    assert!(!Path::new(recovered).exists());
    assert_eq!(store.snapshot()[0].revision, Some(2));
    assert_eq!(*effects.lock().unwrap(), 0);
    remove_compose_file(Path::new(&path)).await.unwrap();
    if reopened != path {
        remove_compose_file(Path::new(&reopened)).await.unwrap();
    }
}

#[tokio::test]
async fn failed_provider_save_reports_its_accepted_revision_even_after_another_writer_advances() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let account = AccountId::new();
    let id = DraftId::new();
    let store = FakeDraftStore::with(vec![markdown_draft(
        &id,
        &account,
        "Partial",
        " original whitespace \n",
        Utc::now(),
    )]);
    let responder = draft_store_responder(store.clone(), sample_account(&account));
    let provider_store = store.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket,
        move |request| match request {
            Request::SaveDraftToServer { draft } => {
                assert_eq!(draft.revision, Some(2));
                let mut other = provider_store.snapshot()[0].clone();
                other.content = DraftContent::markdown("another editor's newer text");
                provider_store.update(other).unwrap();
                Some(Response::Error {
                    message: "Provider conflict".into(),
                    kind: mxr_protocol::IpcErrorKind::InvalidRequest,
                    code: "draft_revision_conflict".into(),
                    retryable: false,
                    details: Some(json!({"expected_revision": 2, "current_revision": 3})),
                })
            }
            other => responder(other),
        },
        None,
    )
    .await;
    let addr = serve(socket).await;
    let client = reqwest::Client::new();
    let path = restore_session_path(&client, addr, &id).await;
    let response = client.post(format!("http://{addr}/api/v1/mail/compose/session/save"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&json!({"draft_path": path, "draft_id": id, "account_id": account, "expected_revision": 1, "save_to_server": true}))
        .send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
    let failure: serde_json::Value = response.json().await.unwrap();
    assert_eq!(failure["error"], "Provider conflict");
    assert_eq!(failure["code"], "draft_revision_conflict");
    assert_eq!(failure["details"]["current_revision"], 3);
    assert_eq!(failure["accepted_draft"]["draft_id"], id.to_string());
    assert_eq!(failure["accepted_draft"]["revision"], 2);
    assert_eq!(
        read_draft_session_metadata(Path::new(&path))
            .await
            .unwrap()
            .unwrap()
            .revision,
        Some(2)
    );
    assert_eq!(store.snapshot()[0].revision, Some(3));
    remove_compose_file(Path::new(&path)).await.unwrap();
}

#[tokio::test]
async fn stale_shared_path_send_cannot_send_another_editors_message() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let account = AccountId::new();
    let id = DraftId::new();
    let store = FakeDraftStore::with(vec![markdown_draft(
        &id,
        &account,
        "Reviewed",
        "reviewed by old tab",
        Utc::now(),
    )]);
    let responder = draft_store_responder(store.clone(), sample_account(&account));
    let sends = Arc::new(Mutex::new(0));
    let seen = sends.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket,
        move |request| match request {
            Request::SendDraft { .. } | Request::CheckDraftSafety { .. } => {
                *seen.lock().unwrap() += 1;
                ok(ResponseData::Ack)
            }
            other => responder(other),
        },
        None,
    )
    .await;
    let addr = serve(socket).await;
    let client = reqwest::Client::new();
    let path = restore_session_path(&client, addr, &id).await;
    edit_session(&client, addr, &path, "Reviewed", "another tab's message").await;
    for route in ["send", "safety-check"] {
        let response = client
            .post(format!("http://{addr}/api/v1/mail/compose/session/{route}"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&json!({ "draft_path": path, "account_id": account, "expected_revision": 1 }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
        let failure: serde_json::Value = response.json().await.unwrap();
        assert_eq!(failure["code"], "draft_revision_conflict");
        assert_eq!(failure["details"]["current_revision"], 2);
    }
    assert_eq!(*sends.lock().unwrap(), 0);
    assert!(Path::new(&path).exists());
    assert_eq!(
        store.snapshot()[0].content.analysis_text(),
        "another tab's message"
    );
    remove_compose_file(Path::new(&path)).await.unwrap();
}

#[tokio::test]
async fn draft_attachment_generic_send_routes_preserve_explicit_review_marker() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorded = seen.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::SendStoredDraft { .. } | Request::ScheduleSend { .. } => {
                recorded.lock().unwrap().push(request.clone());
                ok(ResponseData::Ack)
            }
            _ => None,
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let client = reqwest::Client::new();
    let draft_id = DraftId::new();
    for marker in [None, Some(true)] {
        let suffix = marker.map_or(String::new(), |value| {
            format!("&managed_attachments_reviewed={value}")
        });
        let response = client.post(format!("http://{addr}/api/v1/mail/drafts/{draft_id}/send-stored?operation_id=review-op&expected_revision=1{suffix}"))
            .bearer_auth(TEST_AUTH_TOKEN).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let mut body = serde_json::json!({"draft_id":draft_id,"expected_revision":1,"send_at":"2099-01-01T00:00:00Z"});
        if let Some(value) = marker {
            body["managed_attachments_reviewed"] = serde_json::json!(value);
        }
        let response = client
            .post(format!("http://{addr}/api/v1/mail/scheduled-sends"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }
    let requests = seen.lock().unwrap();
    assert!(matches!(
        &requests[0],
        Request::SendStoredDraft {
            managed_attachments_reviewed: false,
            ..
        }
    ));
    assert!(matches!(
        &requests[1],
        Request::ScheduleSend {
            managed_attachments_reviewed: false,
            ..
        }
    ));
    assert!(matches!(
        &requests[2],
        Request::SendStoredDraft {
            managed_attachments_reviewed: true,
            ..
        }
    ));
    assert!(matches!(
        &requests[3],
        Request::ScheduleSend {
            managed_attachments_reviewed: true,
            ..
        }
    ));
}
