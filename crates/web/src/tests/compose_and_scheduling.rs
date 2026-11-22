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
            "expected_revision": started["session"]["revision"],
        }))
        .send()
        .await
        .unwrap();
    let status = updated.status();
    let body = updated.text().await.unwrap();
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "compose update failed: {body}"
    );
    let accepted: serde_json::Value = serde_json::from_str(&body).unwrap();
    (
        draft_path,
        account_id,
        accepted["session"]["revision"].as_i64(),
    )
}

#[tokio::test]
async fn scheduling_a_reply_session_stores_it_with_reply_headers_then_schedules_it() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let envelope = sample_envelope();
    let account = sample_account(&envelope.account_id);
    let source_message_id = envelope.id.clone();
    let now = chrono::Utc::now();
    let canonical = mxr_core::Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id: envelope.account_id.clone(),
        from: None,
        reply_headers: Some(mxr_core::ReplyHeaders {
            in_reply_to: "<msg-1@example.com>".into(),
            references: vec!["<root@example.com>".into()],
            thread_id: None,
        }),
        intent: mxr_core::DraftIntent::Reply,
        to: vec![mxr_core::Address {
            name: None,
            email: "sender@example.com".into(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: "Re: Mailroom".into(),
        content: mxr_core::DraftContent::markdown(""),
        managed_attachments: None,
        attachments: Vec::new(),
        inline_assets: Vec::new(),
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    };
    let listed_draft = canonical.clone();
    let ensured_draft = canonical.clone();
    let canonical_state = Arc::new(Mutex::new(canonical.clone()));
    let daemon_state = canonical_state.clone();
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
                Request::EnsureReplyDraft { .. } => ok(ResponseData::Draft {
                    draft: ensured_draft.clone(),
                }),
                Request::ListDrafts => ok(ResponseData::Drafts {
                    drafts: vec![listed_draft.clone()],
                }),
                Request::GetDraft { .. } => ok(ResponseData::Draft {
                    draft: daemon_state.lock().unwrap().clone(),
                }),
                Request::UpdateDraft { draft } => {
                    let mut draft = draft.clone();
                    draft.revision = Some(2);
                    *daemon_state.lock().unwrap() = draft.clone();
                    ok(ResponseData::Draft { draft })
                }
                Request::SaveDraft { draft } => ok(ResponseData::Draft {
                    draft: draft.clone(),
                }),
                Request::ScheduleSend { .. } => ok(ResponseData::Ack),
                _ => None,
            };
            seen.lock().unwrap().push(request);
            response
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let client = test_http_client();
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
            Request::SaveDraft { draft } | Request::UpdateDraft { draft } => Some(draft.clone()),
            _ => None,
        })
        .unwrap_or_else(|| canonical.clone());
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
    let client = test_http_client();
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
    let client = test_http_client();
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
    let client = test_http_client();
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
    let client = test_http_client();
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
