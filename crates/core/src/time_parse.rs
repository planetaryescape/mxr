//! Conversational time parser for snooze, send-later, and reminders.
//!
//! Accepts a small set of human-friendly forms and returns an absolute
//! UTC instant. Designed to be unsurprising and explicit about
//! ambiguity rather than to handle every edge case — power users can
//! always supply RFC3339 if the heuristics get in their way.
//!
//! Forms accepted:
//!
//! * `"in N<unit>"` — `"in 2h"`, `"in 5d"`, `"in 30m"`. Units: m / h / d / w.
//! * `"tomorrow"` and `"tomorrow <time>"` — next calendar day, default 09:00.
//! * `"today <time>"` — same calendar day; must be in the future.
//! * `"<weekday>"` and `"<weekday> <time>"` — next occurrence of the named
//!   weekday (today doesn't count even if it's the same day-of-week
//!   earlier in the day; "monday" on Monday is "next Monday").
//! * RFC3339 (`"2026-06-01T15:00:00Z"`).
//!
//! Time formats: `"9am"`, `"5pm"`, `"17:00"`, `"09:30am"`. Ambiguity in
//! 12h forms (`"12am"`, `"12pm"`) follows the common convention:
//! `12am = 00:00`, `12pm = 12:00`.
//!
//! All parsing is case-insensitive and tolerates extra whitespace.

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeParseError {
    /// Empty string after trimming.
    Empty,
    /// Parsed time is in the past relative to the supplied `now`.
    InPast,
    /// Couldn't parse with any known form. Carries the offending input
    /// for error messages.
    Unknown(String),
}

impl std::fmt::Display for TimeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty input"),
            Self::InPast => write!(f, "time is in the past"),
            Self::Unknown(input) => write!(f, "couldn't parse `{input}` as a time"),
        }
    }
}

impl std::error::Error for TimeParseError {}

const DEFAULT_HOUR: u32 = 9;
const DEFAULT_MINUTE: u32 = 0;

/// Parse a conversational time expression into an absolute UTC instant.
/// `now` anchors all relative forms — pass `Utc::now()` in production
/// and a fixed value in tests.
pub fn parse_relative_time(
    input: &str,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, TimeParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(TimeParseError::Empty);
    }
    let lower = trimmed.to_lowercase();

    // RFC3339 first — it's unambiguous and reasonably easy to detect by
    // the presence of `T` plus `:`.
    if let Ok(absolute) = DateTime::parse_from_rfc3339(trimmed) {
        let absolute_utc = absolute.with_timezone(&Utc);
        return ensure_future(absolute_utc, now);
    }

    if let Some(rest) = lower.strip_prefix("in ") {
        let parsed = parse_in_duration(rest.trim())?;
        let result = now + parsed;
        return ensure_future(result, now);
    }

    if let Some(rest) = lower.strip_prefix("tomorrow") {
        let time = parse_optional_time(rest.trim())?;
        let date = (now + chrono::Duration::days(1)).date_naive();
        return build_at(date, time, now);
    }

    if let Some(rest) = lower.strip_prefix("today") {
        let rest = rest.trim();
        if rest.is_empty() {
            // "today" alone is ambiguous — refuse rather than guess.
            return Err(TimeParseError::Unknown(input.to_string()));
        }
        let time = parse_time(rest)?;
        let date = now.date_naive();
        return build_at(date, time, now);
    }

    if let Some((weekday, rest)) = strip_weekday_prefix(&lower) {
        let time = parse_optional_time(rest.trim())?;
        let date = next_occurrence(now, weekday);
        return build_at(date, time, now);
    }

    Err(TimeParseError::Unknown(input.to_string()))
}

fn ensure_future(
    candidate: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, TimeParseError> {
    if candidate <= now {
        Err(TimeParseError::InPast)
    } else {
        Ok(candidate)
    }
}

fn build_at(
    date: NaiveDate,
    time: NaiveTime,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, TimeParseError> {
    let naive = date.and_time(time);
    let candidate = Utc.from_utc_datetime(&naive);
    ensure_future(candidate, now)
}

fn parse_in_duration(input: &str) -> Result<chrono::Duration, TimeParseError> {
    if input.is_empty() {
        return Err(TimeParseError::Unknown(format!("in {input}")));
    }
    let (num_part, unit_char) = input.split_at(input.len() - 1);
    let num: i64 = num_part
        .trim()
        .parse()
        .map_err(|_| TimeParseError::Unknown(format!("in {input}")))?;
    if num <= 0 {
        return Err(TimeParseError::InPast);
    }
    let duration = match unit_char {
        "m" => chrono::Duration::minutes(num),
        "h" => chrono::Duration::hours(num),
        "d" => chrono::Duration::days(num),
        "w" => chrono::Duration::weeks(num),
        _ => return Err(TimeParseError::Unknown(format!("in {input}"))),
    };
    Ok(duration)
}

fn parse_optional_time(input: &str) -> Result<NaiveTime, TimeParseError> {
    if input.is_empty() {
        return NaiveTime::from_hms_opt(DEFAULT_HOUR, DEFAULT_MINUTE, 0)
            .ok_or_else(|| TimeParseError::Unknown(input.to_string()));
    }
    parse_time(input)
}

fn parse_time(input: &str) -> Result<NaiveTime, TimeParseError> {
    let normalized = input.trim().replace(' ', "");
    // 12h forms: "9am", "5pm", "12:30am"
    if let Some(stripped) = normalized.strip_suffix("am") {
        return parse_12h(stripped, false);
    }
    if let Some(stripped) = normalized.strip_suffix("pm") {
        return parse_12h(stripped, true);
    }
    // 24h: "17", "17:00", "09:30"
    let (h, m) = match normalized.split_once(':') {
        Some((h, m)) => (h, m),
        None => (normalized.as_str(), "0"),
    };
    let hour: u32 = h
        .parse()
        .map_err(|_| TimeParseError::Unknown(input.to_string()))?;
    let minute: u32 = m
        .parse()
        .map_err(|_| TimeParseError::Unknown(input.to_string()))?;
    if hour > 23 || minute > 59 {
        return Err(TimeParseError::Unknown(input.to_string()));
    }
    NaiveTime::from_hms_opt(hour, minute, 0)
        .ok_or_else(|| TimeParseError::Unknown(input.to_string()))
}

fn parse_12h(stem: &str, is_pm: bool) -> Result<NaiveTime, TimeParseError> {
    let (h_str, m_str) = match stem.split_once(':') {
        Some((h, m)) => (h, m),
        None => (stem, "0"),
    };
    let mut hour: u32 = h_str
        .parse()
        .map_err(|_| TimeParseError::Unknown(stem.to_string()))?;
    let minute: u32 = m_str
        .parse()
        .map_err(|_| TimeParseError::Unknown(stem.to_string()))?;
    if !(1..=12).contains(&hour) || minute > 59 {
        return Err(TimeParseError::Unknown(stem.to_string()));
    }
    // 12am = 00:00, 12pm = 12:00. Other hours: pm shifts +12.
    if hour == 12 {
        hour = if is_pm { 12 } else { 0 };
    } else if is_pm {
        hour += 12;
    }
    NaiveTime::from_hms_opt(hour, minute, 0)
        .ok_or_else(|| TimeParseError::Unknown(stem.to_string()))
}

fn strip_weekday_prefix(input: &str) -> Option<(Weekday, &str)> {
    const WEEKDAYS: &[(&str, Weekday)] = &[
        ("monday", Weekday::Mon),
        ("tuesday", Weekday::Tue),
        ("wednesday", Weekday::Wed),
        ("thursday", Weekday::Thu),
        ("friday", Weekday::Fri),
        ("saturday", Weekday::Sat),
        ("sunday", Weekday::Sun),
        // Three-letter abbreviations
        ("mon", Weekday::Mon),
        ("tue", Weekday::Tue),
        ("wed", Weekday::Wed),
        ("thu", Weekday::Thu),
        ("fri", Weekday::Fri),
        ("sat", Weekday::Sat),
        ("sun", Weekday::Sun),
    ];
    for (name, weekday) in WEEKDAYS {
        if let Some(rest) = input.strip_prefix(name) {
            // Must be followed by end-of-input or whitespace, so
            // "tuesday" doesn't match the start of an unrelated word.
            if rest.is_empty() || rest.starts_with(' ') {
                return Some((*weekday, rest));
            }
        }
    }
    None
}

/// Find the next occurrence of `target` starting from the day AFTER
/// `now`'s date. "Monday" on a Monday means "next Monday" — never today,
/// since the user invoking a snooze typically means the future occurrence.
fn next_occurrence(now: DateTime<Utc>, target: Weekday) -> NaiveDate {
    let today = now.date_naive();
    for offset in 1..=7 {
        let candidate = today + chrono::Duration::days(offset);
        if candidate.weekday() == target {
            return candidate;
        }
    }
    // Unreachable: in 7 days each weekday occurs exactly once.
    today
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    fn anchor() -> DateTime<Utc> {
        // A fixed Tuesday, May 7 2024, 14:00 UTC. Tuesdays are useful
        // for testing because every weekday name then exercises a
        // distinct future-day calculation.
        Utc.with_ymd_and_hms(2024, 5, 7, 14, 0, 0).unwrap()
    }

    #[test]
    fn parses_in_n_minutes() {
        let got = parse_relative_time("in 30m", anchor()).unwrap();
        assert_eq!(got, anchor() + chrono::Duration::minutes(30));
    }

    #[test]
    fn parses_in_n_hours() {
        let got = parse_relative_time("in 2h", anchor()).unwrap();
        assert_eq!(got, anchor() + chrono::Duration::hours(2));
    }

    #[test]
    fn parses_in_n_days() {
        let got = parse_relative_time("in 5d", anchor()).unwrap();
        assert_eq!(got, anchor() + chrono::Duration::days(5));
    }

    #[test]
    fn parses_in_n_weeks() {
        let got = parse_relative_time("in 2w", anchor()).unwrap();
        assert_eq!(got, anchor() + chrono::Duration::weeks(2));
    }

    #[test]
    fn parses_named_weekday_defaults_to_morning() {
        // Anchor = Tuesday May 7. "monday" should be next Monday May 13 at 09:00.
        let got = parse_relative_time("monday", anchor()).unwrap();
        let expected = Utc.with_ymd_and_hms(2024, 5, 13, 9, 0, 0).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn parses_named_weekday_with_24h_time() {
        let got = parse_relative_time("monday 17:00", anchor()).unwrap();
        let expected = Utc.with_ymd_and_hms(2024, 5, 13, 17, 0, 0).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn parses_named_weekday_with_12h_time() {
        let got = parse_relative_time("monday 5pm", anchor()).unwrap();
        let expected = Utc.with_ymd_and_hms(2024, 5, 13, 17, 0, 0).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn parses_three_letter_weekday() {
        let got = parse_relative_time("fri", anchor()).unwrap();
        // Anchor Tuesday → next Friday May 10 at 09:00.
        let expected = Utc.with_ymd_and_hms(2024, 5, 10, 9, 0, 0).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn same_weekday_means_next_week() {
        // Anchor is Tuesday; "tuesday" should be next Tuesday, not today.
        let got = parse_relative_time("tuesday", anchor()).unwrap();
        let expected = Utc.with_ymd_and_hms(2024, 5, 14, 9, 0, 0).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn parses_tomorrow_with_default_time() {
        let got = parse_relative_time("tomorrow", anchor()).unwrap();
        let expected = Utc.with_ymd_and_hms(2024, 5, 8, 9, 0, 0).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn parses_tomorrow_with_explicit_time() {
        let got = parse_relative_time("tomorrow 9am", anchor()).unwrap();
        let expected = Utc.with_ymd_and_hms(2024, 5, 8, 9, 0, 0).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn parses_today_with_future_time() {
        let got = parse_relative_time("today 17:00", anchor()).unwrap();
        let expected = Utc.with_ymd_and_hms(2024, 5, 7, 17, 0, 0).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn parses_rfc3339() {
        let got = parse_relative_time("2026-06-01T15:00:00Z", anchor()).unwrap();
        let expected = Utc.with_ymd_and_hms(2026, 6, 1, 15, 0, 0).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn rejects_today_past_time() {
        // Anchor is 14:00; "today 9am" is in the past.
        let err = parse_relative_time("today 9am", anchor()).unwrap_err();
        assert_eq!(err, TimeParseError::InPast);
    }

    #[test]
    fn rejects_past_rfc3339() {
        let err = parse_relative_time("2020-01-01T00:00:00Z", anchor()).unwrap_err();
        assert_eq!(err, TimeParseError::InPast);
    }

    #[test]
    fn rejects_garbage() {
        let err = parse_relative_time("asdf", anchor()).unwrap_err();
        assert!(matches!(err, TimeParseError::Unknown(_)));
    }

    #[test]
    fn rejects_empty_string() {
        let err = parse_relative_time("   ", anchor()).unwrap_err();
        assert_eq!(err, TimeParseError::Empty);
    }

    #[test]
    fn rejects_today_alone() {
        // "today" without a time is too ambiguous — refuse rather than
        // assume morning when the anchor is afternoon.
        let err = parse_relative_time("today", anchor()).unwrap_err();
        assert!(matches!(err, TimeParseError::Unknown(_)));
    }

    #[test]
    fn case_insensitive_input() {
        let got = parse_relative_time("MONDAY 5PM", anchor()).unwrap();
        let expected = Utc.with_ymd_and_hms(2024, 5, 13, 17, 0, 0).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn tolerates_extra_whitespace() {
        let got = parse_relative_time("   tomorrow   9am  ", anchor()).unwrap();
        let expected = Utc.with_ymd_and_hms(2024, 5, 8, 9, 0, 0).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn twelve_am_is_midnight_and_twelve_pm_is_noon() {
        let midnight = parse_relative_time("tomorrow 12am", anchor()).unwrap();
        let noon = parse_relative_time("tomorrow 12pm", anchor()).unwrap();
        assert_eq!(midnight.hour(), 0);
        assert_eq!(noon.hour(), 12);
    }

    #[test]
    fn rejects_invalid_24h_hour() {
        let err = parse_relative_time("tomorrow 25:00", anchor()).unwrap_err();
        assert!(matches!(err, TimeParseError::Unknown(_)));
    }

    #[test]
    fn rejects_zero_or_negative_in_duration() {
        // Both zero and negative durations land in the past — surface a
        // single InPast variant rather than two near-identical Unknowns.
        assert_eq!(
            parse_relative_time("in 0h", anchor()).unwrap_err(),
            TimeParseError::InPast
        );
        assert_eq!(
            parse_relative_time("in -1h", anchor()).unwrap_err(),
            TimeParseError::InPast
        );
    }
}
