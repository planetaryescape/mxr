//! Real TCP reconnection fixtures pin a native session's original owner.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "owned fixtures assert directly"
)]
use crate::{
    client::DaemonSession,
    ipc::{ipc_call, spawn_ipc_worker_for_session},
};
use futures::{SinkExt, StreamExt};
use mxr_protocol::{
    ArchiveWorkspaceData, ClientKind, IpcCodec, IpcMessage, IpcPayload, Request, Response,
    ResponseData,
};
use tokio::net::TcpListener;
use tokio_util::codec::Framed;

fn owner(workspace: &str) -> ArchiveWorkspaceData {
    ArchiveWorkspaceData {
        workspace_id: workspace.into(),
        recovery_generation: "original-generation".into(),
    }
}
fn status() -> ResponseData {
    serde_json::from_value(serde_json::json!({"kind":"Status","uptime_secs":0,"accounts":[],"total_messages":1,"workspace_owner_supported":true,"protocol_version":mxr_protocol::IPC_PROTOCOL_VERSION,"daemon_version":"different-package","daemon_build_id":"different-build"})).unwrap()
}

#[tokio::test]
async fn safe_retry_stays_on_selected_target_and_refuses_replacement_owner() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = mxr_client::SelectedDaemonTarget::new(
        mxr_client::TransportAddr::parse(&format!("tcp://{}", listener.local_addr().unwrap()))
            .unwrap(),
        None,
    );
    let server = tokio::spawn(async move {
        let mut business_calls = 0;
        for workspace in ["original", "replacement"] {
            let (stream, _) = listener.accept().await.unwrap();
            let mut framed = Framed::new(stream, IpcCodec::new());
            while let Some(Ok(message)) = framed.next().await {
                let IpcPayload::Request(request) = message.payload else {
                    panic!("request")
                };
                let data = match request {
                    Request::GetStatus => status(),
                    Request::GetArchiveWorkspace => ResponseData::ArchiveWorkspace {
                        workspace: owner(workspace),
                    },
                    Request::Ping => {
                        business_calls += 1;
                        assert_eq!(workspace, "original");
                        break;
                    }
                    other => panic!("unexpected {other:?}"),
                };
                framed
                    .send(IpcMessage {
                        id: message.id,
                        expected_workspace: None,
                        source: ClientKind::Daemon,
                        payload: IpcPayload::Response(Response::Ok { data }),
                    })
                    .await
                    .unwrap();
            }
        }
        business_calls
    });
    let (result_tx, _result_rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = spawn_ipc_worker_for_session(
        DaemonSession {
            target,
            owner: Some(owner("original")),
        },
        result_tx,
    );
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        ipc_call(&worker, Request::Ping),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("workspace or recovery generation changed"),
        "{error}"
    );
    drop(worker);
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn failed_initial_owner_capture_is_not_adopted_on_background_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = mxr_client::SelectedDaemonTarget::new(
        mxr_client::TransportAddr::parse(&format!("tcp://{}", listener.local_addr().unwrap()))
            .unwrap(),
        None,
    );
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut framed = Framed::new(stream, IpcCodec::new());
        let message = framed.next().await.unwrap().unwrap();
        assert!(matches!(
            message.payload,
            IpcPayload::Request(Request::GetStatus)
        ));
        framed
            .send(IpcMessage {
                id: message.id,
                expected_workspace: None,
                source: ClientKind::Daemon,
                payload: IpcPayload::Response(Response::Ok { data: status() }),
            })
            .await
            .unwrap();
        assert!(
            framed.next().await.is_none(),
            "must not capture a new workspace owner"
        );
    });
    let (result_tx, _result_rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = spawn_ipc_worker_for_session(
        DaemonSession {
            target,
            owner: None,
        },
        result_tx,
    );
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        ipc_call(&worker, Request::GetArchiveWorkspace),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.to_string().contains("no original workspace owner"));
    drop(worker);
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn failed_timeout_reconnect_then_safe_retry_closes_modal_and_refreshes_views() {
    use crate::{
        app::{App, ConnectionState, MailboxView},
        async_result::AsyncResult,
    };
    use mxr_core::{AccountId, Draft, DraftContent, DraftId, DraftIntent};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = mxr_client::SelectedDaemonTarget::new(
        mxr_client::TransportAddr::parse(&format!("tcp://{}", listener.local_addr().unwrap()))
            .unwrap(),
        None,
    );
    let (first_ping_tx, first_ping_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let mut tasks = Vec::new();
        let mut first_ping_tx = Some(first_ping_tx);
        for connection in 1..=3 {
            let (stream, _) = listener.accept().await.unwrap();
            let signal = if connection == 1 {
                first_ping_tx.take()
            } else {
                None
            };
            tasks.push(tokio::spawn(async move {
                let mut framed = Framed::new(stream, IpcCodec::new());
                let mut signal = signal;
                let mut pings = 0;
                while let Some(Ok(message)) = framed.next().await {
                    let IpcPayload::Request(request) = message.payload else {
                        panic!("request")
                    };
                    if connection == 2 {
                        break;
                    } // The timeout reconnect really fails.
                    let data = match request {
                        Request::GetStatus => status(),
                        Request::GetArchiveWorkspace => ResponseData::ArchiveWorkspace {
                            workspace: owner("original"),
                        },
                        Request::Ping => {
                            assert_eq!(message.expected_workspace, Some(owner("original")));
                            pings += 1;
                            if connection == 1 {
                                if let Some(signal) = signal.take() {
                                    signal.send(()).unwrap();
                                    continue;
                                } // Leave the first request unanswered.
                                break; // The next safe read must recover through its request retry.
                            }
                            ResponseData::Pong
                        }
                        other => panic!("unexpected {other:?}"),
                    };
                    framed
                        .send(IpcMessage {
                            id: message.id,
                            expected_workspace: None,
                            source: ClientKind::Daemon,
                            payload: IpcPayload::Response(Response::Ok { data }),
                        })
                        .await
                        .unwrap();
                }
                pings
            }));
        }
        let mut pings = Vec::new();
        for task in tasks {
            pings.push(task.await.unwrap());
        }
        pings
    });
    let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = spawn_ipc_worker_for_session(
        DaemonSession {
            target,
            owner: Some(owner("original")),
        },
        result_tx,
    );
    let first_worker = worker.clone();
    let first = tokio::spawn(async move { ipc_call(&first_worker, Request::Ping).await });
    first_ping_rx.await.unwrap();
    tokio::time::pause();
    tokio::time::advance(crate::ipc::IPC_REQUEST_TIMEOUT + std::time::Duration::from_secs(1)).await;
    // Let the real TCP reconnect handshake run on wall-clock time.
    tokio::time::resume();
    assert!(first
        .await
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("may or may not have been applied"));

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private-reply.md");
    std::fs::write(&path, "Original private text\n").unwrap();
    let mut app = App::new();
    app.compose.workspace_owner = Some(owner("original"));
    app.mailbox.mailbox_view = MailboxView::Place(mxr_protocol::MailPlaceData::Reading);
    let now = chrono::Utc::now();
    app.pending_draft_edit_request = Some(Draft {
        revision: Some(7),
        id: DraftId::new(),
        account_id: AccountId::new(),
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "retained".into(),
        content: DraftContent::markdown("Original private text\n"),
        managed_attachments: None,
        attachments: vec![path.clone()],
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    });
    let private = serde_json::to_value(&app.pending_draft_edit_request).unwrap();
    while let Ok(result) = result_rx.try_recv() {
        if let AsyncResult::ConnectionState(state) = result {
            app.set_connection_state(state);
        }
    }
    assert!(matches!(
        app.connection_state,
        ConnectionState::Reconnecting { .. }
    ));
    app.tick_connection_state(std::time::Instant::now() + std::time::Duration::from_secs(6));
    assert!(app.modals.error.is_some());
    app.mailbox.pending_labels_refresh = false;
    app.mailbox.pending_all_envelopes_refresh = false;
    app.mailbox.pending_subscriptions_refresh = false;
    app.mailbox.pending_desk_refresh = false;
    app.mailbox.pending_place_refresh = None;
    app.diagnostics.pending_status_refresh = false;
    assert!(matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            ipc_call(&worker, Request::Ping),
        )
        .await
        .unwrap()
        .unwrap(),
        Response::Ok {
            data: ResponseData::Pong
        }
    ));
    while let Ok(result) = result_rx.try_recv() {
        if let AsyncResult::ConnectionState(state) = result {
            app.set_connection_state(state);
        }
    }
    assert!(matches!(app.connection_state, ConnectionState::Connected));
    app.tick_connection_state(std::time::Instant::now() + std::time::Duration::from_secs(20));
    assert!(app.modals.error.is_none());
    assert!(
        app.mailbox.pending_labels_refresh
            && app.mailbox.pending_all_envelopes_refresh
            && app.mailbox.pending_subscriptions_refresh
            && app.mailbox.pending_desk_refresh
            && app.diagnostics.pending_status_refresh
    );
    assert_eq!(
        app.mailbox.pending_place_refresh,
        Some(mxr_protocol::MailPlaceData::Reading)
    );
    assert_eq!(
        serde_json::to_value(&app.pending_draft_edit_request).unwrap(),
        private
    );
    assert_eq!(app.compose.workspace_owner, Some(owner("original")));
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "Original private text\n"
    );
    drop(worker);
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap(),
        vec![2, 0, 1]
    );
}
