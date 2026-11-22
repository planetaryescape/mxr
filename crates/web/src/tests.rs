use super::*;
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use mxr_core::{
    id::{AccountId, AttachmentId, MessageId, ThreadId},
    types::{
        Address, CalendarMetadata, Draft, DraftContent, Envelope, Label, LabelKind, MessageBody,
        MessageFlags, MessageMetadata, SavedSearch, SortOrder, SubscriptionSummary, Thread,
        UnsubscribeMethod,
    },
};
use mxr_protocol::{
    DaemonEvent, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData,
    SearchResultItem, IPC_PROTOCOL_VERSION,
};
use tempfile::TempDir;
use tokio::net::UnixListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::codec::Framed;

mod activity_filters;
mod arrival_routes;
mod compose_and_scheduling;
mod draft_attachment_bytes;
mod merged_mailboxes;
mod place_routes;
mod record_routes;
mod recovery_attachments;
mod recovery_retry;
mod thread_context_routes;
mod time_routes;
mod todo_routes;
mod unsaved_recovery;
mod workspace_owner;

async fn accept_client_declaration<S>(framed: &mut Framed<S, IpcCodec>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let frame = framed
        .next()
        .await
        .expect("declaration")
        .expect("decode declaration");
    assert!(
        matches!(frame.payload, IpcPayload::Request(Request::DeclareClient { kind, protocol_version })
        if kind == frame.source && protocol_version == IPC_PROTOCOL_VERSION)
    );
    framed
        .send(IpcMessage {
            expected_workspace: None,
            id: frame.id,
            source: mxr_protocol::ClientKind::Daemon,
            payload: IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }),
        })
        .await
        .expect("declaration ack");
}

const TEST_AUTH_TOKEN: &str = "test-token";

#[tokio::test]
async fn attachment_transfer_routes_return_path_free_typed_responses() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let message_id = MessageId::new();
    let attachment_id = AttachmentId::new();
    let expected_message = message_id.clone();
    let expected_attachment = attachment_id.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::GetAttachmentTransferMetadata {
                message_id,
                attachment_id,
            } if message_id == expected_message && attachment_id == expected_attachment => {
                Some(Response::Ok {
                    data: ResponseData::AttachmentTransferMetadata {
                        metadata: mxr_protocol::AttachmentTransferMetadata {
                            message_id,
                            attachment_id,
                            filename: "report.txt".into(),
                            mime_type: "text/plain".into(),
                            length_bytes: 3,
                            sha256:
                                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                                    .into(),
                            max_chunk_bytes: 256 * 1024,
                        },
                    },
                })
            }
            Request::ReadAttachmentChunk {
                message_id,
                attachment_id,
                offset: 0,
                length: 3,
            } if message_id == expected_message && attachment_id == expected_attachment => {
                Some(Response::Ok {
                    data: ResponseData::AttachmentChunk {
                        chunk: mxr_protocol::AttachmentChunk {
                            message_id,
                            attachment_id,
                            offset: 0,
                            data_base64: "YWJj".into(),
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
    let client = test_http_client();
    let base = format!("http://{addr}/api/v1/mail/attachments/transfer");
    let body = serde_json::json!({
        "message_id": message_id.to_string(),
        "attachment_id": attachment_id.to_string(),
    });

    let metadata_response = client
        .post(format!("{base}/metadata"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(metadata_response.status(), reqwest::StatusCode::OK);
    let metadata: serde_json::Value = metadata_response.json().await.unwrap();
    assert_eq!(metadata["filename"], "report.txt");
    assert_eq!(metadata["length_bytes"], 3);
    assert!(metadata.get("path").is_none());

    let chunk_response = client
        .post(format!("{base}/chunk"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "message_id": message_id.to_string(),
            "attachment_id": attachment_id.to_string(),
            "offset": 0,
            "length": 3,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(chunk_response.status(), reqwest::StatusCode::OK);
    let chunk: serde_json::Value = chunk_response.json().await.unwrap();
    assert_eq!(chunk["data_base64"], "YWJj");
    assert!(chunk.get("path").is_none());
}

/// Read `session.draftPath` out of a compose-session response.
///
/// The bridge answers a failed compose start with an error object, so a bare
/// `unwrap` on the missing field reports "unwrap on None" and hides the reason.
/// Failing with the whole body keeps the diagnosis in the failure output, and
/// a present-but-empty path is a different bug from a missing one.
fn compose_draft_path(response: &serde_json::Value) -> String {
    let field = &response["session"]["draftPath"];
    assert!(
        field.is_string(),
        "compose session response carried no session.draftPath: {response}"
    );
    let path = field.as_str().unwrap_or_default();
    assert!(
        !path.is_empty(),
        "compose session response carried an empty session.draftPath: {response}"
    );
    path.to_string()
}

fn test_workspace() -> mxr_protocol::ArchiveWorkspaceData {
    mxr_protocol::ArchiveWorkspaceData {
        workspace_id: "00000000-0000-7000-8000-000000000001".into(),
        recovery_generation: "00000000-0000-7000-8000-000000000002".into(),
    }
}
fn test_http_client() -> reqwest::Client {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "x-mxr-workspace",
        reqwest::header::HeaderValue::from_str(&serde_json::to_string(&test_workspace()).unwrap())
            .unwrap(),
    );
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap()
}

async fn spawn_fake_ipc_server<F>(
    socket_path: &std::path::Path,
    responder: F,
    event: Option<DaemonEvent>,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(Request) -> Option<Response> + Send + Sync + 'static,
{
    let responder = std::sync::Arc::new(responder);
    let fixture_drafts = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
        DraftId,
        Draft,
    >::new()));
    let fixture_resources =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
            mxr_core::DraftAttachmentId,
            mxr_core::DraftAttachment,
        >::new()));
    let listener = UnixListener::bind(socket_path).unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let responder = responder.clone();
            let fixture_drafts = fixture_drafts.clone();
            let fixture_resources = fixture_resources.clone();
            let event = event.clone();
            tokio::spawn(async move {
                let mut framed = Framed::new(stream, IpcCodec::new());
                accept_client_declaration(&mut framed).await;
                if let Some(event) = event {
                    let _ = framed
                        .send(IpcMessage {
                            expected_workspace: None,
                            id: 0,
                            source: ::mxr_protocol::ClientKind::default(),
                            payload: IpcPayload::Event(event),
                        })
                        .await;
                }
                while let Some(message) = framed.next().await {
                    let Ok(message) = message else {
                        break;
                    };
                    if let IpcPayload::Request(request) = message.payload {
                        let mutation_draft = match &request {
                            Request::SaveDraft { draft }
                            | Request::SaveDraftToServer { draft }
                            | Request::UpdateDraft { draft } => Some(draft.clone()),
                            _ => None,
                        };
                        let update = matches!(request, Request::UpdateDraft { .. });
                        let status = matches!(request, Request::GetStatus);
                        let fallback_request = request.clone();
                        let Some(mut response) = (if matches!(request, Request::GetStatus) && message.expected_workspace.is_none() {
                            Some(Response::Ok { data: sample_status() })
                        } else if matches!(request, Request::GetArchiveWorkspace) { Some(Response::Ok { data: ResponseData::ArchiveWorkspace { workspace: test_workspace() } }) } else { responder(request) }).or_else(|| {
                            if status { return Some(Response::Ok { data: sample_status() }); }
                            let mut drafts = fixture_drafts.lock().unwrap();
                            match fallback_request {
                                Request::PreviewDraftAttachment { metadata } => Some(Response::Ok { data: ResponseData::DraftAttachment { metadata } }),
                                Request::BeginDraftAttachment { metadata } => { fixture_resources.lock().unwrap().insert(metadata.id.clone(), metadata.clone()); Some(Response::Ok { data: ResponseData::DraftAttachment { metadata } }) },
                                Request::WriteDraftAttachmentChunk { .. } => Some(Response::Ok { data: ResponseData::Ack }),
                                Request::FinalizeDraftAttachment { resource_id, .. } => fixture_resources.lock().unwrap().get(&resource_id).cloned().map(|metadata| Response::Ok { data: ResponseData::DraftAttachment { metadata } }),
                                Request::SaveDraft { mut draft } => { draft.revision = Some(1); Some(Response::Ok { data: ResponseData::Draft { draft } }) }
                                Request::GetDraft { draft_id } => Some(match drafts.get(&draft_id) { Some(draft) => Response::Ok { data: ResponseData::Draft { draft: draft.clone() } }, None => Response::error_kinded(format!("Draft not found: {draft_id}"), mxr_protocol::IpcErrorKind::NotFound) }),
                                Request::UpdateDraft { mut draft } => {
                                    let current = drafts.get(&draft.id).and_then(|draft| draft.revision);
                                    if draft.revision.is_none() || draft.revision != current { Some(Response::Error { message: "Draft revision conflict".into(), code: "draft_revision_conflict".into(), kind: mxr_protocol::IpcErrorKind::InvalidRequest, retryable: false, details: Some(json!({"current_revision": current, "expected_revision": draft.revision})) }) }
                                    else { draft.revision = current.map(|revision| revision + 1); Some(Response::Ok { data: ResponseData::Draft { draft } }) }
                                }
                                Request::DeleteDraft { draft_id, expected_revision } => {
                                    if drafts.get(&draft_id).and_then(|draft| draft.revision) == expected_revision && expected_revision.is_some() { drafts.remove(&draft_id); Some(Response::Ok { data: ResponseData::Ack }) } else { Some(Response::error("Draft revision conflict")) }
                                }
                                Request::SystemMailboxCounts { .. } => Some(Response::error("unknown request")),
                                _ => None,
                            }
                        }) else { continue; };
                        if matches!(
                            response,
                            Response::Ok {
                                data: ResponseData::Ack
                            }
                        ) {
                            if let Some(mut draft) = mutation_draft {
                                draft.revision = Some(if update {
                                    draft.revision.unwrap_or(0) + 1
                                } else {
                                    draft.revision.unwrap_or(1)
                                });
                                response = Response::Ok {
                                    data: ResponseData::Draft { draft },
                                };
                            }
                        }
                        match &response {
                            Response::Ok {
                                data: ResponseData::Draft { draft },
                            } => {
                                fixture_drafts
                                    .lock()
                                    .unwrap()
                                    .insert(draft.id.clone(), draft.clone());
                            }
                            Response::Ok {
                                data: ResponseData::Drafts { drafts },
                            } => {
                                fixture_drafts.lock().unwrap().extend(
                                    drafts.iter().map(|draft| (draft.id.clone(), draft.clone())),
                                );
                            }
                            _ => {}
                        }
                        let response = IpcMessage {
                            expected_workspace: None,
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
                    count: if mode == Some(SearchMode::Lexical) {
                        7
                    } else {
                        0
                    },
                },
            }),
            Request::SyncNow {
                account_id: None,
                background: true,
            } => Some(Response::Ok {
                data: ResponseData::Ack,
            }),
            Request::SyncAll => Some(Response::Ok {
                data: ResponseData::SyncBatch {
                    results: vec![mxr_protocol::SyncAccountResult {
                        account_id: AccountId::new(),
                        account_name: "Work".into(),
                        result: mxr_protocol::SyncAccountResultKind::Started,
                        error: None,
                    }],
                },
            }),
            Request::PreviewSyncAll => Some(Response::Ok {
                data: ResponseData::SyncPlan { targets: vec![] },
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

    let client = test_http_client();

    let response = client
        .post(format!("http://{addr}/api/v1/admin/ping"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["pong"], true);

    let response = client
        .post(format!("http://{addr}/api/v1/mail/sync"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        json["ok"], true,
        "legacy route still acknowledges default sync"
    );

    let response = client
        .post(format!("http://{addr}/api/v1/mail/sync/all"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["results"][0]["account_name"], "Work");
    assert_eq!(json["results"][0]["result"], "started");

    let response = client
        .post(format!("http://{addr}/api/v1/mail/sync/preview"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["targets"], serde_json::json!([]));

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

    let response = client
        .get(format!("http://{addr}/api/v1/platform/saved-searches"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["kind"], "SavedSearches");

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
                    workspace_owner_supported: true,
                    draft_revision_supported: true,
                    send_outcome_supported: true,
                    archive_recovery_supported: true,
                    recovery_supported: false,
                    recovery: None,
                    attachment_transfer_supported: true,
                    draft_attachment_upload_supported: true,
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
                    degraded: false,
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

    let response = test_http_client()
        .get(format!("http://{addr}/api/v1/admin/status"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

/// Slice 4 — `?token=X` is not a general HTTP auth path. Query strings
/// leak through shell history, logs, and referrers, so authority-bearing
/// HTTP routes require header auth.
#[tokio::test]
async fn auth_rejects_query_string_token_for_http_routes() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        |_| {
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

    let status_response = reqwest::get(format!(
        "http://{addr}/api/v1/admin/status?token={TEST_AUTH_TOKEN}"
    ))
    .await
    .unwrap();
    assert_eq!(
        status_response.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "query-string token must not satisfy regular HTTP auth"
    );

    let sync_response = test_http_client()
        .post(format!(
            "http://{addr}/api/v1/mail/sync?token={TEST_AUTH_TOKEN}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        sync_response.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "query-string token must not satisfy mutating HTTP auth"
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
            .map(axum::http::HeaderValue::as_bytes),
        Some(b"bearer".as_slice()),
        "server must echo `bearer` as the negotiated subprotocol"
    );
    let message = stream.next().await.unwrap().unwrap();
    assert!(matches!(message, Message::Text(_)));
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

    let response = test_http_client()
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
            .expect("308 must include Location header");
        assert_eq!(location, new, "Location header for {old} should be {new}");
    }
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

    let json: serde_json::Value = test_http_client()
        .get(format!("http://{addr}/api/v1/openapi.json"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let paths = &json["paths"];
    let create = &paths["/api/v1/mail/categories"]["post"];
    let create_schema = create
        .pointer("/requestBody/content/application~1json/schema")
        .map(|schema| openapi_schema(&json, schema))
        .expect("category create request schema");
    assert_eq!(create_schema["properties"]["name"]["type"], "string");
    assert!(create_schema["required"]
        .as_array()
        .is_some_and(|required| required.iter().any(|field| field == "name")));

    let apply = &paths["/api/v1/mail/categories/assignment/apply"]["post"];
    let apply_schema = apply
        .pointer("/requestBody/content/application~1json/schema")
        .map(|schema| openapi_schema(&json, schema))
        .expect("category assignment apply request schema");
    assert_eq!(
        apply_schema["properties"]["preview_token"]["type"],
        "string"
    );
    assert!(apply_schema["required"]
        .as_array()
        .is_some_and(|required| required.iter().any(|field| field == "preview_token")));

    let account_filter = paths["/api/v1/mail/categories"]["get"]["parameters"]
        .as_array()
        .expect("category account filter parameters")
        .iter()
        .find(|parameter| parameter["name"] == "account_id")
        .expect("account_id query filter");
    assert_eq!(account_filter["in"], "query");

    let mut schema_names: Vec<&str> = json
        .pointer("/components/schemas")
        .and_then(|v| v.as_object())
        .map(|m| m.keys().map(std::string::String::as_str).collect())
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

#[tokio::test]
async fn forward_session_preview_and_updates_keep_original_html_and_managed_files() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("forward.md");
    let account_id = AccountId::new();
    let draft_id = DraftId::new();
    let original = "---------- Forwarded message ----------\nFrom: original@example.com\n\nOriginal **literal**";
    let source = mxr_core::ForwardSource {
        draft_id: draft_id.clone(), account_id: account_id.clone(), message_id: MessageId::new(),
        content: DraftContent::html("<html><body><table class='original'><tr><td>Original **literal**</td></tr></table></body></html>", Some(original.into())),
        attachments: vec![mxr_core::DraftAttachment {
            id: mxr_core::DraftAttachmentId::new(), draft_id: draft_id.clone(), account_id: account_id.clone(), filename: "report.pdf".into(), mime_type: "application/pdf".into(), content_id: None, content_location: None,
            length_bytes: 0, sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        }],
    };
    let fm = ComposeFrontmatter {
        to: "synthetic@example.com".into(),
        from: "fake@example.com".into(),
        subject: "Fwd: Report".into(),
        intent: mxr_core::DraftIntent::Forward,
        attach: mxr_compose::forward::resource_references(&source),
        forward_source: Some(source),
        ..Default::default()
    };
    let raw = render_compose_file(&fm, &format!("Please review.\n\n{original}"), None).unwrap();
    let session = compose_session_from_snapshot(&path, &raw, None).unwrap();
    assert_eq!(
        session["frontmatter"]["attach"].as_array().unwrap().len(),
        1
    );
    let preview = session["previewHtml"].as_str().unwrap();
    assert!(preview.contains("<table class='original'>"));
    assert!(preview.find("Please review.").unwrap() < preview.find("<table").unwrap());
    let draft = compose_draft_from_snapshot(
        path.to_str().unwrap(),
        &account_id.to_string(),
        None,
        ComposeDraftValidation::Incomplete,
        &raw,
        None,
    )
    .await
    .unwrap();
    assert_eq!(draft.id, draft_id);
    assert!(draft.attachments.is_empty());
    assert_eq!(draft.managed_attachments.as_ref().unwrap().len(), 1);
    let DraftContent::Html { html, .. } = &draft.content else {
        panic!("forward HTML");
    };
    assert_eq!(html, preview);
    let updated_raw =
        render_compose_file(&fm, &format!("New introduction.\n\n{original}"), None).unwrap();
    let updated = compose_draft_from_snapshot(
        path.to_str().unwrap(),
        &account_id.to_string(),
        Some(draft_id.clone()),
        ComposeDraftValidation::Incomplete,
        &updated_raw,
        Some(draft.clone()),
    )
    .await
    .unwrap();
    assert_eq!(updated.id, draft_id);
    assert_eq!(updated.managed_attachments, draft.managed_attachments);
    assert!(updated
        .content
        .analysis_text()
        .starts_with("New introduction."));
}

#[tokio::test]
async fn audience_apply_recovery_keeps_private_body_with_the_accepted_revision() {
    let temp = TempDir::new().unwrap();
    let source_path = temp.path().join("reply.md");
    let account_id = AccountId::new();
    let draft_id = DraftId::new();
    let frontmatter = ComposeFrontmatter {
        to: "sender@example.com".into(),
        from: "me@example.com".into(),
        subject: "Re: Update".into(),
        intent: mxr_core::DraftIntent::Reply,
        ..Default::default()
    };
    let original = render_compose_file(
        &frontmatter,
        "private text typed before audience update",
        None,
    )
    .unwrap();
    std::fs::write(&source_path, &original).unwrap();
    let now = Utc::now();
    let accepted = Draft {
        id: draft_id.clone(),
        revision: Some(8),
        account_id,
        from: None,
        reply_headers: Some(mxr_core::ReplyHeaders {
            in_reply_to: "<original@example.com>".into(),
            references: vec![],
            thread_id: None,
        }),
        intent: mxr_core::DraftIntent::ReplyAll,
        to: vec![Address {
            name: None,
            email: "sender@example.com".into(),
        }],
        cc: vec![Address {
            name: None,
            email: "team@example.com".into(),
        }],
        bcc: vec![],
        subject: "Re: Update".into(),
        content: DraftContent::markdown("canonical body before recovery"),
        managed_attachments: None,
        attachments: vec![],
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    };

    let recovery = preserve_compose_snapshot(&source_path, &original, &accepted)
        .await
        .unwrap();
    let recovery_path = PathBuf::from(&recovery.draft_path);
    let recovered_content = read_compose_file(&recovery_path).await.unwrap();
    let (_, recovered_body) = parse_compose_content(&recovered_content).unwrap();
    let recovered_metadata = read_draft_session_metadata(&recovery_path)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(recovery.draft_id, draft_id);
    assert_eq!(recovery.revision, Some(8));
    assert_eq!(recovered_body, "private text typed before audience update");
    assert_eq!(recovered_metadata.id, accepted.id);
    assert_eq!(recovered_metadata.revision, accepted.revision);
    tokio::fs::remove_file(&recovery_path).await.unwrap();
    tokio::fs::remove_file(draft_session_metadata_path(&recovery_path))
        .await
        .unwrap();
    remove_compose_attachment_dir(&recovery_path).await.unwrap();
}

fn openapi_schema<'a>(
    spec: &'a serde_json::Value,
    schema: &'a serde_json::Value,
) -> &'a serde_json::Value {
    schema
        .get("$ref")
        .and_then(serde_json::Value::as_str)
        .and_then(|reference| spec.pointer(reference.strip_prefix('#').unwrap_or(reference)))
        .unwrap_or(schema)
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
        identity: None,
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
                    workspace_owner_supported: true,
                    draft_revision_supported: true,
                    send_outcome_supported: true,
                    archive_recovery_supported: true,
                    recovery_supported: false,
                    recovery: None,
                    attachment_transfer_supported: true,
                    draft_attachment_upload_supported: true,
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
                    degraded: false,
                },
            }),
            Request::ListLabels {
                account_id: None, ..
            } => Some(Response::Ok {
                data: ResponseData::Labels {
                    labels: labels.clone(),
                },
            }),
            Request::ListCategories { account_id: None } => Some(Response::Ok {
                data: ResponseData::Categories {
                    categories: Vec::new(),
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
                ..
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
                ..
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

    let response = test_http_client()
        .get(format!("http://{addr}/mailbox?view=messages"))
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
                window: None,
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

    let response = test_http_client()
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
                limit: 25,
                offset: 50,
                account_id: None,
                mode: None,
                sort: None,
                explain: false,
                limit_from_config: false,
            } if query == "buildkite" => Some(Response::Ok {
                data: ResponseData::SearchResults {
                    limit: None,
                    results: vec![result.clone()],
                    total: 1,
                    has_more: false,
                    next_offset: None,
                    explain: None,
                    degraded: None,
                },
            }),
            Request::ListEnvelopesByIds {
                message_ids: requested,
            } if requested == message_ids => Some(Response::Ok {
                data: ResponseData::Envelopes {
                    envelopes: vec![envelope.clone()],
                },
            }),
            Request::ListLabels { .. } => Some(Response::Ok {
                data: ResponseData::Labels { labels: Vec::new() },
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

    let response = test_http_client()
        .get(format!(
            "http://{addr}/search?q=buildkite&limit=25&offset=50"
        ))
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
    assert!(
        json.get("degraded").is_none(),
        "an index answer must not mention the limited search"
    );
}

#[tokio::test]
async fn compose_session_endpoint_ensures_canonical_reply_draft() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let envelope = sample_envelope();
    let account = sample_account(&envelope.account_id);
    let message_id = envelope.id.to_string();
    let expected_account_id = envelope.account_id.to_string();
    let expected_message_id = envelope.id.clone();
    let created_at = Utc::now();
    let draft = Draft {
        revision: Some(1),
        id: DraftId::new(),
        account_id: envelope.account_id.clone(),
        from: None,
        reply_headers: Some(mxr_core::ReplyHeaders {
            in_reply_to: "<msg-1@example.com>".into(),
            references: vec!["<root@example.com>".into()],
            thread_id: None,
        }),
        intent: mxr_core::DraftIntent::Reply,
        to: vec![Address {
            name: None,
            email: "sender@example.com".into(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: "Re: Mailroom".into(),
        content: DraftContent::markdown(""),
        managed_attachments: None,
        attachments: Vec::new(),
        inline_assets: Vec::new(),
        inline_calendar_reply: None,
        created_at,
        updated_at: created_at,
    };
    let expected_draft_id = draft.id.to_string();
    let stored_draft = draft.clone();
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
            Request::EnsureReplyDraft {
                message_id,
                reply_all: _,
                dry_run: false,
            } if message_id == expected_message_id => Some(Response::Ok {
                data: ResponseData::Draft {
                    draft: draft.clone(),
                },
            }),
            Request::ListDrafts => Some(Response::Ok {
                data: ResponseData::Drafts {
                    drafts: vec![stored_draft.clone()],
                },
            }),
            Request::GetDraft { draft_id } if draft_id == draft.id => Some(Response::Ok {
                data: ResponseData::Draft {
                    draft: draft.clone(),
                },
            }),
            Request::SetReplyDraftAudience {
                draft_id,
                reply_all,
                dry_run,
                ..
            } if draft_id == draft.id => {
                let mut preview = draft.clone();
                preview.intent = if reply_all {
                    mxr_core::DraftIntent::ReplyAll
                } else {
                    mxr_core::DraftIntent::Reply
                };
                preview.cc = if reply_all {
                    vec![Address {
                        name: None,
                        email: "team@example.com".into(),
                    }]
                } else {
                    Vec::new()
                };
                if !dry_run {
                    preview.revision = Some(2);
                }
                Some(Response::Ok {
                    data: ResponseData::Draft { draft: preview },
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

    let response = test_http_client()
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
    assert_eq!(json["session"]["draftId"], expected_draft_id);
    assert_eq!(json["session"]["issues"].as_array().unwrap().len(), 1);

    let draft_path = json["session"]["draftPath"].as_str().unwrap().to_string();
    let original_file = std::fs::read_to_string(&draft_path).unwrap();
    let private_file = format!("{original_file}\nprivate text from the first reply entry");
    std::fs::write(&draft_path, &private_file).unwrap();
    let resumed = test_http_client()
        .post(format!("http://{addr}/compose/session"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "kind": "reply_all",
            "message_id": message_id,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resumed.status(), reqwest::StatusCode::OK);
    let resumed: serde_json::Value = resumed.json().await.unwrap();
    assert_eq!(resumed["session"]["draftPath"], draft_path);
    assert_eq!(
        resumed["session"]["bodyMarkdown"],
        "private text from the first reply entry"
    );
    assert_eq!(resumed["session"]["kind"], "reply");
    std::fs::write(&draft_path, &original_file).unwrap();

    let preview = test_http_client()
        .post(format!(
            "http://{addr}/api/v1/mail/compose/session/audience"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "draft_id": expected_draft_id,
            "reply_all": true,
            "expected_revision": 1,
            "dry_run": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(preview.status(), reqwest::StatusCode::OK);
    let preview: serde_json::Value = preview.json().await.unwrap();
    assert_eq!(preview["session"]["frontmatter"]["cc"], "team@example.com");
    assert_eq!(preview["session"]["bodyMarkdown"], "");
    assert_eq!(std::fs::read_to_string(&draft_path).unwrap(), original_file);

    let stale_local = format!("{original_file}\nprivate text typed after the saved revision");
    std::fs::write(&draft_path, &stale_local).unwrap();
    let stale = test_http_client()
        .post(format!(
            "http://{addr}/api/v1/mail/compose/session/audience"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "draft_id": expected_draft_id,
            "reply_all": true,
            "expected_revision": 1,
            "dry_run": false,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), reqwest::StatusCode::CONFLICT);
    assert_eq!(std::fs::read_to_string(&draft_path).unwrap(), stale_local);
    std::fs::write(&draft_path, &original_file).unwrap();

    let applied = test_http_client()
        .post(format!(
            "http://{addr}/api/v1/mail/compose/session/audience"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "draft_id": expected_draft_id,
            "reply_all": true,
            "expected_revision": 1,
            "dry_run": false,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(applied.status(), reqwest::StatusCode::OK);
    let applied: serde_json::Value = applied.json().await.unwrap();
    assert_eq!(applied["session"]["draftId"], expected_draft_id);
    assert_eq!(applied["session"]["revision"], 2);
    assert_eq!(applied["session"]["draftIntent"], "reply_all");
    assert_eq!(applied["session"]["kind"], "reply_all");
    assert_eq!(applied["session"]["frontmatter"]["cc"], "team@example.com");
    assert_eq!(applied["session"]["bodyMarkdown"], "");
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
    let client = test_http_client();

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
    let draft_path = compose_draft_path(&started);
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
    let updated: serde_json::Value = update_response.json().await.unwrap();

    let send_response = client
        .post(format!("http://{addr}/compose/session/send"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "draft_path": draft_path,
            "account_id": account_id,
            "expected_revision": updated["session"]["revision"],
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
    assert_eq!(draft.content.analysis_text(), "Hello from bridge");
    assert_eq!(draft.to[0].email, "alice@example.com");
}

#[tokio::test]
async fn route_mutation_endpoint_proxies_atomic_route_request() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let expected = sample_envelope();
    let expected_id = expected.id.to_string();
    let captured = std::sync::Arc::new(std::sync::Mutex::new(
        None::<(Vec<String>, String, String, bool, bool)>,
    ));
    let captured_route = captured.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::Mutation {
                mutation:
                    mxr_protocol::MutationCommand::Route {
                        message_ids,
                        to_label,
                        from_queue_label,
                        archive,
                        dry_run,
                    },
                ..
            } => {
                *captured_route.lock().unwrap() = Some((
                    message_ids.iter().map(ToString::to_string).collect(),
                    to_label,
                    from_queue_label,
                    archive,
                    dry_run,
                ));
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

    let response = test_http_client()
        .post(format!("http://{addr}/api/v1/mail/mutations/route"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "message_ids": [expected_id],
            "to_label": "Follow Up",
            "from_queue_label": "Notto",
            "archive": true,
            "dry_run": false,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(
        *captured.lock().unwrap(),
        Some((
            vec![expected.id.to_string()],
            "Follow Up".to_string(),
            "Notto".to_string(),
            true,
            false,
        ))
    );
}

/// The document a mail-merge or `--html-file` draft carries: hand-authored
/// HTML the sender expects to reach the wire byte-for-byte, including the bits
/// a display sanitiser would strip.
const HTML_DRAFT_BODY: &str = concat!(
    "<html><body><h1>Ship day</h1><p>We go live on Friday.</p>",
    "<img src=\"https://tracker.example.com/pixel.png?id=42\">",
    "<script>track()</script>",
    "</body></html>"
);

fn html_body_draft(draft_id: &DraftId, account_id: &AccountId) -> Draft {
    let now = Utc::now();
    Draft {
        revision: Some(1),
        id: draft_id.clone(),
        account_id: account_id.clone(),
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![Address {
            name: None,
            email: "alice@example.com".into(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: "Launch".into(),
        content: DraftContent::html(HTML_DRAFT_BODY, None),
        managed_attachments: None,
        attachments: Vec::new(),
        inline_assets: Vec::new(),
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    }
}

/// Opening an HTML-bodied draft in the browser composer is a permanent
/// refusal, and the refusal is actionable: a distinct status the client can
/// branch on, a machine-readable code, and the document itself so the client
/// can show the user what they wrote.
#[tokio::test]
async fn restoring_an_html_draft_refuses_with_an_actionable_conflict() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account_id = AccountId::new();
    let draft_id = DraftId::new();
    let draft = html_body_draft(&draft_id, &account_id);
    let account = sample_account(&account_id);
    let saw_account_lookup = std::sync::Arc::new(AtomicBool::new(false));
    let saw_account_lookup_for_server = saw_account_lookup.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::ListDrafts => Some(Response::Ok {
                data: ResponseData::Drafts {
                    drafts: vec![draft.clone()],
                },
            }),
            Request::ListAccounts => {
                saw_account_lookup_for_server.store(true, Ordering::SeqCst);
                Some(Response::Ok {
                    data: ResponseData::Accounts {
                        accounts: vec![account.clone()],
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

    let response = test_http_client()
        .post(format!("http://{addr}/compose/session/restore"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "draft_id": draft_id.to_string() }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "html_draft_not_editable");
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase()
            .contains("html"),
        "the message must name the HTML body as the reason; got {body}"
    );
    assert!(
        body["session"].is_null(),
        "no editing session exists, so the client must not be handed one; got {body}"
    );
    assert!(
        !saw_account_lookup.load(Ordering::SeqCst),
        "the refusal must land before any work that provisions an editing session \
         (resolving the account, then writing a scratch compose file that nothing \
         would ever clean up)"
    );
}

/// A stand-in for the daemon's draft store, modelling only what the compose
/// save path depends on: `ListDrafts` reads it, a create appends a row, and
/// `UpdateDraft` rewrites the row carrying that id while keeping the stored
/// `created_at` (`Store::update_draft` leaves `created_at` out of its SET).
#[derive(Clone, Default)]
struct FakeDraftStore(std::sync::Arc<std::sync::Mutex<Vec<Draft>>>);

impl FakeDraftStore {
    fn with(drafts: Vec<Draft>) -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(drafts)))
    }

    fn snapshot(&self) -> Vec<Draft> {
        self.0.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }

    /// The daemon's fallback for an account with no server-side drafts:
    /// `SaveDraftToServer` -> `save_draft` -> `insert_draft`, a plain INSERT
    /// that stores the payload's `created_at` verbatim.
    fn insert(&self, mut draft: Draft) {
        draft.revision = Some(1);
        self.0.lock().unwrap().push(draft);
    }

    /// `Store::update_draft` + the daemon's not-found reporting.
    fn update(&self, mut draft: Draft) -> Result<(), String> {
        let mut drafts = self.0.lock().unwrap();
        let Some(stored) = drafts.iter_mut().find(|stored| stored.id == draft.id) else {
            return Err(format!("Draft not found: {}", draft.id));
        };
        if draft.revision != stored.revision || draft.revision.is_none() {
            return Err("Draft revision conflict".into());
        }
        draft.revision = Some(stored.revision.unwrap() + 1);
        let created_at = stored.created_at;
        *stored = Draft {
            created_at,
            ..draft
        };
        Ok(())
    }
}

fn draft_store_responder(
    store: FakeDraftStore,
    account: mxr_protocol::AccountSummaryData,
) -> impl Fn(Request) -> Option<Response> + Send + Sync + 'static {
    move |request| {
        let data = match request {
            Request::ListDrafts => ResponseData::Drafts {
                drafts: store.snapshot(),
            },
            Request::ListAccounts => ResponseData::Accounts {
                accounts: vec![account.clone()],
            },
            Request::GetDraft { draft_id } => {
                match store
                    .snapshot()
                    .into_iter()
                    .find(|draft| draft.id == draft_id)
                {
                    Some(draft) => ResponseData::Draft { draft },
                    None => {
                        return Some(Response::error_kinded(
                            format!("Draft not found: {draft_id}"),
                            mxr_protocol::IpcErrorKind::NotFound,
                        ))
                    }
                }
            }
            Request::SaveDraft { draft } | Request::SaveDraftToServer { draft } => {
                if let Some(stored) = store
                    .snapshot()
                    .into_iter()
                    .find(|stored| stored.id == draft.id)
                {
                    ResponseData::Draft { draft: stored }
                } else {
                    store.insert(draft.clone());
                    ResponseData::Draft {
                        draft: Draft {
                            revision: Some(1),
                            ..draft
                        },
                    }
                }
            }
            Request::UpdateDraft { draft } => match store.update(draft) {
                Ok(()) => ResponseData::Ack,
                Err(message) => {
                    return Some(Response::Error {
                        message,
                        kind: Default::default(),
                        code: String::new(),
                        retryable: false,
                        details: None,
                    })
                }
            },
            _ => return None,
        };
        Some(Response::Ok { data })
    }
}

fn markdown_draft(
    draft_id: &DraftId,
    account_id: &AccountId,
    subject: &str,
    body: &str,
    created_at: chrono::DateTime<Utc>,
) -> Draft {
    Draft {
        revision: Some(1),
        id: draft_id.clone(),
        account_id: account_id.clone(),
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![Address {
            name: None,
            email: "alice@example.com".into(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: subject.into(),
        content: DraftContent::markdown(body),
        managed_attachments: None,
        attachments: Vec::new(),
        inline_assets: Vec::new(),
        inline_calendar_reply: None,
        created_at,
        updated_at: created_at,
    }
}

/// Opening a stored draft in the browser composer and saving it must leave the
/// user with the draft they opened, edited — not that draft plus a copy.
///
/// Every save cycle used to mint a fresh `DraftId`, so a draft edited three
/// times became four indistinguishable rows and the user had no way to tell
/// which one the composer was still writing to.
#[tokio::test]
async fn saving_a_restored_draft_updates_it_in_place_instead_of_storing_a_copy() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let account_id = AccountId::new();
    let draft_id = DraftId::new();
    let created_at = Utc::now() - chrono::Duration::days(3);
    let mut draft = markdown_draft(
        &draft_id,
        &account_id,
        "Quarterly plan",
        "First pass.",
        created_at,
    );
    draft.from = Some(Address {
        name: Some("Support".into()),
        email: "support@example.com".into(),
    });
    let store = FakeDraftStore::with(vec![draft]);
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        draft_store_responder(store.clone(), sample_account(&account_id)),
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

    let draft_path = restore_session_path(&client, addr, &draft_id).await;
    let reopened = load_compose_session(Path::new(&draft_path)).await.unwrap();
    assert_eq!(
        reopened["frontmatter"]["from"],
        json!("Support <support@example.com>")
    );
    let raw_content = tokio::fs::read_to_string(&draft_path).await.unwrap();
    let (frontmatter, _) = mxr_compose::frontmatter::parse_compose_file(&raw_content).unwrap();
    let sender = mxr_compose::draft_codec::parse_from_field(&frontmatter.from)
        .unwrap()
        .unwrap();
    assert_eq!(sender.name.as_deref(), Some("Support"));
    assert_eq!(sender.email, "support@example.com");

    edit_session(
        &client,
        addr,
        &draft_path,
        "Quarterly plan v2",
        "Second pass, with numbers.",
    )
    .await;

    let response = client
        .post(format!("http://{addr}/compose/session/save"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&json!({
            "expected_revision": read_draft_session_metadata(Path::new(&draft_path)).await.unwrap().and_then(|draft| draft.revision),
            "draft_path": draft_path,
            "account_id": account_id.to_string(),
            "draft_id": draft_id.to_string(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["draft_id"], json!(draft_id.to_string()));

    let drafts = store.snapshot();
    assert_eq!(
        drafts.len(),
        1,
        "editing one draft must leave one draft; got {:?}",
        drafts
            .iter()
            .map(|draft| &draft.subject)
            .collect::<Vec<_>>()
    );
    let saved = &drafts[0];
    assert_eq!(
        saved.id, draft_id,
        "the edit must land on the draft the user opened"
    );
    assert_eq!(
        saved.created_at, created_at,
        "an edit must not reset when the draft was created"
    );
    assert_eq!(saved.subject, "Quarterly plan v2");
    assert_eq!(
        saved.content,
        DraftContent::markdown("Second pass, with numbers.")
    );

    mxr_compose::delete_draft_file_async(Path::new(&draft_path))
        .await
        .ok();
}

async fn restore_session_path(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
    draft_id: &DraftId,
) -> String {
    let response = client
        .post(format!("http://{addr}/compose/session/restore"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&json!({ "draft_id": draft_id.to_string() }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    compose_draft_path(&body)
}

/// Rewrite the session the way the composer's own autosave does.
async fn edit_session(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
    draft_path: &str,
    subject: &str,
    body: &str,
) {
    let response = client
        .post(format!("http://{addr}/compose/session/update"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .json(&json!({
            "expected_revision": read_draft_session_metadata(Path::new(draft_path)).await.unwrap().and_then(|draft| draft.revision),
            "draft_path": draft_path,
            "to": "alice@example.com",
            "cc": "",
            "bcc": "",
            "subject": subject,
            "from": "me@example.com",
            "body": body,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

#[test]
fn degraded_status_is_not_reported_as_a_quiet_mailbox() {
    use crate::chrome::describe_sync_state;

    let (label, message) = describe_sync_state(true, false, &[]);
    assert_eq!(label, "Daemon busy");
    assert!(
        message.contains("unknown, not empty"),
        "the shell must say the counts are unknown; got {message:?}"
    );

    let (label, message) = describe_sync_state(false, false, &[]);
    assert_eq!(label, "Synced");
    assert_eq!(message, "Local-first and ready");
}

/// Daemon-reported errors carry an `IpcErrorKind`; the bridge maps it to an
/// HTTP status instead of blaming the gateway for every failure. The body
/// keeps `{"error": ...}` and adds the daemon's machine-readable `code`.
#[tokio::test]
async fn daemon_error_kinds_map_to_http_statuses() {
    use mxr_protocol::IpcErrorKind;

    let cases = [
        (
            IpcErrorKind::InvalidRequest,
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            IpcErrorKind::Policy,
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            IpcErrorKind::Unsupported,
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (IpcErrorKind::NotFound, reqwest::StatusCode::NOT_FOUND),
        (
            IpcErrorKind::RateLimited,
            reqwest::StatusCode::TOO_MANY_REQUESTS,
        ),
        (IpcErrorKind::Auth, reqwest::StatusCode::BAD_GATEWAY),
        (IpcErrorKind::Provider, reqwest::StatusCode::BAD_GATEWAY),
        (
            IpcErrorKind::Store,
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            IpcErrorKind::Internal,
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ];

    for (kind, expected) in cases {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |_| Some(Response::error_kinded("daemon said no", kind)),
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

        let response = test_http_client()
            .get(format!("http://{addr}/api/v1/admin/status"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "kind {kind:?}");
        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["error"], "daemon said no", "kind {kind:?}");
        assert_eq!(json["code"], kind.as_code(), "kind {kind:?}");
    }
}

fn sample_status() -> ResponseData {
    ResponseData::Status {
        workspace_owner_supported: true,
        draft_revision_supported: true,
        send_outcome_supported: true,
        archive_recovery_supported: true,
        recovery_supported: false,
        recovery: None,
        attachment_transfer_supported: true,
        draft_attachment_upload_supported: true,
        uptime_secs: 1,
        accounts: vec!["personal".into()],
        total_messages: 10,
        daemon_pid: None,
        sync_statuses: Vec::new(),
        protocol_version: IPC_PROTOCOL_VERSION,
        daemon_version: None,
        daemon_build_id: None,
        repair_required: false,
        semantic_runtime: None,
        feature_health: None,
        degraded: false,
    }
}

/// The parity routes forward to the right daemon request with the query/body
/// fields mapped by name, fall back to the default account when `account` is
/// absent, and pass the daemon payload through.
#[tokio::test]
async fn parity_routes_dispatch_daemon_requests() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let default_account = AccountId::new();
    let other_account = AccountId::new();
    let saved_search_id = mxr_core::SavedSearchId::new();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Request>::new()));
    let seen_for_ipc = seen.clone();
    let account_for_ipc = default_account.clone();
    let search_for_ipc = saved_search_id.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            let data = match &request {
                Request::ListAccounts => ResponseData::Accounts {
                    accounts: vec![sample_account(&account_for_ipc)],
                },
                Request::ListOwedReplies { .. } => ResponseData::OwedReplies {
                    rows: Vec::new(),
                    people: None,
                },
                Request::ListSavedSearchUnreadCounts => ResponseData::SavedSearchUnreadCounts {
                    counts: [(search_for_ipc.clone(), 4)].into_iter().collect(),
                },
                Request::ListCadenceDrift { .. } => {
                    ResponseData::CadenceDriftList { rows: Vec::new() }
                }
                Request::ListCadenceWatch { .. } => ResponseData::CadenceWatchList {
                    entries: Vec::new(),
                },
                Request::StartMutationJob { .. } => ResponseData::JobStarted {
                    job: mxr_protocol::JobData {
                        job_id: "job-7".into(),
                        kind: "archive".into(),
                        status: mxr_protocol::JobStatusData::Queued,
                        progress: mxr_protocol::JobProgressData {
                            total: 1,
                            completed: 0,
                            succeeded: 0,
                            skipped: 0,
                            failed: 0,
                        },
                        undo_ids: Vec::new(),
                        error: None,
                        started_at: 0,
                        finished_at: None,
                        result: None,
                        selection: None,
                        failures: Vec::new(),
                        paused_until: None,
                        action: None,
                        undo_id_count: 0,
                        failure_count: 0,
                    },
                },
                Request::ExplainEntity { .. }
                | Request::SendTimeRecommendation { .. }
                | Request::ArchiveAsk { .. }
                | Request::WatchCadence { .. }
                | Request::UnwatchCadence { .. } => ResponseData::Ack,
                _ => return Some(Response::error("unexpected request")),
            };
            seen_for_ipc.lock().unwrap().push(request);
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
    let get = |path: String| {
        let client = client.clone();
        async move {
            let response = client
                .get(format!("http://{addr}{path}"))
                .bearer_auth(TEST_AUTH_TOKEN)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::OK, "{path}");
            response.json::<serde_json::Value>().await.unwrap()
        }
    };
    let post = |path: &'static str, body: serde_json::Value| {
        let client = client.clone();
        async move {
            let response = client
                .post(format!("http://{addr}{path}"))
                .bearer_auth(TEST_AUTH_TOKEN)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::OK, "{path}");
            response.json::<serde_json::Value>().await.unwrap()
        }
    };

    let json = get("/api/v1/mail/owed?older_than_days=3&limit=5".into()).await;
    assert_eq!(json["kind"], "OwedReplies");
    get(format!(
        "/api/v1/mail/owed?account={other_account}&within_days=30&all=true"
    ))
    .await;
    let json = get(format!(
        "/api/v1/mail/whois?query=alice%40example.com&account={other_account}"
    ))
    .await;
    assert_eq!(json["kind"], "Ack");
    get("/api/v1/mail/send-time?to=a%40example.com,%20b%40example.com".into()).await;
    let json = get("/api/v1/platform/saved-searches/unread-counts".into()).await;
    assert_eq!(json["counts"][saved_search_id.to_string()], 4);
    let json = get("/api/v1/platform/analytics/cadence-drift".into()).await;
    assert_eq!(json["kind"], "CadenceDriftList");
    let json = get(format!(
        "/api/v1/platform/cadence/watch?account={other_account}"
    ))
    .await;
    assert_eq!(json["kind"], "CadenceWatchList");
    let json = post(
        "/api/v1/platform/cadence/watch",
        serde_json::json!({ "email": "mum@example.com", "expected_days": 7.0 }),
    )
    .await;
    assert_eq!(json["ok"], true);
    post(
        "/api/v1/platform/cadence/unwatch",
        serde_json::json!({ "account_id": other_account.to_string(), "email": "mum@example.com" }),
    )
    .await;
    let message_id = MessageId::new();
    let json = post(
        "/api/v1/mail/mutation-jobs",
        serde_json::json!({ "mutation": "Archive", "message_ids": [message_id.to_string()] }),
    )
    .await;
    assert_eq!(json["kind"], "JobStarted");
    assert_eq!(json["job_id"], "job-7");
    assert_eq!(json["job"]["job_id"], "job-7");
    post(
        "/api/v1/mail/archive-ask",
        serde_json::json!({ "question": "When did we agree the launch date?", "filters": { "from": "alice@example.com" } }),
    )
    .await;

    let seen = seen.lock().unwrap();
    let requests = seen
        .iter()
        .filter(|request| !matches!(request, Request::ListAccounts))
        .collect::<Vec<_>>();
    assert!(matches!(
        requests[0],
        Request::ListOwedReplies { account_id, older_than_days: Some(3), within_days: None, limit: 5, all: false }
            if account_id == &default_account
    ));
    assert!(matches!(
        requests[1],
        Request::ListOwedReplies { account_id, within_days: Some(30), limit: 50, all: true, .. }
            if account_id == &other_account
    ));
    assert!(matches!(
        requests[2],
        Request::ExplainEntity { account_id, query, limit: 10 }
            if account_id == &other_account && query == "alice@example.com"
    ));
    assert!(matches!(
        requests[3],
        Request::SendTimeRecommendation { account_id, recipients, proposed_at: None }
            if account_id == &default_account
                && recipients == &vec!["a@example.com".to_string(), "b@example.com".to_string()]
    ));
    assert!(matches!(requests[4], Request::ListSavedSearchUnreadCounts));
    assert!(matches!(
        requests[5],
        Request::ListCadenceDrift { account_id } if account_id == &default_account
    ));
    assert!(matches!(
        requests[6],
        Request::ListCadenceWatch { account_id } if account_id == &other_account
    ));
    assert!(matches!(
        requests[7],
        Request::WatchCadence { account_id, email, expected_days: Some(days), allow_list_sender: false, .. }
            if account_id == &default_account && email == "mum@example.com" && (*days - 7.0).abs() < f64::EPSILON
    ));
    assert!(matches!(
        requests[8],
        Request::UnwatchCadence { account_id, .. } if account_id == &other_account
    ));
    assert!(matches!(
        requests[9],
        Request::StartMutationJob {
            mutation: mxr_protocol::MutationCommand::Archive { message_ids },
            ..
        } if message_ids == &vec![message_id.clone()]
    ));
    assert!(matches!(
        requests[10],
        Request::ArchiveAsk { question, filters, limit: 8 }
            if question.starts_with("When did") && filters.from.as_deref() == Some("alice@example.com")
    ));
}

/// `dry_run: true` on unsubscribe-purge returns the would-affect selection
/// (`status: "preview"`, `message_count`, `message_ids`) without mutating.
#[tokio::test]
async fn unsubscribe_purge_dry_run_returns_would_affect_ids() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let message_ids = vec![MessageId::new(), MessageId::new()];
    let ids_for_ipc = message_ids.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::UnsubscribePurge {
                address,
                dry_run: true,
                ..
            } => Some(Response::Ok {
                data: ResponseData::UnsubscribePurgeResult {
                    result: mxr_protocol::UnsubscribePurgeResultData {
                        address,
                        query: "from:news@example.com".into(),
                        account_id: None,
                        dry_run: true,
                        method: UnsubscribeMethod::None,
                        status: mxr_protocol::UnsubscribePurgeStatusData::Preview,
                        message_count: ids_for_ipc.len() as u32,
                        archived_count: 0,
                        message_ids: ids_for_ipc.clone(),
                        mutation_id: None,
                        error: None,
                        preview_token: Some("tok".into()),
                    },
                },
            }),
            _ => Some(Response::error("dry run must not mutate")),
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

    let json: serde_json::Value = test_http_client()
        .post(format!(
            "http://{addr}/api/v1/mail/actions/unsubscribe-purge"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({ "address": "news@example.com", "dry_run": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["result"]["status"], "preview");
    assert_eq!(json["result"]["dry_run"], true);
    assert_eq!(json["result"]["message_count"], 2);
    assert_eq!(json["result"]["archived_count"], 0);
    assert_eq!(
        json["result"]["message_ids"],
        serde_json::json!([message_ids[0].to_string(), message_ids[1].to_string()])
    );
}

#[tokio::test]
async fn inline_image_route_serves_materialized_cid_parts_only() {
    use mxr_core::types::{HtmlImageAsset, HtmlImageAssetStatus, HtmlImageSourceKind};

    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let image_path = temp.path().join("logo.png");
    std::fs::write(&image_path, b"\x89PNG fake bytes").unwrap();
    let message_id = MessageId::new();
    let served_path = image_path.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| match request {
            Request::GetHtmlImageAssets {
                message_id: id,
                allow_remote,
            } => {
                assert!(
                    !allow_remote,
                    "the inline route must never fetch remote images"
                );
                Some(Response::Ok {
                    data: ResponseData::HtmlImageAssets {
                        message_id: id,
                        assets: vec![
                            HtmlImageAsset {
                                source: "cid:logo@acme".into(),
                                kind: HtmlImageSourceKind::Cid,
                                status: HtmlImageAssetStatus::Ready,
                                mime_type: Some("image/png".into()),
                                path: Some(served_path.clone()),
                                detail: None,
                            },
                            HtmlImageAsset {
                                source: "cid:pixel.svg".into(),
                                kind: HtmlImageSourceKind::Cid,
                                status: HtmlImageAssetStatus::Ready,
                                mime_type: Some("image/svg+xml".into()),
                                path: Some(served_path.clone()),
                                detail: None,
                            },
                            HtmlImageAsset {
                                source: "https://tracker.example/pixel.gif".into(),
                                kind: HtmlImageSourceKind::Remote,
                                status: HtmlImageAssetStatus::Ready,
                                mime_type: Some("image/gif".into()),
                                path: Some(served_path.clone()),
                                detail: None,
                            },
                        ],
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
    let client = test_http_client();
    let url = |source: &str| {
        format!(
            "http://{addr}/api/v1/mail/messages/{message_id}/inline-image?source={}",
            urlencoding(source)
        )
    };

    let ok = client
        .get(url("cid:logo@acme"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), reqwest::StatusCode::OK);
    assert_eq!(ok.headers()["content-type"], "image/png");
    assert_eq!(ok.headers()["x-content-type-options"], "nosniff");
    assert_eq!(ok.headers()["content-disposition"], "inline");
    assert_eq!(
        ok.headers()["content-security-policy"],
        "default-src 'none'; sandbox"
    );
    assert_eq!(ok.bytes().await.unwrap().as_ref(), b"\x89PNG fake bytes");

    let svg = client
        .get(url("cid:pixel.svg"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(svg.status(), reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(svg.headers().get("content-type").is_none());

    let missing = client
        .get(url("cid:nope"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

    let remote = client
        .get(url("https://tracker.example/pixel.gif"))
        .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(remote.status(), reqwest::StatusCode::NOT_FOUND);
}

fn urlencoding(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[tokio::test]
async fn draft_revision_conflict_is_409_and_preserves_browser_scratch_text() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let account_id = AccountId::new();
    let id = DraftId::new();
    let store = FakeDraftStore::with(vec![markdown_draft(
        &id,
        &account_id,
        "Concurrent draft",
        "original",
        Utc::now(),
    )]);
    let _ipc = spawn_fake_ipc_server(
        &socket,
        draft_store_responder(store.clone(), sample_account(&account_id)),
        None,
    )
    .await;
    let address = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket.clone(), TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();
    let second_bridge = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket.clone(), TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();
    let client = test_http_client();
    let first = restore_session_path(&client, address, &id).await;
    let second = restore_session_path(&client, second_bridge, &id).await;
    edit_session(
        &client,
        address,
        &first,
        "Concurrent draft",
        "accepted browser text",
    )
    .await;
    let response = client.post(format!("http://{second_bridge}/api/v1/mail/compose/session/update")).bearer_auth(TEST_AUTH_TOKEN)
        .json(&json!({"draft_path": second, "expected_revision": 1, "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Concurrent draft", "from": "me@example.com", "body": "stale browser text"})).send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
    let error: serde_json::Value = response.json().await.unwrap();
    assert_eq!(error["code"], "draft_revision_conflict");
    assert_eq!(error["details"]["current_revision"], 2);
    assert_eq!(
        store.snapshot()[0].content.analysis_text(),
        "accepted browser text"
    );
    let recovered = error["recovery_session"]["draft_path"].as_str().unwrap();
    assert_ne!(recovered, second);
    assert!(read_compose_file(Path::new(recovered))
        .await
        .unwrap()
        .contains("stale browser text"));
    assert_eq!(error["recovery_session"]["revision"], 1);
    assert!(error.get("accepted_draft").is_none());
    remove_compose_file(Path::new(recovered)).await.unwrap();
    assert_eq!(
        read_draft_session_metadata(Path::new(&second))
            .await
            .unwrap()
            .unwrap()
            .revision,
        Some(1)
    );
    remove_compose_file(Path::new(&first)).await.unwrap();
    remove_compose_file(Path::new(&second)).await.unwrap();
}

#[tokio::test]
async fn global_sync_endpoints_dispatch_after_current_protocol_check() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Request>::new()));
    let recorded_requests = requests.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            recorded_requests.lock().unwrap().push(request.clone());
            let data = match request {
                Request::GetStatus => status_for_protocol(IPC_PROTOCOL_VERSION),
                Request::SyncAll => ResponseData::SyncBatch {
                    results: Vec::new(),
                },
                Request::PreviewSyncAll => ResponseData::SyncPlan {
                    targets: Vec::new(),
                },
                _ => return None,
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

    for path in ["/api/v1/mail/sync/all", "/api/v1/mail/sync/preview"] {
        let response = client
            .post(format!("http://{addr}{path}"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }

    let requests = requests.lock().unwrap();
    assert!(matches!(
        requests.as_slice(),
        [
            Request::GetStatus,
            Request::SyncAll,
            Request::GetStatus,
            Request::PreviewSyncAll
        ]
    ));
}

fn status_for_protocol(protocol_version: u32) -> ResponseData {
    let mut status = sample_status();
    if let ResponseData::Status {
        protocol_version: status_version,
        ..
    } = &mut status
    {
        *status_version = protocol_version;
    }
    status
}

/// SEC-4: a browser-marked cross-site request is refused even when it carries
/// a valid credential. Without this check the hosted-browser cookie could be
/// driven from another origin.
#[tokio::test]
async fn cross_site_requests_are_forbidden_even_with_a_valid_token() {
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
    let url = format!("http://{addr}/api/v1/i18n");

    let forged_origin = test_http_client()
        .get(&url)
        .bearer_auth(TEST_AUTH_TOKEN)
        .header("origin", "http://evil.example.com")
        .send()
        .await
        .unwrap();
    assert_eq!(forged_origin.status(), reqwest::StatusCode::FORBIDDEN);

    let forged_fetch_metadata = test_http_client()
        .get(&url)
        .bearer_auth(TEST_AUTH_TOKEN)
        .header("sec-fetch-site", "cross-site")
        .send()
        .await
        .unwrap();
    assert_eq!(
        forged_fetch_metadata.status(),
        reqwest::StatusCode::FORBIDDEN
    );

    for origin in [
        "http://localhost:5173",
        &url[..url.len() - "/api/v1/i18n".len()],
    ] {
        let allowed = test_http_client()
            .get(&url)
            .bearer_auth(TEST_AUTH_TOKEN)
            .header("origin", origin)
            .send()
            .await
            .unwrap();
        assert_eq!(allowed.status(), reqwest::StatusCode::OK, "{origin}");
    }
}

/// SEC-4: every `.route(...)` literal in the bridge's router modules is probed
/// unauthenticated at every mount point. A matched route must answer 401 with
/// no token, so the only public `/api/v1` routes are the two asserted above.
/// A literal mounted nowhere answers 404 and is ignored.
#[tokio::test]
async fn every_api_v1_route_requires_a_bridge_token() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();
    let client = reqwest::Client::new();

    let mut matched_paths = 0usize;
    for literal in bridge_route_literals() {
        let prefixes: &[&str] = if literal.starts_with("/api/v1") {
            &[""]
        } else {
            &[
                "/api/v1/mail",
                "/api/v1/platform",
                "/api/v1/admin",
                "/api/v1/client",
                "/api/v1",
            ]
        };
        for prefix in prefixes {
            let path = format!("{prefix}{literal}");
            let mut statuses = Vec::new();
            for method in [reqwest::Method::GET, reqwest::Method::POST] {
                let mut request = client.request(method.clone(), format!("http://{addr}{path}"));
                if method == reqwest::Method::POST {
                    request = request.json(&serde_json::json!({}));
                }
                let status = request.send().await.unwrap().status();
                // 404: this literal is not mounted at this prefix. 405: the
                // path exists but the other method is the one registered, and
                // axum answers method-not-allowed before the auth layer.
                if status != reqwest::StatusCode::NOT_FOUND
                    && status != reqwest::StatusCode::METHOD_NOT_ALLOWED
                {
                    statuses.push(status);
                }
            }
            if statuses.is_empty() {
                continue;
            }
            matched_paths += 1;
            for status in statuses {
                if path == "/api/v1/health" || path == "/api/v1/auth/local-token" {
                    assert_eq!(status, reqwest::StatusCode::OK, "{path} is public");
                } else {
                    assert_eq!(
                        status,
                        reqwest::StatusCode::UNAUTHORIZED,
                        "{path} answered without a token; the public allowlist must be \
                         exactly /api/v1/health and /api/v1/auth/local-token"
                    );
                }
            }
        }
    }
    assert!(
        matched_paths > 250,
        "route inventory unexpectedly small ({matched_paths}); the source scan missed the router"
    );
}

/// Every route literal in the modules that build the bridge router, with
/// `{param}` placeholders replaced by a concrete segment.
fn bridge_route_literals() -> Vec<String> {
    const MODULES: &[&str] = &[
        "router.rs",
        "routes_v6.rs",
        "arrival_routes.rs",
        "assistant_routes.rs",
        "chime_routes.rs",
        "compose_assist_routes.rs",
        "freshness_routes.rs",
        "insight_routes.rs",
        "keymap_routes.rs",
        "messages_routes.rs",
        "mode_routes.rs",
        "place_routes.rs",
        "promise_routes.rs",
        "reading_routes.rs",
        "record_routes.rs",
        "thread_context_routes.rs",
        "time_routes.rs",
        "todo_routes.rs",
        "updates_routes.rs",
    ];
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut literals: Vec<String> = Vec::new();
    for module in MODULES {
        let text = std::fs::read_to_string(src.join(module))
            .unwrap_or_else(|error| panic!("read {module}: {error}"));
        let mut rest = text.as_str();
        while let Some(at) = rest.find(".route(") {
            rest = &rest[at + ".route(".len()..];
            let Some(after_quote) = rest.trim_start().strip_prefix('"') else {
                continue;
            };
            let Some(end) = after_quote.find('"') else {
                continue;
            };
            let literal = &after_quote[..end];
            if literal.starts_with('/') {
                literals.push(concrete_path(literal));
            }
            rest = &after_quote[end..];
        }
    }
    literals.sort();
    literals.dedup();
    literals
}

fn concrete_path(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(character) = chars.next() {
        if character == '{' {
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
            }
            out.push_str("test");
        } else {
            out.push(character);
        }
    }
    out
}
