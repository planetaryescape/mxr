#![cfg_attr(test, allow(clippy::unwrap_used))]

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Keyring reference (e.g., "mxr/work-smtp"). Looked up at runtime.
    pub password_ref: String,
    #[serde(default = "default_true")]
    pub auth_required: bool,
    #[serde(default = "default_true")]
    pub use_tls: bool,
}

fn default_true() -> bool {
    true
}

impl SmtpConfig {
    /// Retrieve the SMTP password from the system keyring.
    pub fn resolve_password(&self) -> Result<String, SmtpError> {
        let entry = keyring::Entry::new(&self.password_ref, &self.username)
            .map_err(|e| SmtpError::Keyring(e.to_string()))?;
        entry
            .get_password()
            .map_err(|e| SmtpError::Keyring(format!("Failed to retrieve password: {e}")))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SmtpError {
    #[error("Keyring error: {0}")]
    Keyring(String),
    #[error("SMTP transport error: {0}")]
    Transport(String),
    #[error("Message build error: {0}")]
    MessageBuild(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_use_tls_is_true() {
        let json =
            r#"{"host":"smtp.example.com","port":587,"username":"user","password_ref":"mxr/test"}"#;
        let config: SmtpConfig = serde_json::from_str(json).unwrap();
        assert!(config.auth_required);
        assert!(config.use_tls);
    }
}
