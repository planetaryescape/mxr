use super::DraftProvenanceData;
use mxr_core::id::*;
use mxr_core::types::Address;
use serde::{Deserialize, Serialize};

/// The draft a writing request is about. A stored draft is read from the
/// database; the other targets name the thing that does not exist yet, so a
/// client can ask for help before the first save.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DraftTargetData {
    Stored {
        draft_id: DraftId,
    },
    Reply {
        message_id: MessageId,
        all: bool,
    },
    Forward {
        message_id: MessageId,
        to: Vec<Address>,
    },
    New {
        account_id: AccountId,
        to: Vec<Address>,
        subject: String,
    },
}

/// What the writing help does with the text. A request without one lets the
/// daemon choose from what is typed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AssistActionData {
    /// Write the whole message; nothing is typed yet.
    Write,
    /// Keep what is typed and add what the message still needs.
    Finish,
    /// Rework the typed text without adding to it.
    Polish,
    /// Change only the selected part.
    EditSelection,
}

/// One-line adjustments the user ticked, each a single instruction to the model.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistAdjustData {
    #[serde(default)]
    pub shorter: bool,
    #[serde(default)]
    pub warmer: bool,
    #[serde(default)]
    pub more_formal: bool,
    #[serde(default)]
    pub proofread: bool,
}

/// A byte range into the request's `text`; both ends fall on char boundaries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TextRangeData {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TextChangeKindData {
    Kept,
    Added,
    Removed,
}

/// One run of a word-level diff from the user's text to the proposal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TextChangeData {
    pub kind: TextChangeKindData,
    pub text: String,
}

/// A place the model could not fill, written `[[?: label]]` in the text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GapData {
    pub label: String,
    /// Byte range of the whole marker in the proposal text.
    pub start: u32,
    pub end: u32,
}

/// What the writing help proposes. Nothing changes the draft until the user
/// accepts it with `ApplyComposeProposal`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ComposeProposalData {
    pub proposal_id: String,
    pub action: AssistActionData,
    /// Hex SHA-256 of the request text with trailing blank space ignored: the
    /// text region the proposal was written against. Applying refuses when the
    /// draft's text region no longer hashes to it.
    pub base: String,
    pub text: String,
    pub changes: Vec<TextChangeData>,
    pub gaps: Vec<GapData>,
    /// Always filled: the model that answered, where it ran and what it read.
    pub provenance: DraftProvenanceData,
    /// Positions, from zero, of the user's sentences that the proposal changed
    /// or dropped. Sentences are the request text split at `.`, `!`, `?` and
    /// line breaks, with empty ones skipped; case and spacing do not count as a
    /// change.
    pub yours_changed: Vec<u32>,
    /// Version of the rules that checked this proposal.
    pub composer_version: String,
}
