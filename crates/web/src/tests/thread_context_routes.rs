//! Bridge tests for the reader's context block: both routes forward the
//! thread id (and `refresh`) and pass the daemon payload through.

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
                Request::GetThreadContext { thread_id } => ResponseData::ThreadContext {
                    context: mxr_protocol::ThreadContextData {
                        thread_id,
                        account_id: AccountId::new(),
                        counterparty: None,
                        owed_reply: None,
                        commitments: vec![],
                    },
                },
                Request::GetThreadGist { thread_id, .. } => ResponseData::ThreadGist {
                    gist: mxr_protocol::ThreadGistData {
                        thread_id,
                        status: mxr_protocol::ThreadGistStatusData::Disabled,
                        gist: None,
                        ask: None,
                        provenance: None,
                        reason: Some("No language model is configured.".into()),
                        generated_at: None,
                        from_cache: false,
                    },
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
    let client = reqwest::Client::new();
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

    let seen = seen.lock().unwrap();
    assert!(matches!(&seen[0], Request::GetThreadContext { thread_id: id } if *id == thread_id));
    assert!(matches!(
        &seen[1],
        Request::GetThreadGist { thread_id: id, refresh: true } if *id == thread_id
    ));
}
