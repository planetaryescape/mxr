//! Connection setup applies equally to request and event-only consumers.
use super::*;
use mxr_transport::{MemoryTransport, ServerTransport};

fn reply(id: u64, data: ResponseData) -> IpcMessage {
    IpcMessage {
        expected_workspace: None,
        id,
        source: ClientKind::Daemon,
        payload: IpcPayload::Response(Response::Ok { data }),
    }
}

#[tokio::test]
async fn every_surface_declares_before_an_event_only_consumer_returns() {
    for kind in [
        ClientKind::Cli,
        ClientKind::Tui,
        ClientKind::Web,
        ClientKind::Mcp,
        ClientKind::Agent,
        ClientKind::Script,
        ClientKind::Human,
        ClientKind::Assistant,
        ClientKind::Daemon,
    ] {
        let transport = MemoryTransport::new();
        let mut listener = transport.bind().await.expect("listener");
        let connector = transport.connector();
        let server = tokio::spawn(async move {
            let (io, _) = listener.accept().await.expect("accept");
            let mut framed = Framed::new(io, IpcCodec::new());
            let declaration = framed.next().await.expect("declaration").expect("decode");
            assert_eq!(declaration.source, kind);
            assert!(
                matches!(declaration.payload, IpcPayload::Request(Request::DeclareClient { kind: declared, protocol_version })
                if declared == kind && protocol_version == mxr_protocol::IPC_PROTOCOL_VERSION)
            );
            framed
                .send(reply(declaration.id, ResponseData::Ack))
                .await
                .expect("ack");
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: 0,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Event(DaemonEvent::EventsLagged { skipped: 0 }),
                })
                .await
                .expect("event");
            // No ordinary request is necessary to receive the first event.
            assert!(framed.next().await.is_none());
        });
        let mut connection = IpcConnection::connect_with(&connector, kind)
            .await
            .expect("declared connection");
        assert!(matches!(
            connection.next_event().await.expect("event").payload,
            IpcPayload::Event(DaemonEvent::EventsLagged { skipped: 0 })
        ));
        drop(connection);
        server.await.expect("server");
    }
}

#[tokio::test(start_paused = true)]
async fn connection_setup_is_bounded_when_the_peer_never_acknowledges() {
    let transport = MemoryTransport::new();
    let mut listener = transport.bind().await.expect("listener");
    let connector = transport.connector();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (io, _) = listener.accept().await.expect("accept");
        let mut framed = Framed::new(io, IpcCodec::new());
        let frame = framed.next().await.expect("declaration").expect("decode");
        assert!(matches!(
            frame.payload,
            IpcPayload::Request(Request::DeclareClient { .. })
        ));
        let _ = stopped.await;
    });
    let result = IpcConnection::connect_with(&connector, ClientKind::Mcp).await;
    assert!(matches!(result, Err(ClientError::Timeout(limit)) if limit == Duration::from_secs(5)));
    stop.send(()).expect("stop fixture");
    server.await.expect("server");
}

#[tokio::test]
async fn setup_rejects_events_and_wrong_acknowledgements_instead_of_hiding_them() {
    for premature_event in [false, true] {
        let transport = MemoryTransport::new();
        let mut listener = transport.bind().await.expect("listener");
        let connector = transport.connector();
        let server = tokio::spawn(async move {
            let (io, _) = listener.accept().await.expect("accept");
            let mut framed = Framed::new(io, IpcCodec::new());
            let frame = framed.next().await.expect("declaration").expect("decode");
            let response = if premature_event {
                IpcMessage {
                    expected_workspace: None,
                    id: 0,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Event(DaemonEvent::EventsLagged { skipped: 7 }),
                }
            } else {
                reply(frame.id, ResponseData::Pong)
            };
            framed.send(response).await.expect("wrong setup frame");
        });
        let result = IpcConnection::connect_with(&connector, ClientKind::Agent).await;
        if premature_event {
            assert!(matches!(
                result,
                Err(ClientError::UnexpectedFrame {
                    is_response: false,
                    ..
                })
            ));
        } else {
            assert!(matches!(
                result,
                Err(ClientError::DaemonIncompatible { .. })
            ));
        }
        server.await.expect("server");
    }
}

#[tokio::test]
async fn command_stdio_connector_declares_an_event_only_mcp_client() {
    let script = r#"
import json, struct, sys
reader, writer = sys.stdin.buffer, sys.stdout.buffer
size = struct.unpack('>I', reader.read(4))[0]
frame = json.loads(reader.read(size))
assert frame['source'] == 'mcp'
assert frame['payload'] == {'type':'Request', 'cmd':'DeclareClient', 'kind':'mcp', 'protocol_version':int(sys.argv[1])}
def send(value):
    body = json.dumps(value).encode()
    writer.write(struct.pack('>I',len(body)) + body)
    writer.flush()
send({'id':frame['id'], 'source':'daemon', 'payload':{'type':'Response', 'status':'Ok', 'data':{'kind':'Ack'}}})
send({'id':0, 'source':'daemon', 'payload':{'type':'Event', 'event':'EventsLagged', 'skipped':0}})
"#;
    let connector = mxr_transport::CmdConnector::new(vec![
        "python3".into(),
        "-u".into(),
        "-c".into(),
        script.into(),
        mxr_protocol::IPC_PROTOCOL_VERSION.to_string(),
    ]);
    let mut connection = IpcConnection::connect_with(&connector, ClientKind::Mcp)
        .await
        .expect("declared stdio connection");
    assert!(matches!(
        connection.next_event().await.expect("stdio event").payload,
        IpcPayload::Event(DaemonEvent::EventsLagged { skipped: 0 })
    ));
}
