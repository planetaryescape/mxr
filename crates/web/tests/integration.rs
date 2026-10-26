//! Slice 7 — out-of-process integration harness.
//!
//! These tests spin up the bridge router in-process backed by a fake
//! Unix-socket IPC server that can answer any `Request` variant and
//! emit any `DaemonEvent`. They sit beside the existing unit tests in
//! `crates/web/src/lib.rs`; the goal of this file is to be the harness
//! the v0.5 release verification flow runs against (and Schemathesis
//! talks to in CI — see .github/workflows/openapi-conformance.yml).
//!
//! These tests do NOT exercise a real `mxr daemon` process — that's
//! covered in `crates/daemon/tests/daemon_lifecycle.rs`. The point here
//! is the HTTP↔IPC contract. Real-daemon-against-FakeProvider end-to-end
//! coverage lives with the daemon and web app smoke tests.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests use panic and unwrap for direct fixture failures"
)]

use futures::{SinkExt, StreamExt};
use mxr_core::id::{AccountId, MessageId};
use mxr_protocol::{
    DaemonEvent, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData,
    IPC_PROTOCOL_VERSION,
};
use mxr_web::{bind_and_serve, WebServerConfig};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::net::UnixListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::codec::Framed;

const TEST_TOKEN: &str = "integration-test-token";

/// Spawn a fake IPC server that calls `responder` for every Request and
/// optionally emits `events` to every connected client right after
/// accepting. Returns the join handle of the accept loop.
fn spawn_fake<F>(
    socket_path: &Path,
    responder: F,
    events: Vec<DaemonEvent>,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(Request) -> Option<Response> + Send + Sync + 'static,
{
    let responder = Arc::new(responder);
    let events = Arc::new(events);
    let listener = UnixListener::bind(socket_path).unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let responder = responder.clone();
            let events = events.clone();
            tokio::spawn(async move {
                let mut framed = Framed::new(stream, IpcCodec::new());
                for event in events.iter() {
                    let _ = framed
                        .send(IpcMessage {
                            expected_workspace: None,
                            id: 0,
                            source: ::mxr_protocol::ClientKind::default(),
                            payload: IpcPayload::Event(event.clone()),
                        })
                        .await;
                }
                while let Some(message) = framed.next().await {
                    let Ok(message) = message else { break };
                    if let IpcPayload::Request(request) = message.payload {
                        let response = if matches!(request, Request::GetArchiveWorkspace) {
                            Some(Response::Ok {
                                data: ResponseData::ArchiveWorkspace {
                                    workspace: mxr_protocol::ArchiveWorkspaceData {
                                        workspace_id: "00000000-0000-7000-8000-000000000001".into(),
                                        recovery_generation: "00000000-0000-7000-8000-000000000002"
                                            .into(),
                                    },
                                },
                            })
                        } else {
                            responder(request)
                        };
                        if let Some(response) = response {
                            let _ = framed
                                .send(IpcMessage {
                                    expected_workspace: None,
                                    id: message.id,
                                    source: ::mxr_protocol::ClientKind::default(),
                                    payload: IpcPayload::Response(response),
                                })
                                .await;
                        }
                    }
                }
            });
        }
    })
}

async fn boot_bridge(socket_path: std::path::PathBuf) -> SocketAddr {
    bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_TOKEN.into()),
    )
    .await
    .unwrap()
}

// --------------------------------------------------------------------------
// suite: discovery surface

#[tokio::test]
async fn openapi_spec_is_3_1_and_lists_v1_paths() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let _server = spawn_fake(&socket, |_| None, vec![]);
    let addr = boot_bridge(socket).await;

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/v1/openapi.json"))
        .header("x-mxr-bridge-token", TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["openapi"], "3.1.0");

    // bearer scheme registered for SDK / Swagger UI
    assert_eq!(
        json["components"]["securitySchemes"]["bearer"]["scheme"],
        "bearer"
    );
}

#[tokio::test]
async fn openapi_documented_paths_are_reachable_by_documented_method() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let _server = spawn_fake(
        &socket,
        |_| {
            Some(Response::error_kinded(
                "conformance fixture",
                mxr_protocol::IpcErrorKind::Unsupported,
            ))
        },
        vec![],
    );
    let addr = boot_bridge(socket).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let spec: serde_json::Value = client
        .get(format!("http://{addr}/api/v1/openapi.json"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let paths = spec["paths"].as_object().expect("OpenAPI paths object");

    let mut failures = Vec::new();
    for (path, methods) in paths {
        let Some(methods) = methods.as_object() else {
            continue;
        };
        for method in methods.keys() {
            if path == "/api/v1/events" {
                continue;
            }
            let method = method.to_ascii_uppercase();
            let Ok(method) = reqwest::Method::from_bytes(method.as_bytes()) else {
                continue;
            };
            let url = format!("http://{addr}{}", sample_path(path));
            let mut request = client.request(method.clone(), &url).bearer_auth(TEST_TOKEN);
            if matches!(
                method,
                reqwest::Method::POST | reqwest::Method::PUT | reqwest::Method::PATCH
            ) {
                request = request.json(&serde_json::json!({}));
            }
            let result =
                tokio::time::timeout(std::time::Duration::from_millis(250), request.send()).await;
            let status = match result {
                Ok(Ok(response)) => response.status(),
                Ok(Err(error)) => {
                    failures.push(format!("{method} {path} -> request error: {error}"));
                    continue;
                }
                Err(_) => {
                    failures.push(format!("{method} {path} -> timed out"));
                    continue;
                }
            };
            if matches!(
                status,
                reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED
            ) {
                failures.push(format!("{method} {path} -> {status}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "documented OpenAPI paths must be mounted by the bridge router:\n{}",
        failures.join("\n")
    );
}

fn sample_path(path: &str) -> String {
    let id = "018f2f9d-1111-7111-8111-111111111111";
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let (prefix, after_prefix) = rest.split_at(start);
        out.push_str(prefix);
        let Some(end) = after_prefix.find('}') else {
            out.push_str(after_prefix);
            return out;
        };
        out.push_str(id);
        rest = &after_prefix[end + 1..];
    }
    out.push_str(rest);
    out
}

#[tokio::test]
async fn health_is_unauthenticated_and_returns_protocol_version() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let _server = spawn_fake(&socket, |_| None, vec![]);
    let addr = boot_bridge(socket).await;

    let response = reqwest::get(format!("http://{addr}/api/v1/health"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["protocol_version"], IPC_PROTOCOL_VERSION);
}

// --------------------------------------------------------------------------
// suite: daemon-event coverage on the WebSocket

/// Feed every `DaemonEvent` variant through the WebSocket and assert each
/// surfaces with its `event` discriminator (the protocol uses
/// `#[serde(tag = "event")]`). Running every variant through the real
/// relay catches regressions where a new event variant was added but the
/// WS relay loop dropped it.
#[tokio::test]
async fn websocket_relays_every_daemon_event_variant() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");

    let account_id = AccountId::new();
    let message_id = MessageId::new();
    let events = vec![
        DaemonEvent::SyncCompleted {
            account_id: account_id.clone(),
            messages_synced: 3,
        },
        DaemonEvent::SyncError {
            account_id: account_id.clone(),
            error: "transient".into(),
        },
        DaemonEvent::NewMessages {
            envelopes: vec![],
            total: 0,
        },
        DaemonEvent::MessageUnsnoozed {
            message_id: message_id.clone(),
        },
        DaemonEvent::ReminderTriggered {
            sent_message_id: message_id.clone(),
        },
        DaemonEvent::LabelCountsUpdated { counts: vec![] },
        DaemonEvent::OperationStarted {
            operation_id: "op-1".into(),
            operation: "rebuild_analytics".into(),
            account_id: Some(account_id.clone()),
            message: "starting".into(),
        },
        DaemonEvent::OperationProgress {
            operation_id: "op-1".into(),
            operation: "rebuild_analytics".into(),
            account_id: Some(account_id.clone()),
            current: 5,
            total: Some(10),
            message: "halfway".into(),
        },
        DaemonEvent::OperationCompleted {
            operation_id: "op-1".into(),
            operation: "rebuild_analytics".into(),
            account_id: Some(account_id.clone()),
            message: "done".into(),
        },
        DaemonEvent::OperationFailed {
            operation_id: "op-2".into(),
            operation: "sync".into(),
            account_id: Some(account_id.clone()),
            error: "boom".into(),
            retryable: true,
        },
        DaemonEvent::OperationCancelled {
            operation_id: "op-3".into(),
            operation: "sync".into(),
            account_id: Some(account_id.clone()),
            message: "user cancelled".into(),
        },
        DaemonEvent::MutationReconciliationFailed {
            client_correlation_id: "7".into(),
            error_summary: "incomplete".into(),
        },
    ];
    let _server = spawn_fake(&socket, |_| None, events.clone());
    let addr = boot_bridge(socket).await;

    let (mut stream, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/api/v1/events?token={TEST_TOKEN}"))
            .await
            .unwrap();

    let expected_tags = [
        "SyncCompleted",
        "SyncError",
        "NewMessages",
        "MessageUnsnoozed",
        "ReminderTriggered",
        "LabelCountsUpdated",
        "OperationStarted",
        "OperationProgress",
        "OperationCompleted",
        "OperationFailed",
        "OperationCancelled",
        "MutationReconciliationFailed",
    ];
    assert_eq!(
        events.len(),
        expected_tags.len(),
        "test fixture should cover every DaemonEvent variant"
    );

    for tag in expected_tags {
        let frame = stream.next().await.expect("websocket frame").unwrap();
        let text = match frame {
            Message::Text(t) => t.to_string(),
            other => panic!("expected text frame, got {other:?}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            parsed["event"], tag,
            "event #{tag} must surface its `event` discriminator"
        );
    }
}

// --------------------------------------------------------------------------
// suite: representative HTTP routes per bucket

/// One assertion per IPC bucket — admin / mail / platform / events —
/// that the daemon-hosted bridge can dispatch and return the
/// expected ResponseData. Per the client-parity rule in
/// `.agents/skills/mxr-development/SKILL.md`: drift in a bucket should
/// fail this immediately.
#[tokio::test]
async fn one_route_per_bucket_dispatches() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");

    let _server = spawn_fake(
        &socket,
        |request| match request {
            Request::GetStatus => Some(Response::Ok {
                data: ResponseData::Status {
                    workspace_owner_supported: false,
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
                    daemon_version: Some("0.5.0".into()),
                    daemon_build_id: None,
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                    degraded: false,
                },
            }),
            Request::Ping => Some(Response::Ok {
                data: ResponseData::Pong,
            }),
            Request::ListSubscriptions { .. } => Some(Response::Ok {
                data: ResponseData::Subscriptions {
                    subscriptions: vec![],
                },
            }),
            Request::ListLabels { account_id: None } => Some(Response::Ok {
                data: ResponseData::Labels { labels: vec![] },
            }),
            Request::ListCategories { account_id: None } => Some(Response::Ok {
                data: ResponseData::Categories { categories: vec![] },
            }),
            Request::ListSavedSearches => Some(Response::Ok {
                data: ResponseData::SavedSearches { searches: vec![] },
            }),
            _ => None,
        },
        vec![],
    );
    let addr = boot_bridge(socket).await;
    let client = reqwest::Client::new();

    // admin
    let response = client
        .get(format!("http://{addr}/api/v1/admin/status"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["daemon_version"], "0.5.0");

    // admin (slice 6 addition)
    let response = client
        .post(format!("http://{addr}/api/v1/admin/ping"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // platform
    let response = client
        .get(format!("http://{addr}/api/v1/platform/subscriptions"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // platform (slice 6 addition)
    let response = client
        .get(format!("http://{addr}/api/v1/platform/saved-searches"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // client
    let response = client
        .get(format!("http://{addr}/api/v1/client/shell"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

// --------------------------------------------------------------------------
// suite: compose recipient autocomplete

fn sender_summary(email: &str, display_name: Option<&str>) -> mxr_protocol::SenderSummaryData {
    mxr_protocol::SenderSummaryData {
        account_id: AccountId::new(),
        display_name: display_name.map(str::to_owned),
        sender_email: email.to_owned(),
        message_count: 1,
        unread_count: 0,
        latest_subject: "hello".into(),
        latest_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn contacts_autocomplete_matches_addresses_and_returns_typed_suggestions() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let _server = spawn_fake(
        &socket,
        |request| match request {
            // The client probes daemon status before each request.
            Request::GetStatus => Some(Response::Ok {
                data: ResponseData::Status {
                    workspace_owner_supported: false,
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
                    daemon_version: Some("0.5.0".into()),
                    daemon_build_id: None,
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                    degraded: false,
                },
            }),
            Request::ListSenders { .. } => Some(Response::Ok {
                data: ResponseData::Senders {
                    // One row per account: the same person can appear more than once,
                    // with different casing and with a name on only some rows.
                    senders: vec![
                        sender_summary("nora@foundry.example", Some("Nora Okafor")),
                        sender_summary("NORA@foundry.example", Some("Nora Kim")),
                        sender_summary("bob@example.com", Some("   ")),
                        sender_summary("pat@example.com", None),
                        sender_summary("pat@example.com", Some("Pat Lee")),
                    ],
                },
            }),
            _ => None,
        },
        vec![],
    );
    let addr = boot_bridge(socket).await;
    let client = reqwest::Client::new();
    let autocomplete = |q: &'static str| {
        client
            .get(format!(
                "http://{addr}/api/v1/mail/contacts/autocomplete?q={q}"
            ))
            .bearer_auth(TEST_TOKEN)
            .send()
    };

    // The query matches only the address, not the display name.
    let response = autocomplete("foundry").await.unwrap();
    assert_eq!(response.status(), 200);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "contacts": [{ "name": "Nora Okafor", "email": "nora@foundry.example" }]
        })
    );

    // A blank display name is reported as absent, not as an empty string.
    let json: serde_json::Value = autocomplete("bob").await.unwrap().json().await.unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "contacts": [{ "name": null, "email": "bob@example.com" }] })
    );

    // An address seen under two accounts is suggested once, with the first name seen.
    let json: serde_json::Value = autocomplete("nora").await.unwrap().json().await.unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "contacts": [{ "name": "Nora Okafor", "email": "nora@foundry.example" }]
        })
    );

    // A name is filled from a later row when the first row for the address has none.
    let json: serde_json::Value = autocomplete("pat").await.unwrap().json().await.unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "contacts": [{ "name": "Pat Lee", "email": "pat@example.com" }] })
    );

    // The limit counts distinct addresses, so a duplicate row does not use up a slot.
    let json: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/v1/mail/contacts/autocomplete?q=example&limit=2"
        ))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "contacts": [
                { "name": "Nora Okafor", "email": "nora@foundry.example" },
                { "name": null, "email": "bob@example.com" }
            ]
        })
    );
}

// --------------------------------------------------------------------------
// suite: writing help (compose assist)

fn status_ok() -> Response {
    Response::Ok {
        data: ResponseData::Status {
            workspace_owner_supported: false,
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
            daemon_version: Some("0.5.0".into()),
            daemon_build_id: None,
            repair_required: false,
            semantic_runtime: None,
            feature_health: None,
            degraded: false,
        },
    }
}

#[tokio::test]
async fn compose_assist_returns_the_proposal_and_apply_writes_it() {
    use mxr_protocol::{
        AiLocalityData, AssistActionData, ComposeProposalData, DraftProvenanceData, DraftTargetData,
    };
    let draft_id = mxr_core::id::DraftId::new();
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let applied_draft = draft_id.clone();
    let _server = spawn_fake(
        &socket,
        move |request| match request {
            Request::GetStatus => Some(status_ok()),
            // The proposal echoes what the bridge sent, so the test can see
            // that every field of the body reached the daemon request.
            Request::ComposeAssist {
                draft,
                text,
                selection,
                action,
                instruction,
                adjust,
                model,
                revise,
            } => {
                let kind = match draft {
                    DraftTargetData::Stored { .. } => "stored",
                    DraftTargetData::Reply { all: true, .. } => "reply-all",
                    DraftTargetData::Reply { .. } => "reply",
                    DraftTargetData::Forward { .. } => "forward",
                    DraftTargetData::New { .. } => "new",
                };
                let selection = selection.map(|range| (range.start, range.end));
                Some(Response::Ok {
                    data: ResponseData::ComposeProposal {
                        proposal: ComposeProposalData {
                            proposal_id: "p1".into(),
                            action: action.unwrap_or(AssistActionData::Finish),
                            base: "abc".into(),
                            text: format!(
                                "{kind}|{text}|{selection:?}|{instruction:?}|{}|{model:?}|{revise:?}",
                                adjust.shorter,
                            ),
                            changes: vec![],
                            gaps: vec![],
                            provenance: DraftProvenanceData {
                                model: "m".into(),
                                locality: AiLocalityData::Local,
                                history_used: false,
                                voice_examples: vec![],
                                conversation: vec![],
                                rewrite: None,
                            },
                            yours_changed: vec![1],
                            composer_version: "c1".into(),
                            style_marks: Vec::new(),
                            style_score: 100,
                            style_version: String::new(),
                        },
                    },
                })
            }
            Request::ApplyComposeProposal {
                proposal_id,
                draft_id,
            } => Some(if proposal_id == "p1" && draft_id == applied_draft {
                Response::Ok {
                    data: ResponseData::ComposeApplied {
                        draft_id,
                        text_hash: "deadbeef".into(),
                    },
                }
            } else {
                Response::error_kinded(
                    "That proposal expired or was never made.",
                    mxr_protocol::IpcErrorKind::InvalidRequest,
                )
            }),
            _ => None,
        },
        vec![],
    );
    let addr = boot_bridge(socket).await;
    let client = reqwest::Client::new();

    // Reply, with a selection and a chip.
    let message_id = MessageId::new();
    let response = client
        .post(format!("http://{addr}/api/v1/mail/compose/assist"))
        .bearer_auth(TEST_TOKEN)
        .json(&serde_json::json!({
            "draft": {"kind": "reply", "message_id": message_id, "all": true},
            "text": "Thursday works.",
            "selection": {"start": 0, "end": 8},
            "instruction": "warmer",
            "adjust": {"shorter": true},
            "model": "gpt-6-luna",
            "revise": "p0",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let json: serde_json::Value = response.json().await.unwrap();
    // The answer is the typed envelope `{ "proposal": … }`, not a tagged variant.
    assert!(json.get("kind").is_none(), "{json}");
    assert_eq!(json["proposal"]["proposal_id"], "p1");
    assert_eq!(json["proposal"]["action"], "finish");
    assert_eq!(
        json["proposal"]["text"],
        "reply-all|Thursday works.|Some((0, 8))|Some(\"warmer\")|true|Some(\"gpt-6-luna\")|Some(\"p0\")"
    );
    assert_eq!(json["proposal"]["yours_changed"], serde_json::json!([1]));
    assert!(json["proposal"]["provenance"].is_object());

    // Optional fields may be left out.
    let json: serde_json::Value = client
        .post(format!("http://{addr}/api/v1/mail/compose/assist"))
        .bearer_auth(TEST_TOKEN)
        .json(&serde_json::json!({
            "draft": {"kind": "stored", "draft_id": draft_id},
            "text": "",
            "action": "write",
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(json["proposal"]["action"], "write");
    assert_eq!(
        json["proposal"]["text"],
        "stored||None|None|false|None|None"
    );

    // A body the daemon would not understand is refused here.
    let bad = client
        .post(format!("http://{addr}/api/v1/mail/compose/assist"))
        .bearer_auth(TEST_TOKEN)
        .json(&serde_json::json!({"draft": {"kind": "elsewhere"}, "text": ""}))
        .send()
        .await
        .unwrap();
    assert!(bad.status().is_client_error(), "{}", bad.status());

    // The routes sit behind the bridge token.
    let unauthorized = client
        .post(format!("http://{addr}/api/v1/mail/compose/assist"))
        .json(&serde_json::json!({"draft": {"kind": "stored", "draft_id": draft_id}, "text": ""}))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), 401);

    // Apply answers the typed data of `ComposeApplied`.
    let applied = client
        .post(format!("http://{addr}/api/v1/mail/compose/apply"))
        .bearer_auth(TEST_TOKEN)
        .json(&serde_json::json!({"proposal_id": "p1", "draft_id": draft_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(applied.status(), 200);
    assert_eq!(
        applied.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!({"draft_id": draft_id, "text_hash": "deadbeef"})
    );

    // A daemon refusal reaches the caller as a validation error with its words.
    let refused = client
        .post(format!("http://{addr}/api/v1/mail/compose/apply"))
        .bearer_auth(TEST_TOKEN)
        .json(&serde_json::json!({"proposal_id": "gone", "draft_id": draft_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 422);
    assert!(refused.text().await.unwrap().contains("expired"));

    // A draft id that is not one is the caller's mistake.
    let malformed = client
        .post(format!("http://{addr}/api/v1/mail/compose/apply"))
        .bearer_auth(TEST_TOKEN)
        .json(&serde_json::json!({"proposal_id": "p1", "draft_id": "nope"}))
        .send()
        .await
        .unwrap();
    assert_eq!(malformed.status(), 400);
}

// --------------------------------------------------------------------------
// suite: legacy migration

#[tokio::test]
async fn legacy_path_redirect_round_trip_smoke() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let _server = spawn_fake(
        &socket,
        |request| match request {
            Request::GetStatus => Some(Response::Ok {
                data: ResponseData::Status {
                    workspace_owner_supported: false,
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
            }),
            _ => None,
        },
        vec![],
    );
    let addr = boot_bridge(socket).await;

    // reqwest's default Policy follows up to 10 redirects; the v0.4.x
    // client should land on the v1 handler without intervention.
    let response = reqwest::Client::new()
        .get(format!("http://{addr}/status"))
        .bearer_auth(TEST_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["protocol_version"], IPC_PROTOCOL_VERSION);
}

#[tokio::test]
async fn archive_boundary_errors_are_structured_before_any_ipc_dispatch() {
    let temp = TempDir::new().unwrap();
    let addr = boot_bridge(temp.path().join("no-daemon.sock")).await;
    for route in ["preview", "submit", "query"] {
        let response = reqwest::Client::new()
            .post(format!(
                "http://{addr}/api/v1/mail/archive-operations/{route}"
            ))
            .bearer_auth(TEST_TOKEN)
            .json(&serde_json::json!({"identity": 42}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["code"], "bad_request");
        assert!(body["error"]
            .as_str()
            .unwrap()
            .contains("Invalid Archive request"));
    }
}
