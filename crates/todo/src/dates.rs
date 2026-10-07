//! Due dates in an email's text.
//!
//! A due phrase is a trigger ("due", "pay by", "reply by", "expires") and a
//! date. Worded dates ("9 October", "Oct 9, 2026", "Friday") resolve through
//! `mxr_core::natural_time`, anchored at the message's own date so "Friday"
//! means the Friday after it was sent. Numeric dates are read here: the
//! parser doesn't take them, and "09/10/2026" is 9 October in the UK and
//! September 10 in the US. When both readings are real dates, the sender's
//! likely order is used and the date is marked unchecked.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use mxr_core::natural_time::{resolve_time, TimePrefs};
use once_cell::sync::Lazy;
use regex::Regex;

/// Which number comes first in a numeric date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOrder {
    DayFirst,
    MonthFirst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundDate {
    /// End of the named day in the user's zone, or the exact instant for a
    /// lifetime ("expires in 24 hours").
    pub at: DateTime<Utc>,
    /// The phrase, verbatim: trigger and date.
    pub words: String,
    /// False for a numeric date that reads two ways.
    pub checked: bool,
}

const MONTHS: &str = r"jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?";
const WEEKDAYS: &str = r"(?:mon|tue(?:s)?|wed(?:nes)?|thu(?:r(?:s)?)?|fri|sat(?:ur)?|sun)(?:day)?";

/// One date expression. Kept in one place so every trigger reads the same
/// dates.
pub static DATE_EXPR: Lazy<String> = Lazy::new(|| {
    format!(
        r"(?:(?:{WEEKDAYS}),?\s+)?(?:\d{{1,2}}(?:st|nd|rd|th)?\s+(?:of\s+)?(?:{MONTHS})\.?(?:,?\s+\d{{4}})?|(?:{MONTHS})\.?\s+\d{{1,2}}(?:st|nd|rd|th)?(?:,?\s+\d{{4}})?|\d{{4}}-\d{{2}}-\d{{2}}|\d{{1,2}}[/.]\d{{1,2}}[/.]\d{{2,4}}|today|tomorrow|{WEEKDAYS})\b"
    )
});

static NUMERIC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(\d{1,2})[/.](\d{1,2})[/.](\d{2,4})$").expect("valid numeric date regex")
});
static ISO: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(\d{4})-(\d{2})-(\d{2})$").expect("valid iso date regex"));
static WEEKDAY_PREFIX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"(?i)^(?:{WEEKDAYS}),?\s+(\d|(?:{MONTHS}))"))
        .expect("valid weekday prefix regex")
});
static ORDINAL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b(\d{1,2})(?:st|nd|rd|th)\b").expect("valid ordinal regex"));
static HAS_YEAR: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b\d{4}\b").expect("valid year regex"));

static LIFETIME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(?:expires?|expiring|valid|active|good)\s+(?:in|for|within)\s+(?:the\s+next\s+)?(\d{1,3}|an?|one|two|three|seven)\s+(minutes?|mins?|hours?|hrs?|days?)\b",
    )
    .expect("valid lifetime regex")
});

/// Build a trigger regex: any of `triggers`, then a date expression.
pub fn trigger_regex(triggers: &str) -> Regex {
    Regex::new(&format!(
        r"(?i)\b(?:{triggers})(?:\s*(?:on|is|date|by|:|-))*\s*(?P<date>{})",
        *DATE_EXPR
    ))
    .expect("valid due trigger regex")
}

/// The first due phrase in `text` that `triggers` matches and that resolves
/// to a date. Relative words resolve from `anchor`, the message's date.
pub fn find_due<Tz>(
    text: &str,
    triggers: &Regex,
    anchor: &DateTime<Tz>,
    order: DateOrder,
) -> Option<FoundDate>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    triggers.captures_iter(text).find_map(|caps| {
        let date = caps.name("date")?;
        let whole = caps.get(0)?;
        let words = whole.as_str().trim();
        // "was due" puts its tense before the trigger the regex matched.
        let lead_in = crate::text::floor_char_boundary(text, whole.start().saturating_sub(12));
        let intent = if PAST_INTENT.is_match(&text[lead_in..whole.end()]) {
            YearIntent::JustGone
        } else {
            YearIntent::Next
        };
        let (day, checked) = parse_date(date.as_str(), anchor, order, intent)?;
        Some(FoundDate {
            at: end_of_day(day, &anchor.timezone()),
            words: words.to_string(),
            checked,
        })
    })
}

/// Which year a date without one means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YearIntent {
    /// A deadline or expiry ahead ("due", "renew by", "expires"): the next
    /// occurrence after the message was sent.
    Next,
    /// A date already passed ("was due", "expired on", "overdue since"):
    /// the most recent one on or before the message.
    JustGone,
}

static PAST_INTENT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(was|were)\s+(due|payable)|overdue|past\s+due|expired|lapsed")
        .expect("valid past intent regex")
});
static MONTH_NAME: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"(?i)\b(?:{MONTHS})\b")).expect("valid month regex"));

/// A stated lifetime ("expires in 24 hours") from `sent`.
pub fn find_lifetime(text: &str, sent: DateTime<Utc>) -> Option<FoundDate> {
    let caps = LIFETIME.captures(text)?;
    let count = match caps.get(1)?.as_str().to_ascii_lowercase().as_str() {
        "a" | "an" | "one" => 1,
        "two" => 2,
        "three" => 3,
        "seven" => 7,
        digits => digits.parse::<i64>().ok()?,
    };
    let unit = caps.get(2)?.as_str().to_ascii_lowercase();
    let lifetime = if unit.starts_with("min") {
        Duration::minutes(count)
    } else if unit.starts_with('h') {
        Duration::hours(count)
    } else {
        Duration::days(count)
    };
    Some(FoundDate {
        at: sent + lifetime,
        words: caps.get(0)?.as_str().to_string(),
        checked: true,
    })
}

/// A date expression to a calendar day, and whether it is unambiguous.
/// A yearless day and month ("9 October") is a guess about the year, so
/// it comes back unchecked; `intent` picks which year.
pub fn parse_date<Tz>(
    expr: &str,
    anchor: &DateTime<Tz>,
    order: DateOrder,
    intent: YearIntent,
) -> Option<(NaiveDate, bool)>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let expr = expr.trim();
    if let Some(caps) = NUMERIC.captures(expr) {
        return parse_numeric(&caps[1], &caps[2], &caps[3], order);
    }
    if let Some(caps) = ISO.captures(expr) {
        let day = NaiveDate::from_ymd_opt(
            caps[1].parse().ok()?,
            caps[2].parse().ok()?,
            caps[3].parse().ok()?,
        )?;
        return Some((day, true));
    }
    let lower = expr.to_ascii_lowercase();
    if lower == "today" {
        return Some((anchor.date_naive(), true));
    }
    // "Friday 9 October": the date says it all, and the parser would read
    // the weekday and the date as two days.
    let mut normalized = lower.clone();
    if let Some(caps) = WEEKDAY_PREFIX.captures(&lower) {
        if let Some(rest) = caps.get(1) {
            normalized = lower[rest.start()..].to_string();
        }
    }
    let normalized = ORDINAL.replace_all(&normalized, "$1");
    let normalized = normalized
        .replace(" of ", " ")
        .replace([',', '.'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let resolution = resolve_time(&normalized, anchor, &TimePrefs::default()).ok()?;
    let mut day = resolution.at.with_timezone(&anchor.timezone()).date_naive();
    let year_guessed = !HAS_YEAR.is_match(&normalized) && MONTH_NAME.is_match(&normalized);
    // The parser takes the next occurrence after the message; a date said
    // to have passed means the one just gone.
    if year_guessed && intent == YearIntent::JustGone && day > anchor.date_naive() {
        day = day.with_year(day.year() - 1)?;
    }
    Some((day, !year_guessed))
}

fn parse_numeric(
    first: &str,
    second: &str,
    year: &str,
    order: DateOrder,
) -> Option<(NaiveDate, bool)> {
    let first: u32 = first.parse().ok()?;
    let second: u32 = second.parse().ok()?;
    let mut year: i32 = year.parse().ok()?;
    if year < 100 {
        year += 2000;
    }
    let day_first = NaiveDate::from_ymd_opt(year, second, first);
    let month_first = NaiveDate::from_ymd_opt(year, first, second);
    match (day_first, month_first) {
        (Some(a), Some(b)) if a == b => Some((a, true)),
        (Some(a), Some(b)) => Some(match order {
            DateOrder::DayFirst => (a, false),
            DateOrder::MonthFirst => (b, false),
        }),
        (Some(a), None) => Some((a, true)),
        (None, Some(b)) => Some((b, true)),
        (None, None) => None,
    }
}

/// The last second of `day` in `tz`, as UTC: a bill due on the 9th is not
/// late until the 9th is over.
pub fn end_of_day<Tz: TimeZone>(day: NaiveDate, tz: &Tz) -> DateTime<Utc> {
    let time = NaiveTime::from_hms_opt(23, 59, 59).unwrap_or(NaiveTime::MIN);
    tz.from_local_datetime(&day.and_time(time))
        .latest()
        .map_or_else(
            || Utc.from_utc_datetime(&day.and_time(time)),
            |local| local.with_timezone(&Utc),
        )
}

/// The likely numeric date order for a sender: month first for US mail,
/// day first otherwise.
pub fn order_for(sender_domain: &str, currency: Option<&str>) -> DateOrder {
    let us_domain = sender_domain.ends_with(".us") || sender_domain.ends_with(".gov");
    let uk_domain = sender_domain.ends_with(".uk");
    if us_domain || (currency == Some("USD") && !uk_domain) {
        DateOrder::MonthFirst
    } else {
        DateOrder::DayFirst
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::London;

    fn sent(y: i32, m: u32, d: u32) -> DateTime<chrono_tz::Tz> {
        London
            .with_ymd_and_hms(y, m, d, 10, 0, 0)
            .single()
            .expect("valid date")
    }

    fn due_regex() -> Regex {
        trigger_regex(r"payment\s+due|due|pay\s+by|by")
    }

    #[test]
    fn worded_dates_resolve_from_the_message_date() {
        let found = find_due(
            "Payment due 9 October.",
            &due_regex(),
            &sent(2026, 9, 28),
            DateOrder::DayFirst,
        )
        .expect("due");
        assert_eq!(
            found.at.with_timezone(&London).date_naive(),
            NaiveDate::from_ymd_opt(2026, 10, 9).expect("date")
        );
        assert_eq!(found.words, "Payment due 9 October");
        assert!(!found.checked, "no year stated, so the year is a guess");

        let found = find_due(
            "Please pay by Friday, 9th October 2026",
            &due_regex(),
            &sent(2026, 9, 28),
            DateOrder::DayFirst,
        )
        .expect("due");
        assert_eq!(
            found.at.with_timezone(&London).date_naive(),
            NaiveDate::from_ymd_opt(2026, 10, 9).expect("date")
        );

        let found = find_due(
            "Due date: Oct 9, 2026",
            &due_regex(),
            &sent(2026, 9, 28),
            DateOrder::DayFirst,
        )
        .expect("due");
        assert_eq!(
            found.at.with_timezone(&London).date_naive(),
            NaiveDate::from_ymd_opt(2026, 10, 9).expect("date")
        );
    }

    #[test]
    fn a_deadline_without_a_year_rolls_forward_and_is_unchecked() {
        // Sent 1 December: "expires 3 March" is next March, a guess.
        let found = find_due(
            "Your passport expires 3 March",
            &trigger_regex("expires"),
            &sent(2026, 12, 1),
            DateOrder::DayFirst,
        )
        .expect("due");
        assert_eq!(
            found.at.with_timezone(&London).date_naive(),
            NaiveDate::from_ymd_opt(2027, 3, 3).expect("date")
        );
        assert!(!found.checked, "the year is a guess");
        let stated = find_due(
            "Your passport expires 3 March 2027",
            &trigger_regex("expires"),
            &sent(2026, 12, 1),
            DateOrder::DayFirst,
        )
        .expect("due");
        assert!(stated.checked);
    }

    #[test]
    fn a_reminder_after_the_date_means_the_date_just_gone() {
        let found = find_due(
            "Your payment was due 9 October",
            &due_regex(),
            &sent(2026, 10, 12),
            DateOrder::DayFirst,
        )
        .expect("due");
        assert_eq!(
            found.at.with_timezone(&London).date_naive(),
            NaiveDate::from_ymd_opt(2026, 10, 9).expect("date")
        );
    }

    #[test]
    fn weekday_resolves_to_the_next_one_after_sending() {
        // 28 Sep 2026 is a Monday.
        let found = find_due(
            "Reply by Friday",
            &trigger_regex("reply by"),
            &sent(2026, 9, 28),
            DateOrder::DayFirst,
        )
        .expect("due");
        assert_eq!(
            found.at.with_timezone(&London).date_naive(),
            NaiveDate::from_ymd_opt(2026, 10, 2).expect("date")
        );
    }

    #[test]
    fn ambiguous_numeric_dates_follow_the_order_and_are_unchecked() {
        let anchor = sent(2026, 9, 28);
        let uk =
            find_due("due 09/10/2026", &due_regex(), &anchor, DateOrder::DayFirst).expect("due");
        assert_eq!(
            uk.at.with_timezone(&London).date_naive(),
            NaiveDate::from_ymd_opt(2026, 10, 9).expect("date")
        );
        assert!(!uk.checked);
        let us = find_due(
            "due 09/10/2026",
            &due_regex(),
            &anchor,
            DateOrder::MonthFirst,
        )
        .expect("due");
        assert_eq!(
            us.at.with_timezone(&London).date_naive(),
            NaiveDate::from_ymd_opt(2026, 9, 10).expect("date")
        );
        assert!(!us.checked);
    }

    #[test]
    fn numeric_dates_that_read_one_way_are_checked() {
        let anchor = sent(2026, 9, 28);
        let found = find_due(
            "due 25/10/2026",
            &due_regex(),
            &anchor,
            DateOrder::MonthFirst,
        )
        .expect("due");
        assert_eq!(
            found.at.with_timezone(&London).date_naive(),
            NaiveDate::from_ymd_opt(2026, 10, 25).expect("date")
        );
        assert!(found.checked);
        let found =
            find_due("due 10/10/26", &due_regex(), &anchor, DateOrder::MonthFirst).expect("due");
        assert!(found.checked, "same both ways");
        assert_eq!(
            find_due("due 31/02/2026", &due_regex(), &anchor, DateOrder::DayFirst),
            None
        );
    }

    #[test]
    fn lifetimes_count_from_sending() {
        let at = Utc
            .with_ymd_and_hms(2026, 10, 1, 9, 0, 0)
            .single()
            .expect("date");
        let found = find_lifetime("This link expires in 24 hours.", at).expect("lifetime");
        assert_eq!(found.at, at + Duration::hours(24));
        let found = find_lifetime("The code is valid for 10 minutes", at).expect("lifetime");
        assert_eq!(found.at, at + Duration::minutes(10));
        assert_eq!(find_lifetime("expires soon", at), None);
    }

    #[test]
    fn date_order_follows_the_sender() {
        assert_eq!(order_for("camden.gov.uk", Some("GBP")), DateOrder::DayFirst);
        assert_eq!(order_for("comcast.com", Some("USD")), DateOrder::MonthFirst);
        assert_eq!(order_for("example.com", None), DateOrder::DayFirst);
    }
}
