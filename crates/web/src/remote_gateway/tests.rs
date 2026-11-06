//! Exercise the paired WebSocket boundary with synthetic registration proof.
use super::*;
use futures::SinkExt;
use mxr_protocol::{ArchiveWorkspaceData, ResponseData, IPC_PROTOCOL_VERSION};
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, UnixListener};
use tokio_tungstenite::{tungstenite::client::IntoClientRequest, MaybeTlsStream, WebSocketStream};
use tokio_util::codec::Framed;

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct Fixture {
    _dir: tempfile::TempDir,
    url: String,
    seen: Arc<Mutex<Vec<IpcMessage>>>,
    ipc: tokio::task::JoinHandle<()>,
    http: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.http.abort();
        self.ipc.abort();
    }
}

fn owner() -> ArchiveWorkspaceData {
    ArchiveWorkspaceData {
        workspace_id: "synthetic-workspace".into(),
        recovery_generation: "synthetic-generation".into(),
    }
}

async fn fixture(missing_profile: bool) -> Fixture {
    let dir = tempfile::tempdir().expect("temp");
    let socket = dir.path().join("ipc.sock");
    let listener = UnixListener::bind(&socket).expect("ipc listener");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let records = seen.clone();
    let ipc = tokio::spawn(async move {
        while let Ok((io, _)) = listener.accept().await {
            let records = records.clone();
            tokio::spawn(async move {
                let mut framed = Framed::new(io, IpcCodec::new());
                while let Some(Ok(frame)) = framed.next().await {
                    records.lock().expect("records").push(frame.clone());
                    let response = match &frame.payload {
                        IpcPayload::Request(Request::DeclareClient { kind, .. }) if missing_profile && *kind == ClientKind::Mcp =>
                            Response::error_kinded("missing host profile", mxr_protocol::IpcErrorKind::Policy),
                        IpcPayload::Request(Request::DeclareClient { kind, protocol_version }) => {
                            assert_eq!(*kind, frame.source);
                            assert_eq!(*protocol_version, IPC_PROTOCOL_VERSION);
                            Response::Ok { data: ResponseData::Ack }
                        }
                        IpcPayload::Request(Request::GetStatus) => Response::Ok { data: serde_json::from_value(serde_json::json!({
                            "kind":"Status", "uptime_secs":0, "accounts":[], "total_messages":0,
                            "protocol_version":IPC_PROTOCOL_VERSION, "daemon_version":env!("CARGO_PKG_VERSION"),
                            "daemon_build_id":mxr_client::current_daemon_build_id(),
                        })).expect("status") },
                        IpcPayload::Request(Request::RemoteRead { client_id, mail_secret, request, .. }) => {
                            assert_eq!(client_id, "synthetic-client");
                            assert_eq!(mail_secret.expose(), "a".repeat(64));
                            let data = match &**request {
                                Request::GetRemoteReady => ResponseData::RemoteReady {
                                    accepting_mail: true,
                                    boot_id: "synthetic-boot".into(),
                                    shutdown_deadline_at: None,
                                    protocol_version: IPC_PROTOCOL_VERSION, host_id: "synthetic-host".into(), workspace: owner(), client_id: client_id.clone(),
                                    capabilities: vec![], attachment_transfer_supported: false, draft_attachment_upload_supported: false, draft_revision_supported: false, send_outcome_supported: false,
                                },
                                Request::Ping => ResponseData::Pong,
                                other => panic!("unexpected remote request {other:?}"),
                            };
                            Response::Ok { data }
                        }
                        IpcPayload::Request(Request::RemoteAuthority { .. }) => {
                            Response::Ok { data: ResponseData::Ack }
                        }
                        other => panic!("unexpected frame {other:?}"),
                    };
                    if framed
                        .send(IpcMessage {
                            expected_workspace: None,
                            id: frame.id,
                            source: ClientKind::Daemon,
                            payload: IpcPayload::Response(response),
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            });
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("http listener");
    let addr = listener.local_addr().expect("address");
    let http = tokio::spawn(async move {
        axum::serve(listener, router(socket)).await.expect("serve");
    });
    Fixture {
        _dir: dir,
        url: format!("ws://{addr}/api/v1/remote/ipc"),
        seen,
        ipc,
        http,
    }
}

async fn connect(fixture: &Fixture) -> Socket {
    let mut request = fixture.url.clone().into_client_request().expect("request");
    request.headers_mut().insert(
        "authorization",
        format!("Bearer synthetic-client:{}", "a".repeat(64))
            .parse()
            .expect("proof"),
    );
    tokio_tungstenite::connect_async(request)
        .await
        .expect("websocket")
        .0
}

fn encode(
    id: u64,
    source: ClientKind,
    request: Request,
) -> tokio_tungstenite::tungstenite::Message {
    let mut bytes = BytesMut::new();
    IpcCodec::new()
        .encode(
            IpcMessage {
                expected_workspace: Some(owner()),
                id,
                source,
                payload: IpcPayload::Request(request),
            },
            &mut bytes,
        )
        .expect("encode");
    tokio_tungstenite::tungstenite::Message::Binary(bytes.freeze())
}

async fn receive(socket: &mut Socket) -> IpcMessage {
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
        .await
        .expect("response deadline")
        .expect("open")
        .expect("frame");
    let tokio_tungstenite::tungstenite::Message::Binary(bytes) = frame else {
        panic!("expected IPC response");
    };
    IpcCodec::new()
        .decode(&mut BytesMut::from(&bytes[..]))
        .expect("decode")
        .expect("IPC frame")
}

#[tokio::test]
async fn paired_gateway_preserves_declared_kind_proof_and_workspace_and_rejects_switches() {
    let fixture = fixture(false).await;
    for kind in [
        ClientKind::Cli,
        ClientKind::Web,
        ClientKind::Agent,
        ClientKind::Mcp,
    ] {
        let mut socket = connect(&fixture).await;
        socket
            .send(encode(
                1,
                kind,
                Request::DeclareClient {
                    kind,
                    protocol_version: IPC_PROTOCOL_VERSION,
                },
            ))
            .await
            .expect("declare");
        assert!(matches!(
            receive(&mut socket).await.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ));
        socket
            .send(encode(2, kind, Request::Ping))
            .await
            .expect("ping");
        assert!(matches!(
            receive(&mut socket).await.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Pong
            })
        ));
        let before = {
            let records = fixture.seen.lock().expect("records");
            let frame = records.last().expect("remote ping");
            assert_eq!(frame.source, kind);
            assert!(
                matches!(&frame.payload, IpcPayload::Request(Request::RemoteRead { expected_workspace, request, .. })
                if expected_workspace.as_ref() == Some(&owner()) && matches!(&**request, Request::Ping))
            );
            records.len()
        };
        let conflict = if kind == ClientKind::Cli {
            ClientKind::Web
        } else {
            ClientKind::Cli
        };
        socket
            .send(encode(3, conflict, Request::Ping))
            .await
            .expect("conflict");
        assert!(matches!(
            receive(&mut socket).await.payload,
            IpcPayload::Response(Response::Error {
                kind: mxr_protocol::IpcErrorKind::Policy,
                ..
            })
        ));
        assert_eq!(
            fixture.seen.lock().expect("records").len(),
            before,
            "conflicting identity reached daemon admission"
        );
        socket.close(None).await.expect("close");
    }
}

#[tokio::test]
async fn paired_gateway_reports_missing_host_profile_and_never_admits_undeclared_traffic() {
    let fixture = fixture(true).await;
    let mut socket = connect(&fixture).await;
    socket
        .send(encode(
            1,
            ClientKind::Mcp,
            Request::DeclareClient {
                kind: ClientKind::Mcp,
                protocol_version: IPC_PROTOCOL_VERSION,
            },
        ))
        .await
        .expect("declare");
    let response = receive(&mut socket).await;
    assert!(
        matches!(response.payload, IpcPayload::Response(Response::Error { kind: mxr_protocol::IpcErrorKind::Policy, message, .. })
        if message.contains("[agents.profiles.mcp]") && message.contains("reconnect"))
    );
    let mut socket = connect(&fixture).await;
    socket
        .send(encode(1, ClientKind::Cli, Request::Ping))
        .await
        .expect("undeclared request");
    let next = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
        .await
        .expect("close");
    assert!(
        !matches!(
            next,
            Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(_)))
        ),
        "undeclared peer received IPC data"
    );
    assert!(!fixture.seen.lock().expect("records").iter().any(|frame| matches!(&frame.payload,
        IpcPayload::Request(Request::RemoteRead { request, .. }) if matches!(&**request, Request::Ping))));
    let error = tokio_tungstenite::connect_async(&fixture.url)
        .await
        .expect_err("headerless clients cannot authenticate");
    assert!(error.to_string().contains("401"));
}
