//! Now: the front page (blueprint 22, "Now shows at most ten things in
//! four fixed sections"). People, Due soon, one Updates card and an
//! evening Reading pick, in that order, at most three items each. The
//! caps live in the daemon so every client shows the same Now.

use mxr_core::id::*;
use serde::{Deserialize, Serialize};

use super::{
    DeskRowData, RecordMomentData, ScreenerQuestionData, TodoData, TodoFirstRunData, UpdateLineData,
};

/// Items each section shows before "and N more".
pub const NOW_SECTION_CAP: usize = 3;

/// The words Now teaches itself with (D118, the #284 copy).
pub mod now_copy {
    pub const HEADER: &str = "The few things that need you now, from every mode.";
    pub const NEVER_HAD_ANY: &str = "Now fills in as mxr sorts your mail, newest first. People waiting on you, things due soon and the latest updates show here.";
    /// Followed by when the next to-do surfaces, where one is due to.
    pub const CLEAR: &str = "Clear.";
    /// The full explanation `?` leads with. Nothing shows it unasked.
    pub const ABOUT: &str = "Now shows at most ten things: people waiting on you, things due soon, the latest updates and, after 17:00, one thing to read. Acting on a row here does it in that row's own mode.";
}

/// A person row: a desk row from You owe, New from people or Waiting on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NowPersonData {
    pub row: DeskRowData,
    /// "From Messages: Maya wrote to you 22h ago."
    pub why: String,
    /// A first-time sender: the one question, on the row (D117).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_sender: Option<ScreenerQuestionData>,
}

/// People: whose turn it is, from Messages.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NowPeopleData {
    /// At most three people, each once at their most pressing
    /// conversation: You owe first, then New from people, then Waiting on.
    pub rows: Vec<NowPersonData>,
    /// Every person in Messages, shown or not.
    pub total: u32,
    /// "and 8 more in Messages", when there are more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more_line: Option<String>,
    /// When You owe runs long: "11 people are waiting on you. The three
    /// below have waited longest past your usual pace."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overload_line: Option<String>,
}

/// A to-do on Now.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NowTodoData {
    pub todo: TodoData,
    /// "From To do: act by Wed 7 Oct · due Fri 9 Oct."
    pub why: String,
}

/// Due soon: To do's Now band, by act-by.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NowDueData {
    pub todos: Vec<NowTodoData>,
    pub total: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more_line: Option<String>,
}

/// One source on the Updates card.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NowUpdateSourceData {
    pub sender_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    pub count: u32,
}

/// The Updates card: the latest digest as one item, whatever the count:
/// its headline, up to three lines that need a look or changed, and how
/// much routine waits behind them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NowUpdatesCardData {
    pub message_count: u32,
    pub source_count: u32,
    /// The busiest three sources.
    pub top_sources: Vec<NowUpdateSourceData>,
    /// "23 updates from 9 sources. Most from GitHub, Vercel and Stripe."
    pub line: String,
    /// The digest's cut: the card holds what arrived by then.
    pub since: chrono::DateTime<chrono::Utc>,
    /// The card's threads, newest first. Letting go of the digest is
    /// `LetGoDigest` with `selection_token`.
    pub thread_ids: Vec<ThreadId>,
    pub early: bool,
    /// "This morning's digest".
    #[serde(default)]
    pub title: String,
    /// "08:00".
    #[serde(default)]
    pub cut_label: String,
    /// "1 needs a look, 2 changed. 23 routine from 9 sources."
    #[serde(default)]
    pub headline: String,
    /// Up to three lines: Needs a look first, then Changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<UpdateLineData>,
    /// "+23 routine", when routine or further lines wait in Updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more_line: Option<String>,
    /// Pass to `LetGoDigest` so the card lets go of what it showed.
    #[serde(default)]
    pub selection_token: String,
    /// "Let go of 31 updates from 12 sources; 2 also in To do stay there."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub let_go_line: Option<String>,
}

/// The evening's one thing to read.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NowReadingPickData {
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub sender_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    pub subject: String,
    pub date: chrono::DateTime<chrono::Utc>,
    /// "From Reading: you've opened 9 of the last 10 from Long Reads."
    pub why: String,
}

/// Returned by `Request::GetNow`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NowData {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    /// `now_copy::HEADER`.
    pub header: String,
    /// "Friday afternoon. 3 people, 2 things to act on."
    pub headline: String,
    /// The sections, in their fixed order. An empty one is omitted by
    /// clients; its total says so.
    pub people: NowPeopleData,
    pub due_soon: NowDueData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates: Option<NowUpdatesCardData>,
    /// From 17:00 local time only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading: Option<NowReadingPickData>,
    /// "Not now: Reading 6 this week", while the pick isn't showing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_now: Option<String>,
    /// Items shown across all sections: never more than ten.
    pub item_count: u32,
    /// Set when every section is empty: "Clear. The next to-do surfaces
    /// Mon 19 Oct 09:00." or the never-had-any line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_state: Option<String>,
    /// When the next to-do surfaces, for the low tide scene.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The newest-first first run, for "Sorting your mail" while it runs.
    pub first_run: TodoFirstRunData,
    /// Archive records with a moment soon (a trip in 72 hours, a ticket
    /// today, a return window closing, a warranty ending), soonest first,
    /// at most three. One line on Now, never a badge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coming_up: Vec<RecordMomentData>,
}
