//! The in-app assistant: a conversation with an agent that works on the
//! mailbox through mxr's own tools. Changes it wants to make, and questions
//! it asks, wait for the user's decision.

use serde::{Deserialize, Serialize};

/// Where a conversation stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AssistantStatusData {
    Idle,
    Running,
    /// Calls wait for the user: see `pending`.
    AwaitingUser,
}

/// How the last turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AssistantOutcomeData {
    Finished,
    Cancelled,
    /// Stopped after the step limit; "go on" continues.
    StepLimit,
    Failed,
    /// The daemon stopped mid-turn.
    Interrupted,
}

/// What the user has open when they send a message, so "this" and
/// "these" mean something.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantViewContextData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_message_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantConversationSummaryData {
    pub id: String,
    pub title: String,
    pub status: AssistantStatusData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_outcome: Option<AssistantOutcomeData>,
    /// Unix seconds.
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: i64,
    pub pending_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantConversationData {
    pub id: String,
    pub title: String,
    pub status: AssistantStatusData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_outcome: Option<AssistantOutcomeData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// Reversible changes (archive, move, star, read, labels) run without
    /// asking. Sending, deleting, trash, spam and rules always ask. Set
    /// only by the user (`SetAssistantAutoApprove`).
    #[serde(default)]
    pub auto_approve: bool,
    pub items: Vec<AssistantItemData>,
    pub pending: Vec<AssistantPendingData>,
}

/// One entry in the conversation as the user sees it. A tool call and its
/// result are one item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssistantItemData {
    User {
        seq: i64,
        text: String,
        created_at: i64,
    },
    Assistant {
        seq: i64,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        created_at: i64,
    },
    /// Earlier messages were summarised to fit the model's context window.
    /// Everything before this point reaches the model only as `text`.
    Summary {
        seq: i64,
        text: String,
        created_at: i64,
    },
    ToolCall {
        seq: i64,
        call_id: String,
        tool: String,
        /// The model's one-line reason, for the user.
        reason: String,
        arguments: serde_json::Value,
        state: AssistantCallStateData,
        /// The start of the result as mxr returned it, for a details view.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<String>,
        /// What the result found, in words, read from the whole result.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        readout: Option<AssistantReadoutData>,
        created_at: i64,
    },
}

/// A tool result for people: what it lists (the first few, with where to
/// open each) or, when it lists nothing, its plain fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantReadoutData {
    /// How many things the result lists in all, when it lists any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<AssistantReadoutRowData>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<AssistantReadoutFactData>,
}

/// One thing a result lists: an email, a person, a to-do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantReadoutRowData {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<AssistantLinkData>,
}

/// A plain field of a result, such as "Archived: 3".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantReadoutFactData {
    pub label: String,
    pub value: String,
}

/// Where a client opens something the assistant found. Replies link the
/// same places as markdown links: `mxr:thread/<id>` and `mxr:person/<id>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssistantLinkData {
    /// An email conversation.
    Thread { thread_id: String },
    /// Someone's row in Messages.
    Person { person_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AssistantCallStateData {
    /// Running now.
    Running,
    /// Ran and returned.
    Done,
    /// Ran and failed.
    Failed,
    /// Waiting for the user.
    Waiting,
    /// The user approved it and it ran.
    Approved,
    /// It ran without asking: a reversible change in a conversation the
    /// user set to approve those automatically.
    AutoApproved,
    /// The user declined, or wrote instead of answering.
    Declined,
    /// The user answered the question.
    Answered,
    /// The user stopped the turn before it ran.
    Cancelled,
    /// No result was recorded (the daemon stopped while it ran).
    Interrupted,
}

/// A call waiting for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssistantPendingData {
    /// A change. Approving runs exactly `arguments`.
    Approval {
        call_id: String,
        tool: String,
        arguments: serde_json::Value,
        reason: String,
        /// What the change will do, from the tool's own preview.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preview: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preview_error: Option<String>,
    },
    Question {
        call_id: String,
        reason: String,
        questions: Vec<AssistantQuestionData>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantQuestionData {
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default)]
    pub options: Vec<AssistantQuestionOptionData>,
    #[serde(default)]
    pub multi_select: bool,
    #[serde(default)]
    pub allow_other: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantQuestionOptionData {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The user's answer to one waiting call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantDecisionData {
    pub call_id: String,
    #[serde(flatten)]
    pub decision: AssistantDecisionKindData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum AssistantDecisionKindData {
    Approve,
    Reject {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// One answer per question, in order.
    Answer {
        answers: Vec<AssistantAnswerData>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantAnswerData {
    #[serde(default)]
    pub selected: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

/// What a running turn is doing now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssistantProgressData {
    Thinking {
        step: u32,
    },
    Calling {
        tool: String,
        reason: String,
    },
    /// The reply so far, while the model writes it. The whole text each
    /// time, so a missed event loses nothing.
    Writing {
        text: String,
    },
    /// Summarising earlier messages to fit the context window.
    Summarizing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantDeleteData {
    pub conversation_id: String,
    pub title: String,
    pub message_count: i64,
    pub dry_run: bool,
    pub deleted: bool,
}

/// Whether the assistant can run, and on what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantAvailabilityData {
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// True when conversations go to a model off this machine.
    pub leaves_machine: bool,
    /// Why it can't run, in words for the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
