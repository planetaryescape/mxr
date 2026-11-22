//! The declared cooperative profile is effective before any ordinary request.
use super::*;

#[tokio::test]
async fn missing_malformed_and_incompatible_declarations_fail_closed() {
    for declaration in [
        serde_json::json!({"cmd":"DeclareClient", "protocol_version": mxr_protocol::IPC_PROTOCOL_VERSION}),
        serde_json::json!({"cmd":"DeclareClient", "kind":"unknown", "protocol_version": mxr_protocol::IPC_PROTOCOL_VERSION}),
        serde_json::json!({"cmd":"DeclareClient", "kind":"mcp", "protocol_version":"invalid"}),
        serde_json::json!({"cmd":"DeclareClient", "kind":"mcp"}),
    ] {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let (hot, bulk) = lanes();
        let h = DuplexHarness::start().await;
        let (mut io, server) = spawn_server(&h, state.clone(), hot, bulk, emit).await;
        let mut frame = serde_json::json!({"id":1,"source":"mcp","payload":{"type":"Request"}});
        frame["payload"]
            .as_object_mut()
            .unwrap()
            .extend(declaration.as_object().unwrap().clone());
        write_raw_frame(&mut io, &serde_json::to_vec(&frame).unwrap()).await;
        let mut framed = Framed::new(io, IpcCodec::new());
        assert!(
            tokio::time::timeout(RECV_TIMEOUT, framed.next())
                .await
                .unwrap()
                .is_none(),
            "malformed declaration must close without events"
        );
        drop(framed);
        join_server(server).await;
        state.shutdown_runtime_tasks(JOIN_TIMEOUT).await;
    }
}

#[tokio::test]
async fn setup_deadline_closes_an_event_only_peer_that_never_declares() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();
    let h = DuplexHarness::start().await;
    let (io, server) = spawn_server(&h, state.clone(), hot, bulk, emit).await;
    tokio::time::pause();
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(6)).await;
    join_server(server).await;
    let mut framed = Framed::new(io, IpcCodec::new());
    assert!(
        framed.next().await.is_none(),
        "undeclared peers receive no buffered event or lag count"
    );
    tokio::time::resume();
    state.shutdown_runtime_tasks(JOIN_TIMEOUT).await;
}
