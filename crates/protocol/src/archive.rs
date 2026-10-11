//! Durable, reviewed Archive identity and per-message outcomes.
use mxr_core::{AccountId, MessageId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArchiveWorkspaceData {
    pub workspace_id: String,
    pub recovery_generation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArchiveIdentityData {
    pub operation_id: String,
    pub caller_id: String,
    pub workspace: ArchiveWorkspaceData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ArchiveItemStateData {
    /// No dispatch intent was committed. Same-identity submission can resume.
    Pending,
    /// Dispatch intent is durable; acceptance is unproved. Never resubmit.
    Unknown,
    /// Provider completion is durable; local reconciliation is incomplete.
    ProviderSucceeded,
    Completed,
    /// Rejected before provider dispatch, or explicitly rejected by the adapter.
    NotExecuted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArchiveItemData {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub provider_id: String,
    pub was_inbox: bool,
    pub prior_flags_bits: u32,
    pub prior_label_provider_ids: Vec<String>,
    pub state: ArchiveItemStateData,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArchiveOperationData {
    pub identity: ArchiveIdentityData,
    pub review_id: String,
    pub mutation_id: String,
    pub accepted: bool,
    #[serde(default)]
    pub undo_available: bool,
    #[serde(default)]
    pub running: bool,
    pub started_at: i64,
    pub items: Vec<ArchiveItemData>,
}

impl ArchiveOperationData {
    pub fn settled(&self) -> bool {
        self.accepted
            && !self.running
            && self
                .items
                .iter()
                .all(|item| item.state != ArchiveItemStateData::Pending)
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests unwrap fixture setup")]
    use super::*;
    #[test]
    fn archive_identity_and_review_require_every_boundary_field() {
        let identity = serde_json::json!({"operation_id":"op", "caller_id":"caller", "workspace":{"workspace_id":"workspace", "recovery_generation":"generation"}});
        for field in ["operation_id", "caller_id", "workspace"] {
            let mut invalid = identity.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<ArchiveIdentityData>(invalid).is_err());
        }
        for mutation in ["Archive", "ReadAndArchive", "Trash"] {
            let request = serde_json::json!({"cmd":"SubmitArchive", "identity":identity, "mutation":{"mutation":mutation,"message_ids":[]}});
            assert!(
                serde_json::from_value::<crate::Request>(request).is_err(),
                "a missing review token must fail"
            );
        }
    }
}
