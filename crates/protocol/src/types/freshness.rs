//! Freshness: is the local copy of my mail current? The newest arrival,
//! each account's sync health and the last few arrivals with the modes
//! they went to (`Request::GetFreshness`).
//!
//! Built from what the daemon already keeps: the messages table, the sync
//! runtime status and mode membership. Nothing here is stored.

use chrono::{DateTime, TimeZone, Utc};
use mxr_core::id::*;
use mxr_core::types::Address;
use serde::{Deserialize, Serialize};

use super::ModeMembershipData;

/// Arrivals returned when the request names no limit.
pub const FRESHNESS_DEFAULT_ARRIVALS: u32 = 5;
/// The most arrivals one request may ask for.
pub const FRESHNESS_MAX_ARRIVALS: u32 = 50;

/// How an account's sync is doing, least worrying first: the "All
/// accounts" scope shows the last one in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SyncHealthData {
    /// The last sync worked and was recent.
    Ok,
    /// The first sync is running; nothing has finished yet.
    Syncing,
    /// Never synced, and nothing running.
    Never,
    /// No error, but the last good sync is older than `stale_after_secs`.
    Stale,
    /// The provider asked us to wait (rate limited); a retry is scheduled.
    Paused,
    /// The last sync failed.
    Failing,
}

impl SyncHealthData {
    /// The calm states: nothing for the user to look at.
    pub const fn is_calm(self) -> bool {
        matches!(self, Self::Ok | Self::Syncing)
    }
}

/// What kind of failure stopped the last sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SyncErrorKindData {
    RateLimited,
    Auth,
    Offline,
    Provider,
    Store,
    Unknown,
}

impl SyncErrorKindData {
    /// From the daemon's stored failure class (`classify_sync_error`).
    pub fn from_failure_class(class: Option<&str>) -> Self {
        match class {
            Some("rate_limit") => Self::RateLimited,
            Some("auth") => Self::Auth,
            Some("network") => Self::Offline,
            Some("protocol") => Self::Provider,
            Some("store_index") => Self::Store,
            _ => Self::Unknown,
        }
    }
}

/// The last sync failure of one account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SyncErrorData {
    pub kind: SyncErrorKindData,
    /// The provider's own words, for the details view.
    pub message: String,
    /// When the daemon will try again, when it has scheduled a retry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub consecutive_failures: u32,
}

/// One account's freshness.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AccountFreshnessData {
    pub account_id: AccountId,
    pub account_name: String,
    /// How the account is named in a warning: "Gmail" or "Outlook" when it
    /// is the only account, else the account's own name, so two Gmail
    /// accounts never read the same.
    pub label: String,
    /// The newest message received in any mailbox, whatever its mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newest_message_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_ok_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_attempt_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub sync_in_progress: bool,
    /// As of `generated_at`. A client ticking forward turns `ok` into
    /// `stale` itself (`effective_health`).
    pub health: SyncHealthData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_error: Option<SyncErrorData>,
}

impl AccountFreshnessData {
    /// `health` at `now`: an `ok` account whose last good sync has aged
    /// past `stale_after_secs` since the reply was built is stale.
    pub fn effective_health(&self, now: DateTime<Utc>, stale_after_secs: u64) -> SyncHealthData {
        match (self.health, self.last_sync_ok_at) {
            (SyncHealthData::Ok, Some(at))
                if (now - at).num_seconds()
                    > i64::try_from(stale_after_secs).unwrap_or(i64::MAX) =>
            {
                SyncHealthData::Stale
            }
            (health, _) => health,
        }
    }
}

/// One message that arrived, with the modes its conversation went to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArrivalData {
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub from: Address,
    pub subject: String,
    pub received_at: DateTime<Utc>,
    /// The message is in the provider's inbox.
    pub in_inbox: bool,
    /// The mode its sender's rule sent it to (the sender decision, list
    /// headers, the address), with its one-word `tag`. Empty when it
    /// arrived out of the inbox or from someone screened out.
    pub modes: Vec<ModeMembershipData>,
}

/// Returned by `Request::GetFreshness`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FreshnessData {
    pub generated_at: DateTime<Utc>,
    /// The newest message received in scope, any mailbox, any mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newest_message_at: Option<DateTime<Utc>>,
    /// An account whose last good sync is older than this is stale.
    pub stale_after_secs: u64,
    /// The account in the worst sync state: the one the scope's line is
    /// about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worst_account_id: Option<AccountId>,
    pub accounts: Vec<AccountFreshnessData>,
    /// Newest first.
    pub arrivals: Vec<ArrivalData>,
}

impl FreshnessData {
    /// The account the scope's sync line is about, at `now`.
    pub fn worst_account(&self, now: DateTime<Utc>) -> Option<&AccountFreshnessData> {
        self.accounts
            .iter()
            .max_by_key(|account| account.effective_health(now, self.stale_after_secs))
    }
}

/// The words clients show for freshness, shared by the TUI and the CLI.
/// The web app keeps the same words in `features/freshness/copy.ts`.
pub mod freshness_copy {
    use super::*;

    /// "just now", "5m ago", "2h ago", "3d ago".
    pub fn ago(at: DateTime<Utc>, now: DateTime<Utc>) -> String {
        let seconds = (now - at).num_seconds().max(0);
        if seconds < 60 {
            "just now".to_string()
        } else if seconds < 3_600 {
            format!("{}m ago", seconds / 60)
        } else if seconds < 86_400 {
            format!("{}h ago", seconds / 3_600)
        } else {
            format!("{}d ago", seconds / 86_400)
        }
    }

    /// "Latest mail 5m ago", or "No mail yet".
    pub fn latest_mail(newest: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
        newest.map_or_else(
            || "No mail yet".to_string(),
            |at| format!("Latest mail {}", ago(at, now)),
        )
    }

    /// When a failing account tries again: "retrying now" while a retry
    /// runs or is due, "retrying 09:50" when one is scheduled.
    fn retrying<Tz: TimeZone>(
        account: &AccountFreshnessData,
        now: DateTime<Utc>,
        tz: &Tz,
    ) -> Option<String>
    where
        Tz::Offset: std::fmt::Display,
    {
        let retry_at = account
            .last_sync_error
            .as_ref()
            .and_then(|error| error.retry_at);
        if account.sync_in_progress || retry_at.is_some_and(|at| at <= now) {
            return Some("retrying now".to_string());
        }
        retry_at.map(|at| format!("retrying {}", at.with_timezone(tz).format("%H:%M")))
    }

    /// One account's sync line at `now`, with clock times in `tz`:
    /// "synced 2m ago", "Gmail paused: rate limited, retrying 09:50",
    /// "Last sync 2h ago".
    pub fn sync_line<Tz: TimeZone>(
        account: &AccountFreshnessData,
        stale_after_secs: u64,
        now: DateTime<Utc>,
        tz: &Tz,
    ) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        let last_ok = account.last_sync_ok_at.map(|at| ago(at, now));
        let label = &account.label;
        match account.effective_health(now, stale_after_secs) {
            SyncHealthData::Ok => match last_ok {
                Some(age) => format!("synced {age}"),
                None => "synced".to_string(),
            },
            SyncHealthData::Syncing => "first sync running".to_string(),
            SyncHealthData::Never => "not synced yet".to_string(),
            SyncHealthData::Stale => match last_ok {
                Some(age) => format!("Last sync {age}"),
                None => "not synced yet".to_string(),
            },
            SyncHealthData::Paused => match retrying(account, now, tz) {
                Some(retry) => format!("{label} paused: rate limited, {retry}"),
                None => format!("{label} paused: rate limited"),
            },
            SyncHealthData::Failing => {
                let kind = account
                    .last_sync_error
                    .as_ref()
                    .map_or(SyncErrorKindData::Unknown, |error| error.kind);
                let what = match kind {
                    SyncErrorKindData::Auth => {
                        return format!("{label} needs you to sign in again")
                    }
                    SyncErrorKindData::RateLimited => "rate limited",
                    SyncErrorKindData::Offline => "unreachable",
                    SyncErrorKindData::Provider
                    | SyncErrorKindData::Store
                    | SyncErrorKindData::Unknown => "sync failing",
                };
                match (retrying(account, now, tz), last_ok) {
                    (Some(retry), _) => format!("{label} {what}, {retry}"),
                    (None, Some(age)) => format!("{label} {what}, last sync {age}"),
                    (None, None) => format!("{label} {what}"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::freshness_copy::*;
    use super::*;
    use chrono::Duration;

    fn account(health: SyncHealthData, last_ok: Option<DateTime<Utc>>) -> AccountFreshnessData {
        AccountFreshnessData {
            account_id: AccountId::new(),
            account_name: "work".into(),
            label: "Gmail".into(),
            newest_message_at: None,
            last_sync_ok_at: last_ok,
            last_sync_attempt_at: last_ok,
            sync_in_progress: false,
            health,
            last_sync_error: None,
        }
    }

    #[test]
    fn ages_read_in_the_largest_whole_unit() {
        let now = Utc::now();
        assert_eq!(ago(now - Duration::seconds(30), now), "just now");
        assert_eq!(ago(now - Duration::minutes(5), now), "5m ago");
        assert_eq!(ago(now - Duration::hours(2), now), "2h ago");
        assert_eq!(ago(now - Duration::days(3), now), "3d ago");
        // A clock a little ahead never reads as the future.
        assert_eq!(ago(now + Duration::minutes(2), now), "just now");
        assert_eq!(latest_mail(None, now), "No mail yet");
    }

    #[test]
    fn an_ok_account_turns_stale_as_time_passes() {
        let built = Utc::now();
        let ok = account(SyncHealthData::Ok, Some(built - Duration::minutes(1)));
        assert_eq!(ok.effective_health(built, 900), SyncHealthData::Ok);
        let later = built + Duration::hours(2);
        assert_eq!(ok.effective_health(later, 900), SyncHealthData::Stale);
        assert_eq!(sync_line(&ok, 900, later, &Utc), "Last sync 2h ago");
        assert_eq!(sync_line(&ok, 900, built, &Utc), "synced 1m ago");
    }

    #[test]
    fn warnings_name_the_provider_and_what_happens_next() {
        let now = Utc.with_ymd_and_hms(2026, 10, 7, 9, 42, 0).unwrap();
        let mut paused = account(SyncHealthData::Paused, Some(now - Duration::minutes(3)));
        paused.last_sync_error = Some(SyncErrorData {
            kind: SyncErrorKindData::RateLimited,
            message: "Rate limited".into(),
            retry_at: Some(now + Duration::minutes(8)),
            consecutive_failures: 1,
        });
        assert_eq!(
            sync_line(&paused, 900, now, &Utc),
            "Gmail paused: rate limited, retrying 09:50"
        );

        let mut auth = account(SyncHealthData::Failing, None);
        auth.last_sync_error = Some(SyncErrorData {
            kind: SyncErrorKindData::Auth,
            message: "oauth".into(),
            retry_at: None,
            consecutive_failures: 2,
        });
        assert_eq!(
            sync_line(&auth, 900, now, &Utc),
            "Gmail needs you to sign in again"
        );

        let mut offline = account(SyncHealthData::Failing, Some(now - Duration::minutes(12)));
        offline.last_sync_error = Some(SyncErrorData {
            kind: SyncErrorKindData::Offline,
            message: "dns".into(),
            retry_at: None,
            consecutive_failures: 1,
        });
        assert_eq!(
            sync_line(&offline, 900, now, &Utc),
            "Gmail unreachable, last sync 12m ago"
        );
        if let Some(error) = offline.last_sync_error.as_mut() {
            error.retry_at = Some(now + Duration::minutes(1));
        }
        assert_eq!(
            sync_line(&offline, 900, now, &Utc),
            "Gmail unreachable, retrying 09:43"
        );
    }

    #[test]
    fn a_retry_that_is_running_says_so_and_stays_a_warning() {
        let now = Utc::now();
        let mut retrying = account(SyncHealthData::Failing, Some(now - Duration::minutes(12)));
        retrying.sync_in_progress = true;
        retrying.last_sync_error = Some(SyncErrorData {
            kind: SyncErrorKindData::Offline,
            message: "dns".into(),
            retry_at: Some(now - Duration::minutes(1)),
            consecutive_failures: 2,
        });
        assert_eq!(
            sync_line(&retrying, 900, now, &Utc),
            "Gmail unreachable, retrying now"
        );
        assert!(!retrying.effective_health(now, 900).is_calm());
    }

    #[test]
    fn the_worst_account_speaks_for_all_accounts() {
        let now = Utc::now();
        let ok = account(SyncHealthData::Ok, Some(now));
        let failing = account(SyncHealthData::Failing, Some(now));
        let data = FreshnessData {
            generated_at: now,
            newest_message_at: None,
            stale_after_secs: 900,
            worst_account_id: None,
            accounts: vec![ok, failing.clone()],
            arrivals: vec![],
        };
        assert_eq!(
            data.worst_account(now).map(|a| &a.account_id),
            Some(&failing.account_id)
        );
    }
}
