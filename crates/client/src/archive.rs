//! Local recovery journal. Retention failure must prevent submission.
use crate::{ClientError, IpcConnection};
use mxr_protocol::{
    ArchiveIdentityData, ArchiveOperationData, ArchiveWorkspaceData, MutationCommand, Request,
    ResponseData,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub struct ArchiveJournal {
    root: PathBuf,
}
impl ArchiveJournal {
    pub fn new(data_dir: &Path) -> io::Result<Self> {
        let root = data_dir.join("archive-operations");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
    pub fn caller_id(&self) -> io::Result<String> {
        let path = self.root.join("caller-id");
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let id = uuid::Uuid::now_v7().to_string();
                file.write_all(id.as_bytes())?;
                file.sync_all()?;
                File::open(&self.root)?.sync_all()?;
                Ok(id)
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let id = fs::read_to_string(path)?;
                uuid::Uuid::parse_str(&id).map_err(io::Error::other)?;
                Ok(id)
            }
            Err(error) => Err(error),
        }
    }
    pub fn identity(&self, workspace: ArchiveWorkspaceData) -> io::Result<ArchiveIdentityData> {
        Ok(ArchiveIdentityData {
            workspace,
            caller_id: self.caller_id()?,
            operation_id: uuid::Uuid::now_v7().to_string(),
        })
    }
    fn path(&self, operation_id: &str) -> io::Result<PathBuf> {
        uuid::Uuid::parse_str(operation_id).map_err(io::Error::other)?;
        Ok(self.root.join(format!("{operation_id}.json")))
    }
    pub fn retain(&self, operation: &ArchiveOperationData) -> io::Result<()> {
        let path = self.path(&operation.identity.operation_id)?;
        if path.exists() {
            let existing = self.load(&operation.identity.operation_id)?;
            if existing.identity != operation.identity || existing.review_id != operation.review_id
            {
                return Err(io::Error::other("Archive journal identity conflict"));
            }
        }
        let temp = self.root.join(format!("{}.tmp", uuid::Uuid::now_v7()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(&serde_json::to_vec(operation).map_err(io::Error::other)?)?;
        file.sync_all()?;
        fs::rename(&temp, &path)?;
        File::open(&self.root)?.sync_all()?;
        Ok(())
    }
    pub fn load(&self, operation_id: &str) -> io::Result<ArchiveOperationData> {
        serde_json::from_slice(&fs::read(self.path(operation_id)?)?).map_err(io::Error::other)
    }
    pub fn list(&self) -> io::Result<Vec<ArchiveOperationData>> {
        let mut operations: Vec<ArchiveOperationData> = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                operations
                    .push(serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?);
            }
        }
        operations.sort_by_key(|operation| operation.started_at);
        Ok(operations)
    }
}

impl IpcConnection {
    pub async fn preview_archive(
        &mut self,
        journal: &ArchiveJournal,
        mutation: MutationCommand,
    ) -> Result<ArchiveOperationData, ClientError> {
        let ResponseData::ArchiveWorkspace { workspace } =
            self.request(Request::GetArchiveWorkspace).await?
        else {
            return Err(ClientError::Io(io::Error::other(
                "Unexpected Archive workspace response",
            )));
        };
        let identity = journal.identity(workspace)?;
        let ResponseData::ArchiveOperation { operation } = self
            .request(Request::PreviewArchive { identity, mutation })
            .await?
        else {
            return Err(ClientError::Io(io::Error::other(
                "Unexpected Archive preview response",
            )));
        };
        journal.retain(&operation)?;
        Ok(operation)
    }
    pub async fn submit_archive(
        &mut self,
        journal: &ArchiveJournal,
        operation: &ArchiveOperationData,
    ) -> Result<ArchiveOperationData, ClientError> {
        // Reload the retained review; a lost caller buffer must never create a new identity.
        let retained = journal.load(&operation.identity.operation_id)?;
        let mutation = MutationCommand::Archive {
            message_ids: retained
                .items
                .iter()
                .map(|item| item.message_id.clone())
                .collect(),
        };
        let ResponseData::ArchiveOperation { operation } = self
            .request(Request::SubmitArchive {
                identity: retained.identity,
                review_id: retained.review_id,
                mutation,
            })
            .await?
        else {
            return Err(ClientError::Io(io::Error::other(
                "Unexpected Archive submit response",
            )));
        };
        Ok(operation)
    }
    pub async fn wait_archive(
        &mut self,
        identity: ArchiveIdentityData,
    ) -> Result<ArchiveOperationData, ClientError> {
        loop {
            let ResponseData::ArchiveOperation { operation } = self
                .request(Request::GetArchiveOperation {
                    identity: identity.clone(),
                })
                .await?
            else {
                return Err(ClientError::Io(io::Error::other(
                    "Unexpected Archive outcome response",
                )));
            };
            if !operation.running {
                return Ok(operation);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

#[cfg(test)]
mod tests {
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
}
