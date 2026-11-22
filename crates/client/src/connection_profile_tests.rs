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
