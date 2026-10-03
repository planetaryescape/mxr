//! To do: things email asked you to do, as a runway ordered by when to act.
//!
//! The daemon detects rows with rules (no model), places them with a fixed
//! lead-time table and relevancy windows, and serves the runway's bands
//! ready to draw: every client shows the same rows, labels and reasons.
//! Mutations take `dry_run`, and a preview selects exactly the rows the
//! real call changes.

use mxr_core::id::*;
use serde::{Deserialize, Serialize};

/// The words To do teaches itself with (D118). One table, so the CLI, TUI,
/// web and agents say the same thing.
pub mod todo_copy {
    /// Under the mode's name, and the first line of `mxr todo --help`.
    pub const HEADER: &str = "Things email asked you to do, ordered by when to act.";
    /// The empty state before anything has ever landed here.
    pub const NEVER_HAD_ANY: &str = "When an email asks you to pay, sign, reply by a date or confirm something, it shows up here as one line: what to do and when to act.";
    /// How to add one by hand, per client.
    pub const ADD_ONE_KEYS: &str = "Press `t` on any email to add one yourself.";
    pub const ADD_ONE_CLI: &str = "`mxr todo add --from MESSAGE_ID` adds one yourself.";
    /// The empty state once everything is done, before the next item's
    /// line: "Nothing needs you. Next: renew car insurance shows up Mon 19
    /// Oct."
    pub const CLEAR_FOR_NOW: &str = "Nothing needs you.";
    pub const CARD: &str = "Each row is one thing to do, written as what to do, not the email's subject. Act by the first date; the bar fills from when it showed up to when it's due, and Enter does what the button says.";
    pub const CARD_KEYS: &str = "Enter do it · e tick off · Z schedule · X not a to-do";
    /// The first run's summary card title.
    pub const FIRST_RUN_TITLE: &str = "Your last two weeks, sorted";
    /// The first run's summary line for To do, after the counts.
    pub const FIRST_RUN_LINE: &str = "What email asked you to do";
    /// Why the catch-up exists, shown once with it.
    pub const CATCH_UP_WHY: &str = "These are from before mxr sorted your mail, so they're shown once instead of all landing in Now. Keep what still needs you; let go of the rest.";
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TodoStateData {
    Open,
    Done,
    /// "Not a to-do": kept as a correction, never shown again.
    Dismissed,
    /// Past its window, or let go in the catch-up. One key from restore.
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoAmountData {
    /// In the currency's minor unit.
    pub minor: i64,
    /// ISO 4217.
    pub currency: String,
    /// "£142.00".
    pub display: String,
}

/// What the row's primary action does.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TodoActionKindData {
    /// Open the source email in mxr with `url` highlighted. mxr never opens
    /// an external link straight from a to-do: a one-click pay link is
    /// deferred until a design survives review (docs/issues/one-click-pay-link.md).
    #[default]
    OpenEmail,
}

/// The one thing Enter does.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoActionData {
    /// Always `open_email` in this phase.
    #[serde(default)]
    pub kind: TodoActionKindData,
    /// "Open email to pay", "Open email to verify".
    pub label: String,
    /// The email to open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    /// The link in that email the action is about, to highlight there.
    /// Not an action: never open it from the row.
    pub url: String,
    /// The link's registrable domain, shown beside the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Always false: nothing on a to-do is trusted to open directly. Kept
    /// so earlier clients read the action as untrusted.
    pub trusted: bool,
}

/// Where one field came from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoFieldData {
    /// title, kind, counterparty, amount, due_at, act_by_at, surface_at,
    /// relevant_until, action_url, event_start
    pub field: String,
    /// schema | ics | rule | table | model | user
    pub source: String,
    /// False for a guess to confirm, such as a numeric date that reads
    /// differently in UK and US order. Shown with an open dot.
    pub checked: bool,
    /// The words it was read from, or how it was worked out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    /// "a pattern in the email", "the lead-time table", "you".
    pub source_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoLooksDoneData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    /// "payment received 3 Oct".
    pub reason: String,
}

/// One row: a thing to do, titled verb plus object, never the subject.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoData {
    pub id: String,
    pub account_id: AccountId,
    /// bill | payment_failed | renewal | document | lease | return | rsvp |
    /// verify | sign | promise | other
    pub kind: String,
    pub verb: String,
    /// "Pay council tax".
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterparty: Option<String>,
    /// "you promised Priya" on promise rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<TodoAmountData>,
    /// The outside deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The phrase the due date was read from, verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_words: Option<String>,
    /// When you must act: due minus processing time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act_by_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the row shows up in Now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Your own date; wins over `surface_at`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_for: Option<chrono::DateTime<chrono::Utc>>,
    /// When it stops mattering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relevant_until: Option<chrono::DateTime<chrono::Utc>>,
    pub state: TodoStateData,
    /// pending | kept | let_go | overflow
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catchup: Option<String>,
    /// rule | schema | ics | model | handoff | manual
    pub origin: String,
    /// "Here because: \"payment due 9 October\" (rule)."
    pub why: String,
    /// What happens next, where it's true: "When the receipt arrives, the
    /// row says it looks done."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// "act by Wed 7 Oct · due Fri 9 Oct", "was due Fri 9 Oct", "shows up
    /// Mon 12 Oct", "no date".
    pub when_label: String,
    /// Past the due date.
    pub overdue: bool,
    /// How far the runway bar is filled, from surfacing (0) to due (1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runway: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<TodoActionData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub looks_done: Option<TodoLooksDoneData>,
    /// Where each field came from.
    pub fields: Vec<TodoFieldData>,
    /// You made or changed it: it never expires and re-runs leave it be.
    pub user_touched: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<ThreadId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_message_id: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_date: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surfaced_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expired_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismissed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// One week of Coming up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoWeekData {
    /// The Monday the week starts, in the user's zone.
    pub week_start: chrono::NaiveDate,
    /// "wk of 12 Oct".
    pub label: String,
    pub todos: Vec<TodoData>,
}

/// The next row to show up, for the empty state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoNextData {
    pub todo_id: String,
    pub title: String,
    pub at: chrono::DateTime<chrono::Utc>,
    /// "Mon 19 Oct".
    pub label: String,
}

/// How far the newest-first first run has got.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoFirstRunData {
    pub complete: bool,
    /// Messages classified so far.
    pub scanned: i64,
    /// The oldest message date reached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reached: Option<chrono::DateTime<chrono::Utc>>,
}

/// Returned by `Request::GetTodoRunway`: the bands, ready to draw.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoRunwayData {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    /// `todo_copy::HEADER`.
    pub header: String,
    /// "3 things need you this week. Council tax first, act by Wed." Empty
    /// when Now is.
    pub headline: String,
    /// Set when Now is empty: what lands here, or when the next thing does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_state: Option<String>,
    /// Shown now, by act-by; overdue rows after the ones still in time.
    pub now: Vec<TodoData>,
    /// Showing up in the next 30 days, by week.
    pub coming_up: Vec<TodoWeekData>,
    /// Dated, showing up after 30 days.
    pub later: Vec<TodoData>,
    /// No date.
    pub whenever: Vec<TodoData>,
    pub done_this_week: Vec<TodoData>,
    /// Rows waiting in the one-time catch-up.
    pub catchup_count: u32,
    /// "3 expired since you last looked", when above zero.
    pub expired_since_last_looked: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_surface: Option<TodoNextData>,
    pub first_run: TodoFirstRunData,
}

/// A kind and how many, for "Already over, so not shown: 50 past invites".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoKindCountData {
    pub kind: String,
    pub count: u32,
    /// "past invites".
    pub label: String,
}

/// Returned by `Request::GetTodoCatchup`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoCatchupData {
    /// "Catch up: 12 things from the last two weeks might still need you."
    pub title: String,
    /// `todo_copy::CATCH_UP_WHY`.
    pub why: String,
    /// The catch-up window, in days.
    pub window_days: u32,
    pub todos: Vec<TodoData>,
    /// Found for the batch after it was full; in the Expired list.
    pub overflow_count: u32,
    /// What the first run found already over, by kind.
    pub already_over: Vec<TodoKindCountData>,
    /// "Already over, so not shown: 50 past invites, 63 quiet parcels."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub already_over_line: Option<String>,
    pub first_run: TodoFirstRunData,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TodoStateActionData {
    /// Tick it off.
    Done,
    /// Back to open from done, not a to-do, or expired; yours from now on.
    Undo,
    /// Not a to-do.
    Dismiss,
}

/// Keep or let go in the catch-up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum TodoCatchupDecisionData {
    Keep {
        todo_ids: Vec<String>,
    },
    LetGo {
        todo_ids: Vec<String>,
    },
    /// Every row still waiting in the batch.
    LetGoAll,
}

/// One edit: `field` is title, due, amount, counterparty or kind; an
/// empty value clears due, amount and counterparty.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoEditData {
    pub field: String,
    pub value: String,
}

/// Returned by every to-do mutation. With `dry_run`, `changed` is what
/// would change and nothing was written.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TodoChangeData {
    pub dry_run: bool,
    /// done | undo | dismiss | schedule | edit | create | keep | let_go
    pub action: String,
    /// The rows as they are (or would be) after the change.
    pub changed: Vec<TodoData>,
    /// Ids that matched but were not in a state this change applies to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unchanged: Vec<String>,
    /// One line saying what happened: "Ticked off 2.", "Would let go of 12."
    pub summary: String,
}
