#![cfg_attr(test, allow(clippy::unwrap_used))]

#[cfg(target_os = "macos")]
use security_framework::base::Error as SecurityError;
#[cfg(target_os = "macos")]
use security_framework::os::macos::keychain::SecKeychain;
#[cfg(target_os = "macos")]
use security_framework::os::macos::passwords::find_generic_password;
#[cfg(target_os = "macos")]
use security_framework::passwords::{
    generic_password, set_generic_password_options, PasswordOptions,
};
#[cfg(target_os = "macos")]
use security_framework_sys::base::errSecItemNotFound;

#[derive(Debug, Clone, PartialEq, Eq)]
enum KeychainErrorKind {
    NotFound,
    Access,
    InvalidData,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
#[error("{message}")]
pub struct KeychainError {
    kind: KeychainErrorKind,
    message: String,
}

impl KeychainError {
    fn access(message: impl Into<String>) -> Self {
        Self {
            kind: KeychainErrorKind::Access,
            message: message.into(),
        }
    }

    fn invalid_data(message: impl Into<String>) -> Self {
        Self {
            kind: KeychainErrorKind::InvalidData,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            kind: KeychainErrorKind::NotFound,
            message: message.into(),
        }
    }

    fn is_not_found(&self) -> bool {
        self.kind == KeychainErrorKind::NotFound
    }
}

#[cfg(target_os = "macos")]
fn map_security_error(context: &str, error: SecurityError) -> KeychainError {
    let code = error.code();
    if code == errSecItemNotFound {
        return KeychainError::not_found(format!("{context}: {error}"));
    }
    KeychainError::access(format!("{context}: {error}"))
}

#[cfg(target_os = "macos")]
fn decode_password(bytes: Vec<u8>, context: &str) -> Result<String, KeychainError> {
    String::from_utf8(bytes).map_err(|_| {
        KeychainError::invalid_data(format!("{context}: stored password was not valid UTF-8"))
    })
}

#[cfg(target_os = "macos")]
fn protected_password_options(service: &str, account: &str) -> PasswordOptions {
    let mut options = PasswordOptions::new_generic_password(service, account);
    options.use_protected_keychain();
    options
}

#[cfg(target_os = "macos")]
fn read_protected_password(service: &str, account: &str) -> Result<String, KeychainError> {
    let _interaction_lock = SecKeychain::disable_user_interaction()
        .map_err(|error| map_security_error("Failed to disable keychain UI", error))?;
    let password =
        generic_password(protected_password_options(service, account)).map_err(|error| {
            map_security_error("Failed to read password from protected keychain", error)
        })?;
    decode_password(password, "Failed to decode protected keychain password")
}

#[cfg(target_os = "macos")]
fn write_protected_password(
    service: &str,
    account: &str,
    password: &str,
) -> Result<(), KeychainError> {
    set_generic_password_options(
        password.as_bytes(),
        protected_password_options(service, account),
    )
    .map_err(|error| map_security_error("Failed to store password in protected keychain", error))
}

#[cfg(target_os = "macos")]
fn read_legacy_password_without_ui(service: &str, account: &str) -> Result<String, KeychainError> {
    let _interaction_lock = SecKeychain::disable_user_interaction()
        .map_err(|error| map_security_error("Failed to disable legacy keychain UI", error))?;
    let (password, _) = find_generic_password(None, service, account).map_err(|error| {
        map_security_error("Failed to read password from legacy keychain", error)
    })?;
    decode_password(
        password.to_owned(),
        "Failed to decode legacy keychain password",
    )
}

#[cfg(target_os = "macos")]
struct MacosKeychainOps {
    read_protected: fn(&str, &str) -> Result<String, KeychainError>,
    read_legacy_without_ui: fn(&str, &str) -> Result<String, KeychainError>,
    write_protected: fn(&str, &str, &str) -> Result<(), KeychainError>,
}

#[cfg(target_os = "macos")]
impl Default for MacosKeychainOps {
    fn default() -> Self {
        Self {
            read_protected: read_protected_password,
            read_legacy_without_ui: read_legacy_password_without_ui,
            write_protected: write_protected_password,
        }
    }
}

#[cfg(target_os = "macos")]
fn get_password_macos_with(
    service: &str,
    account: &str,
    ops: &MacosKeychainOps,
) -> Result<String, KeychainError> {
    match (ops.read_protected)(service, account) {
        Ok(password) => Ok(password),
        Err(error) if error.is_not_found() => {
            let password = (ops.read_legacy_without_ui)(service, account).map_err(|error| {
                if error.is_not_found() {
                    return KeychainError::not_found(format!(
                        "No password was found in either the protected or legacy keychain store for {service}/{account}"
                    ));
                }
                KeychainError::access(format!(
                    "Password for {service}/{account} is still stored in a legacy macOS keychain item that mxr will not open interactively anymore. Re-save that account password once to migrate it to the protected keychain. Original error: {error}"
                ))
            })?;
            if let Err(migration_error) = (ops.write_protected)(service, account, &password) {
                tracing::warn!(
                    service,
                    account,
                    error = %migration_error,
                    "Failed to migrate legacy keychain password to protected keychain"
                );
            } else {
                tracing::debug!(
                    service,
                    account,
                    "Migrated legacy keychain password to protected keychain"
                );
            }
            Ok(password)
        }
        Err(error) => Err(error),
    }
}

pub fn get_password(service: &str, account: &str) -> Result<String, KeychainError> {
    #[cfg(target_os = "macos")]
    {
        return get_password_macos_with(service, account, &MacosKeychainOps::default());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let entry = keyring::Entry::new(service, account)
            .map_err(|error| KeychainError::access(error.to_string()))?;
        entry
            .get_password()
            .map_err(|error| KeychainError::access(format!("Failed to retrieve password: {error}")))
    }
}

pub fn set_password(service: &str, account: &str, password: &str) -> Result<(), KeychainError> {
    #[cfg(target_os = "macos")]
    {
        return write_protected_password(service, account, password);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let entry = keyring::Entry::new(service, account)
            .map_err(|error| KeychainError::access(error.to_string()))?;
        entry
            .set_password(password)
            .map_err(|error| KeychainError::access(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use std::sync::atomic::{AtomicBool, Ordering};

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_prefers_protected_password_without_legacy_lookup() {
        let ops = MacosKeychainOps {
            read_protected: |_, _| Ok("protected".to_string()),
            read_legacy_without_ui: |_, _| {
                panic!("legacy lookup should not run when protected item exists")
            },
            write_protected: |_, _, _| panic!("migration should not run"),
        };

        assert_eq!(
            get_password_macos_with("mxr/test", "user", &ops).unwrap(),
            "protected"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_migrates_legacy_password_when_protected_item_missing() {
        static MIGRATED: AtomicBool = AtomicBool::new(false);
        MIGRATED.store(false, Ordering::SeqCst);

        let ops = MacosKeychainOps {
            read_protected: |_, _| Err(KeychainError::not_found("missing")),
            read_legacy_without_ui: |_, _| Ok("legacy".to_string()),
            write_protected: |_, _, password| {
                assert_eq!(password, "legacy");
                MIGRATED.store(true, Ordering::SeqCst);
                Ok(())
            },
        };

        assert_eq!(
            get_password_macos_with("mxr/test", "user", &ops).unwrap(),
            "legacy"
        );
        assert!(MIGRATED.load(Ordering::SeqCst));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_returns_legacy_error_without_prompting() {
        let ops = MacosKeychainOps {
            read_protected: |_, _| Err(KeychainError::not_found("missing")),
            read_legacy_without_ui: |_, _| Err(KeychainError::access("interaction denied")),
            write_protected: |_, _, _| panic!("migration should not run"),
        };

        let error = get_password_macos_with("mxr/test", "user", &ops).unwrap_err();
        assert_eq!(error.to_string(), "interaction denied");
    }
}
