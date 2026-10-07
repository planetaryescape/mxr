//! Each source's window and how you read it.
//!
//! The window comes from the source's last 20 stored issues. The counts
//! come from local engagement only, and only for issues that arrived after
//! you first opened Reading: before that mxr saw nothing, so it claims
//! nothing ("You opened 0 of the last 11 issues" is always true of what it
//! watched). The provider's read flag is not used: letting go marks mail
//! read at the provider, so it would count a skipped issue as opened.

use super::Issue;
use crate::handler::HandlerError;
use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::AccountId;
use mxr_protocol::{ReadingSourceData, ReadingUnsubscribeData};
use mxr_reading::edition::SourceStats;
use mxr_reading::fade::{self, SourceWindow};
use std::collections::HashMap;

/// Issues read per source for its window and counts.
const ISSUES_PER_SOURCE: u32 = 20;
/// Counts cover issues from the last 90 days.
const COUNT_WINDOW: Duration = Duration::days(90);

pub(in crate::handler) struct SourceInfo {
    pub data: ReadingSourceData,
    pub window: SourceWindow,
    pub stats: SourceStats,
}

impl SourceInfo {
    /// "you read 9 of 10", "you opened 3 of 8", "new"; nothing before
    /// there is anything to say.
    pub fn engagement_line(&self) -> Option<String> {
        let s = &self.stats;
        if s.is_new() {
            return Some("new source".to_string());
        }
        if s.issues == 0 {
            return None;
        }
        Some(if s.finished > 0 {
            format!("you read {} of {}", s.finished, s.issues)
        } else {
            format!("you opened {} of {}", s.opened, s.issues)
        })
    }
}

fn days(duration: Duration) -> f64 {
    duration.num_minutes() as f64 / (24.0 * 60.0)
}

/// Every source of a set of issues, by account and lowercase sender.
pub(in crate::handler) struct Sources(HashMap<(AccountId, String), SourceInfo>);

impl Sources {
    pub async fn load(
        state: &AppState,
        issues: &[&Issue],
        tracking_since: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Self, HandlerError> {
        // The newest issue of each source names it and says how to leave.
        let mut newest: HashMap<(AccountId, String), &Issue> = HashMap::new();
        for issue in issues {
            let key = (issue.account_id.clone(), issue.sender.clone());
            match newest.get(&key) {
                Some(seen) if seen.date >= issue.date => {}
                _ => {
                    newest.insert(key, issue);
                }
            }
        }
        let mut by_account: HashMap<AccountId, Vec<String>> = HashMap::new();
        for (account, sender) in newest.keys() {
            by_account
                .entry(account.clone())
                .or_default()
                .push(sender.clone());
        }
        let tracking = state.activity.is_enabled();
        let mut out = HashMap::new();
        for (account, senders) in by_account {
            // Stored addresses keep their case; ask for every spelling seen.
            let mut spellings: Vec<String> = issues
                .iter()
                .filter(|issue| issue.account_id == account)
                .map(|issue| issue.raw_sender.clone())
                .collect();
            spellings.sort_unstable();
            spellings.dedup();
            let history = state
                .store
                .reading_source_issues(&account, &spellings, ISSUES_PER_SOURCE)
                .await?;
            let totals = state
                .store
                .reading_source_totals(&account, &spellings)
                .await?;
            let prefs = state.store.reading_source_prefs(&account).await?;
            let mut dates: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();
            let mut stats: HashMap<String, SourceStats> = HashMap::new();
            for issue in &history {
                let sender = issue.from_email.to_ascii_lowercase();
                dates.entry(sender.clone()).or_default().push(issue.date);
                let counted = tracking_since.is_some_and(|since| issue.date >= since)
                    && issue.date >= now - COUNT_WINDOW;
                if counted {
                    let entry = stats.entry(sender).or_default();
                    entry.issues += 1;
                    entry.opened += u32::from(issue.opened);
                    entry.finished += u32::from(issue.finished);
                }
            }
            let mut total_by_sender: HashMap<String, u32> = HashMap::new();
            for (spelling, count) in totals {
                *total_by_sender
                    .entry(spelling.to_ascii_lowercase())
                    .or_default() += count;
            }
            for sender in senders {
                let Some(issue) = newest.get(&(account.clone(), sender.clone())) else {
                    continue;
                };
                let window = fade::source_window(dates.get(&sender).map_or(&[][..], Vec::as_slice));
                let mut source_stats = stats.get(&sender).copied().unwrap_or_default();
                source_stats.total_issues = total_by_sender.get(&sender).copied().unwrap_or(1);
                let pref = prefs.get(&sender).copied().unwrap_or_default();
                let data = ReadingSourceData {
                    account_id: account.clone(),
                    sender_email: sender.clone(),
                    name: issue.source_name.clone(),
                    issues: source_stats.issues,
                    opened: source_stats.opened,
                    finished: source_stats.finished,
                    evidence: if source_stats.issues == 0 {
                        "mxr hasn't seen you read any of its issues yet".to_string()
                    } else {
                        source_stats.evidence()
                    },
                    affinity: source_stats.affinity(),
                    window_days: days(window.window),
                    median_gap_days: window.median_gap.map(days),
                    new_source: source_stats.is_new(),
                    original_layout: pref.original_layout,
                    unsubscribe: ReadingUnsubscribeData::from(&issue.unsubscribe),
                    suggest_unsubscribe: tracking
                        && !pref.unsubscribe_offer_dismissed
                        && source_stats.suggests_unsubscribe(),
                };
                out.insert(
                    (account.clone(), sender),
                    SourceInfo {
                        data,
                        window,
                        stats: source_stats,
                    },
                );
            }
        }
        Ok(Self(out))
    }

    pub fn get(&self, account: &AccountId, sender: &str) -> Option<&SourceInfo> {
        self.0.get(&(account.clone(), sender.to_string()))
    }

    /// Best read first, then by name.
    pub fn ranked(&self) -> Vec<ReadingSourceData> {
        let mut sources: Vec<ReadingSourceData> =
            self.0.values().map(|info| info.data.clone()).collect();
        sources.sort_by(|a, b| {
            b.affinity
                .partial_cmp(&a.affinity)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        sources
    }
}
