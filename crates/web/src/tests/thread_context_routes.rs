//! Bridge tests for the reader's context block and list gists: the routes
//! forward the thread ids (and `refresh`, `generate`, `automated`) and pass the daemon
//! payload through.

use super::*;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn context_and_gist_routes_forward_to_the_daemon() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let seen = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen_for_ipc = seen.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            seen_for_ipc.lock().unwrap().push(request.clone());
            let data = match request {
                Request::GetThreadContext { thread_id, .. } => ResponseData::ThreadContext {
                    context: mxr_protocol::ThreadContextData {
                        thread_id,
                        account_id: AccountId::new(),
                        counterparty: None,
                        owed_reply: None,
                        promises: vec![],
                    },
                },
                Request::GetThreadGist { thread_id, .. } => ResponseData::ThreadGist {
                    gist: mxr_protocol::ThreadGistData {
                        account_id: None,
                        thread_id,
                        status: mxr_protocol::ThreadGistStatusData::Disabled,
                        gist: None,
                        ask: None,
                        provenance: None,
                        reason: Some("No language model is configured.".into()),
                        generated_at: None,
                        from_cache: false,
                        newest_message_id: None,
                    },
                },
                Request::GetThreadGists { thread_ids, .. } => ResponseData::ThreadGists {
                    batch: mxr_protocol::ThreadGistBatchData {
                        model: mxr_protocol::GistModelData::Available,
                        gists: vec![],
                        queued: thread_ids,
                        in_flight: vec![],
                        skipped: vec![],
                    },
                },
                Request::GetThreadPreviews { thread_ids, .. } => ResponseData::ThreadPreviews {
                    previews: thread_ids
                        .into_iter()
                        .map(|thread_id| mxr_protocol::ThreadPreviewData {
                            thread_id,
                            subject: "Invoice".into(),
                            sender_name: None,
                            sender_email: "dana@example.com".into(),
                            latest_at: 1_760_000_000,
                            message_count: 2,
                            unread_count: 1,
                            snippet: "Please confirm".into(),
                            has_attachments: false,
                            account_id: None,
                        })
                        .collect(),
                },
                _ => return Some(Response::error("unexpected request")),
            };
            Some(Response::Ok { data })
        },
        None,
    )
    .await;
    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();
    let client = test_http_client();
    let thread_id = ThreadId::new();

    let context: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/v1/mail/threads/{thread_id}/context"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(context["kind"], "ThreadContext");
    assert_eq!(context["context"]["thread_id"], thread_id.to_string());

    let gist: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/v1/mail/threads/{thread_id}/context/gist?refresh=true"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(gist["kind"], "ThreadGist");
    assert_eq!(gist["gist"]["status"], "disabled");

    let bad = client
        .get(format!(
            "http://{addr}/api/v1/mail/threads/not-a-thread/context"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);

    let other = ThreadId::new();
    let batch: serde_json::Value = client
        .post(format!("http://{addr}/api/v1/mail/gists"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "thread_ids": [thread_id.to_string(), other.to_string()],
            "generate": true,
            "automated": true,
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(batch["kind"], "ThreadGists");
    assert_eq!(batch["batch"]["queued"][1], other.to_string());

    let bad_batch = client
        .post(format!("http://{addr}/api/v1/mail/gists"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "thread_ids": ["nope"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_batch.status(), reqwest::StatusCode::BAD_REQUEST);

    let previews: serde_json::Value = client
        .post(format!("http://{addr}/api/v1/mail/threads/previews"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "thread_ids": [other.to_string()] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(previews["kind"], "ThreadPreviews");
    assert_eq!(previews["previews"][0]["thread_id"], other.to_string());
    assert!(previews["previews"][0].get("body").is_none());

    for body in [
        serde_json::json!({ "thread_ids": [] }),
        serde_json::json!({ "thread_ids": ["nope"] }),
    ] {
        let bad = client
            .post(format!("http://{addr}/api/v1/mail/threads/previews"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);
    }

    let seen = seen.lock().unwrap();
    assert!(
        matches!(&seen[0], Request::GetThreadContext { thread_id: id, .. } if *id == thread_id)
    );
    assert!(matches!(
        &seen[1],
        Request::GetThreadGist { thread_id: id, refresh: true, .. } if *id == thread_id
    ));
    assert!(matches!(
        &seen[2],
        Request::GetThreadGists { thread_ids, generate: true, automated: true, .. } if thread_ids == &vec![thread_id.clone(), other.clone()]
    ));
    assert!(matches!(
        &seen[3],
        Request::GetThreadPreviews { thread_ids, .. } if thread_ids == &vec![other.clone()]
    ));
    assert_eq!(
        seen.len(),
        4,
        "a bad or empty batch never reaches the daemon"
    );
}

/// A row of a thread id two accounts hold names its account; the bridge hands
/// it to the daemon, so the reader's calls stay on that account.
#[tokio::test]
async fn thread_routes_forward_the_rows_account() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let seen = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen_for_ipc = seen.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            seen_for_ipc.lock().unwrap().push(request);
            Some(Response::Ok {
                data: ResponseData::Ack,
            })
        },
        None,
    )
    .await;
    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();
    let client = test_http_client();
    let thread_id = ThreadId::new();
    let account = AccountId::new();

    for path in [
        format!("/api/v1/mail/threads/{thread_id}/context?account_id={account}"),
        format!("/api/v1/mail/threads/{thread_id}/context/gist?account_id={account}"),
        format!("/api/v1/mail/threads/{thread_id}/briefing?account_id={account}"),
        format!("/api/v1/mail/threads/{thread_id}/export?account_id={account}"),
    ] {
        client
            .get(format!("http://{addr}{path}"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
    }
    for (path, body) in [
        (
            "/api/v1/mail/threads/previews",
            serde_json::json!({ "thread_accounts": [{ "thread_id": thread_id, "account_id": account }] }),
        ),
        (
            "/api/v1/mail/desk/dismiss",
            serde_json::json!({ "thread_ids": [], "thread_accounts": [{ "thread_id": thread_id, "account_id": account }] }),
        ),
        (
            "/api/v1/mail/desk/restore",
            serde_json::json!({ "thread_ids": [], "thread_accounts": [{ "thread_id": thread_id, "account_id": account }] }),
        ),
    ] {
        client
            .post(format!("http://{addr}{path}"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&body)
            .send()
            .await
            .unwrap();
    }

    let seen = seen.lock().unwrap();
    let names = |pairs: &[mxr_protocol::ThreadAccountRef]| {
        pairs.len() == 1 && pairs[0].thread_id == thread_id && pairs[0].account_id == account
    };
    assert!(
        matches!(&seen[0], Request::GetThreadContext { account_id: Some(a), .. } if *a == account)
    );
    assert!(
        matches!(&seen[1], Request::GetThreadGist { account_id: Some(a), .. } if *a == account)
    );
    assert!(
        matches!(&seen[2], Request::GetThreadBriefing { account_id: Some(a), .. } if *a == account)
    );
    assert!(matches!(&seen[3], Request::ExportThread { account_id: Some(a), .. } if *a == account));
    assert!(
        matches!(&seen[4], Request::GetThreadPreviews { thread_accounts, .. } if names(thread_accounts))
    );
    assert!(
        matches!(&seen[5], Request::DismissDeskThreads { thread_accounts, .. } if names(thread_accounts))
    );
    assert!(
        matches!(&seen[6], Request::RestoreDeskThreads { thread_accounts, .. } if names(thread_accounts))
    );
}
