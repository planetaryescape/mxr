#![cfg_attr(test, allow(clippy::unwrap_used))]

#[cfg(target_os = "macos")]
use security_framework::base::Error as SecurityError;
#[cfg(target_os = "macos")]
use security_framework::os::macos::keychain::SecKeychain;
#[cfg(target_os = "macos")]
use security_framework::passwords::{
    delete_generic_password, generic_password, set_generic_password_options, PasswordOptions,
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
fn keychain_password_options(service: &str, account: &str) -> PasswordOptions {
    PasswordOptions::new_generic_password(service, account)
}

#[cfg(target_os = "macos")]
fn read_password_without_ui(service: &str, account: &str) -> Result<String, KeychainError> {
    let _interaction_lock = SecKeychain::disable_user_interaction()
        .map_err(|error| map_security_error("Failed to disable keychain UI", error))?;
    let password = generic_password(keychain_password_options(service, account))
        .map_err(|error| map_security_error("Failed to read password from keychain", error))?;
    decode_password(password, "Failed to decode keychain password")
}

#[cfg(target_os = "macos")]
fn write_password(service: &str, account: &str, password: &str) -> Result<(), KeychainError> {
    match delete_generic_password(service, account) {
        Ok(()) => {}
        Err(error) if error.code() == errSecItemNotFound => {}
        Err(error) => {
            return Err(map_security_error(
                "Failed to replace existing password in keychain",
                error,
            ));
        }
    }
    set_generic_password_options(
        password.as_bytes(),
        keychain_password_options(service, account),
    )
    .map_err(|error| map_security_error("Failed to store password in keychain", error))
}

#[cfg(target_os = "macos")]
struct MacosKeychainOps {
    read_without_ui: fn(&str, &str) -> Result<String, KeychainError>,
}

#[cfg(target_os = "macos")]
impl Default for MacosKeychainOps {
    fn default() -> Self {
        Self {
            read_without_ui: read_password_without_ui,
        }
    }
}

#[cfg(target_os = "macos")]
fn get_password_macos_with(
    service: &str,
    account: &str,
    ops: &MacosKeychainOps,
) -> Result<String, KeychainError> {
    match (ops.read_without_ui)(service, account) {
        Ok(password) => Ok(password),
        Err(error) if error.is_not_found() => Err(KeychainError::not_found(format!(
            "No password was found in the macOS keychain for {service}/{account}"
        ))),
        Err(error) => Err(KeychainError::access(format!(
            "Password for {service}/{account} requires interactive macOS keychain approval. Re-save that account password once with `mxr accounts repair` so mxr can read it non-interactively. Original error: {error}"
        ))),
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
        return write_password(service, account, password);
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
    #[test]
    fn macos_reads_password_without_extra_write() {
        let ops = MacosKeychainOps {
            read_without_ui: |_, _| Ok("stored".to_string()),
        };

        assert_eq!(
            get_password_macos_with("mxr/test", "user", &ops).unwrap(),
            "stored"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_reports_interactive_item_as_repairable() {
        let ops = MacosKeychainOps {
            read_without_ui: |_, _| Err(KeychainError::access("interaction denied")),
        };

        let error = get_password_macos_with("mxr/test", "user", &ops).unwrap_err();
        assert!(error
            .to_string()
            .contains("Re-save that account password once with `mxr accounts repair`"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_reports_not_found_cleanly() {
        let ops = MacosKeychainOps {
            read_without_ui: |_, _| Err(KeychainError::not_found("missing")),
        };

        let error = get_password_macos_with("mxr/test", "user", &ops).unwrap_err();
        assert_eq!(
            error.to_string(),
            "No password was found in the macOS keychain for mxr/test/user"
        );
    }
}
