//! Remote authority records, independent of transport and mail providers.
use serde::{Deserialize, Serialize};
use std::fmt;

/// Credential or verifier on the wire. Debug output never reveals its contents.
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct RemoteSecret(String);
impl RemoteSecret {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for RemoteSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HostAuthority {
    /// Absent until the first pairing starts. Inspection never initializes authority.
    pub host_id: Option<String>,
    pub workspace_id: Option<String>,
    pub recovery_generation: Option<String>,
    pub hosting_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PendingPairing {
    pub capabilities: Vec<RemoteCapability>,
    pub request_id: String,
    pub name: String,
    /// Both screens must display this exact verifier-bound challenge.
    pub challenge: String,
    pub expires_at: i64,
    pub approved: bool,
    pub failed_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RegisteredClient {
    pub capabilities: Vec<RemoteCapability>,
    pub client_id: String,
    pub name: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RemoteCapability {
    MailRead,
}
