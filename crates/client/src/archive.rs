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
    pub fn open_existing(data_dir: &Path) -> io::Result<Option<Self>> {
        let root = data_dir.join("archive-operations");
        match fs::read_dir(&root) {
            Ok(_) => Ok(Some(Self { root })),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn caller_id(&self) -> io::Result<String> {
        let path = self.root.join("caller-id");
        match read_caller_id(&path) {
            Ok(id) => return Ok(id),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }

        let id = uuid::Uuid::now_v7().to_string();
        let temp = self
            .root
            .join(format!(".caller-id-{}.tmp", uuid::Uuid::now_v7()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(id.as_bytes())?;
            file.sync_all()?;
            match fs::hard_link(&temp, &path) {
                Ok(()) => {
                    File::open(&self.root)?.sync_all()?;
                    Ok(id.clone())
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => read_caller_id(&path),
                Err(error) => Err(error),
            }
        })();
        let cleanup = fs::remove_file(&temp);
        if let Err(error) = cleanup {
            if error.kind() != io::ErrorKind::NotFound {
                return Err(error);
            }
        }
        if result.is_ok() {
            File::open(&self.root)?.sync_all()?;
        }
        result
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

fn read_caller_id(path: &Path) -> io::Result<String> {
    let id = fs::read_to_string(path)?;
    uuid::Uuid::parse_str(&id).map_err(io::Error::other)?;
    Ok(id)
}

impl IpcConnection {
    pub async fn preview_archive(
        &mut self,
        journal: &ArchiveJournal,
        mutation: MutationCommand,
    ) -> Result<ArchiveOperationData, ClientError> {
        self.preview_archive_with_events(journal, mutation, |_| {})
            .await
    }

    pub async fn preview_archive_with_events<F>(
        &mut self,
        journal: &ArchiveJournal,
        mutation: MutationCommand,
        on_event: F,
    ) -> Result<ArchiveOperationData, ClientError>
    where
        F: FnMut(mxr_protocol::DaemonEvent),
    {
        let mut on_event = on_event;
        let ResponseData::ArchiveWorkspace { workspace } = self
            .request_with_events_default_timeout(Request::GetArchiveWorkspace, &mut on_event)
            .await?
        else {
            return Err(ClientError::Io(io::Error::other(
                "Unexpected Archive workspace response",
            )));
        };
        let identity = journal.identity(workspace)?;
        let ResponseData::ArchiveOperation { operation } = self
            .request_with_events_default_timeout(
                Request::PreviewArchive { identity, mutation },
                &mut on_event,
            )
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
        self.submit_archive_with_events(journal, operation, |_| {})
            .await
    }

    pub async fn submit_archive_with_events<F>(
        &mut self,
        journal: &ArchiveJournal,
        operation: &ArchiveOperationData,
        on_event: F,
    ) -> Result<ArchiveOperationData, ClientError>
    where
        F: FnMut(mxr_protocol::DaemonEvent),
    {
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
            .request_with_events_default_timeout(
                Request::SubmitArchive {
                    identity: retained.identity,
                    review_id: retained.review_id,
                    mutation,
                },
                on_event,
            )
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
        self.wait_archive_with_events(identity, |_| {}).await
    }

    pub async fn wait_archive_with_events<F>(
        &mut self,
        identity: ArchiveIdentityData,
        mut on_event: F,
    ) -> Result<ArchiveOperationData, ClientError>
    where
        F: FnMut(mxr_protocol::DaemonEvent),
    {
        loop {
            let ResponseData::ArchiveOperation { operation } = self
                .request_with_events_default_timeout(
                    Request::GetArchiveOperation {
                        identity: identity.clone(),
                    },
                    &mut on_event,
                )
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
mod tests;

#[cfg(test)]
mod event_tests;
