use crate::error::ImapProviderError;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Keyring reference (e.g., "mxr/fastmail-imap"). Looked up at runtime.
    pub password_ref: String,
    #[serde(default = "default_true")]
    pub use_tls: bool,
}

fn default_true() -> bool {
    true
}

impl ImapConfig {
    /// Retrieve the IMAP password from the system keyring.
    pub fn resolve_password(&self) -> Result<String, ImapProviderError> {
        let entry = keyring::Entry::new(&self.password_ref, &self.username)
            .map_err(|e| ImapProviderError::Keyring(e.to_string()))?;
        entry
            .get_password()
            .map_err(|e| ImapProviderError::Keyring(format!("Failed to retrieve password: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imap_config_parses() {
        let json = r#"{
            "host": "imap.fastmail.com",
            "port": 993,
            "username": "user@fastmail.com",
            "password_ref": "mxr/fastmail-imap"
        }"#;
        let config: ImapConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.host, "imap.fastmail.com");
        assert_eq!(config.port, 993);
        assert!(config.use_tls);
    }
}
