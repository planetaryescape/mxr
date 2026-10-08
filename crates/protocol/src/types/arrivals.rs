//! Sorting shows its work (blueprint 22, D119): the arrivals line on Now,
//! the emails each count opens, "Not sure" questions, per-email moves and
//! the corrections they leave.
//!
//! Where an email went is stored when it arrives, because membership
//! (D097) is computed from today's inbox and forgets mail that left it.

use mxr_core::id::*;
use serde::{Deserialize, Serialize};

use super::ModeKindData;

/// The arrivals line's words (D119 copy table).
pub mod arrivals_copy {
    /// Asked once after a move, beside undo.
    pub const ALWAYS_FOR_SENDER: &str = "Always for this sender?";
    /// Shown under the line's `?`: only the rules that are live in code.
    pub const NEVER_BURY: &str = "People you've written to always reach Messages when the mail is addressed to you, whatever list it came through.";
    pub const TRACK_RECORD_HELP: &str =
        "Counts the emails you moved. Mail you never opened isn't checked.";
    /// The most "Not sure" questions Now asks in one day.
    pub const NOT_SURE_DAILY_CAP: usize = 3;
    /// The hint on the first Not-sure item (contextual, never a card).
    pub const NOT_SURE_HINT: &str = "Two of mxr's rules disagreed about this one. One key sends it where it belongs: m Messages, x To do, u Updates, r Reading, e Archive.";
    /// The hint on the first move's toast.
    pub const MOVE_HINT: &str =
        "X moves just this email. K sends everything from this sender there from now on.";
}

/// Where an arrival is counted: a mode, or one of the buckets that make
/// the counts sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ArrivalBucketData {
    Messages,
    Todo,
    Updates,
    Reading,
    Archive,
    ScreenedOut,
    Spam,
    /// Stored, not placed yet.
    Sorting,
}

impl ArrivalBucketData {
    /// The line's order.
    pub const ALL: [Self; 8] = [
        Self::Messages,
        Self::Todo,
        Self::Updates,
        Self::Reading,
        Self::Archive,
        Self::ScreenedOut,
        Self::Spam,
        Self::Sorting,
    ];

    /// The name stored in `arrivals.mode` and used by the CLI.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::Todo => "todo",
            Self::Updates => "updates",
            Self::Reading => "reading",
            Self::Archive => "archive",
            Self::ScreenedOut => "screened_out",
            Self::Spam => "spam",
            Self::Sorting => "sorting",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let wanted = raw.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        Self::ALL
            .into_iter()
            .find(|bucket| bucket.id() == wanted)
            .or_else(|| ModeKindData::parse(raw).map(Self::from_mode))
    }

    pub const fn from_mode(mode: ModeKindData) -> Self {
        match mode {
            ModeKindData::Messages => Self::Messages,
            ModeKindData::Todo => Self::Todo,
            ModeKindData::Updates => Self::Updates,
            ModeKindData::Reading => Self::Reading,
            ModeKindData::Archive => Self::Archive,
        }
    }

    pub const fn mode(self) -> Option<ModeKindData> {
        match self {
            Self::Messages => Some(ModeKindData::Messages),
            Self::Todo => Some(ModeKindData::Todo),
            Self::Updates => Some(ModeKindData::Updates),
            Self::Reading => Some(ModeKindData::Reading),
            Self::Archive => Some(ModeKindData::Archive),
            Self::ScreenedOut | Self::Spam | Self::Sorting => None,
        }
    }

    /// "8 Messages", "1 spam", "2 screened out", "1 still sorting".
    pub fn label(self, count: u32) -> String {
        match self.mode() {
            Some(mode) => format!("{count} {}", mode.name()),
            None => match self {
                Self::ScreenedOut => format!("{count} screened out"),
                Self::Spam => format!("{count} spam"),
                _ => format!("{count} still sorting"),
            },
        }
    }
}

/// One count on the line, and what opening it lists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArrivalCountData {
    pub bucket: ArrivalBucketData,
    pub count: u32,
    /// "8 Messages".
    pub label: String,
}

/// An email Now asks about: two placement rules disagreed (D119's U1:
/// someone you've written to only copied you).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NotSureData {
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub sender_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    pub subject: String,
    /// Where it is for now.
    pub mode: ModeKindData,
    /// "Maya Ortiz copied you on "Q4 plan". Updates for now."
    pub line: String,
    /// The answers, one key each: Messages, To do, Updates, Reading,
    /// Archive.
    pub choices: Vec<MoveChoiceData>,
}

/// One destination of a move, with its key in the picker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MoveChoiceData {
    pub mode: ModeKindData,
    pub label: String,
    /// "m", "x", "u", "r", "e": the mode's jump letter.
    pub key: String,
}

impl MoveChoiceData {
    /// Every mode, in rail order, keyed by its `g` letter.
    pub fn all() -> Vec<Self> {
        ModeKindData::ALL
            .into_iter()
            .map(|mode| Self {
                mode,
                label: mode.name().to_string(),
                key: mode.key().trim_start_matches("g ").to_string(),
            })
            .collect()
    }
}

/// Returned by `Request::GetArrivals`: the line on Now.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArrivalsData {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    /// The window: since Now was last opened, at most 24 hours back; the
    /// start of today when Now was never opened.
    pub since: chrono::DateTime<chrono::Utc>,
    pub until: chrono::DateTime<chrono::Utc>,
    /// "08:12", or "Tue 22:10" for an earlier day.
    pub since_label: String,
    /// Every inbound email first seen in the window, each counted once.
    pub total: u32,
    /// Non-zero counts in the line's order. They sum to `total`.
    pub counts: Vec<ArrivalCountData>,
    /// To do and Archive, shown as "also" and never added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<ArrivalCountData>,
    /// "Since 08:12: 50 arrived. 8 Messages · 10 Updates · 31 Reading · 1
    /// spam. Also 2 in To do." or "Nothing new since 08:12. Latest mail 2h
    /// ago."
    pub line: String,
    /// When nothing on Now waits: "Clear. All 50 emails since 08:12 are
    /// accounted for." Absent when nothing arrived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_at: Option<chrono::DateTime<chrono::Utc>>,
    /// At most three a day.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_sure: Vec<NotSureData>,
    /// "2 emails I wasn't sure about. Where should these go?"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_sure_line: Option<String>,
    /// The hint the first Not-sure item carries until the user has seen it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_sure_hint: Option<String>,
    /// "Last week mxr sorted 310 emails; you moved 2." Only once a move
    /// exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_record: Option<String>,
    /// The never-bury rules that are live, for the line's `?`.
    pub never_bury: String,
}

/// One email in an arrivals list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArrivalItemData {
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub sender_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    pub subject: String,
    pub date: chrono::DateTime<chrono::Utc>,
    pub first_seen_at: chrono::DateTime<chrono::Utc>,
    /// Where it went when it arrived.
    pub arrived_in: ArrivalBucketData,
    /// Where it is now, after any correction.
    pub bucket: ArrivalBucketData,
    /// "has List-Unsubscribe".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The chip: "→ Updates · automated sender".
    pub chip: String,
    /// Moved by you, this email only.
    #[serde(default)]
    pub moved: bool,
    #[serde(default)]
    pub not_sure: bool,
    #[serde(default)]
    pub unread: bool,
    #[serde(default)]
    pub also_todo: bool,
    #[serde(default)]
    pub also_archive: bool,
}

/// Returned by `Request::ListArrivals`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArrivalListData {
    pub since: chrono::DateTime<chrono::Utc>,
    pub until: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket: Option<ArrivalBucketData>,
    /// How many match in all; `items` is the newest page.
    pub total: u32,
    pub items: Vec<ArrivalItemData>,
}

/// What a move did, or with `dry_run` would do.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MoveOutcomeData {
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub sender_email: String,
    pub from: ArrivalBucketData,
    pub to: ModeKindData,
    /// The sender's mode was set, for this and all their mail.
    pub sender: bool,
    pub dry_run: bool,
    /// "Moved to Reading." / "All mail from maya@example.com goes to
    /// Reading." / "Would move to Reading."
    pub copy: String,
    /// "Always for this sender? (K)": asked once after a per-email move to
    /// Messages, Updates or Reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_sender: Option<String>,
    /// The hint the first move's toast carries until the user has seen it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Undo with `Request::UndoMove`. Absent for a dry run or a move that
    /// changed nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction_id: Option<i64>,
    /// To do and Archive add the email there: the to-do or record made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_id: Option<String>,
}

/// One stored correction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CorrectionData {
    pub id: i64,
    pub account_id: AccountId,
    /// `email` or `sender`.
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    pub sender_email: String,
    pub from_mode: String,
    pub to_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// `move`, `sender` or `not_sure`.
    pub source: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undone_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap for direct fixture failures"
    )]

    use super::*;

    #[test]
    fn buckets_round_trip_through_their_ids_and_serde() {
        for bucket in ArrivalBucketData::ALL {
            assert_eq!(ArrivalBucketData::parse(bucket.id()), Some(bucket));
            assert_eq!(
                serde_json::to_value(bucket).unwrap(),
                serde_json::Value::String(bucket.id().to_string())
            );
        }
        assert_eq!(
            ArrivalBucketData::parse("To do"),
            Some(ArrivalBucketData::Todo)
        );
        assert_eq!(
            ArrivalBucketData::parse("screened-out"),
            Some(ArrivalBucketData::ScreenedOut)
        );
        assert_eq!(ArrivalBucketData::Spam.label(1), "1 spam");
        assert_eq!(ArrivalBucketData::Reading.label(31), "31 Reading");
    }

    #[test]
    fn every_mode_has_one_picker_key_and_none_repeat() {
        let choices = MoveChoiceData::all();
        let keys: Vec<&str> = choices.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, ["m", "x", "u", "r", "e"]);
    }
}
