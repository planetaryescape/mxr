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
    /// Why the last turn failed, in a sentence for the user with what to do
    /// next. Older daemons send the raw error here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// The raw error behind `last_error`, when the daemon reworded it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error_detail: Option<String>,
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
        /// Whether the model that wrote this reply runs off this machine,
        /// as it was when it wrote it. Absent for replies from before the
        /// daemon recorded it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        leaves_machine: Option<bool>,
        /// The steps right before this reply that failed and weren't redone
        /// after: a small model can say it did what these didn't, so
        /// clients name them under the reply.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        failed_call_ids: Vec<String>,
        /// When the reply links two or more emails or people that one step
        /// listed, those rows, in the order the reply names them (at most
        /// 8), for clients to draw as a list under the sentence that
        /// introduces them. Empty otherwise.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        rows: Vec<AssistantReadoutRowData>,
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
        /// The step in the daemon's words, for the wire line. Absent from
        /// older daemons: clients then fall back to `reason`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<AssistantStepTitleData>,
        created_at: i64,
    },
}

/// A step in plain words, in the two tenses a client shows. Built by the
/// daemon from the tool and its arguments, never from the model's reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantStepTitleData {
    /// While it runs: "Searching your mail for "shipped"".
    pub running: String,
    /// Once it ended, whichever way it ended: "Searched your mail: 14
    /// found", "Couldn't star 1 email: it needs your confirmation first",
    /// "Waiting for you: archive 14 emails".
    pub done: String,
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
        /// The change in words, "Archive 14 emails". Absent from older
        /// daemons.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// The daemon's warning for a change that can't be put back, "Sends
        /// an email. Can't be undone.". Absent for a reversible change and
        /// from older daemons, whose clients then work it out themselves.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        warning: Option<String>,
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
        /// The call in words while it runs, "Searching your mail for
        /// "shipped"". Absent from older daemons.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
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

/// One model the assistant can run on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantModelChoiceData {
    /// The model id, as `set` takes it.
    pub id: String,
    /// What a person calls it ("Claude Haiku 5.5").
    pub label: String,
    /// True when conversations with it go to a provider off this machine.
    pub cloud: bool,
    /// Whether it can be picked now.
    pub available: bool,
    /// Why it can't be picked, with the command that fixes it when there is
    /// one. Set exactly when `available` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

/// The models the assistant can be set to, and the one it uses now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantModelsData {
    /// The model the next turn uses. It may not be in `choices`: a model set
    /// by hand in config.toml stays as it is until one is picked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    pub choices: Vec<AssistantModelChoiceData>,
}

/// What `SetAssistantModel` did, or with `dry_run` would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssistantModelChangeData {
    pub dry_run: bool,
    /// False when the model was already the one in use.
    pub changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    /// The models after the change (with `dry_run`, as they would be).
    pub models: AssistantModelsData,
}

impl AssistantProgressData {
    /// The line a client shows above the composer while the turn runs, in
    /// place of "Thinking". The static phrases live here rather than on
    /// the wire; only a call's title varies, so it travels in `Calling`.
    #[must_use]
    pub fn live_line(&self) -> String {
        match self {
            Self::Thinking { step } if *step > 1 => "Deciding what to do next".to_string(),
            Self::Thinking { .. } => "Reading your request".to_string(),
            Self::Calling {
                title: Some(title), ..
            } if !title.is_empty() => title.clone(),
            Self::Calling { tool, .. } => {
                let words = tool.strip_prefix("mxr_").unwrap_or(tool).replace('_', " ");
                format!("Using {words}")
            }
            Self::Writing { .. } => "Writing the answer".to_string(),
            Self::Summarizing => "Making room: summarising earlier messages".to_string(),
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests unwrap fixtures to keep failures direct"
)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn old_json_without_the_title_fields_still_reads() {
        let call: AssistantItemData = serde_json::from_value(json!({
            "kind": "tool_call", "seq": 2, "call_id": "c1", "tool": "mxr_search",
            "reason": "r", "arguments": {}, "state": "done", "created_at": 0,
        }))
        .unwrap();
        assert!(matches!(
            call,
            AssistantItemData::ToolCall { title: None, .. }
        ));

        let reply: AssistantItemData = serde_json::from_value(json!({
            "kind": "assistant", "seq": 3, "text": "hi", "created_at": 0,
        }))
        .unwrap();
        assert!(matches!(&reply, AssistantItemData::Assistant { rows, .. } if rows.is_empty()));

        let pending: AssistantPendingData = serde_json::from_value(json!({
            "kind": "approval", "call_id": "c1", "tool": "mxr_mutate",
            "arguments": {}, "reason": "r",
        }))
        .unwrap();
        assert!(matches!(
            pending,
            AssistantPendingData::Approval { title: None, .. }
        ));

        let progress: AssistantProgressData = serde_json::from_value(json!({
            "kind": "calling", "tool": "mxr_search", "reason": "r",
        }))
        .unwrap();
        assert_eq!(progress.live_line(), "Using search");

        let conversation: AssistantConversationData = serde_json::from_value(json!({
            "id": "a", "title": "t", "status": "idle", "created_at": 0, "updated_at": 0,
            "items": [], "pending": [],
        }))
        .unwrap();
        assert_eq!(conversation.last_error_detail, None);
    }

    #[test]
    fn a_step_rows_request_and_response_round_trip() {
        let request = crate::Request::GetAssistantStepRows {
            conversation_id: "c1".into(),
            call_id: "call1".into(),
            seq: 7,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["cmd"], "GetAssistantStepRows");
        let back: crate::Request = serde_json::from_value(json).unwrap();
        assert!(matches!(
            back,
            crate::Request::GetAssistantStepRows { conversation_id, call_id, seq: 7 }
                if conversation_id == "c1" && call_id == "call1"
        ));

        let response = crate::ResponseData::AssistantStepRows {
            rows: vec![AssistantReadoutRowData {
                title: "Quarterly report".into(),
                detail: None,
                link: None,
            }],
            listed: 1,
            total: Some(14),
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["kind"], "AssistantStepRows");
        let back: crate::ResponseData = serde_json::from_value(json).unwrap();
        assert!(matches!(
            back,
            crate::ResponseData::AssistantStepRows { rows, listed: 1, total: Some(14) }
                if rows.len() == 1
        ));
    }

    #[test]
    fn an_older_daemon_fails_a_step_rows_request_it_does_not_know() {
        // An old daemon has no such command, so the web must read the error
        // and keep the step's plain text.
        let unknown = serde_json::from_value::<crate::Request>(json!({
            "cmd": "GetAssistantStepRowsV2", "conversation_id": "c1", "call_id": "call1", "seq": 7,
        }));
        assert!(unknown.is_err());
    }

    #[test]
    fn new_fields_stay_off_the_wire_when_empty() {
        let reply = AssistantItemData::Assistant {
            seq: 1,
            text: "hi".into(),
            model: None,
            leaves_machine: None,
            failed_call_ids: Vec::new(),
            rows: Vec::new(),
            created_at: 0,
        };
        let wire = serde_json::to_value(reply).unwrap();
        assert!(wire.get("rows").is_none());
        assert!(wire.get("leaves_machine").is_none());
    }

    #[test]
    fn the_live_line_names_what_the_turn_is_doing() {
        let line = |progress: AssistantProgressData| progress.live_line();
        assert_eq!(
            line(AssistantProgressData::Thinking { step: 1 }),
            "Reading your request"
        );
        assert_eq!(
            line(AssistantProgressData::Thinking { step: 2 }),
            "Deciding what to do next"
        );
        assert_eq!(
            line(AssistantProgressData::Calling {
                tool: "mxr_search".into(),
                reason: String::new(),
                title: Some("Searching your mail for \"x\"".into()),
            }),
            "Searching your mail for \"x\""
        );
        assert_eq!(
            line(AssistantProgressData::Writing {
                text: String::new()
            }),
            "Writing the answer"
        );
        assert_eq!(
            line(AssistantProgressData::Summarizing),
            "Making room: summarising earlier messages"
        );
    }
}
