//! Disk-first credential storage for password-backed account secrets.
//!
//! mxr stores IMAP/SMTP passwords (and any future password-backed secret) in a
//! single plaintext TOML file at [`secrets_file_path`] (`config_dir()/secrets.toml`),
//! mode `0600` — the same model normal CLIs use (`~/.aws/credentials`,
//! `~/.config/gh/hosts.yml`, `~/.npmrc`).
//!
//! # Why disk-first, and the plaintext tradeoff
//!
//! This is a DELIBERATE tradeoff. The OS keychain encrypts secrets at rest, but
//! for an ad-hoc-signed release binary macOS silently revokes the item's ACL on
//! every upgrade: a non-interactive read then returns `errSecAuthFailed`, which
//! previously **hard-failed daemon startup** for password-auth accounts (they
//! had no fallback, unlike Gmail's disk token cache). A `0600` file is readable
//! by exactly the user's own processes and survives binary upgrades untouched.
//!
//! Security properties:
//! - Secrets are **plaintext at rest**. The only protection is filesystem
//!   permissions (`0600`, owner read/write). Any process running as the user
//!   can read them — the same threat model as `~/.aws/credentials`. We do NOT
//!   encrypt at rest; that is out of scope by design.
//! - **Symlink-safe:** the file is opened once with `O_NOFOLLOW` and verified
//!   to be a regular file through the file descriptor; permissions are read and
//!   tightened via `fchmod` on that same descriptor (never a path-based chmod),
//!   so a planted symlink or a swapped path cannot redirect the read or chmod
//!   an unrelated file (no TOCTOU window).
//! - The file is created `0600`, and its mode is re-tightened to `0600` on
//!   every read if a wider mode is found.
//! - **Crash-safe writes:** a uniquely-named temp file in the same directory is
//!   created `0600`, written, then `fsync`ed (`F_FULLFSYNC` on macOS, so the
//!   bytes reach the platter), `rename(2)`d over the target, and finally the
//!   parent directory is `fsync`ed so the rename itself is durable. A crash or a
//!   concurrent reader therefore never observes a partial or corrupt file, and
//!   an acknowledged credential survives power loss. The temp file is removed by
//!   an RAII guard if any step before the rename fails, so a partial plaintext
//!   temp is never left behind.
//! - **Empty is treated as absent** everywhere: an empty stored value reads back
//!   as `None` (so it can never permanently suppress the keychain fallback), and
//!   [`SecretStore::set`] refuses to persist an empty secret.
//!
//! ## Concurrency invariant (single writer)
//!
//! In-process writes are serialized by a global lock, and the atomic rename
//! keeps cross-process *readers* consistent. There is **no cross-process write
//! lock**: the design assumes a SINGLE writer — the daemon. The CLI and other
//! processes only read. Two processes writing concurrently could lose-update
//! (last rename wins). Every mutating entry point runs inside the daemon, so
//! this holds in practice; see [`SecretStore::set`].
//!
//! The keychain remains an OPTIONAL backend. This module is disk-only; the
//! disk→keychain-fallback and keychain→disk-mirror policy lives in the daemon
//! credential resolver, which serves a mirrored secret from disk forever after.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::resolve::secrets_file_path;

/// Serializes in-process writes so two concurrent daemon handlers performing a
/// read-modify-write can never clobber each other. Cross-process consistency
/// for readers is provided by the atomic rename in [`write_secrets_atomic`].
fn write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SecretsFile {
    /// Array-of-tables (`[[secret]]`). Keyed by (`service`, `account`). An
    /// array avoids TOML key-quoting hazards for services/accounts that contain
    /// `/`, `@`, or `.`.
    #[serde(default, rename = "secret")]
    secrets: Vec<SecretEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SecretEntry {
    /// The scoped credential service (e.g. `mxr/work-imap`, or
    /// `mxr-dev/mxr/work-imap` for non-prod instances). Callers scope before
    /// calling, matching the keychain key layout.
    service: String,
    account: String,
    secret: String,
}

/// Disk-first store for password-backed secrets. Cheap to construct; holds only
/// the file path. Reads parse the whole (tiny) file each call — there are a
/// handful of accounts at most.
#[derive(Debug, Clone)]
pub struct SecretStore {
    path: PathBuf,
}

impl SecretStore {
    /// Store backed by the default location ([`secrets_file_path`]).
    #[must_use]
    pub fn at_default_path() -> Self {
        Self {
            path: secrets_file_path(),
        }
    }

    /// Store backed by an explicit path (used by tests).
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The backing file path.
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
        // Recover from a poisoned lock: the guarded region only does file IO,
        // so a panic elsewhere leaves no in-memory invariant to protect.
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
        // Deterministic on-disk ordering keeps diffs stable and reads cheap.
        file.secrets
            .sort_by(|a, b| (&a.service, &a.account).cmp(&(&b.service, &b.account)));
        // SANITIZED: never surface the toml error (it can echo field values);
        // a serialize failure here is structural, not the caller's fault.
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
        // SANITIZED: a malformed line may contain an actual `secret = "..."`
        // value, so the toml error text (which quotes the offending source) must
        // never propagate into an io::Error that could reach a log, RPC reply,
        // or tracing span. Surface only the path.
        let file: SecretsFile = toml::from_str(&contents).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse secrets file at {}", self.path.display()),
            )
        })?;
        Ok(Some(file))
    }
}

/// Removes a temp file on drop unless [`TempFileGuard::disarm`] was called after
/// a successful rename — so a failed write/rename never leaves a partial
/// plaintext `.secrets.*.tmp` behind.
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
fn read_secrets_file(path: &Path) -> io::Result<Option<String>> {
    use std::io::Read;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW) // refuse to follow a symlink at the final component
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };

    // Verify + tighten THROUGH the fd (fstat/fchmod), never re-resolving the
    // path — this closes the TOCTOU window a path-based stat+chmod would open.
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "secrets file is not a regular file",
        ));
    }
    if metadata.permissions().mode() & 0o777 != 0o600 {
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?; // fchmod on the open fd
    }

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(Some(contents))
}

#[cfg(not(unix))]
fn read_secrets_file(path: &Path) -> io::Result<Option<String>> {
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

/// Durably, atomically, and symlink-safely replace the secrets file with
/// `contents`. See the module docs for the fsync/rename/fsync ordering.
#[cfg(unix)]
fn write_secrets_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let parent = parent_dir(path);
    std::fs::create_dir_all(&parent)?;
    // Unique temp name in the same directory so the rename is atomic on one
    // filesystem. O_EXCL (create_new) + O_NOFOLLOW + mode 0600.
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
    guard.disarm(); // the temp is now the live file; do not delete it

    // Make the rename itself durable by fsyncing the containing directory.
    let dir = std::fs::File::open(&parent)?;
    dir.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn write_secrets_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
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

    #[test]
    fn get_absent_returns_none() {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        // Missing file.
        assert!(store.get("mxr/x", "u").unwrap().is_none());
        // File present, entry absent.
        store.set("mxr/x", "u", "s").unwrap();
        assert!(store.get("mxr/other", "u").unwrap().is_none());
        assert!(store.get("mxr/x", "someone-else").unwrap().is_none());
    }

    #[test]
    fn set_upserts_existing_entry() {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        store.set("mxr/imap", "u", "first").unwrap();
        store.set("mxr/imap", "u", "second").unwrap();
        assert_eq!(
            store.get("mxr/imap", "u").unwrap().as_deref(),
            Some("second")
        );
        // Distinct (service, account) pairs coexist.
        store.set("mxr/smtp", "u", "smtp-pw").unwrap();
        assert_eq!(
            store.get("mxr/imap", "u").unwrap().as_deref(),
            Some("second")
        );
        assert_eq!(
            store.get("mxr/smtp", "u").unwrap().as_deref(),
            Some("smtp-pw")
        );
    }

    #[test]
    fn secret_with_special_characters_round_trips() {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        let secret = "p@ss\"w0rd\nwith = [weird] chars/@.";
        store.set("mxr/work-imap", "user@host.com", secret).unwrap();
        assert_eq!(
            store
                .get("mxr/work-imap", "user@host.com")
                .unwrap()
                .as_deref(),
            Some(secret)
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

    #[cfg(unix)]
    #[test]
    fn read_tightens_too_open_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        store.set("mxr/imap", "u", "s").unwrap();
        // Loosen it behind the store's back.
        std::fs::set_permissions(store.path(), std::fs::Permissions::from_mode(0o644)).unwrap();
        // A read re-tightens to 0600.
        assert_eq!(store.get("mxr/imap", "u").unwrap().as_deref(), Some("s"));
        let mode = std::fs::metadata(store.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn set_survives_a_poisoned_write_lock() {
        // Poison the global lock, then confirm set() still succeeds (it recovers
        // the guard). Uses a unique service so it can't collide with siblings.
        let poisoned = std::panic::catch_unwind(|| {
            let _guard = write_lock().lock().unwrap();
            panic!("poison the lock");
        });
        assert!(poisoned.is_err());

        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        store.set("mxr/poison-test", "u", "s").unwrap();
        assert_eq!(
            store.get("mxr/poison-test", "u").unwrap().as_deref(),
            Some("s")
        );
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
        // Every writer's value is present and the file still parses.
        for i in 0..16 {
            assert_eq!(
                store.get(&format!("mxr/svc-{i}"), "u").unwrap().as_deref(),
                Some(format!("secret-{i}").as_str())
            );
        }
    }

    #[test]
    fn empty_secret_is_treated_as_absent() {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);

        // set("") is a no-op: no file, no empty entry.
        store.set("mxr/imap", "u", "").unwrap();
        assert!(store.get("mxr/imap", "u").unwrap().is_none());

        // A stored value that is later emptied (e.g. hand-edited) reads as None,
        // so it can never permanently suppress the keychain fallback.
        store.set("mxr/imap", "u", "real").unwrap();
        assert_eq!(store.get("mxr/imap", "u").unwrap().as_deref(), Some("real"));
        std::fs::write(
            store.path(),
            "[[secret]]\nservice = \"mxr/imap\"\naccount = \"u\"\nsecret = \"\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(store.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(store.get("mxr/imap", "u").unwrap().is_none());
    }

    #[test]
    fn parse_error_is_sanitized_and_never_leaks_the_secret() {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        // A malformed file whose broken line contains a real-looking secret.
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
        // The real secret lives elsewhere; a symlink at the store path points to it.
        let real = dir.path().join("real-secrets.toml");
        std::fs::write(
            &real,
            "[[secret]]\nservice=\"s\"\naccount=\"a\"\nsecret=\"x\"\n",
        )
        .unwrap();
        let link = dir.path().join("secrets.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let store = SecretStore::new(link);
        // O_NOFOLLOW makes the open fail rather than silently reading through
        // the link (which would also let a path-based chmod tighten `real`).
        assert!(store.get("s", "a").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rename_failure_after_temp_creation_leaves_no_partial_temp() {
        // Call the writer directly so the temp is actually created and written,
        // then fail at the rename by making the destination a DIRECTORY (rename
        // of a file onto a dir fails with EISDIR — a non-permission error, so
        // this holds even under root). This pins the RAII cleanup guard: without
        // it, the fsync'd temp would be orphaned.
        let dir = TempDir::new().unwrap();
        let dest = dir.path().join("secrets.toml");
        std::fs::create_dir(&dest).unwrap();

        let error = write_secrets_atomic(&dest, b"[[secret]]\nservice=\"s\"\n").unwrap_err();
        assert!(
            !matches!(error.kind(), io::ErrorKind::NotFound),
            "expected a rename failure, got: {error}"
        );

        let strays: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.starts_with(".secrets.") && name.ends_with(".tmp")
            })
            .collect();
        assert!(
            strays.is_empty(),
            "guard must remove the temp after a failed rename: {strays:?}"
        );
    }

    #[test]
    fn temp_file_guard_removes_file_only_when_armed() {
        let dir = TempDir::new().unwrap();

        let armed = dir.path().join(".secrets.armed.tmp");
        std::fs::write(&armed, "x").unwrap();
        drop(TempFileGuard::new(armed.clone()));
        assert!(!armed.exists(), "an armed guard removes its temp on drop");

        let disarmed = dir.path().join(".secrets.disarmed.tmp");
        std::fs::write(&disarmed, "x").unwrap();
        let mut guard = TempFileGuard::new(disarmed.clone());
        guard.disarm();
        drop(guard);
        assert!(
            disarmed.exists(),
            "a disarmed guard leaves the (now-live) file in place"
        );
    }

    #[test]
    fn relative_secrets_path_round_trips_and_returns_ok() {
        // Regression: a relative MXR_SECRETS_PATH like `secrets.toml` has an
        // EMPTY parent; the parent-dir fsync must resolve it to `.` rather than
        // `File::open("")`-ing and failing after a successful write. Serialize
        // the chdir so parallel tests never observe the working-dir change.
        static CWD_LOCK: Mutex<()> = Mutex::new(());
        let _cwd = CWD_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let dir = TempDir::new().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        // A bare filename → Path::parent() is the empty path.
        let store = SecretStore::new(PathBuf::from("secrets.toml"));
        let set_result = store.set("mxr/imap", "u", "pw");
        let got = store.get("mxr/imap", "u");

        // Restore the CWD before asserting so a failure can't strand the process.
        std::env::set_current_dir(&original).unwrap();

        set_result.expect("set with a relative path must return Ok");
        assert_eq!(
            got.unwrap().as_deref(),
            Some("pw"),
            "relative-path secret must round-trip"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_is_durable_and_atomic_round_trip() {
        // Exercises the fsync/rename/dir-fsync path end to end.
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        store.set("mxr/imap", "u", "durable-pw").unwrap();
        // Re-open a fresh store to prove the bytes are on disk, not cached.
        let reopened = SecretStore::new(store.path().to_path_buf());
        assert_eq!(
            reopened.get("mxr/imap", "u").unwrap().as_deref(),
            Some("durable-pw")
        );
    }
}
