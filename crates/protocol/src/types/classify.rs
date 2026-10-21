//! `mxr classify eval`: today's classifier measured against labelled
//! scenarios (blueprint 32). Local only; reads stored mail and changes
//! nothing.

use serde::{Deserialize, Serialize};

/// Which labelled set to measure. `Full` and `Mine` arrive with the full
/// generator and with corrections (blueprint 32, build order 1 and 5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ClassifyEvalSetData {
    #[default]
    Quick,
}

/// Accuracy on one question about an email.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalQuestionData {
    /// `who`, `home`, `todo` or `record`.
    pub question: String,
    /// Labelled emails present in this mailbox and answerable (an email
    /// whose home cannot be worked out is not asked about its home).
    pub evaluated: u32,
    pub correct: u32,
    /// `correct / evaluated` as a percentage with one decimal; 0 when
    /// nothing was evaluated.
    pub accuracy_percent: f64,
}

/// One wrong answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalMismatchData {
    pub scenario: String,
    pub provider_id: String,
    pub question: String,
    pub expected: String,
    /// The answer today's code gave.
    pub actual: String,
    /// Why today's code answered so: the deciding rule's reason. Empty
    /// for to-do and record answers.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// The scenario this one is a near-miss twin of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub twin_of: Option<String>,
    /// Why the label itself is a judgement call, when it is: a mismatch
    /// here may be the label's fault, not the classifier's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_review: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClassifyEvalSummaryData {
    pub set: ClassifyEvalSetData,
    /// Version of the questions the expectations answer, so a change to
    /// them never silently invalidates old labels.
    pub label_schema_version: u32,
    /// Labelled emails in the set.
    pub labelled_in_set: u32,
    /// Of those, the ones found in this mailbox.
    pub present: u32,
    /// Present emails with no arrivals row (history older than 30 days when
    /// the account first synced), whose home was worked out live by the same
    /// rules.
    pub placed_live: u32,
    /// Present emails whose home could not be worked out at all, so there is
    /// none to compare.
    pub not_placed: u32,
    /// Stored emails in scope that no scenario labels.
    pub unlabelled: u32,
    pub questions: Vec<ClassifyEvalQuestionData>,
    /// What `home` reads: the arrivals line's current-policy destination.
    pub home_projection: String,
    /// A plain sentence when there is nothing to measure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
