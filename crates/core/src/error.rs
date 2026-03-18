use thiserror::Error;

#[derive(Debug, Error)]
pub enum MxrError {
    #[error("Store error: {0}")]
    Store(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Sync error: {0}")]
    Sync(String),

    #[error("Search error: {0}")]
    Search(String),

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
