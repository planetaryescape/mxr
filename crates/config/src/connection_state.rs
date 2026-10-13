//! Client-only state, excluded from portable daemon backups.
//! Plaintext 0600 matches the established disk-first secret policy. A stable lock
//! guards the whole selection/credential/cleanup commit across processes.
use mxr_core::RemoteSecret;
use serde::{Deserialize, Serialize};
use sha2::Digest;
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
    pub name: String,
    pub mail_verifier: RemoteSecret,
    pub cleanup_secret: RemoteSecret,
}
impl From<PendingConnection> for RetiredPairing {
    fn from(entry: PendingConnection) -> Self {
        Self {
            origin: entry.origin,
            name: entry.name,
            mail_verifier: RemoteSecret::new(base16ct::lower::encode_string(
                &sha2::Sha256::digest(entry.mail_secret.expose().as_bytes()),
            )),
            cleanup_secret: entry.cleanup_secret,
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "isolated state fixtures assert directly"
    )]
    use super::*;
    #[test]
    fn stale_generation_cannot_overwrite_switch_or_drop_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connection.json");
        update(&path, Some(0), |state| {
            state.generation = 1;
            state.cleanup.push(RetiredRegistration {
                origin: "https://fixture.example".into(),
                client_id: "own-id".into(),
                cleanup_secret: RemoteSecret::new("b".repeat(64)),
            });
            Ok(())
        })
        .unwrap();
        assert!(update(&path, Some(0), |state| {
            state.cleanup.clear();
            Ok(())
        })
        .is_err());
        let state = read(&path).unwrap();
        assert_eq!(state.generation, 1);
        assert_eq!(state.cleanup.len(), 1);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("mail_secret"));
    }
    #[test]
    fn cross_process_writers_keep_all_updates_on_stable_lock_inode() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connection.json");
        let mut children = Vec::new();
        for _ in 0..3 {
            children.push(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["connection_state::tests::child_writer", "--exact"])
                    .env("MXR_TEST_CLIENT_STATE", &path)
                    .stdout(std::process::Stdio::null())
                    .spawn()
                    .unwrap(),
            );
        }
        for mut child in children {
            assert!(child.wait().unwrap().success());
        }
        assert_eq!(read(&path).unwrap().generation, 60);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[test]
    fn child_writer() {
        let Some(path) = std::env::var_os("MXR_TEST_CLIENT_STATE") else {
            return;
        };
        for _ in 0..20 {
            update(Path::new(&path), None, |state| {
                state.generation += 1;
                Ok(())
            })
            .unwrap();
        }
    }
    #[test]
    fn symlinked_state_and_lock_are_refused() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connection.json");
        let victim = directory.path().join("victim");
        std::fs::write(&victim, "keep").unwrap();
        std::os::unix::fs::symlink(&victim, &path).unwrap();
        assert!(read(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&victim, path.with_extension("lock")).unwrap();
        assert!(update(&path, None, |_| Ok(())).is_err());
        assert_eq!(std::fs::read_to_string(victim).unwrap(), "keep");
    }
}
