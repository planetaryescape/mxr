//! Promises you make in mail you send ("I'll send the deck by Friday"):
//! found by `DetectPromises`, kept as a dated commitment by `RecordPromise`.
//! Detection only reads; nothing is stored until the user accepts.

use super::AiProvenanceData;
use mxr_core::id::*;
use mxr_core::natural_time::TimeResolution;
use mxr_core::types::Draft;
use serde::{Deserialize, Serialize};

/// Where `DetectPromises` looks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PromiseSourceData {
    /// An outgoing draft, checked while it sends (the web undo window).
    Draft { draft: Box<Draft> },
    /// A message you already sent (TUI and CLI check after the send).
    SentMessage { message_id: MessageId },
}

/// Returned by `Request::DetectPromises`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PromiseDetectionData {
    pub status: PromiseDetectionStatusData,
    /// Promises the sender makes, in the order they appear.
    pub promises: Vec<DetectedPromiseData>,
    /// Set when a model was asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<AiProvenanceData>,
    /// Why there is no answer, for statuses other than `ready`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// A detection that can't run is a normal answer: sending never waits on it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PromiseDetectionStatusData {
    /// Checked. `promises` may be empty, including when the text has no
    /// "I'll" / "I will" at all and no model was asked.
    Ready,
    /// No language model is configured.
    Disabled,
    /// The privacy policy keeps this text from the configured model.
    Blocked,
    /// The model failed or answered with something unusable.
    Failed,
    /// The model took longer than the detection budget.
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DetectedPromiseData {
    /// The deliverable as a short clause, such as "send the deck".
    pub what: String,
    /// When it is due, copied from the message ("by Friday"). Checked
    /// against the text, so it is never the model's invention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_phrase: Option<String>,
    /// `due_phrase` resolved by the natural-time parser in the caller's
    /// zone. None when the phrase names no date the parser understands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<TimeResolution>,
}
