use super::*;
use futures::{SinkExt, StreamExt};
use mxr_protocol::{ClientKind, IpcCodec, IpcMessage, IpcPayload, Response};
use mxr_transport::{MemoryTransport, ServerTransport};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::sync::oneshot;
use tokio_util::codec::Framed;

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn server_with_bytes(
    transport: &MemoryTransport,
    bytes: Vec<u8>,
    corrupt_chunks: bool,
) -> tokio::task::JoinHandle<()> {
    let mut listener = transport.bind().await.expect("test operation succeeds");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("test operation succeeds");
        let mut framed = Framed::new(stream, IpcCodec::new());
        while let Some(Ok(message)) = framed.next().await {
            let IpcPayload::Request(request) = message.payload else {
                continue;
            };
            let data = match request {
                Request::GetStatus => ResponseData::Status {
                    workspace_owner_supported: false,
                    draft_revision_supported: true,
                    send_outcome_supported: true,
                    archive_recovery_supported: true,
                    attachment_transfer_supported: true,
                    draft_attachment_upload_supported: false,
                    uptime_secs: 1,
                    accounts: Vec::new(),
                    total_messages: 0,
                    daemon_pid: None,
                    sync_statuses: Vec::new(),
                    protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
                    daemon_version: None,
                    daemon_build_id: None,
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                    degraded: false,
                },
                Request::GetAttachmentTransferMetadata {
                    message_id,
                    attachment_id,
                } => ResponseData::AttachmentTransferMetadata {
                    metadata: AttachmentTransferMetadata {
                        message_id,
                        attachment_id,
                        filename: "fixture.bin".into(),
                        mime_type: "application/octet-stream".into(),
                        length_bytes: bytes.len() as u64,
                        sha256: hex_digest(&bytes),
                        max_chunk_bytes: MAX_ATTACHMENT_CHUNK_BYTES,
                    },
                },
                Request::ReadAttachmentChunk {
                    message_id,
                    attachment_id,
                    offset,
                    length,
                } => {
                    let start = offset as usize;
                    let end = start + length as usize;
                    let mut chunk = bytes[start..end].to_vec();
                    if corrupt_chunks && !chunk.is_empty() {
                        chunk[0] ^= 0xff;
                    }
                    ResponseData::AttachmentChunk {
                        chunk: AttachmentChunk {
                            message_id,
                            attachment_id,
                            offset,
                            data_base64: STANDARD.encode(chunk),
                        },
                    }
                }
                _ => ResponseData::Pong,
            };
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: message.id,
                    source: ClientKind::default(),
                    payload: IpcPayload::Response(Response::Ok { data }),
                })
                .await
                .expect("test operation succeeds");
        }
    })
}

#[tokio::test]
async fn transfer_writes_and_verifies_a_multichunk_attachment() {
    let transport = MemoryTransport::new();
    let bytes = (0..MAX_ATTACHMENT_CHUNK_BYTES as usize + 31)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let server = server_with_bytes(&transport, bytes.clone(), false).await;
    let mut connection = IpcConnection::connect_with(&transport.connector(), ClientKind::Cli)
        .await
        .expect("test operation succeeds");
    let message_id = MessageId::new();
    let attachment_id = AttachmentId::new();
    let directory = TempDir::new().expect("test operation succeeds");
    let destination = directory.path().join("nested/downloads/attachment.bin");

    let result = download_attachment_to(
        &mut connection,
        &message_id,
        &attachment_id,
        &destination,
        false,
    )
    .await
    .expect("test operation succeeds");

    assert_eq!(
        std::fs::read(&destination).expect("test operation succeeds"),
        bytes
    );
    assert_eq!(result.length_bytes, bytes.len() as u64);
    assert_eq!(result.sha256, hex_digest(&bytes));
    assert_eq!(
        std::fs::read_dir(destination.parent().expect("destination has a parent"))
            .expect("created destination directory is readable")
            .count(),
        1
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&destination)
                .expect("test operation succeeds")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    server.abort();
}

#[tokio::test]
async fn failed_multichunk_verification_does_not_expose_a_file_in_a_created_directory() {
    let transport = MemoryTransport::new();
    let bytes = (0..MAX_ATTACHMENT_CHUNK_BYTES as usize + 31)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let server = server_with_bytes(&transport, bytes, true).await;
    let mut connection = IpcConnection::connect_with(&transport.connector(), ClientKind::Cli)
        .await
        .expect("test operation succeeds");
    let message_id = MessageId::new();
    let attachment_id = AttachmentId::new();
    let directory = TempDir::new().expect("test operation succeeds");
    let destination = directory.path().join("nested/downloads/attachment.bin");
    let parent = destination.parent().expect("destination has a parent");

    assert!(download_attachment_to(
        &mut connection,
        &message_id,
        &attachment_id,
        &destination,
        false,
    )
    .await
    .is_err());

    assert!(!destination.exists());
    assert_eq!(
        std::fs::read_dir(parent)
            .expect("created destination directory is readable")
            .count(),
        0,
        "failed transfer removes its partial file"
    );
    server.abort();
}

#[tokio::test]
async fn failed_verification_preserves_the_confirmed_destination() {
    let transport = MemoryTransport::new();
    let bytes = b"verified source bytes".to_vec();
    let server = server_with_bytes(&transport, bytes, true).await;
    let mut connection = IpcConnection::connect_with(&transport.connector(), ClientKind::Cli)
        .await
        .expect("test operation succeeds");
    let message_id = MessageId::new();
    let attachment_id = AttachmentId::new();
    let directory = TempDir::new().expect("test operation succeeds");
    let destination = directory.path().join("attachment.bin");
    std::fs::write(&destination, b"keep this file").expect("test operation succeeds");

    assert!(download_attachment_to(
        &mut connection,
        &message_id,
        &attachment_id,
        &destination,
        true,
    )
    .await
    .is_err());

    assert_eq!(
        std::fs::read(&destination).expect("test operation succeeds"),
        b"keep this file"
    );
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("test operation succeeds")
            .count(),
        1
    );
    server.abort();
}

#[tokio::test]
async fn no_clobber_rejects_an_existing_destination() {
    let transport = MemoryTransport::new();
    let bytes = b"attachment".to_vec();
    let server = server_with_bytes(&transport, bytes, false).await;
    let mut connection = IpcConnection::connect_with(&transport.connector(), ClientKind::Cli)
        .await
        .expect("test operation succeeds");
    let message_id = MessageId::new();
    let attachment_id = AttachmentId::new();
    let directory = TempDir::new().expect("test operation succeeds");
    let destination = directory.path().join("attachment.bin");
    std::fs::write(&destination, b"existing").expect("test operation succeeds");

    assert!(download_attachment_to(
        &mut connection,
        &message_id,
        &attachment_id,
        &destination,
        false,
    )
    .await
    .is_err());
    assert_eq!(
        std::fs::read(&destination).expect("test operation succeeds"),
        b"existing"
    );
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("test operation succeeds")
            .count(),
        1
    );
    server.abort();
}

#[tokio::test]
async fn no_clobber_preserves_a_destination_created_during_transfer() {
    let transport = MemoryTransport::new();
    let mut listener = transport.bind().await.expect("test operation succeeds");
    let bytes = b"verified attachment bytes".to_vec();
    let server_bytes = bytes.clone();
    let (chunk_requested, chunk_requested_rx) = oneshot::channel();
    let (allow_chunk, allow_chunk_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("test operation succeeds");
        let mut framed = Framed::new(stream, IpcCodec::new());
        let mut chunk_requested = Some(chunk_requested);
        let mut allow_chunk_rx = Some(allow_chunk_rx);
        while let Some(Ok(message)) = framed.next().await {
            let IpcPayload::Request(request) = message.payload else {
                continue;
            };
            let data = match request {
                Request::GetStatus => ResponseData::Status {
                    workspace_owner_supported: false,
                    draft_revision_supported: true,
                    send_outcome_supported: true,
                    archive_recovery_supported: true,
                    attachment_transfer_supported: true,
                    draft_attachment_upload_supported: false,
                    uptime_secs: 1,
                    accounts: Vec::new(),
                    total_messages: 0,
                    daemon_pid: None,
                    sync_statuses: Vec::new(),
                    protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
                    daemon_version: None,
                    daemon_build_id: None,
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                    degraded: false,
                },
                Request::GetAttachmentTransferMetadata {
                    message_id,
                    attachment_id,
                } => ResponseData::AttachmentTransferMetadata {
                    metadata: AttachmentTransferMetadata {
                        message_id,
                        attachment_id,
                        filename: "fixture.bin".into(),
                        mime_type: "application/octet-stream".into(),
                        length_bytes: server_bytes.len() as u64,
                        sha256: hex_digest(&server_bytes),
                        max_chunk_bytes: MAX_ATTACHMENT_CHUNK_BYTES,
                    },
                },
                Request::ReadAttachmentChunk {
                    message_id,
                    attachment_id,
                    offset,
                    ..
                } => {
                    chunk_requested
                        .take()
                        .expect("chunk request is observed once")
                        .send(())
                        .expect("test is waiting for chunk request");
                    allow_chunk_rx
                        .take()
                        .expect("chunk gate is available")
                        .await
                        .expect("test releases chunk gate");
                    ResponseData::AttachmentChunk {
                        chunk: AttachmentChunk {
                            message_id,
                            attachment_id,
                            offset,
                            data_base64: STANDARD.encode(&server_bytes),
                        },
                    }
                }
                _ => ResponseData::Pong,
            };
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: message.id,
                    source: ClientKind::default(),
                    payload: IpcPayload::Response(Response::Ok { data }),
                })
                .await
                .expect("test operation succeeds");
        }
    });
    let mut connection = IpcConnection::connect_with(&transport.connector(), ClientKind::Cli)
        .await
        .expect("test operation succeeds");
    let message_id = MessageId::new();
    let attachment_id = AttachmentId::new();
    let directory = TempDir::new().expect("test operation succeeds");
    let destination = directory.path().join("attachment.bin");
    let transfer_destination = destination.clone();
    let transfer = tokio::spawn(async move {
        download_attachment_to(
            &mut connection,
            &message_id,
            &attachment_id,
            &transfer_destination,
            false,
        )
        .await
    });

    chunk_requested_rx
        .await
        .expect("transfer requested its first chunk");
    std::fs::write(&destination, b"created while downloading")
        .expect("concurrent destination is created");
    allow_chunk.send(()).expect("chunk gate is released");
    assert!(transfer.await.expect("transfer task completes").is_err());
    assert_eq!(
        std::fs::read(&destination).expect("concurrent file remains"),
        b"created while downloading"
    );
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("download directory is readable")
            .count(),
        1,
        "failed transfer removes its partial file"
    );
    server.abort();
}

#[tokio::test]
async fn dropping_a_transfer_removes_its_partial_file() {
    let transport = MemoryTransport::new();
    let mut listener = transport.bind().await.expect("test operation succeeds");
    let (requested, requested_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("test operation succeeds");
        let mut framed = Framed::new(stream, IpcCodec::new());
        let mut requested = Some(requested);
        while let Some(Ok(message)) = framed.next().await {
            let IpcPayload::Request(request) = message.payload else {
                continue;
            };
            if matches!(request, Request::ReadAttachmentChunk { .. }) {
                requested
                    .take()
                    .expect("test operation succeeds")
                    .send(())
                    .expect("test operation succeeds");
                std::future::pending::<()>().await;
            }
            let data = if matches!(request, Request::GetStatus) {
                ResponseData::Status {
                    workspace_owner_supported: false,
                    draft_revision_supported: true,
                    send_outcome_supported: true,
                    archive_recovery_supported: true,
                    attachment_transfer_supported: true,
                    draft_attachment_upload_supported: false,
                    uptime_secs: 1,
                    accounts: Vec::new(),
                    total_messages: 0,
                    daemon_pid: None,
                    sync_statuses: Vec::new(),
                    protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
                    daemon_version: None,
                    daemon_build_id: None,
                    repair_required: false,
                    semantic_runtime: None,
                    feature_health: None,
                    degraded: false,
                }
            } else {
                let Request::GetAttachmentTransferMetadata {
                    message_id,
                    attachment_id,
                } = request
                else {
                    unreachable!()
                };
                let bytes = b"one chunk";
                ResponseData::AttachmentTransferMetadata {
                    metadata: AttachmentTransferMetadata {
                        message_id,
                        attachment_id,
                        filename: "fixture.bin".into(),
                        mime_type: "application/octet-stream".into(),
                        length_bytes: bytes.len() as u64,
                        sha256: hex_digest(bytes),
                        max_chunk_bytes: MAX_ATTACHMENT_CHUNK_BYTES,
                    },
                }
            };
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: message.id,
                    source: ClientKind::default(),
                    payload: IpcPayload::Response(Response::Ok { data }),
                })
                .await
                .expect("test operation succeeds");
        }
    });
    let mut connection = IpcConnection::connect_with(&transport.connector(), ClientKind::Cli)
        .await
        .expect("test operation succeeds");
    let message_id = MessageId::new();
    let attachment_id = AttachmentId::new();
    let directory = TempDir::new().expect("test operation succeeds");
    let destination = directory.path().join("attachment.bin");
    let task = tokio::spawn(async move {
        download_attachment_to(
            &mut connection,
            &message_id,
            &attachment_id,
            &destination,
            false,
        )
        .await
    });

    requested_rx.await.expect("test operation succeeds");
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("test operation succeeds")
            .count(),
        1
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let partial = std::fs::read_dir(directory.path())
            .expect("test operation succeeds")
            .next()
            .expect("test operation succeeds")
            .expect("test operation succeeds")
            .path();
        assert_eq!(
            std::fs::metadata(partial)
                .expect("test operation succeeds")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    task.abort();
    let _ = task.await;
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("test operation succeeds")
            .count(),
        0
    );
    server.abort();
}
