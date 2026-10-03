//! Messages: people you talk with, one row each, with your conversations
//! inside as topics (blueprint 22, phase 3). A group thread is its own row
//! keyed by its thread, never by who is on it. The daemon builds the bands,
//! the topics and each message's new text, so the CLI, TUI, web and agents
//! show the same people, the same order and the same words.

use mxr_core::id::*;
use mxr_core::types::Address;
use serde::{Deserialize, Serialize};

/// The words Messages teaches itself with (D118, the #284 copy).
pub mod messages_copy {
    pub const HEADER: &str = "People you talk with, one row each. Reply or mark done.";
    pub const NEVER_HAD_ANY: &str = "When someone writes to you and you've written to them, they show up here, one row per person, with what they asked you.";
    /// Followed by up to three people whose usual pace lapsed, as facts.
    pub const CLEAR: &str = "Nobody is waiting on you.";
    pub const CARD: &str = "Each row is a person, not an email, with your conversations inside as topics. The quoted line is what they asked; the row leaves Your turn when you reply or press `.` for got it.";
    pub const LANDS_HERE: &str = "People you write to and who write to you, one row each; a group thread is its own row, and a thread you were only copied on goes to Updates.";
    pub const FIRST_RUN_LINE: &str = "People you talk with";
    /// How many seconds Got it waits, showing its text, before it sends.
    pub const ACK_COUNTDOWN_SECONDS: u32 = 5;
}

/// The bands, top to bottom.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MessagesBandData {
    /// Their latest message to you is unanswered.
    YourTurn,
    /// People you pinned, up to nine.
    Pinned,
    /// Everyone else in Messages, by recency.
    Recent,
    /// Done here, or a turn that went quiet; back when they write again.
    Quiet,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MessagesRowKindData {
    /// One person, merged across their addresses, with their one-to-one
    /// conversations as topics.
    Person,
    /// One group thread: two or more other people took part.
    Group,
}

/// How close you are, from your history with them: reciprocity, recency
/// and longevity (Whittaker, Jones and Terveen, CSCW 2002).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ClosenessData {
    Close,
    Regular,
    Occasional,
    /// You've never written to them.
    New,
}

impl ClosenessData {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::Regular => "regular",
            Self::Occasional => "occasional",
            Self::New => "new",
        }
    }
}

/// A thread's shape, judged in the daemon.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ThreadShapeData {
    /// Exactly one other person took part.
    OneToOne,
    /// Two or more other people took part.
    Group,
    /// You were only copied, or it went to a crowd, and you never wrote
    /// in it: Updates, not Messages.
    Copied,
}

/// Whose turn it is on one topic.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TopicStateData {
    YourTurn,
    /// You wrote last and they haven't answered yet.
    Waiting,
    /// Nobody's turn.
    Quiet,
    /// Done here, until they write again.
    Done,
}

impl TopicStateData {
    pub const fn label(self) -> &'static str {
        match self {
            Self::YourTurn => "your turn",
            Self::Waiting => "waiting",
            Self::Quiet => "quiet",
            Self::Done => "done here",
        }
    }
}

/// Someone on a row or a message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PersonRefData {
    /// The person's id: their primary address, lowercased.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Every address that is this person, primary first.
    pub addresses: Vec<String>,
}

impl PersonRefData {
    /// The name, else the address.
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    /// The first name, for a group's title and a greeting.
    pub fn first_name(&self) -> String {
        first_name(self.name.as_deref(), &self.id)
    }
}

/// "Samir" from "Samir Patel", or "samir" from "samir@launchpad.example".
pub fn first_name(name: Option<&str>, email: &str) -> String {
    name.and_then(|name| name.split_whitespace().next())
        .map(str::to_string)
        .unwrap_or_else(|| email.split('@').next().unwrap_or(email).to_string())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MessagesPreviewKindData {
    /// The ask from the conversation's gist, quoted verbatim.
    Ask,
    /// What they wrote last, quotes and signature removed.
    Latest,
    /// You wrote last: "You: ...".
    You,
}

/// The line under a row's name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MessagesPreviewData {
    pub kind: MessagesPreviewKindData,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    /// The model whose gist found the ask, for an `ask`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// One conversation inside a row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MessagesTopicData {
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    /// The subject without "Re:" and "Fwd:".
    pub subject: String,
    pub shape: ThreadShapeData,
    pub state: TopicStateData,
    pub last_at: chrono::DateTime<chrono::Utc>,
    pub message_count: u32,
    /// For a group topic on a person page: the other people in it, by first
    /// name ("with Ruth").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub with: Vec<String>,
    /// Every message id in the thread: what done here covers.
    pub message_ids: Vec<MessageId>,
    /// The message a reply answers: their latest, else the latest.
    pub reply_to_message_id: MessageId,
}

/// One row in a band.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MessagesRowData {
    /// `person:<email>` or `group:<thread_id>`: what `GetPerson` takes.
    pub id: String,
    pub kind: MessagesRowKindData,
    pub account_id: AccountId,
    pub band: MessagesBandData,
    /// "Samir Patel", or a group's first names: "Samir, Ruth".
    pub title: String,
    /// The person, for a person row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<PersonRefData>,
    /// The other people, for a group row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<PersonRefData>,
    pub closeness: ClosenessData,
    /// Their latest message to you is unanswered on some topic.
    pub your_turn: bool,
    /// The latest activity on any topic.
    pub last_at: chrono::DateTime<chrono::Utc>,
    /// When your turn started, for a Your turn row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_since: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<MessagesPreviewData>,
    /// Most pressing first: your turn, then waiting, then by recency.
    pub topics: Vec<MessagesTopicData>,
    /// How fast you usually reply to them, in seconds, from past replies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usual_reply_seconds: Option<i64>,
    /// "usually 47m", when there is enough history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pace_label: Option<String>,
    /// Your turn has run past your usual pace with them.
    pub overdue: bool,
    pub pinned: bool,
    pub unread: bool,
    /// "Here because: Samir asked you a question, and you write to him
    /// often (rule)."
    pub why: String,
}

/// Someone whose usual pace lapsed, for the clear state: a fact, never a
/// nudge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MessagesLapsedData {
    pub account_id: AccountId,
    pub person: PersonRefData,
    /// "Ari usually writes every week. Last: 19 days ago."
    pub line: String,
}

/// Returned by `Request::ListMessages`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MessagesData {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    /// `messages_copy::HEADER`.
    pub header: String,
    pub your_turn: Vec<MessagesRowData>,
    pub pinned: Vec<MessagesRowData>,
    pub recent: Vec<MessagesRowData>,
    pub quiet: Vec<MessagesRowData>,
    /// Rows in each band before the list limit.
    pub recent_total: u32,
    pub quiet_total: u32,
    /// People and groups in Messages, every band.
    pub row_count: u32,
    /// Conversations behind them.
    pub thread_count: u32,
    /// Set when Your turn is empty: the clear line or the never-had-any
    /// line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_state: Option<String>,
    /// Up to three people whose usual pace lapsed, under the clear line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lapsed: Vec<MessagesLapsedData>,
    /// Merges mxr could suggest, for the hint on a person page.
    pub merge_suggestion_count: u32,
}

/// Only rows where it is this side's turn.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MessagesTurnData {
    /// Your turn.
    Mine,
    /// Waiting on them.
    Theirs,
}

/// Short notes read like chat; longer ones are letters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MessageLayoutData {
    /// Three lines or fewer: shown compactly, theirs left, yours right.
    Compact,
    /// Longer: a full-width letter block that expands. Never a bubble.
    Letter,
}

/// What `new text` removed from a message.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TrimmedData {
    pub quote: bool,
    pub signature: bool,
}

impl TrimmedData {
    pub const fn any(self) -> bool {
        self.quote || self.signature
    }

    /// "trimmed: quote, sig", or `None` when nothing was removed.
    pub fn label(self) -> Option<String> {
        let parts: Vec<&str> = [(self.quote, "quote"), (self.signature, "sig")]
            .into_iter()
            .filter_map(|(on, word)| on.then_some(word))
            .collect();
        (!parts.is_empty()).then(|| format!("trimmed: {}", parts.join(", ")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConversationAttachmentData {
    pub attachment_id: AttachmentId,
    pub filename: String,
    pub size_bytes: u64,
}

/// One message in a conversation, as its new text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConversationMessageData {
    pub message_id: MessageId,
    pub from: Address,
    pub from_me: bool,
    pub date: chrono::DateTime<chrono::Utc>,
    /// What this message says that the thread didn't already: quotes and
    /// signature removed, the same text in every client.
    pub text: String,
    pub trimmed: TrimmedData,
    /// "trimmed: quote, sig", when something was removed. `o` (or `v`)
    /// shows the message as sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trimmed_label: Option<String>,
    pub layout: MessageLayoutData,
    pub paragraphs: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<ConversationAttachmentData>,
    /// The ask, verbatim, when this message makes it: clients highlight it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_quote: Option<String>,
}

/// The box at the bottom, already addressed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ComposerData {
    /// "Reply to Samir · Contract renewal".
    pub label: String,
    pub reply_to_message_id: MessageId,
    /// Reply all is on by default in a group, off on a one-to-one topic.
    pub reply_all: bool,
}

/// The selected topic, as a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConversationData {
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub shape: ThreadShapeData,
    pub state: TopicStateData,
    /// Oldest first.
    pub messages: Vec<ConversationMessageData>,
    /// Earlier messages left out of a long thread.
    pub earlier_count: u32,
    pub composer: ComposerData,
}

/// A merge mxr may suggest: the same name on addresses you've written to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MergeSuggestionData {
    pub account_id: AccountId,
    pub name: String,
    /// Every address with the name, the one you write to most first: the
    /// merge would make it the person's primary address.
    pub addresses: Vec<String>,
    /// "Same name, and you've written to both."
    pub reason: String,
}

/// Returned by `Request::GetPerson`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PersonPageData {
    pub row: MessagesRowData,
    /// "You've written 48 times since 2023. Last: Tuesday."
    pub relationship_line: String,
    /// "close · usually 47m".
    pub header_line: String,
    /// Every topic with them: one-to-one, then groups ("with Ruth").
    pub topics: Vec<MessagesTopicData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationData>,
    /// Merges suggested for this person.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merge_suggestions: Vec<MergeSuggestionData>,
}

/// Returned by `Request::AckMessage`: the exact acknowledgement Got it
/// sends, built from how you write to them (no model).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AckPlanData {
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub reply_to_message_id: MessageId,
    pub to: Vec<Address>,
    pub subject: String,
    /// The exact text that is sent.
    pub text: String,
    /// "Your usual greeting and sign-off with Samir." or "No greeting or
    /// sign-off of yours to go on, so a plain thanks."
    pub built_from: String,
    /// Clients show the text this long before sending, with undo.
    pub countdown_seconds: u32,
    pub dry_run: bool,
    /// Set once sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_message_id: Option<MessageId>,
}

/// Returned by `Request::MergePeople` and `Request::SplitPerson`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PersonMergeData {
    pub account_id: AccountId,
    /// The person after the change.
    pub person: PersonRefData,
    /// Addresses the change moves.
    pub changed: Vec<String>,
    /// Conversations that now show under this person.
    pub thread_count: u32,
    pub dry_run: bool,
    /// "Samir Patel is now one person with 2 addresses."
    pub summary: String,
}
