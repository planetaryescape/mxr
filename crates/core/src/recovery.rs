use crate::{AccountId, Draft, DraftId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecoveredDraftData {
    pub draft_id: DraftId,
    pub account_id: AccountId,
    pub revision: i64,
    pub subject: String,
    pub prepared_draft_id: Option<DraftId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecoveryStatusData {
    pub workspace_id: String,
    pub recovery_generation: String,
    pub restore_pending: bool,
    pub source_owner_stopped: bool,
    pub retired_intentions: u64,
    pub retained_intentions: Vec<RetainedIntentData>,
    pub held_drafts: Vec<RecoveredDraftData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RetainedIntentData {
    pub original_id: String,
    pub kind: String,
    pub account_id: Option<AccountId>,
    pub original_state: String,
    pub outcome_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_evidence: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RecoveryActionData {
    pub status: RecoveryStatusData,
    pub prepared_draft: Option<Draft>,
    pub dry_run: bool,
}
