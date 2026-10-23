//! One stored answer about an inbound email. Live attention state belongs to projections.
use crate::{AccountId, MessageId};
use serde::{Deserialize, Serialize};

pub const CLASSIFIER_POLICY_VERSION: &str = "r2-1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssessmentClaim {
    pub rule: String,
    pub reason: String,
    pub source: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MailAssessment {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub policy_version: String,
    pub home: String,
    pub facts: AssessmentFacts,
    pub who: String,
    pub deciding: AssessmentClaim,
    pub overridden: Vec<AssessmentClaim>,
    pub not_sure: Option<String>,
    pub fallback: bool,
    pub pending_body: bool,
    /// Digest of the canonical observed inputs, never mail content.
    pub input_fingerprint: String,
}

/// Canonical observations useful for finding missing classifier distinctions.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssessmentFacts {
    pub sender_shape: String,
    pub has_list_id: bool,
    pub has_unsubscribe: bool,
    pub auto_generated: bool,
    pub bulk: bool,
    pub templated_sender: bool,
    pub written_to: bool,
    pub body_cached: bool,
    pub recipients: usize,
    pub addressed: bool,
}
