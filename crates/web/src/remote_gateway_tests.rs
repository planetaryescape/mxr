use super::remote_gateway;
use tempfile::TempDir;
use tokio::{net::TcpListener, task::JoinHandle};

async fn gateway() -> (TempDir, String, JoinHandle<()>) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("missing.sock");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, remote_gateway::router(socket))
            .await
            .unwrap();
    });
    (dir, format!("http://{address}"), task)
}

fn websocket_request(client: &reqwest::Client, url: String) -> reqwest::RequestBuilder {
    client
        .get(url)
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
}

#[tokio::test]
async fn gateway_exposes_only_pairing_cleanup_and_authenticated_mail_routes() {
    let (_dir, base, server) = gateway().await;
    let client = reqwest::Client::new();

    let missing_proof = websocket_request(&client, format!("{base}/api/v1/remote/ipc"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_proof.status(), reqwest::StatusCode::UNAUTHORIZED);

    let browser_origin = websocket_request(&client, format!("{base}/api/v1/remote/ipc"))
        .header("origin", "https://example.test")
        .header("authorization", format!("Bearer client:{}", "a".repeat(64)))
        .send()
        .await
        .unwrap();
    assert_eq!(browser_origin.status(), reqwest::StatusCode::FORBIDDEN);

    let admin_pairing_request = client
        .post(format!("{base}/api/v1/remote/pairings"))
        .json(&serde_json::json!({"action":"Clients"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        admin_pairing_request.status(),
        reqwest::StatusCode::FORBIDDEN
    );

    let admin_cleanup_request = client
        .post(format!("{base}/api/v1/remote/self/deregister"))
        .json(&serde_json::json!({"action":"Status"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        admin_cleanup_request.status(),
        reqwest::StatusCode::FORBIDDEN
    );

    for path in [
        "/api/v1/auth/local-token",
        "/api/v1/files/config",
        "/api/v1/admin/remote",
    ] {
        let response = client.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND, "{path}");
    }

    server.abort();
}

#[tokio::test]
async fn stopping_notice_arrives_while_a_provider_request_is_still_waiting() {
    use futures::{SinkExt, StreamExt};
    use mxr_protocol::{
        ClientKind, DaemonEvent, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData,
    };
    use tokio::net::UnixListener;
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
    use tokio_util::{
        bytes::BytesMut,
        codec::{Decoder, Encoder, Framed},
    };

    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("daemon.sock");
    let daemon = UnixListener::bind(&socket).unwrap();
    let release = std::sync::Arc::new(tokio::sync::Notify::new());
    let provider = release.clone();
    let daemon_task = tokio::spawn(async move {
        while let Ok((stream, _)) = daemon.accept().await {
            let provider = provider.clone();
            tokio::spawn(async move {
                let mut stream = Framed::new(stream, IpcCodec::new());
                while let Some(Ok(frame)) = stream.next().await {
                    let IpcPayload::Request(request) = frame.payload else {
                        break;
                    };
                    let data = match request {
                        Request::DeclareClient { .. } => ResponseData::Ack,
                        Request::GetStatus => serde_json::from_value(serde_json::json!({
                            "kind":"Status","uptime_secs":0,"accounts":[],"total_messages":0,
                            "protocol_version":mxr_protocol::IPC_PROTOCOL_VERSION,
                            "daemon_version":env!("CARGO_PKG_VERSION"),
                            "daemon_build_id":mxr_client::current_daemon_build_id(),
                        })).unwrap(),
                        Request::RemoteRead { request, .. } if matches!(*request, Request::Ping) => {
                            stream.send(IpcMessage { expected_workspace: None, id:0, source:ClientKind::Daemon,
                                payload:IpcPayload::Event(DaemonEvent::ServerStopping { boot_id:"test-boot".into(), deadline_at:123 }) }).await.unwrap();
                            provider.notified().await;
                            ResponseData::Pong
                        },
                        Request::RemoteRead { .. } => serde_json::from_value(serde_json::json!({
                            "kind":"RemoteReady","boot_id":"test-boot","protocol_version":mxr_protocol::IPC_PROTOCOL_VERSION,
                            "host_id":"host","client_id":"fixture","workspace":{"workspace_id":"work","recovery_generation":"epoch"},
                        })).unwrap(),
                        Request::RemoteAuthority { .. } => serde_json::from_value(serde_json::json!({
                            "kind":"RemoteAuthority","response":{"result":"Client","client":{"client_id":"fixture","name":"test","created_at":0,"expires_at":9999999999_i64,"revoked_at":null,"capabilities":["mail_read"]},"dry_run":false},
                        })).unwrap(),
                        _ => panic!("unexpected fixture request"),
                    };
                    if stream
                        .send(IpcMessage {
                            expected_workspace: None,
                            id: frame.id,
                            source: ClientKind::Daemon,
                            payload: IpcPayload::Response(Response::Ok { data }),
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let gateway = tokio::spawn(async move {
        axum::serve(listener, remote_gateway::router(socket))
            .await
            .unwrap();
    });
    let mut request = format!("ws://{address}/api/v1/remote/ipc")
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "authorization",
        format!("Bearer fixture:{}", "a".repeat(64))
            .parse()
            .unwrap(),
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let mut codec = IpcCodec::new();
    let mut bytes = BytesMut::new();
    codec
        .encode(
            IpcMessage {
                expected_workspace: None,
                id: 1,
                source: ClientKind::Cli,
                payload: IpcPayload::Request(Request::DeclareClient {
                    kind: ClientKind::Cli,
                    protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
                }),
            },
            &mut bytes,
        )
        .unwrap();
    ws.send(Message::Binary(bytes.split().freeze()))
        .await
        .unwrap();
    let ack_frame = ws.next().await.unwrap().unwrap();
    assert!(matches!(ack_frame, Message::Binary(_)));
    codec
        .encode(
            IpcMessage {
                expected_workspace: None,
                id: 7,
                source: ClientKind::Cli,
                payload: IpcPayload::Request(Request::Ping),
            },
            &mut bytes,
        )
        .unwrap();
    ws.send(Message::Binary(bytes.freeze())).await.unwrap();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Binary(bytes) = frame else {
        panic!("expected IPC event");
    };
    let event = codec
        .decode(&mut BytesMut::from(bytes.as_ref()))
        .unwrap()
        .unwrap();
    assert!(matches!(
        event.payload,
        IpcPayload::Event(DaemonEvent::ServerStopping { .. })
    ));
    release.notify_one();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Binary(bytes) = frame else {
        panic!("expected IPC response");
    };
    let response = codec
        .decode(&mut BytesMut::from(bytes.as_ref()))
        .unwrap()
        .unwrap();
    assert_eq!(response.id, 7);
    assert!(matches!(
        response.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Pong
        })
    ));
    ws.close(None).await.unwrap();
    gateway.abort();
    daemon_task.abort();
}
