//! `TokenStorage` impl that persists yup-oauth2 token caches in the OS
//! keychain (Keychain Access on macOS, Secret Service on Linux). Falls back
//! to the on-disk format yup-oauth2 ships with when the keychain is
//! unavailable, so headless Linux without secret-service still functions.
//!
//! Format: `[ { "scopes": [...], "token": <TokenInfo> }, ... ]` — same JSON
//! shape as yup-oauth2's `DiskStorage` so a one-shot migration from the
//! legacy disk path is a straight read-write-mirror.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use yup_oauth2::storage::{TokenInfo, TokenStorage};

pub(crate) const KEYCHAIN_SERVICE: &str = "mxr-gmail-oauth";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeychainTokenCacheErrorKind {
    NotFound,
    Access,
    #[cfg(target_os = "macos")]
    InvalidData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KeychainTokenCacheError {
    kind: KeychainTokenCacheErrorKind,
    message: String,
}

impl KeychainTokenCacheError {
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            kind: KeychainTokenCacheErrorKind::NotFound,
            message: message.into(),
        }
    }

    fn access(message: impl Into<String>) -> Self {
        Self {
            kind: KeychainTokenCacheErrorKind::Access,
            message: message.into(),
        }
    }

    #[cfg(target_os = "macos")]
    fn invalid_data(message: impl Into<String>) -> Self {
        Self {
            kind: KeychainTokenCacheErrorKind::InvalidData,
            message: message.into(),
        }
    }

    fn is_not_found(&self) -> bool {
        self.kind == KeychainTokenCacheErrorKind::NotFound
    }
}

impl std::fmt::Display for KeychainTokenCacheError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct StoredToken {
    scopes: Vec<String>,
    token: TokenInfo,
}

pub(crate) struct KeychainTokenStorage {
    keychain_service: String,
    token_ref: String,
    /// Private disk fallback retained alongside the keychain so noninteractive
    /// keychain access failures cannot strand an otherwise valid token cache.
    fallback_path: PathBuf,
    cache: Mutex<Vec<StoredToken>>,
}

impl KeychainTokenStorage {
    pub(crate) fn new_with_service(
        token_ref: String,
        fallback_path: PathBuf,
        keychain_service: String,
    ) -> Self {
        let cache = load_initial_with_service(&keychain_service, &token_ref, &fallback_path);
        Self {
            keychain_service,
            token_ref,
            fallback_path,
            cache: Mutex::new(cache),
        }
    }

    fn persist(&self, json: &str) -> std::io::Result<()> {
        match write_keychain_token_cache(&self.keychain_service, &self.token_ref, json) {
            Ok(()) => self.persist_to_disk(json),
            Err(error) => {
                tracing::warn!(
                    token_ref = %self.token_ref,
                    error = %error,
                    "keychain write failed; falling back to disk cache"
                );
                self.persist_to_disk(json)
            }
        }
    }

    fn persist_to_disk(&self, json: &str) -> std::io::Result<()> {
        if let Some(parent) = self.fallback_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_fallback_token_file(&self.fallback_path, json)
    }
}

#[cfg(unix)]
fn write_fallback_token_file(path: &std::path::Path, json: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(json.as_bytes())
}

#[cfg(not(unix))]
fn write_fallback_token_file(path: &std::path::Path, json: &str) -> std::io::Result<()> {
    std::fs::write(path, json)
}

#[cfg(target_os = "macos")]
fn keychain_password_options(
    service: &str,
    token_ref: &str,
) -> security_framework::passwords::PasswordOptions {
    security_framework::passwords::PasswordOptions::new_generic_password(service, token_ref)
}

#[cfg(target_os = "macos")]
fn map_security_error(
    context: &str,
    error: security_framework::base::Error,
) -> KeychainTokenCacheError {
    if error.code() == security_framework_sys::base::errSecItemNotFound {
        return KeychainTokenCacheError::not_found(format!("{context}: {error}"));
    }
    KeychainTokenCacheError::access(format!("{context}: {error}"))
}

#[cfg(target_os = "macos")]
fn with_disabled_keychain_ui<T>(
    operation: impl FnOnce() -> Result<T, security_framework::base::Error>,
) -> Result<T, KeychainTokenCacheError> {
    let _interaction_lock =
        security_framework::os::macos::keychain::SecKeychain::disable_user_interaction()
            .map_err(|error| map_security_error("Failed to disable keychain UI", error))?;
    operation().map_err(|error| map_security_error("macOS keychain operation failed", error))
}

#[cfg(target_os = "macos")]
fn read_keychain_token_cache(
    keychain_service: &str,
    token_ref: &str,
) -> Result<String, KeychainTokenCacheError> {
    let bytes = with_disabled_keychain_ui(|| {
        security_framework::passwords::generic_password(keychain_password_options(
            keychain_service,
            token_ref,
        ))
    })?;
    String::from_utf8(bytes)
        .map_err(|_| KeychainTokenCacheError::invalid_data("stored OAuth token cache is not UTF-8"))
}

#[cfg(not(target_os = "macos"))]
fn read_keychain_token_cache(
    keychain_service: &str,
    token_ref: &str,
) -> Result<String, KeychainTokenCacheError> {
    let entry = keyring::Entry::new(keychain_service, token_ref)
        .map_err(|error| KeychainTokenCacheError::access(error.to_string()))?;
    entry.get_password().map_err(|error| match error {
        keyring::Error::NoEntry => KeychainTokenCacheError::not_found(error.to_string()),
        _ => KeychainTokenCacheError::access(error.to_string()),
    })
}

#[cfg(target_os = "macos")]
fn write_keychain_token_cache(
    keychain_service: &str,
    token_ref: &str,
    json: &str,
) -> Result<(), KeychainTokenCacheError> {
    with_disabled_keychain_ui(|| {
        security_framework::passwords::set_generic_password(
            keychain_service,
            token_ref,
            json.as_bytes(),
        )
    })
}

#[cfg(not(target_os = "macos"))]
fn write_keychain_token_cache(
    keychain_service: &str,
    token_ref: &str,
    json: &str,
) -> Result<(), KeychainTokenCacheError> {
    let entry = keyring::Entry::new(keychain_service, token_ref)
        .map_err(|error| KeychainTokenCacheError::access(error.to_string()))?;
    entry
        .set_password(json)
        .map_err(|error| KeychainTokenCacheError::access(error.to_string()))
}

fn log_keychain_read_error(token_ref: &str, error: &KeychainTokenCacheError) {
    if error.is_not_found() {
        tracing::debug!(
            token_ref = %token_ref,
            "OAuth token cache not present in keychain"
        );
    } else {
        tracing::warn!(
            token_ref = %token_ref,
            error = %error,
            "keychain read failed; falling back to disk cache if present"
        );
    }
}

pub(crate) fn has_keychain_token_cache(keychain_service: &str, token_ref: &str) -> bool {
    match read_keychain_token_cache(keychain_service, token_ref) {
        Ok(_) => true,
        Err(error) => {
            log_keychain_read_error(token_ref, &error);
            false
        }
    }
}

fn load_initial_with_service(
    keychain_service: &str,
    token_ref: &str,
    fallback_path: &std::path::Path,
) -> Vec<StoredToken> {
    match read_keychain_token_cache(keychain_service, token_ref) {
        Ok(payload) => {
            if let Ok(parsed) = serde_json::from_str::<Vec<StoredToken>>(&payload) {
                return parsed;
            }
        }
        Err(error) => {
            log_keychain_read_error(token_ref, &error);
        }
    }

    load_fallback_token_cache(token_ref, fallback_path, |json| {
        write_keychain_token_cache(keychain_service, token_ref, json).is_ok()
    })
    .unwrap_or_default()
}

fn load_fallback_token_cache(
    token_ref: &str,
    fallback_path: &std::path::Path,
    mirror_to_keychain: impl FnOnce(&str) -> bool,
) -> Option<Vec<StoredToken>> {
    let bytes = std::fs::read(fallback_path).ok()?;
    let parsed = serde_json::from_slice::<Vec<StoredToken>>(&bytes).ok()?;
    if !parsed.is_empty() {
        if let Ok(json) = serde_json::to_string(&parsed) {
            if mirror_to_keychain(&json) {
                tracing::info!(
                    token_ref = %token_ref,
                    "mirrored OAuth token cache from disk to keychain"
                );
            }
        }
    }
    Some(parsed)
}

#[async_trait]
impl TokenStorage for KeychainTokenStorage {
    async fn set(&self, scopes: &[&str], token: TokenInfo) -> anyhow::Result<()> {
        let scopes_owned = normalize_scopes(scopes);
        let json = {
            let mut cache = self
                .cache
                .lock()
                .map_err(|_| anyhow::anyhow!("token cache mutex poisoned"))?;
            cache.retain(|stored| normalize_scopes_owned(&stored.scopes) != scopes_owned);
            cache.push(StoredToken {
                scopes: scopes_owned,
                token,
            });
            serde_json::to_string(&*cache)?
        };
        self.persist(&json)?;
        Ok(())
    }

    async fn get(&self, scopes: &[&str]) -> Option<TokenInfo> {
        let target = normalize_scopes(scopes);
        let cache = self.cache.lock().ok()?;
        cache
            .iter()
            .find(|stored| normalize_scopes_owned(&stored.scopes) == target)
            .map(|stored| stored.token.clone())
    }
}

/// yup-oauth2's storage layer always passes scopes already sorted+deduped
/// (see `Storage::set` in yup-oauth2/src/storage.rs). But the legacy on-disk
/// `DiskStorage` writes scopes in their *original* order, and our migration
/// from disk preserves that order verbatim. Without this normalisation a
/// migrated cache entry would never match a fresh `get()` call and yup-oauth2
/// would fall through to a full interactive-auth flow.
fn normalize_scopes(scopes: &[&str]) -> Vec<String> {
    let mut owned: Vec<String> = scopes.iter().map(|s| (*s).to_string()).collect();
    owned.sort_unstable();
    owned.dedup();
    owned
}

fn normalize_scopes_owned(scopes: &[String]) -> Vec<String> {
    let mut owned = scopes.to_vec();
    owned.sort_unstable();
    owned.dedup();
    owned
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tempfile::tempdir;

    fn fake_token(value: &str) -> TokenInfo {
        TokenInfo {
            access_token: Some(value.to_string()),
            refresh_token: Some(format!("{value}-refresh")),
            expires_at: None,
            id_token: None,
        }
    }

    fn unique_ref(prefix: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{prefix}-{nanos}")
    }

    #[tokio::test]
    async fn set_and_get_round_trip_in_memory_cache() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("missing.json");
        let storage = KeychainTokenStorage {
            keychain_service: KEYCHAIN_SERVICE.to_string(),
            token_ref: unique_ref("test-mem"),
            fallback_path: path,
            cache: Mutex::new(Vec::new()),
        };
        let scopes = ["scope-a", "scope-b"];
        {
            let mut cache = storage.cache.lock().unwrap();
            cache.push(StoredToken {
                scopes: scopes
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
                token: fake_token("hello"),
            });
        }

        let fetched = storage.get(&scopes).await.unwrap();
        assert_eq!(fetched.access_token.as_deref(), Some("hello"));
    }

    #[test]
    fn keeps_legacy_disk_cache_after_successful_keychain_mirror() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("legacy.json");
        let legacy = vec![StoredToken {
            scopes: vec!["scope-a".to_string()],
            token: fake_token("legacy-access"),
        }];
        std::fs::write(&path, serde_json::to_string(&legacy).unwrap()).unwrap();

        let mirrored = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mirrored_for_closure = std::sync::Arc::clone(&mirrored);
        let loaded = load_fallback_token_cache(&unique_ref("test-migrate"), &path, move |_json| {
            mirrored_for_closure.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        });
        let token = loaded.unwrap().into_iter().next().unwrap().token;
        assert_eq!(token.access_token.as_deref(), Some("legacy-access"));
        assert!(
            mirrored.load(std::sync::atomic::Ordering::SeqCst),
            "legacy disk cache should be mirrored to the keychain when possible"
        );
        assert!(
            path.exists(),
            "disk fallback must stay available after keychain mirroring"
        );
    }

    /// Regression: yup-oauth2's storage layer always passes scopes
    /// sorted+deduped, but the legacy on-disk DiskStorage wrote them in their
    /// original order (typically the order the SDK declared them, which is
    /// `gmail.readonly`, `gmail.modify`, `gmail.labels` — not lexicographic).
    /// A migrated cache row had unsorted scopes; without normalisation, the
    /// ordered comparison missed and yup-oauth2 fell through to a full
    /// interactive-auth flow on the next access — hanging the daemon.
    #[tokio::test]
    async fn get_matches_when_stored_scopes_are_in_a_different_order() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("missing.json");
        let storage = KeychainTokenStorage {
            keychain_service: KEYCHAIN_SERVICE.to_string(),
            token_ref: unique_ref("test-scope-order"),
            fallback_path: path,
            cache: Mutex::new(vec![StoredToken {
                // Stored in the unsorted order the legacy migration produced.
                scopes: vec![
                    "https://www.googleapis.com/auth/gmail.readonly".to_string(),
                    "https://www.googleapis.com/auth/gmail.modify".to_string(),
                    "https://www.googleapis.com/auth/gmail.labels".to_string(),
                ],
                token: fake_token("legacy-token"),
            }]),
        };

        // yup-oauth2 always passes scopes sorted+deduped to TokenStorage::get.
        let sorted_scopes = [
            "https://www.googleapis.com/auth/gmail.labels",
            "https://www.googleapis.com/auth/gmail.modify",
            "https://www.googleapis.com/auth/gmail.readonly",
        ];

        let token = storage.get(&sorted_scopes).await;
        assert!(
            token.is_some(),
            "stored unsorted scopes must still match a sorted lookup"
        );
        assert_eq!(token.unwrap().access_token.as_deref(), Some("legacy-token"));
    }

    #[cfg(unix)]
    #[test]
    fn disk_fallback_token_cache_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let path = temp.path().join("fallback.json");

        write_fallback_token_file(&path, "[]").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
