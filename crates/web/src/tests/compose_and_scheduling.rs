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
/// `(draft_path, account_id)`.
async fn prepared_session(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
    start: serde_json::Value,
    to: &str,
) -> (String, String) {
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
    (draft_path, account_id)
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
    let (draft_path, account_id) = prepared_session(
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
            Request::ScheduleSend { draft_id, send_at } => Some((draft_id.clone(), *send_at)),
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
    let requests = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen = requests.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            let response = match &request {
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
    let (draft_path, account_id) = prepared_session(
        &client,
        addr,
        serde_json::json!({ "kind": "new" }),
        "alice@example.com",
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
            "draft_id": stored_id.to_string(),
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
