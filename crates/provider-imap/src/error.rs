#[derive(Debug, thiserror::Error)]
pub enum ImapProviderError {
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Keyring error: {0}")]
    Keyring(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Invalid provider ID: {0}")]
    InvalidProviderId(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Fetch error: {0}")]
    Fetch(String),
    #[error("UIDVALIDITY changed (was {old}, now {new}) — requires full resync")]
    UidValidityChanged { old: u32, new: u32 },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<ImapProviderError> for crate::mxr_core::error::MxrError {
    fn from(e: ImapProviderError) -> Self {
        crate::mxr_core::error::MxrError::Provider(e.to_string())
    }
}
