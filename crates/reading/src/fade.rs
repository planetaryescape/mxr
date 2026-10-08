//! Each source's items fade after twice its usual gap between issues,
//! clamped to 2 to 14 days (blueprint 22, "Items have a relevancy window").
//! A daily lasts two days, a weekly fourteen. Later never fades; that rule
//! lives with the shelf, not here.

use chrono::{DateTime, Duration, Utc};

pub const MIN_WINDOW_DAYS: i64 = 2;
pub const MAX_WINDOW_DAYS: i64 = 14;
/// A source with fewer than two issues has no gap yet. A week is the
/// middle of the range: judgement, not a finding.
pub const DEFAULT_WINDOW_DAYS: i64 = 7;
/// Issues closer together than this are one send split in two (a resend,
/// a correction), not the source's rhythm.
const SAME_SEND: Duration = Duration::hours(1);

/// Where a source's window came from, for the why line and JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSource {
    /// Twice the median gap between its issues.
    Cadence,
    /// Too few issues to know: the default.
    Default,
}

impl WindowSource {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Cadence => "cadence",
            Self::Default => "default",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceWindow {
    /// How long an item from this source stays in the edition.
    pub window: Duration,
    /// The median gap between issues, when there are two or more.
    pub median_gap: Option<Duration>,
    pub source: WindowSource,
}

/// The window for a source whose issues arrived at `dates`, in any order.
pub fn source_window(dates: &[DateTime<Utc>]) -> SourceWindow {
    let mut sorted = dates.to_vec();
    sorted.sort_unstable();
    let mut gaps: Vec<Duration> = sorted
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|gap| *gap >= SAME_SEND)
        .collect();
    if gaps.is_empty() {
        return SourceWindow {
            window: Duration::days(DEFAULT_WINDOW_DAYS),
            median_gap: None,
            source: WindowSource::Default,
        };
    }
    gaps.sort_unstable();
    let mid = gaps.len() / 2;
    let median = if gaps.len().is_multiple_of(2) {
        (gaps[mid - 1] + gaps[mid]) / 2
    } else {
        gaps[mid]
    };
    let window = (median * 2).clamp(
        Duration::days(MIN_WINDOW_DAYS),
        Duration::days(MAX_WINDOW_DAYS),
    );
    SourceWindow {
        window,
        median_gap: Some(median),
        source: WindowSource::Cadence,
    }
}

/// When an item that arrived at `arrived` leaves the edition.
pub fn expires_at(arrived: DateTime<Utc>, window: &SourceWindow) -> DateTime<Utc> {
    arrived + window.window
}

/// In its last day an item is in the Fading band.
pub fn is_fading(now: DateTime<Utc>, expires_at: DateTime<Utc>) -> bool {
    now < expires_at && now >= expires_at - Duration::days(1)
}

pub fn is_expired(now: DateTime<Utc>, expires_at: DateTime<Utc>) -> bool {
    now >= expires_at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(days: i64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + Duration::days(20_000) + Duration::days(days)
    }

    #[test]
    fn a_weekly_lasts_fourteen_days_and_a_daily_two() {
        let weekly: Vec<_> = (0..6).map(|week| at(week * 7)).collect();
        let window = source_window(&weekly);
        assert_eq!(window.window, Duration::days(14));
        assert_eq!(window.median_gap, Some(Duration::days(7)));
        assert_eq!(window.source, WindowSource::Cadence);

        let daily: Vec<_> = (0..10).map(at).collect();
        assert_eq!(source_window(&daily).window, Duration::days(2));
    }

    #[test]
    fn the_window_is_clamped_at_both_ends() {
        let hourly: Vec<_> = (0..20).map(|h| at(0) + Duration::hours(h * 2)).collect();
        assert_eq!(
            source_window(&hourly).window,
            Duration::days(MIN_WINDOW_DAYS)
        );
        let monthly: Vec<_> = (0..4).map(|m| at(m * 30)).collect();
        assert_eq!(
            source_window(&monthly).window,
            Duration::days(MAX_WINDOW_DAYS)
        );
    }

    #[test]
    fn twice_weekly_is_twice_the_median_gap() {
        // Gaps of 3, 4, 3, 4, 3 days: the median is 3, the window 6.
        let dates = [at(0), at(3), at(7), at(10), at(14), at(17)];
        assert_eq!(source_window(&dates).window, Duration::days(6));
    }

    #[test]
    fn one_issue_or_one_send_split_in_two_takes_the_default() {
        assert_eq!(source_window(&[at(0)]).source, WindowSource::Default);
        assert_eq!(
            source_window(&[]).window,
            Duration::days(DEFAULT_WINDOW_DAYS)
        );
        let resend = [at(0), at(0) + Duration::minutes(5)];
        assert_eq!(source_window(&resend).source, WindowSource::Default);
    }

    #[test]
    fn the_last_day_fades_and_then_it_is_gone() {
        let window = source_window(&[at(0), at(7)]);
        let expires = expires_at(at(7), &window);
        assert_eq!(expires, at(21));
        assert!(!is_fading(at(19), expires));
        assert!(is_fading(at(20) + Duration::hours(1), expires));
        assert!(!is_expired(at(20) + Duration::hours(23), expires));
        assert!(is_expired(at(21), expires));
        assert!(!is_fading(at(21), expires));
    }
}
