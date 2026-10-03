//! The modes as one email sees them (blueprint 22): which modes hold a
//! thread and why, per-mode done with the handoff copy, and the rail.
//!
//! Membership is computed, never stored (D097): from the desk's lanes,
//! open to-dos, the sender classifier and per-mode done marks.

use mxr_core::id::*;
use serde::{Deserialize, Serialize};

use super::SenderKindData;

/// The rail's words where a mode's guide doesn't cover them (#284 copy).
pub mod rail_copy {
    /// Inbox is a lens, not a mode: it gets a header only.
    pub const INBOX_HEADER: &str =
        "Everything, newest first. The modes hold the same mail, sorted.";
}

/// One of the five modes. Now is a view over them, and Inbox a lens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ModeKindData {
    Messages,
    Todo,
    Updates,
    Reading,
    Archive,
}

impl ModeKindData {
    /// Rail order.
    pub const ALL: [Self; 5] = [
        Self::Messages,
        Self::Todo,
        Self::Updates,
        Self::Reading,
        Self::Archive,
    ];

    /// The id clients and the CLI use: "messages", "todo".
    pub const fn id(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::Todo => "todo",
            Self::Updates => "updates",
            Self::Reading => "reading",
            Self::Archive => "archive",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Messages => "Messages",
            Self::Todo => "To do",
            Self::Updates => "Updates",
            Self::Reading => "Reading",
            Self::Archive => "Archive",
        }
    }

    /// The jump: `g` then the mode's letter.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Messages => "g m",
            Self::Todo => "g x",
            Self::Updates => "g u",
            Self::Reading => "g r",
            Self::Archive => "g e",
        }
    }

    /// Whether this mode keeps a thread in the provider's inbox. Archive
    /// never does: filing a record and archiving the email are the same
    /// end, and records stay in Archive whatever the provider does.
    pub const fn holds_inbox(self) -> bool {
        !matches!(self, Self::Archive)
    }

    /// Accepts "todo", "to-do", "To do", "msgs".
    pub fn parse(raw: &str) -> Option<Self> {
        let wanted: String = raw
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        match wanted.as_str() {
            "messages" | "msgs" | "message" => Some(Self::Messages),
            "todo" | "todos" => Some(Self::Todo),
            "updates" | "update" => Some(Self::Updates),
            "reading" | "read" => Some(Self::Reading),
            "archive" | "records" => Some(Self::Archive),
            _ => None,
        }
    }
}

/// One mode holding a thread, and why.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModeMembershipData {
    pub mode: ModeKindData,
    /// "To do".
    pub name: String,
    /// "g x".
    pub key: String,
    /// "Here because: Sam wrote to you and you've written to them (rule)."
    pub reason: String,
    /// The line other modes show: "Also in To do: Sign the lease, act by
    /// Mon 13 Oct · due Wed 15 Oct".
    pub also_in: String,
    /// The mode's view is an early version built on an existing one.
    #[serde(default)]
    pub early: bool,
    /// To do: the open rows holding the thread.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub todo_ids: Vec<String>,
}

/// A first-time sender's one question, asked on their row in the mode
/// their mail landed in (D117). Answering is `SetSenderKind`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ScreenerQuestionData {
    pub account_id: AccountId,
    pub sender_email: String,
    /// "New sender. Keep in Messages?"
    pub question: String,
    /// The answers, the current mode's first.
    pub choices: Vec<ScreenerChoiceData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ScreenerChoiceData {
    /// What `SetSenderKind` stores.
    pub kind: SenderKindData,
    /// "Messages", "Updates", "Reading", "Block".
    pub label: String,
}

/// Every mode one thread is in, returned by `Request::GetModeMembership`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadModesData {
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub subject: String,
    /// In rail order. Empty when no mode holds it (Inbox still shows it).
    pub modes: Vec<ModeMembershipData>,
    /// Modes this thread was marked done in and stays out of until a new
    /// message arrives.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub done_in: Vec<ModeKindData>,
    /// The modes keeping it in the provider's inbox: Inbox's "held by To
    /// do" chip. Archive never holds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held_by: Vec<ModeKindData>,
    /// Any message of the thread is in the provider's inbox.
    pub in_inbox: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_sender: Option<ScreenerQuestionData>,
}

/// What done in one mode did to one thread, or with `dry_run` would do.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModeDoneOutcomeData {
    pub thread_id: ThreadId,
    /// Absent when the thread was not found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<AccountId>,
    pub mode: ModeKindData,
    /// Messages, Updates, Reading: the mode's done mark was written.
    #[serde(default)]
    pub marked: bool,
    /// To do: the rows ticked off.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub todos_ticked: Vec<String>,
    /// Modes still holding the thread after this.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub still_in: Vec<ModeKindData>,
    /// Messages archived at the provider because the last mode let go.
    #[serde(default)]
    pub archived: u32,
    /// Messages marked read with that archive.
    #[serde(default)]
    pub marked_read: u32,
    /// "Gmail", "Outlook" or "the mail server": named in `copy`.
    pub provider: String,
    /// The handoff toast: "Done in Messages. Still in To do (due Wed)." or
    /// "Done. Archived in Gmail."
    pub copy: String,
    /// Why this thread was not done. The others still are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Built in its researched shape, or an early version on an existing view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RailStatusData {
    Built,
    Early,
}

/// One rail entry: Now, a mode, or Inbox.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RailEntryData {
    /// "now", "messages", "todo", "updates", "reading", "archive", "inbox".
    pub id: String,
    pub name: String,
    /// "g h".
    pub key: String,
    /// `home` (Now), `modes`, or `lens` (Inbox): the rail's three groups.
    pub group: String,
    /// What the entry holds now, for a quiet count. Absent where a count
    /// means nothing (Archive, Inbox).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// A badge counts work only, so only Now has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<u32>,
    pub status: RailStatusData,
    /// For an early version: what it is built on, for the clients' "early
    /// version" note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub early_note: Option<String>,
    /// The line under the name, from the mode's guide where the mode is
    /// built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
}

/// A page under More.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RailLinkData {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// "Every sender you've decided on, and new ones to decide."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Returned by `Request::GetRail`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RailData {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    /// Now, the five modes, then Inbox.
    pub entries: Vec<RailEntryData>,
    /// Under More: the Screener (off the rail, D117) and the rest.
    pub more: Vec<RailLinkData>,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap for direct fixture failures"
    )]

    use super::*;

    #[test]
    fn modes_parse_loosely_and_keep_their_keys() {
        for mode in ModeKindData::ALL {
            assert_eq!(ModeKindData::parse(mode.id()), Some(mode));
            assert_eq!(ModeKindData::parse(mode.name()), Some(mode));
            assert!(mode.key().starts_with("g "));
        }
        assert_eq!(ModeKindData::parse("to-do"), Some(ModeKindData::Todo));
        assert_eq!(ModeKindData::parse("now"), None);
        assert!(!ModeKindData::Archive.holds_inbox());
    }

    #[test]
    fn serde_ids_match_the_cli_ids() {
        for mode in ModeKindData::ALL {
            assert_eq!(
                serde_json::to_value(mode).unwrap(),
                serde_json::Value::String(mode.id().to_string())
            );
        }
    }
}
