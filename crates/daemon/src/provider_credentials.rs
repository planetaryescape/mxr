use std::sync::Arc;

/// Whether the OS keychain is fully disabled via `MXR_KEYCHAIN=off` (mirrors
/// `MXR_ACTIVITY=off`). When off, mxr never touches the keychain at all —
/// neither the write mirror ([`crate::handler`] persist path) nor the read
/// fallback ([`disk_first_password`]). Disk (`secrets.toml`) is the sole source
/// of truth, which is the point on headless / disk-only hosts where the
/// keychain is unavailable.
pub(crate) fn keychain_disabled() -> bool {
    std::env::var("MXR_KEYCHAIN").is_ok_and(|value| value.eq_ignore_ascii_case("off"))
}

pub(crate) fn imap_config_with_credentials(
    host: String,
    port: u16,
    username: String,
    password_ref: String,
    auth_required: bool,
    use_tls: bool,
    max_connections: usize,
) -> anyhow::Result<mxr_provider_imap::config::ImapConfig> {
    let config = mxr_provider_imap::config::ImapConfig::new(
        host,
        port,
        username,
        password_ref,
        auth_required,
        use_tls,
    )
    .with_max_connections(max_connections);
    if !auth_required {
        return Ok(config);
    }
    Ok(
        config.with_password_reader(Arc::new(|password_ref, username| {
            disk_first_password(password_ref, username).map_err(|error| {
                mxr_provider_imap::error::ImapProviderError::Keyring(error.to_string())
            })
        })),
    )
}

pub(crate) fn smtp_config_with_credentials(
    host: String,
    port: u16,
    username: String,
    password_ref: String,
    auth_required: bool,
    use_tls: bool,
) -> anyhow::Result<mxr_provider_smtp::config::SmtpConfig> {
    let config = mxr_provider_smtp::config::SmtpConfig::new(
        host,
        port,
        username,
        password_ref,
        auth_required,
        use_tls,
    );
    config.validate()?;
    if !auth_required {
        return Ok(config);
    }
    Ok(
        config.with_password_reader(Arc::new(|password_ref, username| {
            disk_first_password(password_ref, username)
                .map_err(|error| mxr_provider_smtp::config::SmtpError::Keyring(error.to_string()))
        })),
    )
}

/// Resolve a password-backed credential, disk first, keychain as an optional
/// fallback.
///
/// Order: `secrets.toml` (disk) → OS keychain → error. On a disk miss with a
/// keychain hit the secret is MIRRORED back to disk (idempotent, best-effort)
/// so subsequent reads are served from disk forever — immune to the ad-hoc-sign
/// keychain-ACL loss that binary upgrades trigger. `password_ref` is the raw
/// (unscoped) config ref; it is scoped to the runtime identity here so it
/// matches how credentials are stored.
fn disk_first_password(password_ref: &str, username: &str) -> anyhow::Result<String> {
    disk_first_password_with(
        &mxr_config::SecretStore::at_default_path(),
        password_ref,
        username,
        !keychain_disabled(),
        mxr_keychain::get_password,
    )
}

/// The disk half of the resolver, abstracted so tests can inject a store whose
/// `set` fails deterministically (exercising the mirror-failure branch without
/// depending on filesystem permissions, which root would bypass).
trait DiskSecretStore {
    fn get(&self, service: &str, account: &str) -> std::io::Result<Option<String>>;
    fn set(&self, service: &str, account: &str, secret: &str) -> std::io::Result<()>;
}

#[expect(
    clippy::use_self,
    reason = "explicit path disambiguates the inherent methods from these trait methods"
)]
impl DiskSecretStore for mxr_config::SecretStore {
    fn get(&self, service: &str, account: &str) -> std::io::Result<Option<String>> {
        mxr_config::SecretStore::get(self, service, account)
    }
    fn set(&self, service: &str, account: &str, secret: &str) -> std::io::Result<()> {
        mxr_config::SecretStore::set(self, service, account, secret)
    }
}

/// Testable core of [`disk_first_password`]: the disk store and the keychain
/// getter are injected so the migration path can be exercised without touching
/// the real OS keychain.
fn disk_first_password_with<F, E>(
    store: &impl DiskSecretStore,
    password_ref: &str,
    username: &str,
    keychain_enabled: bool,
    keychain_get: F,
) -> anyhow::Result<String>
where
    F: FnOnce(&str, &str) -> Result<String, E>,
    E: std::fmt::Display,
{
    let scoped_ref = scoped_password_ref(password_ref);

    if let Some(secret) = store.get(&scoped_ref, username)? {
        return Ok(secret);
    }

    if !keychain_enabled {
        return Err(anyhow::anyhow!(
            "no credential found on disk for {scoped_ref} (keychain disabled via MXR_KEYCHAIN=off)"
        ));
    }

    match keychain_get(&scoped_ref, username) {
        Ok(secret) if !secret.is_empty() => {
            match store.set(&scoped_ref, username, &secret) {
                Ok(()) => tracing::info!(
                    credential_service = %scoped_ref,
                    "mirrored credential from keychain to disk; future reads served from disk"
                ),
                Err(error) => tracing::warn!(
                    credential_service = %scoped_ref,
                    error = %error,
                    "keychain credential read but disk mirror failed; serving from keychain this time"
                ),
            }
            Ok(secret)
        }
        Ok(_) => Err(anyhow::anyhow!(
            "no usable credential found on disk or in the keychain for {scoped_ref} (keychain value was empty)"
        )),
        Err(error) => Err(anyhow::anyhow!(
            "no credential found on disk or in the keychain for {scoped_ref}: {error}"
        )),
    }
}

pub(crate) fn gmail_auth(
    client_id: String,
    client_secret: String,
    token_ref: String,
) -> mxr_provider_gmail::auth::GmailAuth {
    mxr_provider_gmail::auth::GmailAuth::new(client_id, client_secret, token_ref).with_storage(
        mxr_config::token_dir(),
        mxr_config::gmail_oauth_keychain_service(),
    )
}

pub(crate) fn outlook_auth(
    client_id: String,
    token_ref: String,
    tenant: mxr_provider_outlook::OutlookTenant,
) -> mxr_provider_outlook::OutlookAuth {
    mxr_provider_outlook::OutlookAuth::new(client_id, token_ref, tenant)
        .with_token_root(mxr_config::token_dir())
}

pub(crate) fn scoped_password_ref(password_ref: &str) -> String {
    mxr_config::scoped_credential_service(password_ref)
}

#[cfg(test)]
pub(crate) fn scoped_password_ref_for_test(password_ref: &str) -> String {
    scoped_password_ref(password_ref)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::panic,
        reason = "a test double panics to prove the keychain is never consulted"
    )]

    use super::*;

    fn empty_store() -> (tempfile::TempDir, mxr_config::SecretStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = mxr_config::SecretStore::new(dir.path().join("secrets.toml"));
        (dir, store)
    }

    fn keychain_never(_: &str, _: &str) -> Result<String, String> {
        panic!("keychain must not be consulted when the secret is on disk");
    }

    #[test]
    fn password_refs_are_scoped_to_runtime_identity_before_keychain_lookup() {
        temp_env::with_var("MXR_INSTANCE", Some("mxr-dev"), || {
            assert_eq!(
                super::scoped_password_ref_for_test("mxr/work-imap"),
                "mxr-dev/mxr/work-imap"
            );
        });
    }

    #[test]
    fn keychain_hit_is_mirrored_to_disk_then_served_from_disk() {
        temp_env::with_var("MXR_INSTANCE", Some("mxr"), || {
            let (_dir, store) = empty_store();

            let first =
                disk_first_password_with(&store, "keyring:imap", "user@host", true, |_, _| {
                    Ok::<_, String>("keychain-pw".to_string())
                })
                .unwrap();
            assert_eq!(first, "keychain-pw");

            assert_eq!(
                store.get("keyring:imap", "user@host").unwrap().as_deref(),
                Some("keychain-pw")
            );

            let second =
                disk_first_password_with(&store, "keyring:imap", "user@host", true, keychain_never)
                    .unwrap();
            assert_eq!(second, "keychain-pw");
        });
    }

    #[test]
    fn absent_everywhere_errors_at_resolve_not_construction() {
        temp_env::with_var("MXR_INSTANCE", Some("mxr"), || {
            let (_dir, store) = empty_store();
            let error =
                disk_first_password_with(&store, "keyring:absent", "user@host", true, |_, _| {
                    Err::<String, _>("not found".to_string())
                })
                .unwrap_err();
            assert!(
                error.to_string().contains("no credential found"),
                "unexpected error: {error}"
            );
        });
    }

    #[test]
    fn keychain_disabled_disk_miss_never_reads_the_keychain() {
        temp_env::with_var("MXR_INSTANCE", Some("mxr"), || {
            let (_dir, store) = empty_store();
            let error = disk_first_password_with(
                &store,
                "keyring:imap",
                "user@host",
                false,
                keychain_never,
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("keychain disabled"),
                "unexpected error: {error}"
            );
        });
    }

    #[test]
    fn corrupt_disk_surfaces_sanitized_error_without_the_secret() {
        temp_env::with_var("MXR_INSTANCE", Some("mxr"), || {
            let (dir, store) = empty_store();
            let leaked = "TOP-SECRET-abc123";
            std::fs::write(
                dir.path().join("secrets.toml"),
                format!("garbage secret = {leaked}"),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    dir.path().join("secrets.toml"),
                    std::fs::Permissions::from_mode(0o600),
                )
                .unwrap();
            }

            let error =
                disk_first_password_with(&store, "keyring:imap", "user@host", true, keychain_never)
                    .unwrap_err();
            assert!(
                !error.to_string().contains(leaked),
                "sanitized error leaked the secret: {error}"
            );
        });
    }
}
