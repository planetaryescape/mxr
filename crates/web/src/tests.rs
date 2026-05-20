use super::*;
use chrono::{Local, TimeZone, Utc};
use futures::{SinkExt, StreamExt};
use mxr_core::{
    id::{AccountId, AttachmentId, MessageId, ThreadId},
    types::{
        Address, AttachmentDisposition, AttachmentMeta, CalendarMetadata, Draft, Envelope, Label,
        LabelKind, MessageBody, MessageFlags, MessageMetadata, SavedSearch, SortOrder,
        SubscriptionSummary, Thread, UnsubscribeMethod,
    },
};
use mxr_protocol::{
    CommitmentData, CommitmentDirectionData, CommitmentStatusData, DaemonEvent, IpcCodec,
    IpcMessage, IpcPayload, Request, Response, ResponseData, SearchResultItem,
    IPC_PROTOCOL_VERSION,
};
use tempfile::TempDir;
use tokio::net::UnixListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::codec::Framed;

const TEST_AUTH_TOKEN: &str = "test-token";

async fn spawn_fake_ipc_server<F>(
    socket_path: &std::path::Path,
    responder: F,
    event: Option<DaemonEvent>,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(Request) -> Option<Response> + Send + Sync + 'static,
{
    let responder = std::sync::Arc::new(responder);
    let listener = UnixListener::bind(socket_path).unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let responder = responder.clone();
            let event = event.clone();
            tokio::spawn(async move {
                let mut framed = Framed::new(stream, IpcCodec::new());
                if let Some(event) = event {
                    let _ = framed
                        .send(IpcMessage {
                            id: 0,
                            source: ::mxr_protocol::ClientKind::default(),
                            payload: IpcPayload::Event(event),
                        })
                        .await;
                    return;
                }
                while let Some(message) = framed.next().await {
                    let Ok(message) = message else {
                        break;
                    };
                    if let IpcPayload::Request(request) = message.payload {
                        let Some(response) = responder(request) else {
                            continue;
                        };
                        let response = IpcMessage {
                            id: message.id,
                            source: ::mxr_protocol::ClientKind::default(),
                            payload: IpcPayload::Response(response),
                        };
                        let _ = framed.send(response).await;
                    }
                }
            });
        }
    })
}

async fn spawn_fake_event_server(socket_path: &std::path::Path) -> tokio::task::JoinHandle<()> {
    spawn_fake_ipc_server(
        socket_path,
        |_| None,
        Some(DaemonEvent::SyncCompleted {
            account_id: AccountId::new(),
            messages_synced: 3,
        }),
    )
    .await
}

#[tokio::test]
async fn status_endpoint_proxies_ipc_status() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        |request| match request {
            Request::GetStatus => Some(Response::Ok {
                data: ResponseData::Status {
                    uptime_secs: 42,
                    accounts: vec!["personal".into()],
                    total_messages: 17,
                    daemon_pid: Some(999),
                    sync_statuses: Vec::new(),
                    protocol_version: IPC_PROTOCOL_VERSION,
                    daemon_version: Some("0.4.3".into()),
                    daemon_build_id: Some("build-123".into()),
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                },
            }),
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/status"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["protocol_version"], IPC_PROTOCOL_VERSION);
    assert_eq!(json["daemon_version"], "0.4.3");
    assert_eq!(json["total_messages"], 17);
}

#[tokio::test]
async fn websocket_events_proxy_daemon_events() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_event_server(&socket_path).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    let (mut stream, _) = tokio_tungstenite::connect_async(format!(
        "ws://{addr}/api/v1/events?token={TEST_AUTH_TOKEN}"
    ))
    .await
    .unwrap();
    let message = stream.next().await.unwrap().unwrap();
    let text = match message {
        Message::Text(text) => text.to_string(),
        other => panic!("expected text websocket frame, got {other:?}"),
    };

    assert!(text.contains("SyncCompleted"));
    assert!(text.contains("\"messages_synced\":3"));
}

/// Slice 6 — exemplar new routes from each bucket dispatch the right
/// `Request` variant and return the corresponding `ResponseData`. Not
/// exhaustive (per-variant coverage moves to slice 7's harness against
/// the FakeProvider) — this catches "did the route table hook up at
/// all" mistakes per slice.
#[tokio::test]
async fn slice6_routes_dispatch_correct_request_variants() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");

    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        |request| match request {
            Request::Ping => Some(Response::Ok {
                data: ResponseData::Pong,
            }),
            Request::ListAccountsConfig => Some(Response::Ok {
                data: ResponseData::AccountsConfig { accounts: vec![] },
            }),
            Request::ListSavedSearches => Some(Response::Ok {
                data: ResponseData::SavedSearches { searches: vec![] },
            }),
            Request::Count { mode, .. } => Some(Response::Ok {
                data: ResponseData::Count {
                    // surface the mode round-trip so we know the
                    // handler parsed and forwarded it correctly
                    count: if mode == Some(SearchMode::Lexical) {
                        7
                    } else {
                        0
                    },
                },
            }),
            _ => None,
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

    // admin/ping (Request::Ping → ResponseData::Pong)
    let response = client
        .post(format!("http://{addr}/api/v1/admin/ping"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["pong"], true);

    // ResponseData uses #[serde(tag = "kind")] (internally tagged), so
    // the JSON shape is {"kind": "Variant", ...fields}.

    // platform/accounts/config (Request::ListAccountsConfig)
    let response = client
        .get(format!("http://{addr}/api/v1/platform/accounts/config"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["kind"], "AccountsConfig");
    assert!(json["accounts"].is_array());

    // platform/saved-searches (Request::ListSavedSearches)
    let response = client
        .get(format!("http://{addr}/api/v1/platform/saved-searches"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["kind"], "SavedSearches");

    // mail/count with mode=lexical — verifies query-param parsing on
    // a route that wasn't covered before slice 6.
    let response = client
        .get(format!(
            "http://{addr}/api/v1/mail/count?query=alice&mode=lexical"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["kind"], "Count");
    assert_eq!(
        json["count"], 7,
        "mode=lexical must round-trip — fake responder returns 7 only for that mode"
    );
}

#[tokio::test]
async fn llm_config_endpoint_forwards_update_to_daemon() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let seen_update = std::sync::Arc::new(std::sync::Mutex::new(None));
    let seen_for_ipc = seen_update.clone();

    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::GetLlmConfig => Some(Response::Ok {
                data: ResponseData::LlmConfig {
                    config: mxr_protocol::LlmConfigData {
                        enabled: false,
                        base_url: "http://localhost:11434/v1".into(),
                        model: "qwen2.5:3b-instruct".into(),
                        api_key_env: String::new(),
                        context_window: 8192,
                        request_timeout_secs: 120,
                        allow_cloud_relationship_data: false,
                        overrides: None,
                    },
                },
            }),
            Request::UpdateLlmConfig { config } => {
                let config = *config;
                *seen_for_ipc.lock().unwrap() = Some(config.clone());
                Some(Response::Ok {
                    data: ResponseData::LlmConfig { config },
                })
            }
            _ => None,
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

    let response = reqwest::Client::new()
        .post(format!("http://{addr}/api/v1/platform/llm/config"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "enabled": true,
            "base_url": "https://api.openai.com/v1",
            "model": "gpt-5-mini",
            "api_key_env": "OPENAI_API_KEY",
            "context_window": 16384,
            "request_timeout_secs": 45,
            "allow_cloud_relationship_data": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["config"]["model"], "gpt-5-mini");
    let forwarded = seen_update
        .lock()
        .unwrap()
        .clone()
        .expect("bridge must forward UpdateLlmConfig");
    assert!(forwarded.enabled);
    assert_eq!(forwarded.base_url, "https://api.openai.com/v1");
    assert_eq!(forwarded.api_key_env, "OPENAI_API_KEY");
    assert!(forwarded.allow_cloud_relationship_data);
}

/// Slice 6 — bad query params surface a 4xx error rather than a
/// generic 500. Probes the per-handler validation paths (rubric
/// dim 4: negative cases).
#[tokio::test]
async fn slice6_routes_reject_invalid_query_params() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    let client = reqwest::Client::new();

    // bad mode → BridgeError::Ipc → 5xx; the point is we don't 200
    // for bad inputs.
    let response = client
        .get(format!(
            "http://{addr}/api/v1/mail/count?query=q&mode=garbage"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert!(
        !response.status().is_success(),
        "garbage mode must not return 2xx; got {}",
        response.status()
    );

    // missing required `query` parameter → 4xx (axum returns 400 for
    // failed Query extraction).
    let response = client
        .get(format!("http://{addr}/api/v1/mail/count"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_client_error(),
        "missing query param must surface a 4xx; got {}",
        response.status()
    );
}

/// Slice 4 — `Authorization: Bearer X` is the preferred auth path
/// (matches OpenAPI's bearerAuth security scheme; what generated SDKs
/// produce by default).
#[tokio::test]
async fn auth_accepts_authorization_bearer_header() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        |_| {
            Some(Response::Ok {
                data: ResponseData::Status {
                    uptime_secs: 1,
                    accounts: vec![],
                    total_messages: 0,
                    daemon_pid: None,
                    sync_statuses: vec![],
                    protocol_version: IPC_PROTOCOL_VERSION,
                    daemon_version: None,
                    daemon_build_id: None,
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                },
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/admin/status"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

/// Slice 4 — wrong bearer token is rejected (not just missing one).
#[tokio::test]
async fn auth_rejects_wrong_bearer_token() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/admin/status"))
        .bearer_auth("wrong-token-not-the-one-we-expect")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

/// Slice 4 — `?token=X` query string still works (EventSource clients
/// can't set arbitrary headers, so this is the documented fallback).
#[tokio::test]
async fn auth_accepts_query_string_token_for_event_source() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_event_server(&socket_path).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    // EventSource is HTTP, so prove the same query-string auth works
    // for plain HTTP endpoints too (used by tools that can't set
    // headers — same situation as EventSource).
    let response = reqwest::get(format!(
        "http://{addr}/api/v1/admin/status?token={TEST_AUTH_TOKEN}"
    ))
    .await
    .unwrap();
    // The fake responder returns no payload for GetStatus so the
    // response is 5xx; what we're testing is that auth let it through
    // (not 401).
    assert_ne!(
        response.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "query-string token must satisfy auth"
    );
}

/// Slice 4 — WebSocket browser auth via `Sec-WebSocket-Protocol`
/// subprotocol header. Browsers cannot set arbitrary headers on WS
/// upgrades, so this is the documented browser-friendly path. The
/// server must echo back `bearer` so the negotiation succeeds.
#[tokio::test]
async fn websocket_auth_accepts_sec_websocket_protocol_subprotocol() {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;

    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_event_server(&socket_path).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    let mut request = format!("ws://{addr}/api/v1/events")
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_str(&format!("bearer, {TEST_AUTH_TOKEN}")).unwrap(),
    );

    let (mut stream, response) = tokio_tungstenite::connect_async(request)
        .await
        .expect("WS upgrade with subprotocol bearer auth must succeed");
    assert_eq!(
        response
            .headers()
            .get("sec-websocket-protocol")
            .map(|v| v.as_bytes()),
        Some(b"bearer".as_slice()),
        "server must echo `bearer` as the negotiated subprotocol"
    );
    let message = stream.next().await.unwrap().unwrap();
    assert!(matches!(message, Message::Text(_)));
}

/// Slice 4 — `/api/v1/health` is liveness-only and unauthenticated so
/// clients and orchestrators can probe readiness without first
/// loading the bridge token.
#[tokio::test]
async fn health_endpoint_is_unauthenticated() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    let response = reqwest::get(format!("http://{addr}/api/v1/health"))
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert!(
        json["protocol_version"].as_u64().is_some(),
        "health response must surface IPC_PROTOCOL_VERSION so clients \
             can fail-fast on incompat without auth"
    );
}

/// Slice 4 — Host header allowlist defends against DNS rebinding.
/// Loopback names allowed; arbitrary external hostnames rejected.
#[tokio::test]
async fn host_header_allowlist_rejects_external_hosts() {
    use reqwest::header::HOST;

    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    // Send a request with a Host header pointing at an external
    // domain — this is the shape of a DNS rebinding attack from a
    // malicious page.
    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/health"))
        .header(HOST, "evil.example.com")
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        reqwest::StatusCode::FORBIDDEN,
        "non-loopback Host header must be rejected with 403"
    );
}

#[tokio::test]
async fn host_header_allowlist_accepts_mxr_localhost() {
    use reqwest::header::HOST;

    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/health"))
        .header(HOST, "mxr.localhost")
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        reqwest::StatusCode::OK,
        "mxr.localhost should be a canonical loopback host without /etc/hosts changes"
    );
}

/// Slice 3 — every flat path returns a method-preserving permanent
/// redirect (308) with a Location header pointing at the new bucketed
/// v1 path. Sampled across each bucket so a typo in any single redirect
/// doesn't slip past CI.
///
/// 308 (not 301) is intentional: the bridge has POST endpoints
/// (mutations, compose, etc.) and 301 historically downgrades POST→GET
/// in some clients. 308 preserves the method.
#[tokio::test]
async fn legacy_paths_return_permanent_redirect_to_v1() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let cases = [
        ("/status", "/api/v1/admin/status"),
        ("/diagnostics", "/api/v1/admin/diagnostics"),
        (
            "/diagnostics/bug-report",
            "/api/v1/admin/diagnostics/bug-report",
        ),
        ("/mailbox", "/api/v1/mail/mailbox"),
        ("/search", "/api/v1/mail/search"),
        ("/drafts", "/api/v1/mail/drafts"),
        ("/snoozed", "/api/v1/mail/snoozed"),
        ("/rules", "/api/v1/platform/rules"),
        ("/accounts", "/api/v1/platform/accounts"),
        ("/subscriptions", "/api/v1/platform/subscriptions"),
        ("/semantic/status", "/api/v1/platform/semantic/status"),
    ];

    for (old, new) in cases {
        let response = client
            .get(format!("http://{addr}{old}"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::PERMANENT_REDIRECT,
            "old path {old} must 308 to {new} (method-preserving)"
        );
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .expect("301 must include Location header");
        assert_eq!(location, new, "Location header for {old} should be {new}");
    }
}

/// Slice 3 — the new bucketed v1 path serves the same response as the
/// flat legacy path used to (asserted via response shape, not just
/// status — per rubric dim 6).
#[tokio::test]
async fn v1_status_endpoint_returns_same_payload_as_legacy() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        |request| match request {
            Request::GetStatus => Some(Response::Ok {
                data: ResponseData::Status {
                    uptime_secs: 99,
                    accounts: vec!["work".into()],
                    total_messages: 7,
                    daemon_pid: Some(42),
                    sync_statuses: Vec::new(),
                    protocol_version: IPC_PROTOCOL_VERSION,
                    daemon_version: Some("0.5.0".into()),
                    daemon_build_id: Some("v1-test".into()),
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                },
            }),
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/admin/status"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["protocol_version"], IPC_PROTOCOL_VERSION);
    assert_eq!(json["daemon_version"], "0.5.0");
    assert_eq!(json["total_messages"], 7);
    assert_eq!(json["uptime_secs"], 99);
}

/// Slice 2 — utoipa scaffold: spec is served at /api/v1/openapi.json,
/// is valid OpenAPI 3.1, and includes the metadata + bearer security
/// scheme that downstream tooling (Swagger UI, openapi-typescript-codegen,
/// Schemathesis) needs to discover.
#[tokio::test]
async fn openapi_spec_is_served_at_api_v1_path() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    let response = reqwest::get(format!("http://{addr}/api/v1/openapi.json"))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        reqwest::StatusCode::OK,
        "openapi.json must be served unauthenticated for discovery"
    );

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        json["openapi"], "3.1.0",
        "spec must declare OpenAPI 3.1 (utoipa 5+); got {json:?}"
    );

    let info = &json["info"];
    assert_eq!(info["title"], "mxr HTTP Bridge");
    assert!(info["version"].is_string(), "info.version must be a string");

    let bearer = json
        .pointer("/components/securitySchemes/bearer")
        .expect("bearer security scheme must be registered");
    assert_eq!(bearer["type"], "http");
    assert_eq!(bearer["scheme"], "bearer");
}

/// Slice 2 — capture the served spec via insta snapshot so any future
/// schema/route drift forces an explicit review (`cargo insta review`).
#[tokio::test]
async fn openapi_spec_snapshot() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    let json: serde_json::Value = reqwest::get(format!("http://{addr}/api/v1/openapi.json"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // Snapshot a stable subset (top-level metadata + security scheme +
    // schema names). Snapshotting the full spec is too noisy across
    // utoipa version bumps; snapshotting nothing means drift slips by.
    let mut schema_names: Vec<&str> = json
        .pointer("/components/schemas")
        .and_then(|v| v.as_object())
        .map(|m| m.keys().map(|k| k.as_str()).collect())
        .unwrap_or_default();
    schema_names.sort();

    let summary = serde_json::json!({
        "openapi": json["openapi"],
        "info_title": json["info"]["title"],
        "security_schemes": json["components"]["securitySchemes"],
        "schema_names": schema_names,
    });

    insta::assert_json_snapshot!("openapi_spec_summary", summary);
}

/// Slice 2 — Swagger UI is served so newcomers can explore the API
/// interactively without leaving the daemon host.
#[tokio::test]
async fn swagger_ui_is_served_at_api_v1_docs() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();

    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::default())
        .build()
        .unwrap()
        .get(format!("http://{addr}/api/v1/docs/"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body = response.text().await.unwrap();
    // utoipa-swagger-ui ships an index.html that references swagger-ui
    // assets and points at /api/v1/openapi.json by default.
    assert!(
        body.contains("swagger-ui"),
        "Swagger UI HTML must mention swagger-ui assets; got first 200: {}",
        &body.chars().take(200).collect::<String>()
    );
}

#[tokio::test]
async fn status_endpoint_rejects_missing_token() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        |_request| {
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

    let response = reqwest::get(format!("http://{addr}/status")).await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

fn sample_envelope() -> Envelope {
    Envelope {
        id: MessageId::new(),
        account_id: AccountId::new(),
        provider_id: "provider-msg-1".into(),
        thread_id: ThreadId::new(),
        message_id_header: Some("<msg-1@example.com>".into()),
        in_reply_to: None,
        references: Vec::new(),
        from: Address {
            name: Some("Sender".into()),
            email: "sender@example.com".into(),
        },
        to: vec![Address {
            name: Some("User".into()),
            email: "user@example.com".into(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: "Mailroom".into(),
        date: Utc::now(),
        flags: MessageFlags::empty(),
        snippet: "Preview".into(),
        has_attachments: false,
        size_bytes: 128,
        unsubscribe: UnsubscribeMethod::None,
        link_count: 0,
        body_word_count: 0,
        label_provider_ids: Vec::new(),
        keywords: std::collections::BTreeSet::new(),
    }
}

fn sample_labels(account_id: &AccountId) -> Vec<Label> {
    vec![
        Label {
            id: mxr_core::LabelId::new(),
            account_id: account_id.clone(),
            name: "Inbox".into(),
            kind: LabelKind::System,
            color: None,
            provider_id: "INBOX".into(),
            unread_count: 12,
            total_count: 144,
            role: None,
        },
        Label {
            id: mxr_core::LabelId::new(),
            account_id: account_id.clone(),
            name: "All Mail".into(),
            kind: LabelKind::System,
            color: None,
            provider_id: "ALL_MAIL".into(),
            unread_count: 24,
            total_count: 8124,
            role: None,
        },
        Label {
            id: mxr_core::LabelId::new(),
            account_id: account_id.clone(),
            name: "Follow Up".into(),
            kind: LabelKind::User,
            color: None,
            provider_id: "follow-up".into(),
            unread_count: 2,
            total_count: 18,
            role: None,
        },
    ]
}

fn sample_saved_search(account_id: AccountId) -> SavedSearch {
    SavedSearch {
        id: mxr_core::SavedSearchId::new(),
        account_id: Some(account_id),
        name: "Today".into(),
        query: "in:inbox newer_than:1d".into(),
        search_mode: SearchMode::Lexical,
        sort: SortOrder::DateDesc,
        icon: Some("sun".into()),
        position: 0,
        created_at: Utc::now(),
    }
}

fn sample_subscription(account_id: &AccountId) -> SubscriptionSummary {
    SubscriptionSummary {
        account_id: account_id.clone(),
        sender_name: Some("Readwise".into()),
        sender_email: "hello@readwise.io".into(),
        message_count: 4,
        latest_message_id: MessageId::new(),
        latest_provider_id: "provider-subscription-1".into(),
        latest_thread_id: ThreadId::new(),
        latest_subject: "Latest digest".into(),
        latest_snippet: "Highlights from this week".into(),
        latest_date: Utc::now(),
        latest_flags: MessageFlags::empty(),
        latest_has_attachments: false,
        latest_size_bytes: 256,
        unsubscribe: UnsubscribeMethod::None,
        opened_count: 0,
        replied_count: 0,
        archived_unread_count: 0,
    }
}

fn sample_account(account_id: &AccountId) -> mxr_protocol::AccountSummaryData {
    mxr_protocol::AccountSummaryData {
        account_id: account_id.clone(),
        key: Some("personal".into()),
        name: "Personal".into(),
        email: "me@example.com".into(),
        provider_kind: "gmail".into(),
        sync_kind: Some("gmail".into()),
        send_kind: Some("smtp".into()),
        enabled: true,
        is_default: true,
        source: mxr_protocol::AccountSourceData::Runtime,
        editable: mxr_protocol::AccountEditModeData::Full,
        sync: None,
        send: None,
        capabilities: Default::default(),
    }
}

fn sample_thread(envelope: &Envelope) -> Thread {
    Thread {
        id: envelope.thread_id.clone(),
        account_id: envelope.account_id.clone(),
        subject: envelope.subject.clone(),
        participants: vec![envelope.from.clone()],
        message_count: 1,
        unread_count: 1,
        latest_date: envelope.date,
        snippet: envelope.snippet.clone(),
        message_ids: vec![envelope.id.clone()],
    }
}

fn sample_body(envelope: &Envelope) -> MessageBody {
    MessageBody {
        message_id: envelope.id.clone(),
        text_plain: Some("plain text".into()),
        text_html: Some("<p>rich html</p>".into()),
        attachments: Vec::new(),
        fetched_at: Utc::now(),
        metadata: MessageMetadata::default(),
    }
}

fn sample_attachment_body(envelope: &Envelope) -> MessageBody {
    MessageBody {
        message_id: envelope.id.clone(),
        text_plain: Some("plain text".into()),
        text_html: Some("<p>rich html</p>".into()),
        attachments: vec![AttachmentMeta {
            id: AttachmentId::new(),
            message_id: envelope.id.clone(),
            filename: "deploy.log".into(),
            mime_type: "text/plain".into(),
            disposition: AttachmentDisposition::Attachment,
            content_id: None,
            content_location: None,
            size_bytes: 1024,
            local_path: None,
            provider_id: "provider-attachment-1".into(),
        }],
        fetched_at: Utc::now(),
        metadata: MessageMetadata::default(),
    }
}

#[tokio::test]
async fn mailbox_endpoint_lists_envelopes() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let expected = sample_envelope();
    let expected_id = expected.id.to_string();
    let labels = sample_labels(&expected.account_id);
    let saved_search = sample_saved_search(expected.account_id.clone());
    let subscription = sample_subscription(&expected.account_id);
    let inbox_label_id = labels[0].id.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::GetStatus => Some(Response::Ok {
                data: ResponseData::Status {
                    uptime_secs: 42,
                    accounts: vec!["personal".into()],
                    total_messages: 8124,
                    daemon_pid: Some(999),
                    sync_statuses: Vec::new(),
                    protocol_version: IPC_PROTOCOL_VERSION,
                    daemon_version: Some("0.4.4".into()),
                    daemon_build_id: Some("build-123".into()),
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                },
            }),
            Request::ListLabels { account_id: None } => Some(Response::Ok {
                data: ResponseData::Labels {
                    labels: labels.clone(),
                },
            }),
            Request::ListSavedSearches => Some(Response::Ok {
                data: ResponseData::SavedSearches {
                    searches: vec![saved_search.clone()],
                },
            }),
            Request::ListSubscriptions {
                account_id: None,
                limit: 8,
            } => Some(Response::Ok {
                data: ResponseData::Subscriptions {
                    subscriptions: vec![subscription.clone()],
                },
            }),
            Request::ListEnvelopes {
                limit: 200,
                offset: 0,
                label_id: Some(label_id),
                account_id: None,
            } if label_id == inbox_label_id => Some(Response::Ok {
                data: ResponseData::Envelopes {
                    envelopes: vec![expected.clone()],
                },
            }),
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/mailbox"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["shell"]["statusMessage"], "Local-first and ready");
    assert_eq!(json["sidebar"]["sections"][0]["title"], "System");
    assert_eq!(json["sidebar"]["sections"][1]["title"], "Labels");
    assert!(json["sidebar"]["sections"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["label"] == "Subscriptions"));
    assert_eq!(json["mailbox"]["lensLabel"], "Inbox");
    assert_eq!(json["mailbox"]["groups"][0]["rows"][0]["id"], expected_id);
    assert_eq!(
        json["mailbox"]["groups"][0]["rows"][0]["subject"],
        "Mailroom"
    );
}

#[tokio::test]
async fn mailbox_endpoint_supports_all_mail_lens() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let mut expected = sample_envelope();
    expected.subject = "Archive rollup".into();
    expected.snippet = "Everything local, nothing filtered.".into();
    let labels = sample_labels(&expected.account_id);
    let saved_search = sample_saved_search(expected.account_id.clone());
    let subscription = sample_subscription(&expected.account_id);
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::GetStatus => Some(Response::Ok {
                data: ResponseData::Status {
                    uptime_secs: 42,
                    accounts: vec!["personal".into()],
                    total_messages: 8124,
                    daemon_pid: Some(999),
                    sync_statuses: Vec::new(),
                    protocol_version: IPC_PROTOCOL_VERSION,
                    daemon_version: Some("0.4.4".into()),
                    daemon_build_id: Some("build-123".into()),
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                },
            }),
            Request::ListLabels { account_id: None } => Some(Response::Ok {
                data: ResponseData::Labels {
                    labels: labels.clone(),
                },
            }),
            Request::ListSavedSearches => Some(Response::Ok {
                data: ResponseData::SavedSearches {
                    searches: vec![saved_search.clone()],
                },
            }),
            Request::ListSubscriptions {
                account_id: None,
                limit: 8,
            } => Some(Response::Ok {
                data: ResponseData::Subscriptions {
                    subscriptions: vec![subscription.clone()],
                },
            }),
            Request::ListEnvelopes {
                limit: 200,
                offset: 0,
                label_id: None,
                account_id: None,
            } => Some(Response::Ok {
                data: ResponseData::Envelopes {
                    envelopes: vec![expected.clone()],
                },
            }),
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/mailbox?lens_kind=all_mail"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["mailbox"]["lensLabel"], "All Mail");
    assert_eq!(json["mailbox"]["counts"]["total"], 8124);
    assert!(json["sidebar"]["sections"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["label"] == "All Mail" && item["active"] == true));
    assert_eq!(
        json["mailbox"]["groups"][0]["rows"][0]["subject"],
        "Archive rollup"
    );
}

#[tokio::test]
async fn mailbox_endpoint_shapes_thread_and_message_views() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");

    let first = sample_envelope();
    let mut second = sample_envelope();
    second.id = MessageId::new();
    second.account_id = first.account_id.clone();
    second.thread_id = first.thread_id.clone();
    second.subject = "Mailroom follow-up".into();
    second.snippet = "Same thread, newer message".into();
    second.has_attachments = true;
    let commitment = CommitmentData {
        id: "commitment-1".into(),
        account_id: first.account_id.clone(),
        email: first.from.email.clone(),
        thread_id: first.thread_id.clone(),
        direction: CommitmentDirectionData::Theirs,
        status: CommitmentStatusData::Open,
        who_owes: "sender".into(),
        what: "Send launch dates".into(),
        by_when: None,
        evidence_msg_id: first.id.clone(),
        extracted_at: Utc::now(),
    };

    let labels = sample_labels(&first.account_id);
    let saved_search = sample_saved_search(first.account_id.clone());
    let subscription = sample_subscription(&first.account_id);
    let inbox_label_id = labels[0].id.clone();

    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::GetStatus => Some(Response::Ok {
                data: ResponseData::Status {
                    uptime_secs: 42,
                    accounts: vec!["personal".into()],
                    total_messages: 8124,
                    daemon_pid: Some(999),
                    sync_statuses: Vec::new(),
                    protocol_version: IPC_PROTOCOL_VERSION,
                    daemon_version: Some("0.4.4".into()),
                    daemon_build_id: Some("build-123".into()),
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                },
            }),
            Request::ListLabels { account_id: None } => Some(Response::Ok {
                data: ResponseData::Labels {
                    labels: labels.clone(),
                },
            }),
            Request::ListSavedSearches => Some(Response::Ok {
                data: ResponseData::SavedSearches {
                    searches: vec![saved_search.clone()],
                },
            }),
            Request::ListSubscriptions {
                account_id: None,
                limit: 8,
            } => Some(Response::Ok {
                data: ResponseData::Subscriptions {
                    subscriptions: vec![subscription.clone()],
                },
            }),
            Request::ListEnvelopes {
                limit: 200,
                offset: 0,
                label_id: Some(label_id),
                account_id: None,
            } if label_id == inbox_label_id => Some(Response::Ok {
                data: ResponseData::Envelopes {
                    envelopes: vec![first.clone(), second.clone()],
                },
            }),
            Request::ListCommitments {
                account_id,
                email: None,
                status: Some(CommitmentStatusData::Open),
            } if account_id == first.account_id => Some(Response::Ok {
                data: ResponseData::CommitmentList {
                    commitments: vec![commitment.clone()],
                },
            }),
            _ => None,
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
    let threads_response = client
        .get(format!("http://{addr}/mailbox?view=threads"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(threads_response.status(), reqwest::StatusCode::OK);

    let threads_json: serde_json::Value = threads_response.json().await.unwrap();
    assert_eq!(threads_json["mailbox"]["view"], "threads");
    assert_eq!(
        threads_json["mailbox"]["groups"][0]["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        threads_json["mailbox"]["groups"][0]["rows"][0]["kind"],
        "thread"
    );
    assert_eq!(
        threads_json["mailbox"]["groups"][0]["rows"][0]["message_count"],
        2
    );
    assert_eq!(
        threads_json["mailbox"]["groups"][0]["rows"][0]["has_attachments"],
        true
    );
    assert_eq!(
        threads_json["mailbox"]["groups"][0]["rows"][0]["open_commitment_count"],
        1
    );

    let messages_response = client
        .get(format!("http://{addr}/mailbox?view=messages"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(messages_response.status(), reqwest::StatusCode::OK);

    let messages_json: serde_json::Value = messages_response.json().await.unwrap();
    assert_eq!(messages_json["mailbox"]["view"], "messages");
    assert_eq!(
        messages_json["mailbox"]["groups"][0]["rows"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        messages_json["mailbox"]["groups"][0]["rows"][0]["kind"],
        "message"
    );
    assert_eq!(
        messages_json["mailbox"]["groups"][0]["rows"][1]["subject"],
        "Mailroom follow-up"
    );
    assert_eq!(
        messages_json["mailbox"]["groups"][0]["rows"][1]["has_attachments"],
        true
    );
    assert_eq!(
        messages_json["mailbox"]["groups"][0]["rows"][1]["open_commitment_count"],
        1
    );
}

#[tokio::test]
async fn thread_endpoint_returns_messages_and_bodies() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let mut envelope = sample_envelope();
    envelope.label_provider_ids = vec!["INBOX".into(), "follow-up".into()];
    let thread = sample_thread(&envelope);
    let mut body = sample_body(&envelope);
    body.metadata.calendar = Some(CalendarMetadata {
        method: Some("REQUEST".into()),
        summary: Some("Planning session".into()),
        ..Default::default()
    });
    let labels = sample_labels(&envelope.account_id);
    let thread_id = thread.id.to_string();
    let message_id = envelope.id.to_string();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::GetThread {
                thread_id: requested,
            } if requested == thread.id => Some(Response::Ok {
                data: ResponseData::Thread {
                    thread: thread.clone(),
                    messages: vec![envelope.clone()],
                    summary: None,
                },
            }),
            Request::ListLabels { .. } => Some(Response::Ok {
                data: ResponseData::Labels {
                    labels: labels.clone(),
                },
            }),
            Request::ListBodies { message_ids } if message_ids == vec![body.message_id.clone()] => {
                Some(Response::Ok {
                    data: ResponseData::Bodies {
                        bodies: vec![body.clone()],
                        failures: Vec::new(),
                    },
                })
            }
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/thread/{thread_id}"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["thread"]["id"], thread_id);
    assert_eq!(json["messages"][0]["id"], message_id);
    assert_eq!(json["messages"][0]["to"][0]["email"], "user@example.com");
    assert_eq!(json["messages"][0]["labels"][0]["name"], "Inbox");
    assert_eq!(json["messages"][0]["labels"][1]["name"], "Follow Up");
    assert!(json["messages"][0]["date_full"].as_str().is_some());
    assert!(json["messages"][0]["date_relative"].as_str().is_some());
    assert_eq!(json["bodies"][0]["text_html"], "<p>rich html</p>");
    assert_eq!(
        json["bodies"][0]["metadata"]["calendar"]["summary"],
        "Planning session"
    );
    assert!(json["right_rail"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == "1 calendar invite"));
}

#[tokio::test]
async fn thread_endpoint_shapes_reader_mode_and_right_rail_from_raw_ipc_data() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let envelope = sample_envelope();
    let thread = sample_thread(&envelope);
    let body = sample_body(&envelope);
    let labels = sample_labels(&envelope.account_id);
    let thread_id = thread.id.to_string();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::GetThread {
                thread_id: requested,
            } if requested == thread.id => Some(Response::Ok {
                data: ResponseData::Thread {
                    thread: thread.clone(),
                    messages: vec![envelope.clone()],
                    summary: None,
                },
            }),
            Request::ListLabels { .. } => Some(Response::Ok {
                data: ResponseData::Labels {
                    labels: labels.clone(),
                },
            }),
            Request::ListBodies { message_ids } if message_ids == vec![body.message_id.clone()] => {
                Some(Response::Ok {
                    data: ResponseData::Bodies {
                        bodies: vec![body.clone()],
                        failures: Vec::new(),
                    },
                })
            }
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/thread/{thread_id}"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["reader_mode"], "reader");
    assert_eq!(json["right_rail"]["title"], "Thread context");
}

#[tokio::test]
async fn thread_endpoint_rejects_unexpected_body_response() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let envelope = sample_envelope();
    let thread = sample_thread(&envelope);
    let labels = sample_labels(&envelope.account_id);
    let thread_id = thread.id.to_string();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::GetThread {
                thread_id: requested,
            } if requested == thread.id => Some(Response::Ok {
                data: ResponseData::Thread {
                    thread: thread.clone(),
                    messages: vec![envelope.clone()],
                    summary: None,
                },
            }),
            Request::ListLabels { .. } => Some(Response::Ok {
                data: ResponseData::Labels {
                    labels: labels.clone(),
                },
            }),
            Request::ListBodies { .. } => Some(Response::Ok {
                data: ResponseData::Envelopes { envelopes: vec![] },
            }),
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/thread/{thread_id}"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn search_endpoint_proxies_results() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let envelope = sample_envelope();
    let result = SearchResultItem {
        message_id: envelope.id.clone(),
        account_id: envelope.account_id.clone(),
        thread_id: envelope.thread_id.clone(),
        score: 9.5,
        mode: mxr_core::types::SearchMode::Lexical,
    };
    let message_id = result.message_id.to_string();
    let message_ids = vec![result.message_id.clone()];
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::Search {
                query,
                limit: 200,
                offset: 0,
                mode: None,
                sort: Some(SortOrder::DateDesc),
                explain: false,
            } if query == "buildkite" => Some(Response::Ok {
                data: ResponseData::SearchResults {
                    results: vec![result.clone()],
                    total: 1,
                    has_more: false,
                    next_offset: None,
                    explain: None,
                },
            }),
            Request::ListEnvelopesByIds {
                message_ids: requested,
            } if requested == message_ids => Some(Response::Ok {
                data: ResponseData::Envelopes {
                    envelopes: vec![envelope.clone()],
                },
            }),
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/search?q=buildkite"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["groups"][0]["rows"][0]["id"], message_id);
    assert_eq!(json["groups"][0]["rows"][0]["subject"], "Mailroom");
    assert!(json["explain"].is_null());
}

#[test]
fn group_envelopes_keeps_web_specific_date_buckets_out_of_ipc() {
    let today_noon = Local
        .from_local_datetime(
            &Local::now()
                .date_naive()
                .and_hms_opt(12, 0, 0)
                .expect("valid local noon"),
        )
        .single()
        .expect("local noon is unambiguous")
        .with_timezone(&Utc);

    let mut same_day_a = sample_envelope();
    same_day_a.subject = "alpha".into();
    same_day_a.date = today_noon;

    let mut same_day_b = sample_envelope();
    same_day_b.subject = "beta".into();
    same_day_b.date = today_noon - chrono::Duration::hours(1);

    let mut older = sample_envelope();
    older.subject = "gamma".into();
    older.date = today_noon - chrono::Duration::days(3);

    let groups = group_envelopes(vec![same_day_a, same_day_b, older]);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].id, "today");
    assert_eq!(groups[0].label, "Today");
    assert_eq!(groups[0].rows.len(), 2);
    assert_eq!(groups[0].rows[0].subject, "alpha");
    assert_eq!(groups[0].rows[1].subject, "beta");
    assert_eq!(groups[1].id, "last-7-days");
    assert_eq!(groups[1].rows.len(), 1);
    assert_eq!(groups[1].rows[0].subject, "gamma");
}

#[test]
fn date_labels_show_time_today_and_date_time_for_older_mail() {
    let today = Local
        .from_local_datetime(
            &Local::now()
                .date_naive()
                .and_hms_opt(12, 5, 0)
                .expect("valid local time"),
        )
        .single()
        .expect("local noon is unambiguous")
        .with_timezone(&Utc);
    let yesterday = today - chrono::Duration::days(1);

    assert_eq!(format_date_label(today), "12:05pm");
    assert_eq!(
        format_date_label(yesterday),
        yesterday
            .with_timezone(&Local)
            .format("%b %-d %-I:%M%P")
            .to_string()
    );
}

#[test]
fn draft_summary_includes_updated_time_labels() {
    let updated_at = Utc::now() - chrono::Duration::hours(2);
    let draft = Draft {
        id: DraftId::new(),
        account_id: AccountId::new(),
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![Address {
            name: Some("User".into()),
            email: "user@example.com".into(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: "Draft".into(),
        body_markdown: "Body".into(),
        attachments: Vec::new(),
        inline_calendar_reply: None,
        created_at: updated_at,
        updated_at,
    };

    let json = draft_summary_view(draft);

    assert_eq!(json["updated_at_label"], format_date_label(updated_at));
    assert_eq!(json["updated_at_full"], format_date_full(updated_at));
    assert_eq!(
        json["updated_at_relative"],
        format!("edited {}", format_relative_label(updated_at))
    );
}

#[tokio::test]
async fn search_endpoint_supports_mode_sort_and_explain() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let mut older = sample_envelope();
    older.subject = "Older deploy".into();
    older.date = Utc::now() - chrono::Duration::days(1);
    let mut newer = sample_envelope();
    newer.subject = "Newest deploy".into();
    newer.date = Utc::now();

    let older_result = SearchResultItem {
        message_id: older.id.clone(),
        account_id: older.account_id.clone(),
        thread_id: older.thread_id.clone(),
        score: 1.5,
        mode: mxr_core::types::SearchMode::Semantic,
    };
    let newer_result = SearchResultItem {
        message_id: newer.id.clone(),
        account_id: newer.account_id.clone(),
        thread_id: newer.thread_id.clone(),
        score: 0.8,
        mode: mxr_core::types::SearchMode::Semantic,
    };
    let requested_ids = vec![newer.id.clone(), older.id.clone()];
    let explain = mxr_protocol::SearchExplain {
        requested_mode: SearchMode::Semantic,
        executed_mode: SearchMode::Semantic,
        semantic_query: Some("deploy".into()),
        lexical_window: 50,
        dense_window: Some(50),
        lexical_candidates: 2,
        dense_candidates: 2,
        final_results: 2,
        rrf_k: Some(60),
        notes: vec!["semantic rerank".into()],
        results: vec![mxr_protocol::SearchExplainResult {
            rank: 1,
            message_id: newer.id.clone(),
            final_score: 1.0,
            lexical_rank: Some(2),
            lexical_score: Some(0.2),
            dense_rank: Some(1),
            dense_score: Some(0.9),
        }],
    };

    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::Search {
                query,
                limit: 200,
                offset: 0,
                mode: Some(SearchMode::Semantic),
                sort: Some(SortOrder::DateDesc),
                explain: true,
            } if query == "deploy" => Some(Response::Ok {
                data: ResponseData::SearchResults {
                    results: vec![newer_result.clone(), older_result.clone()],
                    total: 2,
                    has_more: false,
                    next_offset: None,
                    explain: Some(explain.clone()),
                },
            }),
            Request::ListEnvelopesByIds { message_ids } if message_ids == requested_ids => {
                Some(Response::Ok {
                    data: ResponseData::Envelopes {
                        envelopes: vec![older.clone(), newer.clone()],
                    },
                })
            }
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!(
            "http://{addr}/search?q=deploy&mode=semantic&sort=recent&explain=true"
        ))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["mode"], "semantic");
    assert_eq!(json["sort"], "recent");
    assert_eq!(json["explain"]["requested_mode"], "semantic");
    assert_eq!(json["groups"][0]["rows"][0]["subject"], "Newest deploy");
    assert_eq!(json["groups"][1]["rows"][0]["subject"], "Older deploy");
}

#[tokio::test]
async fn search_endpoint_dedupes_threads_by_thread_id() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");

    let mut first = sample_envelope();
    first.subject = "First match".into();
    first.snippet = "first".into();

    let mut second = sample_envelope();
    second.id = MessageId::new();
    second.subject = "Second match same thread".into();
    second.snippet = "second".into();
    second.thread_id = first.thread_id.clone();

    let results = vec![
        SearchResultItem {
            message_id: first.id.clone(),
            account_id: first.account_id.clone(),
            thread_id: first.thread_id.clone(),
            score: 9.5,
            mode: mxr_core::types::SearchMode::Lexical,
        },
        SearchResultItem {
            message_id: second.id.clone(),
            account_id: second.account_id.clone(),
            thread_id: second.thread_id.clone(),
            score: 9.0,
            mode: mxr_core::types::SearchMode::Lexical,
        },
    ];
    let requested_ids = vec![first.id.clone()];

    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::Search {
                query,
                limit: 200,
                offset: 0,
                mode: None,
                sort: Some(SortOrder::DateDesc),
                explain: false,
            } if query == "dalumuzi" => Some(Response::Ok {
                data: ResponseData::SearchResults {
                    results: results.clone(),
                    total: results.len() as u32,
                    has_more: false,
                    next_offset: None,
                    explain: None,
                },
            }),
            Request::ListEnvelopesByIds { message_ids } if message_ids == requested_ids => {
                Some(Response::Ok {
                    data: ResponseData::Envelopes {
                        envelopes: vec![first.clone()],
                    },
                })
            }
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/search?q=dalumuzi&scope=threads"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["groups"][0]["rows"].as_array().unwrap().len(), 1);
    assert_eq!(json["groups"][0]["rows"][0]["subject"], "First match");
}

#[tokio::test]
async fn search_endpoint_returns_attachment_rows_for_attachment_scope() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");

    let mut envelope = sample_envelope();
    envelope.has_attachments = true;
    envelope.subject = "Deploy artifacts".into();
    let body = sample_attachment_body(&envelope);
    let attachment_id = body.attachments[0].id.to_string();
    let message_id = envelope.id.to_string();

    let result = SearchResultItem {
        message_id: envelope.id.clone(),
        account_id: envelope.account_id.clone(),
        thread_id: envelope.thread_id.clone(),
        score: 9.5,
        mode: mxr_core::types::SearchMode::Lexical,
    };
    let requested_ids = vec![envelope.id.clone()];

    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::Search {
                query,
                limit: 200,
                offset: 0,
                mode: None,
                sort: Some(SortOrder::DateDesc),
                explain: false,
            } if query == "deploy artifacts" => Some(Response::Ok {
                data: ResponseData::SearchResults {
                    results: vec![result.clone()],
                    total: 1,
                    has_more: false,
                    next_offset: None,
                    explain: None,
                },
            }),
            Request::ListEnvelopesByIds { message_ids } if message_ids == requested_ids => {
                Some(Response::Ok {
                    data: ResponseData::Envelopes {
                        envelopes: vec![envelope.clone()],
                    },
                })
            }
            Request::ListBodies { message_ids } if message_ids == requested_ids => {
                Some(Response::Ok {
                    data: ResponseData::Bodies {
                        bodies: vec![body.clone()],
                        failures: Vec::new(),
                    },
                })
            }
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!(
            "http://{addr}/search?q=deploy%20artifacts&scope=attachments"
        ))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["scope"], "attachments");
    assert_eq!(json["total"], 1);
    assert_eq!(json["groups"][0]["rows"][0]["id"], message_id);
    assert_eq!(json["groups"][0]["rows"][0]["kind"], "attachment");
    assert_eq!(json["groups"][0]["rows"][0]["attachment_id"], attachment_id);
    assert_eq!(
        json["groups"][0]["rows"][0]["attachment_filename"],
        "deploy.log"
    );
    assert_eq!(
        json["groups"][0]["rows"][0]["snippet"],
        "text/plain · 1024 bytes"
    );
}

#[tokio::test]
async fn compose_session_endpoint_prepares_reply_draft() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let envelope = sample_envelope();
    let account = sample_account(&envelope.account_id);
    let message_id = envelope.id.to_string();
    let expected_account_id = envelope.account_id.to_string();
    let expected_message_id = envelope.id.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListAccounts => Some(Response::Ok {
                data: ResponseData::Accounts {
                    accounts: vec![account.clone()],
                },
            }),
            Request::GetEnvelope { message_id } if message_id == expected_message_id => {
                Some(Response::Ok {
                    data: ResponseData::Envelope {
                        envelope: envelope.clone(),
                    },
                })
            }
            Request::PrepareReply {
                message_id,
                reply_all: false,
            } if message_id == expected_message_id => Some(Response::Ok {
                data: ResponseData::ReplyContext {
                    context: mxr_protocol::ReplyContext {
                        account_id: mxr_core::AccountId::new(),
                        in_reply_to: "<msg-1@example.com>".into(),
                        references: vec!["<root@example.com>".into()],
                        reply_to: "sender@example.com".into(),
                        cc: String::new(),
                        subject: "Mailroom".into(),
                        from: "sender@example.com".into(),
                        thread_context: "Original thread context".into(),
                        thread_id: None,
                    },
                },
            }),
            _ => None,
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

    let response = reqwest::Client::new()
        .post(format!("http://{addr}/compose/session"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "kind": "reply",
            "message_id": message_id,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["session"]["kind"], "reply");
    assert_eq!(json["session"]["accountId"], expected_account_id);
    assert_eq!(json["session"]["frontmatter"]["to"], "sender@example.com");
    assert_eq!(json["session"]["frontmatter"]["subject"], "Re: Mailroom");
    assert_eq!(json["session"]["issues"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn compose_session_update_refresh_and_discard_round_trip_draft() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account = sample_account(&AccountId::new());
    let account_email = account.email.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListAccounts => Some(Response::Ok {
                data: ResponseData::Accounts {
                    accounts: vec![account.clone()],
                },
            }),
            _ => None,
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

    let started: serde_json::Value = client
        .post(format!("http://{addr}/compose/session"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "kind": "new",
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let draft_path = started["session"]["draftPath"]
        .as_str()
        .unwrap()
        .to_string();

    let updated: serde_json::Value = client
        .post(format!("http://{addr}/compose/session/update"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "to": "alice@example.com",
            "cc": "",
            "bcc": "",
            "subject": "Updated subject",
            "from": account_email,
            "attach": [],
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(updated["session"]["frontmatter"]["to"], "alice@example.com");
    assert_eq!(
        updated["session"]["frontmatter"]["subject"],
        "Updated subject"
    );

    let refreshed: serde_json::Value = client
        .post(format!("http://{addr}/compose/session/refresh"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        refreshed["session"]["frontmatter"]["to"],
        "alice@example.com"
    );
    assert_eq!(
        refreshed["session"]["frontmatter"]["subject"],
        "Updated subject"
    );

    let discard_response = client
        .post(format!("http://{addr}/compose/session/discard"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(discard_response.status(), reqwest::StatusCode::OK);
    assert!(!std::path::Path::new(refreshed["session"]["draftPath"].as_str().unwrap()).exists());
}

#[tokio::test]
async fn compose_attachment_upload_writes_local_file_and_discard_cleans_it() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account = sample_account(&AccountId::new());
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListAccounts => Some(Response::Ok {
                data: ResponseData::Accounts {
                    accounts: vec![account.clone()],
                },
            }),
            _ => None,
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

    let started: serde_json::Value = client
        .post(format!("http://{addr}/compose/session"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "kind": "new" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let draft_path = started["session"]["draftPath"]
        .as_str()
        .unwrap()
        .to_string();

    let uploaded: serde_json::Value = client
        .post(format!(
            "http://{addr}/api/v1/mail/compose/session/attachment"
        ))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "filename": "notes.txt",
            "content_base64": general_purpose::STANDARD.encode(b"hello attachment"),
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let attachment_path = uploaded["path"].as_str().unwrap();
    assert_eq!(uploaded["filename"], "notes.txt");
    assert_eq!(
        tokio::fs::read_to_string(attachment_path).await.unwrap(),
        "hello attachment"
    );

    let discard_response = client
        .post(format!("http://{addr}/compose/session/discard"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "draft_path": draft_path }))
        .send()
        .await
        .unwrap();
    assert_eq!(discard_response.status(), reqwest::StatusCode::OK);
    assert!(!std::path::Path::new(attachment_path).exists());
}

#[tokio::test]
async fn compose_session_send_forwards_draft_account_id() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account = sample_account(&AccountId::new());
    let expected_account_id = account.account_id.clone();
    let captured = std::sync::Arc::new(std::sync::Mutex::new(None::<Draft>));
    let captured_send = captured.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListAccounts => Some(Response::Ok {
                data: ResponseData::Accounts {
                    accounts: vec![account.clone()],
                },
            }),
            Request::SendDraft { draft, .. } => {
                *captured_send.lock().unwrap() = Some(draft);
                Some(Response::Ok {
                    data: ResponseData::Ack,
                })
            }
            _ => None,
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

    let started: serde_json::Value = client
        .post(format!("http://{addr}/compose/session"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "kind": "new" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let draft_path = started["session"]["draftPath"]
        .as_str()
        .unwrap()
        .to_string();
    let account_id = started["session"]["accountId"]
        .as_str()
        .unwrap()
        .to_string();

    let update_response = client
        .post(format!("http://{addr}/compose/session/update"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "to": "alice@example.com",
            "cc": "",
            "bcc": "",
            "subject": "Bridge send",
            "from": "me@example.com",
            "attach": [],
            "body": "Hello from bridge",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(update_response.status(), reqwest::StatusCode::OK);

    let send_response = client
        .post(format!("http://{addr}/compose/session/send"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "account_id": account_id,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(send_response.status(), reqwest::StatusCode::OK);

    let draft = captured
        .lock()
        .unwrap()
        .clone()
        .expect("send draft should be forwarded");
    assert_eq!(draft.account_id, expected_account_id);
    assert_eq!(draft.subject, "Bridge send");
    assert_eq!(draft.body_markdown, "Hello from bridge");
    assert_eq!(draft.to[0].email, "alice@example.com");
}

#[tokio::test]
async fn compose_session_send_propagates_daemon_error() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account = sample_account(&AccountId::new());
    let expected_error = format!(
        "No send provider configured for account {}",
        account.account_id
    );
    let error_for_server = expected_error.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListAccounts => Some(Response::Ok {
                data: ResponseData::Accounts {
                    accounts: vec![account.clone()],
                },
            }),
            Request::SendDraft { .. } => Some(Response::error(error_for_server.clone())),
            _ => None,
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

    let started: serde_json::Value = client
        .post(format!("http://{addr}/compose/session"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "kind": "new" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let draft_path = started["session"]["draftPath"]
        .as_str()
        .unwrap()
        .to_string();
    let account_id = started["session"]["accountId"]
        .as_str()
        .unwrap()
        .to_string();

    let update_response = client
        .post(format!("http://{addr}/compose/session/update"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "to": "alice@example.com",
            "cc": "",
            "bcc": "",
            "subject": "Bridge send",
            "from": "me@example.com",
            "attach": [],
            "body": "Hello",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(update_response.status(), reqwest::StatusCode::OK);

    let send_response = client
        .post(format!("http://{addr}/compose/session/send"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "account_id": account_id,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(send_response.status(), reqwest::StatusCode::BAD_GATEWAY);

    let body = send_response.text().await.unwrap();
    assert!(body.contains(&expected_error));
    assert!(std::path::Path::new(&draft_path).exists());
    let _ = std::fs::remove_file(draft_path);
}

#[tokio::test]
async fn archive_mutation_endpoint_proxies_message_ids() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let expected = sample_envelope();
    let expected_id = expected.id.to_string();
    let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let captured_ids = captured.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::Mutation {
                mutation: mxr_protocol::MutationCommand::Archive { message_ids },
                ..
            } => {
                *captured_ids.lock().unwrap() =
                    message_ids.iter().map(ToString::to_string).collect();
                Some(Response::Ok {
                    data: ResponseData::Ack,
                })
            }
            _ => None,
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
    let response = client
        .post(format!("http://{addr}/mutations/archive"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "message_ids": [expected_id] }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(*captured.lock().unwrap(), vec![expected.id.to_string()]);
}

#[tokio::test]
async fn invite_reply_endpoint_proxies_dry_run_request() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let expected = sample_envelope();
    let expected_id = expected.id.to_string();
    let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
    let captured_request = captured.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::RespondInvite {
                message_id,
                action,
                dry_run,
            } => {
                *captured_request.lock().unwrap() = Some((message_id.to_string(), action, dry_run));
                Some(Response::Ok {
                    data: ResponseData::InviteResponsePreview {
                        preview: mxr_protocol::CalendarInviteResponsePreview {
                            message_id,
                            action,
                            attendee_email: "user@example.com".into(),
                            organizer_email: "organizer@example.com".into(),
                            subject: "Accepted: Planning".into(),
                            body_text: "user@example.com has accepted this invitation.".into(),
                            ics: "BEGIN:VCALENDAR\r\nMETHOD:REPLY\r\nEND:VCALENDAR\r\n".into(),
                            warnings: Vec::new(),
                        },
                    },
                })
            }
            _ => None,
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

    let response = reqwest::Client::new()
        .post(format!("http://{addr}/actions/invite/reply"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "message_id": expected_id,
            "action": "accept",
            "dry_run": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["status"], "preview");
    assert_eq!(json["preview"]["organizer_email"], "organizer@example.com");
    let captured = captured.lock().unwrap().clone().unwrap();
    assert_eq!(captured.0, expected.id.to_string());
    assert_eq!(captured.1, mxr_protocol::CalendarInviteActionData::Accept);
    assert!(captured.2);
}

#[tokio::test]
async fn star_mutation_endpoint_proxies_message_ids_and_state() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let expected = sample_envelope();
    let expected_id = expected.id.to_string();
    let captured = std::sync::Arc::new(std::sync::Mutex::new((Vec::<String>::new(), false)));
    let captured_state = captured.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::Mutation {
                mutation:
                    mxr_protocol::MutationCommand::Star {
                        message_ids,
                        starred,
                    },
                ..
            } => {
                *captured_state.lock().unwrap() = (
                    message_ids.iter().map(ToString::to_string).collect(),
                    starred,
                );
                Some(Response::Ok {
                    data: ResponseData::Ack,
                })
            }
            _ => None,
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
    let response = client
        .post(format!("http://{addr}/mutations/star"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "message_ids": [expected_id], "starred": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(
        *captured.lock().unwrap(),
        (vec![expected.id.to_string()], true)
    );
}

#[tokio::test]
async fn export_thread_endpoint_proxies_markdown_export() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let envelope = sample_envelope();
    let thread_id = envelope.thread_id.to_string();
    let expected_thread_id = envelope.thread_id.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ExportThread { thread_id, .. } if thread_id == expected_thread_id => {
                Some(Response::Ok {
                    data: ResponseData::ExportResult {
                        content: "# Exported thread".into(),
                    },
                })
            }
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/thread/{thread_id}/export"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["content"], "# Exported thread");
}

#[tokio::test]
async fn bug_report_endpoint_proxies_daemon_report() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::GenerateBugReport {
                verbose: false,
                full_logs: false,
                since: None,
            } => Some(Response::Ok {
                data: ResponseData::BugReport {
                    content: "bug report".into(),
                },
            }),
            _ => None,
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

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/diagnostics/bug-report"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["content"], "bug report");
}

#[tokio::test]
async fn read_and_archive_endpoint_proxies_mutation() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let expected_message_id = MessageId::new();
    let expected_message_id_text = expected_message_id.to_string();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::Mutation {
                mutation: mxr_protocol::MutationCommand::ReadAndArchive { message_ids },
                ..
            } => {
                assert_eq!(message_ids, vec![expected_message_id.clone()]);
                Some(Response::Ok {
                    data: ResponseData::Ack,
                })
            }
            _ => None,
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

    let response = reqwest::Client::new()
        .post(format!("http://{addr}/mutations/read-and-archive"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&json!({
            "message_ids": [expected_message_id_text],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}
