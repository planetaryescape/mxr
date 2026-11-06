//! The declared cooperative profile is effective before any ordinary request.
use super::*;
use mxr_core::types::MessageDirection;

async fn scoped_state(kind: ClientKind) -> (Arc<AppState>, mxr_core::Envelope, mxr_core::Envelope) {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let own = state.default_account_id();
    let other = AccountId::new();
    state
        .store
        .insert_account(&crate::test_fixtures::test_account_with_id(other.clone()))
        .await
        .unwrap();
    let mut config = state.config_snapshot();
    for (profile_kind, account) in [
        (kind, &own),
        (
            if kind == ClientKind::Mcp {
                ClientKind::Agent
            } else {
                ClientKind::Mcp
            },
            &other,
        ),
    ] {
        config.agent_surfaces.profiles.insert(
            profile_kind.as_str().into(),
            mxr_config::AgentProfileConfig {
                allowed_accounts: vec![account.as_str()],
                ..Default::default()
            },
        );
    }
    state.set_config_for_test(config).await;
    let mut mail = Vec::new();
    for (account, subject) in [(own, "allowed"), (other, "excluded secret")] {
        let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account)
            .thread_id(mxr_core::ThreadId::new())
            .subject(subject)
            .build();
        state
            .store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
        mail.push(envelope);
    }
    (state, mail.remove(0), mail.remove(0))
}

fn emit_mail(state: &Arc<AppState>, mail: &[mxr_core::Envelope]) {
    crate::chimes::emit_daemon_event(
        state,
        DaemonEvent::NewMessages {
            envelopes: mail.to_vec(),
            total: mail.len(),
        },
    );
}

async fn assert_own_mail(served: &mut Served, own: &mxr_core::Envelope) {
    let frame = served.recv().await;
    let IpcPayload::Event(DaemonEvent::NewMessages { envelopes, total }) = frame.payload else {
        panic!("expected only allowed mailbox content, got {frame:?}");
    };
    assert_eq!(total, 1);
    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].id, own.id);
    assert_eq!(envelopes[0].subject, "allowed");
}

async fn profile_before_events<H: Harness>() {
    for kind in [ClientKind::Agent, ClientKind::Mcp] {
        let (state, own, excluded) = scoped_state(kind).await;
        let h = H::start().await;
        let (hot, bulk) = lanes();
        // Both events predate declaration, and no ordinary request is sent.
        // The ack must be first; only the allowed part may follow it.
        let mut served = serve_with_kind(&h, state.clone(), hot, bulk, kind, |state| {
            emit_mail(state, std::slice::from_ref(&excluded));
            emit_mail(state, &[excluded.clone(), own.clone()]);
        })
        .await;
        assert_own_mail(&mut served, &own).await;
        // A different configured profile would permit the excluded account.
        // Conflicting envelope tags must never select it or an unscoped kind.
        for (index, conflict) in [
            ClientKind::Cli,
            ClientKind::Web,
            if kind == ClientKind::Mcp {
                ClientKind::Agent
            } else {
                ClientKind::Mcp
            },
        ]
        .into_iter()
        .enumerate()
        {
            let id = index as u64 + 1;
            served
                .client
                .send(IpcMessage {
                    expected_workspace: None,
                    id,
                    source: conflict,
                    payload: IpcPayload::Request(Request::GetEnvelope {
                        message_id: excluded.id.clone(),
                    }),
                })
                .await
                .unwrap();
            let response = served.recv().await;
            assert_eq!(response.id, id);
            assert!(
                matches!(
                    response.payload,
                    IpcPayload::Response(Response::Error {
                        kind: IpcErrorKind::Policy,
                        ..
                    })
                ),
                "conflicting source was admitted: {response:?}"
            );
            emit_mail(&state, &[excluded.clone(), own.clone()]);
            assert_own_mail(&mut served, &own).await;
        }
        served
            .client
            .send(IpcMessage {
                expected_workspace: None,
                id: 10,
                source: kind,
                payload: IpcPayload::Request(Request::GetStatus),
            })
            .await
            .unwrap();
        let response = served.recv().await;
        let IpcPayload::Response(Response::Ok {
            data:
                ResponseData::Status {
                    accounts,
                    sync_statuses,
                    total_messages,
                    ..
                },
        }) = response.payload
        else {
            panic!("expected scoped status: {response:?}");
        };
        assert_eq!(accounts.len(), 1);
        assert_eq!(sync_statuses.len(), 1);
        assert_eq!(sync_statuses[0].account_id, own.account_id);
        assert_eq!(total_messages, 1);
        // A late declaration cannot switch the pinned kind, even with the
        // original envelope tag and a second valid profile.
        served
            .client
            .send(IpcMessage {
                expected_workspace: None,
                id: 11,
                source: kind,
                payload: IpcPayload::Request(Request::DeclareClient {
                    kind: ClientKind::Cli,
                    protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
                }),
            })
            .await
            .unwrap();
        assert!(matches!(
            served.recv().await.payload,
            IpcPayload::Response(Response::Error { .. })
        ));
        emit_mail(&state, &[excluded, own.clone()]);
        assert_own_mail(&mut served, &own).await;
        finish(&state, served).await;
    }
}

#[tokio::test]
async fn profiles_are_pinned_on_every_transport_before_the_first_request() {
    profile_before_events::<SocketpairHarness>().await;
    profile_before_events::<DuplexHarness>().await;
    profile_before_events::<UdsTransportHarness>().await;
    profile_before_events::<MemoryTransportHarness>().await;
    profile_before_events::<TcpTokenHarness>().await;
}

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

#[tokio::test]
async fn incompatible_identity_and_missing_profiles_are_rejected_before_buffered_events() {
    for (source, kind, protocol_version, expected_kind) in [
        (
            ClientKind::Cli,
            ClientKind::Web,
            mxr_protocol::IPC_PROTOCOL_VERSION,
            IpcErrorKind::Unsupported,
        ),
        (
            ClientKind::Cli,
            ClientKind::Cli,
            mxr_protocol::IPC_PROTOCOL_VERSION - 1,
            IpcErrorKind::Unsupported,
        ),
        (
            ClientKind::Mcp,
            ClientKind::Mcp,
            mxr_protocol::IPC_PROTOCOL_VERSION,
            IpcErrorKind::Policy,
        ),
    ] {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let (hot, bulk) = lanes();
        let h = DuplexHarness::start().await;
        let (io, server) = spawn_server(&h, state.clone(), hot, bulk, emit).await;
        let mut client = Framed::new(io, IpcCodec::new());
        client
            .send(IpcMessage {
                expected_workspace: None,
                id: 1,
                source,
                payload: IpcPayload::Request(Request::DeclareClient {
                    kind,
                    protocol_version,
                }),
            })
            .await
            .unwrap();
        let response = tokio::time::timeout(RECV_TIMEOUT, client.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            matches!(response.payload, IpcPayload::Response(Response::Error { kind, .. }) if kind == expected_kind),
            "invalid declaration released data: {response:?}"
        );
        assert!(
            client.next().await.is_none(),
            "invalid declaration leaves no event stream"
        );
        drop(client);
        join_server(server).await;
        state.shutdown_runtime_tasks(JOIN_TIMEOUT).await;
    }
}
