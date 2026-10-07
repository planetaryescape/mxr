//! Updates: notifications as a briefing by source, gathered at fixed cuts
//! and let go in one key (blueprint 22, phase 4).
//!
//! The daemon reads every automated message into one fact line with the
//! numbers it quotes, groups them by source and sorts sources into Needs a
//! look, Changed and Routine. Counts, latest states and deltas are code;
//! there is no model text in this phase. Letting go of a digest takes
//! `dry_run`, and the preview and the run select the same messages.

use mxr_core::id::*;
use serde::{Deserialize, Serialize};

use super::ModeDoneOutcomeData;

/// The words Updates teaches itself with (D118, the #284 copy).
pub mod updates_copy {
    pub const HEADER: &str = "Notifications gathered twice a day. Read the digest, then let go.";
    pub const NEVER_HAD_ANY: &str = "Notifications from services and apps land here and are gathered into a digest at 08:00 and 16:30. Anything that needs you, like a failed payment, goes straight to To do.";
    /// Filled by code: "Nothing new since 08:00. Next digest at 16:30."
    pub const CLEAR_FOR_NOW: &str = "Nothing new since {cut}. Next digest at {next}.";
    pub const CARD: &str = "Updates gathers notifications into a digest at 08:00 and 16:30, one line per source, like your bank or GitHub, with what changed first. Anything that needs you goes to To do at once, so you can read this and let it go.";
    pub const LANDS_HERE: &str = "Notifications from services and apps: builds, parcels, sign-in alerts, statements, reports and receipts.";
    pub const WHY: &str = "Here because: {evidence} ({source}). In the {cut} digest.";
    pub const FIRST_RUN_LINE: &str = "Notifications, twice a day";
    /// Marks a needs-you line already handed to To do.
    pub const IN_TODO: &str = "already in To do";
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UpdateSignalData {
    Routine,
    Changed,
    NewSource,
    Anomaly,
    NeedsYou,
}

/// Where a line sits in the briefing. Sections never reorder.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UpdateSectionData {
    NeedsALook,
    Changed,
    Routine,
}

/// How a source is tuned with `K`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UpdateSourceSettingData {
    /// In every digest (the default).
    #[default]
    EveryDigest,
    /// Only when something changed or needs a look.
    ChangesOnly,
    /// Never in the digest; mail still reaches Archive and search.
    Muted,
    /// Every message goes to To do on arrival.
    Breakthrough,
}

impl UpdateSourceSettingData {
    pub const ALL: [Self; 4] = [
        Self::EveryDigest,
        Self::ChangesOnly,
        Self::Muted,
        Self::Breakthrough,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EveryDigest => "every_digest",
            Self::ChangesOnly => "changes_only",
            Self::Muted => "muted",
            Self::Breakthrough => "breakthrough",
        }
    }

    /// Accepts "muted", "mute", "changes-only", "every digest" and so on.
    pub fn parse(value: &str) -> Option<Self> {
        let wanted: String = value
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        Some(match wanted.as_str() {
            "everydigest" | "every" | "default" | "normal" | "unmute" => Self::EveryDigest,
            "changesonly" | "changes" => Self::ChangesOnly,
            "muted" | "mute" => Self::Muted,
            "breakthrough" | "always" => Self::Breakthrough,
            _ => return None,
        })
    }

    /// "Strava: only when something changes."
    pub const fn effect(self) -> &'static str {
        match self {
            Self::EveryDigest => "in every digest",
            Self::ChangesOnly => "only when something changes",
            Self::Muted => "muted, its mail stays in Archive and search",
            Self::Breakthrough => "straight to To do on arrival",
        }
    }
}

/// A number as the message quoted it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateNumberData {
    /// Verbatim: "21.3 km", "R 4,210.00".
    pub raw: String,
    pub value: f64,
    /// "km", "run", "%" or an ISO currency code.
    pub unit: String,
}

/// A change computed by code against the previous message of the same
/// template, only when both numbers share a unit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateDeltaData {
    pub raw: String,
    pub previous_raw: String,
    /// Percent change; points for a percentage.
    pub change: f64,
    /// "up 12% on last week".
    pub text: String,
    pub against: chrono::DateTime<chrono::Utc>,
}

/// A thing with a state: a parcel, a build or an incident.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateTrackerData {
    /// `parcel`, `build` or `incident`.
    pub kind: String,
    /// As stored: "out_for_delivery", "failed", "resolved".
    pub state: String,
    /// "out for delivery".
    pub state_label: String,
    /// `good` (ended well), `bad` (ended badly), `progress`, or `quiet`
    /// (a parcel with no news for too long).
    pub outcome: String,
    /// A parcel's steps, "ordered" to "delivered", and where it is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    /// "Arriving by Thu 8 Oct · DHL · 3 emails".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_id: Option<String>,
}

/// The one place a line opens with `L`. Never a pay link.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateLinkData {
    pub url: String,
    /// Shown before it opens: "github.com".
    pub domain: String,
}

/// Where one field of a line came from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateProvenanceData {
    /// "fact", "numbers", "delta", "window", "state", "link".
    pub field: String,
    /// "rule", "schema", "code" or "default".
    pub source: String,
    /// "the subject", "21.3 km quoted from the body", "against 30 Sep".
    pub evidence: String,
}

/// One line of the briefing: a source's latest fact per template, or one
/// tracker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateLineData {
    /// Stable within a digest: source, template or tracker.
    pub id: String,
    pub section: UpdateSectionData,
    pub account_id: AccountId,
    pub source_key: String,
    /// "GitHub acme/api", "Strava".
    pub source_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sender_email: String,
    /// The fact, written by rules from the subject or the body's first
    /// line: "Run failed: CI - main".
    pub fact: String,
    /// "subject" or "body".
    pub fact_source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub numbers: Vec<UpdateNumberData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<UpdateDeltaData>,
    pub signal: UpdateSignalData,
    /// Messages the line folds.
    pub count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub message_ids: Vec<MessageId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thread_ids: Vec<ThreadId>,
    /// The newest message, for `o`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_message_id: Option<MessageId>,
    /// The message `fact` and `todo_title` came from, for `t`. In Changed
    /// it can be older than the newest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_message_id: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_thread_id: Option<ThreadId>,
    pub latest_at: chrono::DateTime<chrono::Utc>,
    /// Only when the time is the fact (a sign-in, a failing build): "06:12".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<UpdateLinkData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracker: Option<UpdateTrackerData>,
    /// The open to-do this line already went to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo_id: Option<String>,
    /// `updates_copy::IN_TODO` when `todo_id` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_todo: Option<String>,
    /// What `t` prefills: "Check new sign-in to Google".
    pub todo_title: String,
    /// "Here because: automated sender, not a person (rule). In the 08:00
    /// digest."
    pub why: String,
    pub setting: UpdateSourceSettingData,
    /// Asked once a month at most: "You've let go of Strava 8 digests in a
    /// row without opening it. Mute it, or changes only?"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<UpdateProvenanceData>,
}

/// The cut a digest shows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdatesCutData {
    pub at: chrono::DateTime<chrono::Utc>,
    /// "08:00" in your zone.
    pub label: String,
    /// "This morning's digest".
    pub title: String,
    pub previous_at: chrono::DateTime<chrono::Utc>,
    pub next_at: chrono::DateTime<chrono::Utc>,
    pub next_label: String,
    /// Every cut of the day, from `updates.cuts`: ["08:00", "16:30"].
    pub cuts: Vec<String>,
}

/// What arrived after the cut: always current, quiet and uncounted.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdatesSinceData {
    /// "arriving for 16:30".
    pub label: String,
    pub message_count: u32,
    pub source_count: u32,
    /// One line per source, newest first.
    pub lines: Vec<UpdateLineData>,
}

/// An update whose window closed: it never shows in a cut.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateExpiredData {
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub source_name: String,
    pub fact: String,
    /// "one-time code", "sign-in alert", "offer", "verify link".
    pub kind: String,
    pub expired_at: chrono::DateTime<chrono::Utc>,
}

/// Returned by `Request::GetUpdatesDigest`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdatesDigestData {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    /// `updates_copy::HEADER`.
    pub header: String,
    pub cut: UpdatesCutData,
    /// "2 changed, 1 needs a look. 23 routine from 9 sources." Empty when
    /// the digest is.
    pub headline: String,
    /// Messages in the digest that are shown.
    pub message_count: u32,
    pub source_count: u32,
    pub needs_a_look: Vec<UpdateLineData>,
    pub changed: Vec<UpdateLineData>,
    pub routine: Vec<UpdateLineData>,
    pub since: UpdatesSinceData,
    /// "4 from muted sources, 2 from changes-only sources not shown."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden_line: Option<String>,
    /// "3 expired since you last looked."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expired_line: Option<String>,
    pub expired_count: u32,
    /// With `expired: true`: every expired update still in the mode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expired: Vec<UpdateExpiredData>,
    /// What letting go of this digest would do: "Let go of 31 updates from
    /// 12 sources; 2 also in To do stay there."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub let_go_line: Option<String>,
    /// Pass back to `LetGoDigest` so it acts only on what was shown.
    pub selection_token: String,
    /// Set when the digest is empty: the never-had-any or the clear line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_state: Option<String>,
    /// Sources seen in this account's Updates, and how many are muted.
    pub source_total: u32,
    pub muted_total: u32,
}

/// Returned by `Request::LetGoDigest`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdatesLetGoData {
    pub dry_run: bool,
    pub cut_at: chrono::DateTime<chrono::Utc>,
    /// "Let go of 31 updates from 12 sources; 2 also in To do stay there."
    pub line: String,
    pub message_count: u32,
    pub source_count: u32,
    /// Of those, hidden from the digest by tuning or past their window.
    pub hidden_count: u32,
    /// Threads To do also holds: they stay in the inbox.
    pub in_todo_count: u32,
    pub thread_ids: Vec<ThreadId>,
    pub message_ids: Vec<MessageId>,
    pub selection_token: String,
    /// One per thread, as `SetModeDone` reports it.
    pub items: Vec<ModeDoneOutcomeData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation_id: Option<String>,
    #[serde(default)]
    pub undo_unavailable: bool,
}

/// Returned by `Request::SetUpdateSource`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateSourceChangeData {
    pub dry_run: bool,
    pub account_id: AccountId,
    pub source_key: String,
    pub setting: UpdateSourceSettingData,
    /// What it was, for undo.
    pub prior: UpdateSourceSettingData,
    /// "Strava: only when something changes."
    pub copy: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_parse_loosely_and_round_trip() {
        for setting in UpdateSourceSettingData::ALL {
            assert_eq!(
                UpdateSourceSettingData::parse(setting.as_str()),
                Some(setting)
            );
        }
        assert_eq!(
            UpdateSourceSettingData::parse("Changes-only"),
            Some(UpdateSourceSettingData::ChangesOnly)
        );
        assert_eq!(
            UpdateSourceSettingData::parse("mute"),
            Some(UpdateSourceSettingData::Muted)
        );
        assert_eq!(UpdateSourceSettingData::parse("loud"), None);
    }
}
