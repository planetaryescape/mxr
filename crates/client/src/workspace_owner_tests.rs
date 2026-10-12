#![expect(
    clippy::panic,
    reason = "tests fail directly on unexpected protocol responses"
)]
use super::*;
use mxr_core::{AccountId, Draft, DraftContent, DraftId, DraftIntent};
use mxr_protocol::ArchiveWorkspaceData;
use mxr_transport::{MemoryTransport, ServerTransport};
use std::sync::{Arc, Mutex};

fn status() -> ResponseData {
    serde_json::from_value(serde_json::json!({
        "kind": "Status", "workspace_owner_supported": true,
        "draft_revision_supported": true, "send_outcome_supported": true,
        "draft_attachment_upload_supported": true,
        "uptime_secs": 1, "accounts": [], "total_messages": 0,
        "sync_statuses": [], "protocol_version": mxr_protocol::IPC_PROTOCOL_VERSION,
        "repair_required": false, "degraded": false,
    }))
    .expect("valid modern status")
}
fn owner() -> ArchiveWorkspaceData {
    ArchiveWorkspaceData {
        workspace_id: uuid::Uuid::now_v7().to_string(),
        recovery_generation: uuid::Uuid::now_v7().to_string(),
    }
}
fn draft() -> Draft {
    let now = chrono::Utc::now();
    Draft {
        id: DraftId::new(),
        account_id: AccountId::new(),
        revision: None,
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "private draft".into(),
        content: DraftContent::markdown("private text"),
        managed_attachments: None,
        attachments: vec![],
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    }
}

async fn fixture() -> (
    IpcConnection,
    Arc<Mutex<ArchiveWorkspaceData>>,
    Arc<Mutex<Vec<IpcMessage>>>,
    tokio::task::JoinHandle<()>,
) {
    let transport = MemoryTransport::new();
    let mut listener = transport.bind().await.expect("listener");
    let current = Arc::new(Mutex::new(owner()));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let server_owner = current.clone();
    let server_seen = seen.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut framed = Framed::new(stream, IpcCodec::new());
        while let Some(Ok(message)) = framed.next().await {
            let IpcPayload::Request(request) = &message.payload else {
                continue;
            };
            let data = match request {
                Request::GetStatus => status(),
                Request::GetArchiveWorkspace => ResponseData::ArchiveWorkspace {
                    workspace: server_owner.lock().expect("owner").clone(),
                },
                Request::SaveDraft { draft } => {
                    assert_eq!(
                        message.expected_workspace.as_ref(),
                        Some(&*server_owner.lock().expect("owner"))
                    );
                    let mut draft = draft.clone();
                    draft.revision = Some(1);
                    ResponseData::Draft { draft }
                }
                Request::DeleteDraft { .. } => ResponseData::Ack,
                other => panic!("unexpected {other:?}"),
            };
            server_seen.lock().expect("seen").push(message.clone());
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: message.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok { data }),
                })
                .await
                .expect("reply");
        }
    });
    let connection = IpcConnection::connect_with(&transport.connector(), ClientKind::Cli)
        .await
        .expect("connection");
    (connection, current, seen, server)
}

#[tokio::test]
async fn true_owner_capability_accepts_bound_save_and_delete() {
    let (mut connection, current, seen, server) = fixture().await;
    let original = current.lock().expect("owner").clone();
    let saved = connection
        .request_owned(Request::SaveDraft { draft: draft() }, &original)
        .await
        .expect("modern save");
    let ResponseData::Draft { draft } = saved else {
        panic!("saved draft");
    };
    assert_eq!(draft.revision, Some(1));
    connection
        .request_owned(
            Request::DeleteDraft {
                draft_id: draft.id,
                expected_revision: Some(1),
            },
            &original,
        )
        .await
        .expect("modern delete");
    assert_eq!(
        seen.lock()
            .expect("seen")
            .iter()
            .filter(|message| matches!(
                message.payload,
                IpcPayload::Request(Request::SaveDraft { .. } | Request::DeleteDraft { .. })
            ))
            .count(),
        2
    );
    drop(connection);
    server.await.expect("server");
}

#[tokio::test]
async fn changed_owner_refuses_before_reading_or_uploading_private_attachment() {
    let (mut connection, current, seen, server) = fixture().await;
    let original = connection
        .capture_workspace()
        .await
        .expect("capture original owner");
    *current.lock().expect("owner") = owner();
    let mut private = draft();
    private
        .attachments
        .push(PathBuf::from("/nonexistent/private-owner-file"));
    let error = connection
        .request_owned(Request::SaveDraft { draft: private }, &original)
        .await
        .expect_err("owner conflict before file access");
    assert!(
        matches!(error, ClientError::Daemon { ref code, retryable: false, .. } if code == "workspace_conflict")
    );
    assert!(seen.lock().expect("seen").iter().all(|message| matches!(
        message.payload,
        IpcPayload::Request(Request::GetStatus | Request::GetArchiveWorkspace)
    )));
    drop(connection);
    server.await.expect("server");
}

#[tokio::test]
async fn missing_owner_refuses_before_reading_or_uploading_private_attachment() {
    let (mut connection, _, seen, server) = fixture().await;
    let mut private = draft();
    private
        .attachments
        .push(PathBuf::from("/nonexistent/private-owner-file"));
    let error = connection
        .request(Request::SaveDraft { draft: private })
        .await
        .expect_err("missing owner before file access");
    assert!(
        matches!(error, ClientError::Daemon { ref code, retryable: false, .. } if code == "workspace_owner_required")
    );
    assert!(seen.lock().expect("seen").is_empty());
    drop(connection);
    server.await.expect("server");
}
