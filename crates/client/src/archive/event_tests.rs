#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests unwrap fixtures and reject unexpected mock requests"
)]
use super::*;

#[tokio::test]
async fn archive_helpers_forward_events_in_order_without_resubmitting() {
    use futures::{SinkExt, StreamExt};
    use mxr_protocol::{ClientKind, DaemonEvent, IpcCodec, IpcMessage, IpcPayload, Response};
    use tokio::net::UnixListener;
    use tokio_util::codec::Framed;

    fn event(name: &str) -> IpcMessage {
        IpcMessage {
            expected_workspace: None,
            id: 0,
            source: ClientKind::Daemon,
            payload: IpcPayload::Event(DaemonEvent::OperationStarted {
                operation_id: format!("archive-event-{name}"),
                operation: name.to_string(),
                account_id: None,
                message: name.to_string(),
            }),
        }
    }

    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("archive-events.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut framed = Framed::new(stream, IpcCodec::new());
        let workspace = ArchiveWorkspaceData {
            workspace_id: uuid::Uuid::now_v7().to_string(),
            recovery_generation: uuid::Uuid::now_v7().to_string(),
        };
        let mut operation: Option<ArchiveOperationData> = None;
        let mut submissions = 0;
        let mut polls = 0;

        while let Some(Ok(frame)) = framed.next().await {
            let IpcPayload::Request(request) = frame.payload else {
                panic!("expected request")
            };
            let (event_name, data) = match request {
                Request::GetStatus => (
                    "status",
                    serde_json::from_value(serde_json::json!({
                        "kind":"Status",
                        "archive_recovery_supported":true,
                        "uptime_secs":0,
                        "accounts":[],
                        "total_messages":0
                    }))
                    .unwrap(),
                ),
                Request::GetArchiveWorkspace => (
                    "workspace",
                    ResponseData::ArchiveWorkspace {
                        workspace: workspace.clone(),
                    },
                ),
                Request::PreviewArchive { identity, .. } => {
                    let reviewed = ArchiveOperationData {
                        identity,
                        review_id: uuid::Uuid::now_v7().to_string(),
                        mutation_id: uuid::Uuid::now_v7().to_string(),
                        accepted: false,
                        undo_available: false,
                        running: false,
                        started_at: 1,
                        items: vec![],
                    };
                    operation = Some(reviewed.clone());
                    (
                        "preview",
                        ResponseData::ArchiveOperation {
                            operation: reviewed,
                        },
                    )
                }
                Request::SubmitArchive {
                    identity,
                    review_id,
                    ..
                } => {
                    submissions += 1;
                    let submitted = operation.as_mut().unwrap();
                    assert_eq!(submitted.identity, identity);
                    assert_eq!(submitted.review_id, review_id);
                    submitted.accepted = true;
                    submitted.running = true;
                    (
                        "submit",
                        ResponseData::ArchiveOperation {
                            operation: submitted.clone(),
                        },
                    )
                }
                Request::GetArchiveOperation { identity } => {
                    polls += 1;
                    let current = operation.as_mut().unwrap();
                    assert_eq!(current.identity, identity);
                    current.running = polls == 1;
                    let name = if polls == 1 { "poll-1" } else { "poll-2" };
                    (
                        name,
                        ResponseData::ArchiveOperation {
                            operation: current.clone(),
                        },
                    )
                }
                other => panic!("unexpected request: {other:?}"),
            };

            framed.send(event(event_name)).await.unwrap();
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: frame.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok { data }),
                })
                .await
                .unwrap();
            if polls == 2 {
                break;
            }
        }

        assert_eq!(submissions, 1, "Archive was submitted exactly once");
        assert_eq!(polls, 2, "the operation was polled until completion");
    });

    let mut connection = IpcConnection::connect(&socket, ClientKind::Cli)
        .await
        .unwrap();
    let journal = ArchiveJournal::new(temp.path()).unwrap();
    let mut events = Vec::new();
    let mut capture = |event| {
        if let DaemonEvent::OperationStarted { operation, .. } = event {
            events.push(operation);
        }
    };
    let reviewed = connection
        .preview_archive_with_events(
            &journal,
            MutationCommand::Archive {
                message_ids: vec![],
            },
            &mut capture,
        )
        .await
        .unwrap();
    let submitted = connection
        .submit_archive_with_events(&journal, &reviewed, &mut capture)
        .await
        .unwrap();
    assert!(submitted.running);
    let finished = connection
        .wait_archive_with_events(reviewed.identity, &mut capture)
        .await
        .unwrap();
    assert!(!finished.running);
    server.await.unwrap();

    assert_eq!(
        events,
        [
            "status",
            "workspace",
            "status",
            "preview",
            "status",
            "submit",
            "status",
            "poll-1",
            "status",
            "poll-2"
        ]
    );
}

#[tokio::test]
async fn archive_event_helper_preserves_default_request_timeout() {
    use futures::StreamExt;
    use mxr_protocol::{ClientKind, IpcCodec, IpcPayload};
    use tokio::net::UnixListener;
    use tokio_util::codec::Framed;

    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("archive-timeout.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut framed = Framed::new(stream, IpcCodec::new());
        let request = framed.next().await.unwrap().unwrap();
        assert!(matches!(
            request.payload,
            IpcPayload::Request(Request::GetStatus)
        ));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    });

    let mut connection = IpcConnection::connect(&socket, ClientKind::Cli)
        .await
        .unwrap()
        .with_default_timeout(Some(std::time::Duration::from_millis(20)));
    let journal = ArchiveJournal::new(temp.path()).unwrap();
    let result = connection
        .preview_archive_with_events(
            &journal,
            MutationCommand::Archive {
                message_ids: vec![],
            },
            |_| {},
        )
        .await;
    assert!(matches!(result, Err(ClientError::Timeout(_))));
    server.await.unwrap();
}
