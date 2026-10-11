#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests unwrap fixtures and reject unexpected mock requests"
)]
use super::*;
#[test]
fn caller_survives_restart_and_operations_do_not_overwrite_each_other() {
    let temp = tempfile::tempdir().unwrap();
    let first = ArchiveJournal::new(temp.path()).unwrap();
    let id = first.caller_id().unwrap();
    assert_eq!(
        ArchiveJournal::new(temp.path())
            .unwrap()
            .caller_id()
            .unwrap(),
        id
    );
    let workspace = ArchiveWorkspaceData {
        workspace_id: uuid::Uuid::now_v7().to_string(),
        recovery_generation: uuid::Uuid::now_v7().to_string(),
    };
    for _ in 0..2 {
        let operation = ArchiveOperationData {
            identity: first.identity(workspace.clone()).unwrap(),
            review_id: uuid::Uuid::now_v7().to_string(),
            mutation_id: uuid::Uuid::now_v7().to_string(),
            accepted: false,
            undo_available: false,
            running: false,
            started_at: 1,
            items: vec![],
        };
        first.retain(&operation).unwrap();
        assert_eq!(
            first.load(&operation.identity.operation_id).unwrap(),
            operation
        );
    }
    assert_eq!(first.list().unwrap().len(), 2);
    assert!(first.load("../escape").is_err());
}

#[test]
fn concurrent_caller_initialization_publishes_one_complete_identity() {
    use std::sync::{Arc, Barrier};

    let temp = tempfile::tempdir().unwrap();
    let barrier = Arc::new(Barrier::new(4));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let root = temp.path().to_path_buf();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                ArchiveJournal::new(&root).unwrap().caller_id().unwrap()
            })
        })
        .collect();
    let ids: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();

    assert!(ids.iter().all(|id| id == &ids[0]));
    uuid::Uuid::parse_str(
        &fs::read_to_string(temp.path().join("archive-operations/caller-id")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        ids[0],
        fs::read_to_string(temp.path().join("archive-operations/caller-id")).unwrap()
    );
}

#[test]
fn open_existing_does_not_create_an_archive_journal() {
    let temp = tempfile::tempdir().unwrap();
    assert!(ArchiveJournal::open_existing(temp.path())
        .unwrap()
        .is_none());
    assert!(!temp.path().join("archive-operations").exists());

    let created = ArchiveJournal::new(temp.path()).unwrap();
    assert!(ArchiveJournal::open_existing(temp.path())
        .unwrap()
        .is_some());
    drop(created);
}

#[tokio::test]
async fn submission_response_loss_retains_exact_review_for_reconnected_query() {
    use futures::{SinkExt, StreamExt};
    use mxr_protocol::{ClientKind, IpcCodec, IpcMessage, IpcPayload, Response};
    use tokio::net::UnixListener;
    use tokio_util::codec::Framed;
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("archive.sock");
    let journal = ArchiveJournal::new(temp.path()).unwrap();
    let journal_root = temp.path().to_path_buf();
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let workspace = ArchiveWorkspaceData {
            workspace_id: uuid::Uuid::now_v7().to_string(),
            recovery_generation: uuid::Uuid::now_v7().to_string(),
        };
        let (stream, _) = listener.accept().await.unwrap();
        let mut framed = Framed::new(stream, IpcCodec::new());
        let mut retained = None;
        while let Some(frame) = framed.next().await {
            let frame = frame.unwrap();
            let IpcPayload::Request(request) = frame.payload else {
                panic!("request");
            };
            let response = match request {
                    Request::GetStatus => serde_json::from_value(serde_json::json!({"kind":"Status","archive_recovery_supported":true,"uptime_secs":0,"accounts":[],"total_messages":0})).unwrap(),
                    Request::GetArchiveWorkspace => ResponseData::ArchiveWorkspace { workspace: workspace.clone() },
                    Request::PreviewArchive { identity, .. } => {
                        let operation = ArchiveOperationData { identity, review_id: uuid::Uuid::now_v7().to_string(), mutation_id: uuid::Uuid::now_v7().to_string(), accepted: false, undo_available: false, running: false, started_at: 1, items: vec![] };
                        retained = Some(operation.clone());
                        ResponseData::ArchiveOperation { operation }
                    }
                    Request::SubmitArchive { identity, review_id, .. } => {
                        let operation = retained.as_ref().unwrap();
                        let on_disk = ArchiveJournal::new(&journal_root).unwrap().load(&identity.operation_id).unwrap();
                        assert_eq!(on_disk.identity, identity);
                        assert_eq!(on_disk.review_id, review_id);
                        assert_eq!(operation.identity, identity);
                        break; // Acceptance happened but its response is lost.
                    }
                    _ => panic!("unexpected request"),
                };
            framed
                .send(IpcMessage {
                    id: frame.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok { data: response }),
                })
                .await
                .unwrap();
        }
        drop(framed);
        let mut operation = retained.unwrap();
        operation.accepted = true;
        let (stream, _) = listener.accept().await.unwrap();
        let mut framed = Framed::new(stream, IpcCodec::new());
        for _ in 0..2 {
            let frame = framed.next().await.unwrap().unwrap();
            let response = match frame.payload {
                    IpcPayload::Request(Request::GetStatus) => serde_json::from_value(serde_json::json!({"kind":"Status","archive_recovery_supported":true,"uptime_secs":0,"accounts":[],"total_messages":0})).unwrap(),
                    IpcPayload::Request(Request::GetArchiveOperation { identity }) => { assert_eq!(identity, operation.identity); ResponseData::ArchiveOperation { operation: operation.clone() } },
                    _ => panic!("reconnection must query original identity"),
                };
            framed
                .send(IpcMessage {
                    id: frame.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok { data: response }),
                })
                .await
                .unwrap();
        }
    });
    let mut client = IpcConnection::connect(&socket, ClientKind::Cli)
        .await
        .unwrap();
    let reviewed = client
        .preview_archive(
            &journal,
            MutationCommand::Archive {
                message_ids: vec![],
            },
        )
        .await
        .unwrap();
    assert!(client.submit_archive(&journal, &reviewed).await.is_err());
    assert_eq!(
        journal
            .load(&reviewed.identity.operation_id)
            .unwrap()
            .identity,
        reviewed.identity
    );
    let mut reconnected = IpcConnection::connect(&socket, ClientKind::Cli)
        .await
        .unwrap();
    let recovered = reconnected
        .wait_archive(reviewed.identity.clone())
        .await
        .unwrap();
    assert_eq!(recovered.identity, reviewed.identity);
    assert!(recovered.accepted);
    server.await.unwrap();
}

#[tokio::test]
async fn v068_status_fixture_refuses_archive_submission_without_capability() {
    use futures::{SinkExt, StreamExt};
    use mxr_protocol::{ClientKind, IpcCodec, IpcMessage, IpcPayload, Response};
    use tokio::net::UnixListener;
    use tokio_util::codec::Framed;
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("legacy.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut framed = Framed::new(stream, IpcCodec::new());
        let frame = framed.next().await.unwrap().unwrap();
        assert!(matches!(
            frame.payload,
            IpcPayload::Request(Request::GetStatus)
        ));
        let status = serde_json::from_value(serde_json::json!({"kind":"Status","draft_revision_supported":true,"send_outcome_supported":true,"uptime_secs":0,"accounts":[],"total_messages":0})).unwrap();
        framed
            .send(IpcMessage {
                id: frame.id,
                source: ClientKind::Daemon,
                payload: IpcPayload::Response(Response::Ok { data: status }),
            })
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), framed.next())
                .await
                .is_err()
        );
    });
    let mut client = IpcConnection::connect(&socket, ClientKind::Cli)
        .await
        .unwrap();
    let response = client.request(Request::GetArchiveWorkspace).await;
    assert!(matches!(
        response,
        Err(ClientError::Daemon {
            kind: mxr_protocol::IpcErrorKind::Unsupported,
            ..
        })
    ));
    server.await.unwrap();
}
#[tokio::test]
async fn v068_status_fixture_still_accepts_existing_draft_and_send_requests() {
    use futures::{SinkExt, StreamExt};
    use mxr_protocol::{ClientKind, IpcCodec, IpcMessage, IpcPayload, Response};
    use tokio::net::UnixListener;
    use tokio_util::codec::Framed;
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("v068.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut framed = Framed::new(stream, IpcCodec::new());
        for expect_send in [false, true] {
            let status_request = framed.next().await.unwrap().unwrap();
            assert!(matches!(
                status_request.payload,
                IpcPayload::Request(Request::GetStatus)
            ));
            let status = serde_json::from_value(serde_json::json!({"kind":"Status","draft_revision_supported":true,"send_outcome_supported":true,"uptime_secs":0,"accounts":[],"total_messages":0})).unwrap();
            framed
                .send(IpcMessage {
                    id: status_request.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok { data: status }),
                })
                .await
                .unwrap();
            let action = framed.next().await.unwrap().unwrap();
            if expect_send {
                assert!(matches!(
                    action.payload,
                    IpcPayload::Request(Request::GetSendOutcome { .. })
                ));
            } else {
                assert!(matches!(
                    action.payload,
                    IpcPayload::Request(Request::DeleteDraft { .. })
                ));
            }
            framed
                .send(IpcMessage {
                    id: action.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok {
                        data: ResponseData::Ack,
                    }),
                })
                .await
                .unwrap();
        }
    });
    let mut client = IpcConnection::connect(&socket, ClientKind::Cli)
        .await
        .unwrap();
    let draft_id = mxr_core::DraftId::new();
    assert!(matches!(
        client
            .request(Request::DeleteDraft {
                draft_id: draft_id.clone(),
                expected_revision: Some(1)
            })
            .await
            .unwrap(),
        ResponseData::Ack
    ));
    assert!(matches!(
        client
            .request(Request::GetSendOutcome {
                account_id: mxr_core::AccountId::new(),
                operation_id: "reviewed-send".into()
            })
            .await
            .unwrap(),
        ResponseData::Ack
    ));
    server.await.unwrap();
}
