//! Client-only state, excluded from portable daemon backups.
//! Plaintext 0600 matches the established disk-first secret policy. A stable lock
//! guards the whole selection/credential/cleanup commit across processes.
use mxr_core::RemoteSecret;
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRegistration {
    pub origin: String,
    pub client_id: String,
    pub host_id: String,
    pub workspace_id: String,
    pub recovery_generation: String,
    pub mail_secret: RemoteSecret,
    pub cleanup_secret: RemoteSecret,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConnection {
    pub origin: String,
    pub name: String,
    pub mail_secret: RemoteSecret,
    pub cleanup_secret: RemoteSecret,
    pub request_id: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConnectionState {
    pub generation: u64,
    pub selected: Option<ConnectionRegistration>,
    pub pending_pairing: Option<PendingConnection>,
    pub cleanup: Vec<RetiredRegistration>,
    #[serde(default)]
    pub retired_pairings: Vec<RetiredPairing>,
}

pub fn path() -> PathBuf {
    crate::config_dir().join("client-connection.json")
}
pub fn read(path: &Path) -> io::Result<ConnectionState> {
    match super::secret_store::read_secrets_file(path)? {
        None => Ok(ConnectionState::default()),
        Some(data) => serde_json::from_str(&data).map_err(|_| {
            io::Error::other("Invalid saved client connection state; preserve it for recovery")
        }),
    }
}

/// Call from a blocking thread: OS locks must never block an async executor.
pub fn update<T>(
    path: &Path,
    expected: Option<u64>,
    mutate: impl FnOnce(&mut ConnectionState) -> io::Result<T>,
) -> io::Result<T> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Connection state has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path.with_extension("lock"))?;
    if !lock.metadata()?.is_file() {
        return Err(io::Error::other("Connection lock must be a regular file"));
    }
    lock.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    lock.lock()?;
    let mut state = read(path)?;
    if expected.is_some_and(|generation| generation != state.generation) {
        return Err(io::Error::other(
            "Saved connection changed concurrently; retry explicitly",
        ));
    }
    let result = mutate(&mut state)?;
    let bytes = serde_json::to_vec(&state).map_err(io::Error::other)?;
    super::secret_store::write_secrets_atomic(path, &bytes)?;
    Ok(result)
}

/// Retired intent deliberately retains no mail credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetiredRegistration {
    pub origin: String,
    pub client_id: String,
    pub cleanup_secret: RemoteSecret,
}
impl From<ConnectionRegistration> for RetiredRegistration {
    fn from(entry: ConnectionRegistration) -> Self {
        Self {
            origin: entry.origin,
            client_id: entry.client_id,
            cleanup_secret: entry.cleanup_secret,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetiredPairing {
    pub origin: String,
    pub cleanup_secret: RemoteSecret,
}
impl From<PendingConnection> for RetiredPairing {
    fn from(entry: PendingConnection) -> Self {
        Self {
            origin: entry.origin,
            cleanup_secret: entry.cleanup_secret,
        }
    }
}
