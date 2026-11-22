use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::resolve::secrets_file_path;

fn write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SecretsFile {
    #[serde(default, rename = "secret")]
    secrets: Vec<SecretEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SecretEntry {
    service: String,
    account: String,
    secret: String,
}

#[derive(Debug, Clone)]
pub struct SecretStore {
    path: PathBuf,
}

impl SecretStore {
    #[must_use]
    pub fn at_default_path() -> Self {
        Self {
            path: secrets_file_path(),
        }
    }

    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Fetch the secret for `(service, account)`, or `Ok(None)` if the file or
    /// the entry is absent. Reading re-tightens the file to `0600`. An empty
    /// stored value reads back as `None` (empty is treated as absent, so a
    /// hand-edited or mis-mirrored empty can never suppress the keychain
    /// fallback). Safe to call concurrently from any process.
    pub fn get(&self, service: &str, account: &str) -> io::Result<Option<String>> {
        let Some(file) = self.read_file()? else {
            return Ok(None);
        };
        Ok(file
            .secrets
            .into_iter()
            .find(|entry| entry.service == service && entry.account == account)
            .map(|entry| entry.secret)
            .filter(|secret| !secret.is_empty()))
    }

    /// Upsert the secret for `(service, account)` and persist atomically at
    /// `0600`. Disk is authoritative.
    ///
    /// An empty `secret` is refused: it is a no-op (nothing is written and no
    /// empty entry is created), keeping "empty == absent" consistent with
    /// [`SecretStore::get`] and with the upstream persistence guard.
    ///
    /// **Single-writer invariant:** this serializes writers *within* the
    /// process only. It is safe against concurrent readers in any process, but
    /// concurrent *writers* across processes could lose-update. Only the daemon
    /// is expected to call this (see the module-level concurrency note).
    pub fn set(&self, service: &str, account: &str, secret: &str) -> io::Result<()> {
        if secret.is_empty() {
            return Ok(());
        }
        let _guard = write_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut file = self.read_file()?.unwrap_or_default();
        if let Some(entry) = file
            .secrets
            .iter_mut()
            .find(|entry| entry.service == service && entry.account == account)
        {
            entry.secret = secret.to_string();
        } else {
            file.secrets.push(SecretEntry {
                service: service.to_string(),
                account: account.to_string(),
                secret: secret.to_string(),
            });
        }
        file.secrets
            .sort_by(|a, b| (&a.service, &a.account).cmp(&(&b.service, &b.account)));
        let contents = toml::to_string_pretty(&file).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "failed to serialize secrets file",
            )
        })?;
        write_secrets_atomic(&self.path, contents.as_bytes())
    }

    pub fn remove(&self, service: &str, account: &str) -> io::Result<()> {
        let _guard = write_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(mut file) = self.read_file()? else {
            return Ok(());
        };
        let before = file.secrets.len();
        file.secrets
            .retain(|entry| !(entry.service == service && entry.account == account));
        if file.secrets.len() == before {
            return Ok(());
        }
        let contents = toml::to_string_pretty(&file).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "failed to serialize secrets file",
            )
        })?;
        write_secrets_atomic(&self.path, contents.as_bytes())
    }

    fn read_file(&self) -> io::Result<Option<SecretsFile>> {
        let Some(contents) = read_secrets_file(&self.path)? else {
            return Ok(None);
        };
        let file: SecretsFile = toml::from_str(&contents).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse secrets file at {}", self.path.display()),
            )
        })?;
        Ok(Some(file))
    }
}

struct TempFileGuard {
    path: Option<PathBuf>,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Read the secrets file symlink-safely through a single descriptor. `Ok(None)`
/// for a missing file; errors for a symlink/non-regular file or IO failure.
#[cfg(unix)]
pub(crate) fn read_secrets_file(path: &Path) -> io::Result<Option<String>> {
    use std::io::Read;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };

    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "secrets file is not a regular file",
        ));
    }
    if metadata.permissions().mode() & 0o777 != 0o600 {
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(Some(contents))
}

#[cfg(not(unix))]
pub(crate) fn read_secrets_file(path: &Path) -> io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The directory containing `path`, resolving a bare filename (whose
/// `Path::parent()` is the EMPTY path) to `.`. Without this, a relative
/// `MXR_SECRETS_PATH` like `secrets.toml` would make `create_dir_all("")` and
/// the parent-dir `fsync` (`File::open("")`) fail — reporting an error even
/// though the credential was written and is live.
fn parent_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

#[cfg(unix)]
pub(crate) fn write_secrets_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let parent = parent_dir(path);
    std::fs::create_dir_all(&parent)?;
    let tmp = path.with_file_name(format!(".secrets.{}.tmp", uuid::Uuid::now_v7()));
    let mut guard = TempFileGuard::new(tmp.clone());

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(&tmp)?;
    file.write_all(contents)?;
    // Flush the bytes to durable storage before the rename. On macOS
    // `sync_all` issues F_FULLFSYNC (a plain fsync would not reach the platter).
    file.sync_all()?;
    drop(file);

    std::fs::rename(&tmp, path)?;
    guard.disarm();

    let dir = std::fs::File::open(&parent)?;
    dir.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn write_secrets_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    use std::io::Write;

    std::fs::create_dir_all(parent_dir(path))?;
    let tmp = path.with_file_name(format!(".secrets.{}.tmp", uuid::Uuid::now_v7()));
    let mut guard = TempFileGuard::new(tmp.clone());
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)?;
    guard.disarm();
    Ok(())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests unwrap fixture setup and deliberately poison a lock"
    )]

    use super::*;
    use tempfile::TempDir;

    fn store_in(dir: &TempDir) -> SecretStore {
        SecretStore::new(dir.path().join("secrets.toml"))
    }

    #[test]
    fn set_then_get_round_trip() {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        store.set("mxr/work-imap", "me@corp.com", "app-pw").unwrap();
        assert_eq!(
            store
                .get("mxr/work-imap", "me@corp.com")
                .unwrap()
                .as_deref(),
            Some("app-pw")
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_is_created_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        store.set("mxr/imap", "u", "s").unwrap();
        let mode = std::fs::metadata(store.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn concurrent_writes_do_not_corrupt_file() {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        std::thread::scope(|scope| {
            for i in 0..16 {
                let store = store.clone();
                scope.spawn(move || {
                    store
                        .set(&format!("mxr/svc-{i}"), "u", &format!("secret-{i}"))
                        .unwrap();
                });
            }
        });
        for i in 0..16 {
            assert_eq!(
                store.get(&format!("mxr/svc-{i}"), "u").unwrap().as_deref(),
                Some(format!("secret-{i}").as_str())
            );
        }
    }

    #[test]
    fn parse_error_is_sanitized_and_never_leaks_the_secret() {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        let leaked = "SUPER-SECRET-PASSWORD-9f3a";
        std::fs::write(store.path(), format!("this is not toml secret = {leaked}")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(store.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
        }

        let error = store.get("mxr/imap", "u").unwrap_err();
        let rendered = error.to_string();
        assert!(
            !rendered.contains(leaked),
            "parse error leaked the secret: {rendered}"
        );
        assert!(
            rendered.contains("failed to parse secrets file"),
            "unexpected error: {rendered}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_refuses_to_follow_a_symlink() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real-secrets.toml");
        std::fs::write(
            &real,
            "[[secret]]\nservice=\"s\"\naccount=\"a\"\nsecret=\"x\"\n",
        )
        .unwrap();
        let link = dir.path().join("secrets.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let store = SecretStore::new(link);
        assert!(store.get("s", "a").is_err());
    }
}
