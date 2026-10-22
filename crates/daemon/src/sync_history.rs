//! Summarises each account's recent `sync_log` rows for `mxr doctor`: how long
//! finished syncs took, whether that is drifting, and which failure messages
//! keep coming back. Error text is normalised so one failure that differs only
//! in ids, numbers, timestamps or addresses groups together. Message bodies
//! are never read here.

use chrono::{DateTime, Utc};
use mxr_core::AccountId;
use mxr_protocol::{AccountSyncHistory, RecurringSyncError, SyncDurationTrend};
use mxr_store::{Store, SyncLogEntry, SyncStatus};
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Newest `sync_log` rows read per account.
pub(crate) const WINDOW: u32 = 50;
/// The trend compares the newest this-many finished syncs with the rest of the
/// window.
const TREND_RECENT: usize = 10;
const TREND_MIN_FINISHED: usize = 5;
/// "Slower" or "faster" needs the recent median to move at least this share of
/// the prior median, and by at least `TREND_MIN_DIFF_SECS`. The absolute floor
/// stops whole-second noise (0s to 1s) from reading as a trend.
const TREND_RATIO_PERCENT: u64 = 50;
const TREND_MIN_DIFF_SECS: u64 = 5;
const RECURRING_MIN_COUNT: u32 = 2;
const RECURRING_MAX_GROUPS: usize = 5;
const ERROR_CLIP_CHARS: usize = 160;

pub(crate) async fn collect(store: &Store) -> Result<Vec<AccountSyncHistory>, sqlx::Error> {
    let mut summaries = Vec::new();
    for account in store.list_accounts().await? {
        let rows = store.list_recent_sync_logs(&account.id, WINDOW).await?;
        summaries.push(summarize(account.id, account.name, &rows));
    }
    Ok(summaries)
}

/// `rows` are newest first, as `list_recent_sync_logs` returns them.
pub(crate) fn summarize(
    account_id: AccountId,
    account_name: String,
    rows: &[SyncLogEntry],
) -> AccountSyncHistory {
    // Unfinished rows are excluded from durations entirely: a run with no end
    // time says nothing about how long a sync takes.
    let durations: Vec<u64> = rows.iter().filter_map(duration_secs).collect();
    let recent_len = durations.len().min(TREND_RECENT);
    let (recent, prior) = durations.split_at(recent_len);

    AccountSyncHistory {
        account_id,
        account_name,
        recent_syncs: rows.len() as u32,
        failed_syncs: rows
            .iter()
            .filter(|row| row.status == SyncStatus::Error)
            .count() as u32,
        unfinished_syncs: rows.iter().filter(|row| row.finished_at.is_none()).count() as u32,
        finished_syncs: durations.len() as u32,
        median_duration_secs: median_of(&durations),
        slowest_duration_secs: durations.iter().max().copied(),
        latest_duration_secs: durations.first().copied(),
        trend: trend(recent, prior),
        recurring_errors: recurring_errors(rows),
    }
}

fn duration_secs(row: &SyncLogEntry) -> Option<u64> {
    row.finished_at
        .map(|finished| (finished - row.started_at).num_seconds().max(0) as u64)
}

fn median_of(values: &[u64]) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    Some(if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2
    })
}

/// `recent` and `prior` are finished durations, newest first.
pub(crate) fn trend(recent: &[u64], prior: &[u64]) -> Option<SyncDurationTrend> {
    if recent.len() < TREND_MIN_FINISHED || prior.len() < TREND_MIN_FINISHED {
        return None;
    }
    let now = median_of(recent)?;
    let before = median_of(prior)?;
    if now.abs_diff(before) < TREND_MIN_DIFF_SECS {
        return Some(SyncDurationTrend::Steady);
    }
    // Cross-multiplied so the comparison stays in integers.
    if now * 100 >= before * (100 + TREND_RATIO_PERCENT) {
        Some(SyncDurationTrend::Slower)
    } else if now * 100 <= before * (100 - TREND_RATIO_PERCENT) {
        Some(SyncDurationTrend::Faster)
    } else {
        Some(SyncDurationTrend::Steady)
    }
}

fn recurring_errors(rows: &[SyncLogEntry]) -> Vec<RecurringSyncError> {
    let mut groups: HashMap<String, (u32, DateTime<Utc>)> = HashMap::new();
    for row in rows.iter().filter(|row| row.status == SyncStatus::Error) {
        let Some(message) = row
            .error_message
            .as_deref()
            .map(str::trim)
            .filter(|message| !message.is_empty())
        else {
            continue;
        };
        let entry = groups
            .entry(normalise_error(message))
            .or_insert((0, row.started_at));
        entry.0 += 1;
        entry.1 = entry.1.max(row.started_at);
    }

    let mut recurring: Vec<RecurringSyncError> = groups
        .into_iter()
        .filter(|(_, (count, _))| *count >= RECURRING_MIN_COUNT)
        .map(|(message, (count, last_seen))| RecurringSyncError {
            message,
            count,
            last_seen_at: last_seen.to_rfc3339(),
        })
        .collect();
    recurring.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| b.last_seen_at.cmp(&a.last_seen_at))
            .then_with(|| a.message.cmp(&b.message))
    });
    recurring.truncate(RECURRING_MAX_GROUPS);
    recurring
}

/// Replaces the parts of a provider or transport error that vary between
/// occurrences of the same failure, collapses whitespace, and clips the result.
/// Order matters: timestamps and quoted values are replaced before the bare
/// number and hex rules can split them.
pub(crate) fn normalise_error(message: &str) -> String {
    let mut text = message.split_whitespace().collect::<Vec<_>>().join(" ");
    for (pattern, replacement) in normalisation_rules() {
        text = pattern.replace_all(&text, *replacement).into_owned();
    }
    clip(text)
}

fn normalisation_rules() -> &'static [(Regex, &'static str)] {
    static RULES: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    RULES.get_or_init(|| {
        [
            (r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", "<email>"),
            (
                r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:?\d{2})?",
                "<time>",
            ),
            (r#""[^"]*""#, "<value>"),
            (r"\b0x[0-9a-fA-F]+\b", "<hex>"),
            (r"\b[0-9a-fA-F]{8,}\b", "<hex>"),
            (r"\d+", "<n>"),
        ]
        .into_iter()
        .map(|(pattern, replacement)| {
            (
                Regex::new(pattern).expect("normalisation pattern is a valid regex"),
                replacement,
            )
        })
        .collect()
    })
}

fn clip(text: String) -> String {
    if text.chars().count() <= ERROR_CLIP_CHARS {
        return text;
    }
    let mut clipped: String = text.chars().take(ERROR_CLIP_CHARS - 3).collect();
    clipped.push_str("...");
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + secs, 0).unwrap()
    }

    fn finished(start: i64, secs: i64, status: SyncStatus, error: Option<&str>) -> SyncLogEntry {
        SyncLogEntry {
            id: start,
            account_id: AccountId::new(),
            started_at: at(start),
            finished_at: Some(at(start + secs)),
            status,
            messages_synced: 0,
            error_message: error.map(str::to_string),
        }
    }

    fn unfinished(start: i64) -> SyncLogEntry {
        SyncLogEntry {
            id: start,
            account_id: AccountId::new(),
            started_at: at(start),
            finished_at: None,
            status: SyncStatus::Running,
            messages_synced: 0,
            error_message: None,
        }
    }

    #[test]
    fn normalise_replaces_volatile_parts() {
        assert_eq!(
            normalise_error("IMAP timeout after 30 seconds for uid 4417"),
            "IMAP timeout after <n> seconds for uid <n>"
        );
        assert_eq!(
            normalise_error("fetch failed at 2026-10-21T10:02:11.532Z (retry 2)"),
            "fetch failed at <time> (retry <n>)"
        );
        assert_eq!(
            normalise_error("unknown message \"<abc@mail.example>\" in 0xdeadbeef"),
            "unknown message <value> in <hex>"
        );
        assert_eq!(
            normalise_error("token 9f8e7d6c5b4a3210 rejected by ana@example.com"),
            "token <hex> rejected by <email>"
        );
    }

    #[test]
    fn normalise_collapses_whitespace_and_clips_long_text() {
        assert_eq!(normalise_error("a\n\n  b\tc  "), "a b c");
        let long = "x".repeat(400);
        let clipped = normalise_error(&long);
        assert_eq!(clipped.chars().count(), ERROR_CLIP_CHARS);
        assert!(clipped.ends_with("..."));
    }

    #[test]
    fn trend_needs_five_finished_in_each_window() {
        let five = [10, 10, 10, 10, 10];
        assert_eq!(trend(&five[..4], &[1; 20]), None);
        assert_eq!(trend(&five, &five[..4]), None);
        assert_eq!(trend(&[30; 5], &[10; 5]), Some(SyncDurationTrend::Slower));
    }

    #[test]
    fn trend_uses_fifty_percent_and_five_second_floor() {
        // 15 is exactly 1.5x 10, so it is slower.
        assert_eq!(trend(&[15; 5], &[10; 5]), Some(SyncDurationTrend::Slower));
        // 14 is under 1.5x, so steady.
        assert_eq!(trend(&[14; 5], &[10; 5]), Some(SyncDurationTrend::Steady));
        // 5 is exactly half of 10 and 5 seconds apart: faster.
        assert_eq!(trend(&[5; 5], &[10; 5]), Some(SyncDurationTrend::Faster));
        // 0s to 4s is 4x but under the absolute floor.
        assert_eq!(trend(&[4; 5], &[0; 5]), Some(SyncDurationTrend::Steady));
        // 0s to 5s crosses the floor.
        assert_eq!(trend(&[5; 5], &[0; 5]), Some(SyncDurationTrend::Slower));
    }

    #[test]
    fn summarize_excludes_unfinished_rows_from_durations() {
        let rows = vec![
            unfinished(100),
            finished(90, 4, SyncStatus::Success, None),
            finished(80, 8, SyncStatus::Error, Some("boom")),
            finished(70, 6, SyncStatus::Success, None),
        ];
        let summary = summarize(AccountId::new(), "Work".into(), &rows);
        assert_eq!(summary.recent_syncs, 4);
        assert_eq!(summary.failed_syncs, 1);
        assert_eq!(summary.unfinished_syncs, 1);
        assert_eq!(summary.finished_syncs, 3);
        assert_eq!(summary.latest_duration_secs, Some(4));
        assert_eq!(summary.slowest_duration_secs, Some(8));
        assert_eq!(summary.median_duration_secs, Some(6));
        assert_eq!(summary.trend, None);
    }

    #[test]
    fn median_of_even_count_averages_middle_pair() {
        assert_eq!(median_of(&[1, 2, 3, 10]), Some(2));
        assert_eq!(median_of(&[]), None);
    }

    #[test]
    fn recurring_errors_group_and_cap_at_five() {
        let mut rows = Vec::new();
        // Group A appears 3 times with different ids and numbers.
        for (i, n) in [11, 22, 33].iter().enumerate() {
            rows.push(finished(
                200 + i as i64,
                1,
                SyncStatus::Error,
                Some(format!("timeout on uid {n}").as_str()),
            ));
        }
        // A single occurrence is not recurring.
        rows.push(finished(300, 1, SyncStatus::Error, Some("one-off")));
        // Successful rows never group.
        rows.push(finished(301, 1, SyncStatus::Success, Some("ignored")));
        // Six more distinct recurring groups; only five may show.
        for group in 0..6 {
            for rep in 0..2 {
                rows.push(finished(
                    400 + group * 10 + rep,
                    1,
                    SyncStatus::Error,
                    Some(format!("kind {} failed", "z".repeat(group as usize + 1)).as_str()),
                ));
            }
        }
        rows.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        let summary = summarize(AccountId::new(), "Work".into(), &rows);
        assert_eq!(summary.recurring_errors.len(), RECURRING_MAX_GROUPS);
        assert_eq!(summary.recurring_errors[0].message, "timeout on uid <n>");
        assert_eq!(summary.recurring_errors[0].count, 3);
        assert!(summary
            .recurring_errors
            .iter()
            .all(|group| group.count >= RECURRING_MIN_COUNT));
    }
}
