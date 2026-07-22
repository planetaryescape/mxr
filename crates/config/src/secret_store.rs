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
//! - The file is created `0600`, and its mode is re-tightened to `0600` on
//!   every read (a too-open file is fixed in place, mirroring the daemon-token
//!   behavior).
//! - Writes are atomic: a uniquely-named temp file in the same directory is
//!   created `0600`, then `rename(2)`d over the target, so a crash or a
//!   concurrent reader never observes a partial or corrupt file.
//! - In-process writes are serialized by a global lock; the atomic rename keeps
//!   cross-process readers consistent. In practice only the daemon writes;
//!   both the daemon and the CLI read.
//!
//! The keychain remains an OPTIONAL backend. This module is disk-only; the
//! disk→keychain-fallback and keychain→disk-mirror policy lives in the daemon
//! credential resolver, which serves a mirrored secret from disk forever after.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::resolve::{create_new_secret_0600, enforce_mode_0600, secrets_file_path};

/// Serializes in-process writes so two concurrent daemon handlers performing a
/// read-modify-write can never clobber each other. Cross-process consistency is
/// provided by the atomic rename in [`SecretStore::write_atomic`].
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
    /// the entry is absent. Reading re-tightens the file to `0600`.
    pub fn get(&self, service: &str, account: &str) -> io::Result<Option<String>> {
        let Some(file) = self.read_file()? else {
            return Ok(None);
        };
        Ok(file
            .secrets
            .into_iter()
            .find(|entry| entry.service == service && entry.account == account)
            .map(|entry| entry.secret))
    }

    /// Upsert the secret for `(service, account)` and persist atomically at
    /// `0600`. Disk is authoritative.
    pub fn set(&self, service: &str, account: &str, secret: &str) -> io::Result<()> {
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
        self.write_atomic(&file)
    }

    fn read_file(&self) -> io::Result<Option<SecretsFile>> {
        match std::fs::read_to_string(&self.path) {
            Ok(contents) => {
                enforce_mode_0600(&self.path)?;
                let file: SecretsFile = toml::from_str(&contents)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                Ok(Some(file))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write_atomic(&self, file: &SecretsFile) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(file)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        // Unique temp name in the same directory so the rename is atomic on the
        // same filesystem. `create_new_secret_0600` uses O_EXCL + mode 0600.
        let tmp = self
            .path
            .with_file_name(format!(".secrets.{}.tmp", uuid::Uuid::now_v7()));
        create_new_secret_0600(&tmp, &contents)?;
        match std::fs::rename(&tmp, &self.path) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = std::fs::remove_file(&tmp);
                Err(error)
            }
        }
    }
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
}
