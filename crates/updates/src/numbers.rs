//! Numbers an update quotes, and the delta between two messages of one
//! template. Every number shown appears verbatim in its message; a delta
//! is computed here, never by a model, and only when both numbers carry
//! the same unit (blueprint 22, "Numbers are quoted, deltas are
//! computed").

use chrono::{DateTime, TimeZone, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quoted {
    pub raw: String,
    pub value: f64,
    /// The comparable unit: "km", "run", "%", or an ISO currency code.
    pub unit: String,
}

/// Money before the number, a unit after it. A number with neither is
/// left alone: "3" alone can't be compared with anything.
static NUMBER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?ix)
        (?P<cur>[£$€¥₹]|\bR\s?|\b(?:usd|gbp|eur|zar|cad|aud)\s?)?
        (?P<num>\d{1,3}(?:,\d{3})+(?:\.\d+)?|\d+(?:\.\d+)?)
        (?P<unit>\s?%|\s?(?:usd|gbp|eur|zar)\b|\s?(?:km|kms|kilometres|kilometers|mi|miles|kg|kgs|lbs?|gb|mb|tb|ms|hrs?|hours|mins?|minutes|runs?|rides?|walks?|steps|visitors?|visits?|views?|pageviews|sign-?ups?|signups?|users?|subscribers?|followers?|orders?|deploys?|deployments?|builds?|issues?|commits?|stars?|downloads?|sessions?|clicks?|opens?|members?|messages?|calories|kcal|requests?|errors?|incidents?|tests?|points?)\b)?",
    )
    .expect("valid number regex")
});

/// Digits glued to letters or a hash are ids, versions or times, not
/// quantities: "v2", "#123", "06:12", "2026-10-07".
fn glued(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    let glue = |c: Option<char>, extra: &[char]| {
        c.is_some_and(|c| c.is_alphanumeric() || extra.contains(&c))
    };
    glue(before, &['#', ':', '-', '/', '.', '_']) || glue(after, &[':', '/', '_']) || {
        after == Some('-')
            && text[end..]
                .chars()
                .nth(1)
                .is_some_and(|c| c.is_ascii_digit())
    }
}

fn unit_of(currency: Option<&str>, unit: Option<&str>) -> Option<String> {
    if let Some(cur) = currency.map(str::trim).filter(|c| !c.is_empty()) {
        return Some(currency_code(cur).to_string());
    }
    let unit = unit?.trim().to_ascii_lowercase();
    if unit.is_empty() {
        return None;
    }
    Some(match unit.as_str() {
        "%" => "%".to_string(),
        "usd" | "gbp" | "eur" | "zar" => unit.to_ascii_uppercase(),
        "kms" | "kilometres" | "kilometers" => "km".to_string(),
        "miles" => "mi".to_string(),
        "kgs" => "kg".to_string(),
        "lb" => "lbs".to_string(),
        "hr" | "hrs" | "hours" => "hour".to_string(),
        "min" | "mins" | "minutes" => "minute".to_string(),
        "signup" | "signups" | "sign-up" | "sign-ups" => "sign-up".to_string(),
        "kcal" | "calories" => "calorie".to_string(),
        "deployments" | "deployment" => "deploy".to_string(),
        other => other
            .strip_suffix('s')
            .filter(|stem| stem.len() > 2)
            .unwrap_or(other)
            .to_string(),
    })
}

fn currency_code(symbol: &str) -> &'static str {
    match symbol.to_ascii_lowercase().as_str() {
        "£" | "gbp" => "GBP",
        "$" | "usd" => "USD",
        "€" | "eur" => "EUR",
        "¥" => "JPY",
        "₹" => "INR",
        "r" | "zar" => "ZAR",
        "cad" => "CAD",
        "aud" => "AUD",
        _ => "XXX",
    }
}

/// Every quantity `text` quotes, in order, at most `max`.
pub fn extract_numbers(text: &str, max: usize) -> Vec<Quoted> {
    let mut out = Vec::new();
    for caps in NUMBER.captures_iter(text) {
        let (Some(whole), Some(num)) = (caps.get(0), caps.get(2)) else {
            continue;
        };
        let currency = caps.name("cur").map(|m| m.as_str());
        let unit_text = caps.name("unit").map(|m| m.as_str());
        if glued(text, whole.start(), whole.end()) {
            continue;
        }
        let digits = num.as_str().replace(',', "");
        let Ok(value) = digits.parse::<f64>() else {
            continue;
        };
        let Some(unit) = unit_of(currency, unit_text) else {
            continue;
        };
        out.push(Quoted {
            raw: whole.as_str().trim().to_string(),
            value,
            unit,
        });
        if out.len() >= max {
            break;
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delta {
    pub raw: String,
    pub previous_raw: String,
    pub change: f64,
    pub text: String,
    pub against: DateTime<Utc>,
}

/// The change in the first number both messages quote with the same unit,
/// matched by position among numbers of that unit. `None` when no unit
/// matches, when the count of that unit differs (which number is which?),
/// or when the previous value is zero.
pub fn delta<Tz: TimeZone>(
    current: &[Quoted],
    previous: &[Quoted],
    current_at: DateTime<Utc>,
    previous_at: DateTime<Utc>,
    tz: &Tz,
) -> Option<Delta>
where
    Tz::Offset: std::fmt::Display,
{
    for (index, now) in current.iter().enumerate() {
        let ordinal = current[..index]
            .iter()
            .filter(|q| q.unit == now.unit)
            .count();
        let same_unit_now = current.iter().filter(|q| q.unit == now.unit).count();
        let earlier: Vec<&Quoted> = previous.iter().filter(|q| q.unit == now.unit).collect();
        if earlier.len() != same_unit_now {
            continue;
        }
        let Some(before) = earlier.get(ordinal) else {
            continue;
        };
        let period = period_label(current_at, previous_at, tz);
        let (change, text) = if now.unit == "%" {
            let points = now.value - before.value;
            (points, change_text(points, " points", &period))
        } else {
            if before.value == 0.0 {
                continue;
            }
            let percent = (now.value - before.value) / before.value * 100.0;
            (percent, change_text(percent, "%", &period))
        };
        return Some(Delta {
            raw: now.raw.clone(),
            previous_raw: before.raw.clone(),
            change,
            text,
            against: previous_at,
        });
    }
    None
}

fn change_text(change: f64, suffix: &str, period: &str) -> String {
    if change == 0.0 {
        return format!("same as {}", period.trim_start_matches("on "));
    }
    let direction = if change > 0.0 { "up" } else { "down" };
    let size = change.abs();
    let amount = if size < 1.0 {
        format!("{size:.1}")
    } else {
        format!("{size:.0}")
    };
    format!("{direction} {amount}{suffix} {period}")
}

/// How the previous message is named: "on last week" for about a week
/// apart, "on the day before", "on last month", else its date in the
/// user's zone.
fn period_label<Tz: TimeZone>(current: DateTime<Utc>, previous: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let hours = (current - previous).num_hours();
    match hours {
        18..=30 => "on the day before".to_string(),
        144..=192 => "on last week".to_string(),
        648..=768 => "on last month".to_string(),
        _ => format!("on {}", previous.with_timezone(tz).format("%-d %b")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, FixedOffset};

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    #[test]
    fn a_dated_delta_names_the_day_in_the_users_zone() {
        let previous = Utc.with_ymd_and_hms(2026, 10, 5, 23, 30, 0).unwrap();
        let now = previous + Duration::days(3);
        let found = delta(
            &extract_numbers("998 visitors", 8),
            &extract_numbers("1,204 visitors", 8),
            now,
            previous,
            &FixedOffset::east_opt(2 * 3600).unwrap(),
        )
        .unwrap();
        assert_eq!(found.text, "down 17% on 6 Oct");
    }

    #[test]
    fn deltas_need_the_same_unit_and_template_position() {
        let at = DateTime::UNIX_EPOCH + Duration::days(20_000);
        let week_ago = at - Duration::days(7);
        let now = extract_numbers("Your week: 3 runs, 21.3 km", 8);
        let before = extract_numbers("Your week: 2 runs, 19 km", 8);
        let found = delta(&now, &before, at, week_ago, &utc()).expect("a delta");
        assert_eq!(found.raw, "3 runs");
        assert_eq!(found.text, "up 50% on last week");

        let miles = extract_numbers("You ran 13.2 mi", 8);
        let km = extract_numbers("You ran 21.3 km", 8);
        assert!(delta(&miles, &km, at, week_ago, &utc()).is_none());
        let pounds = extract_numbers("Balance £10", 8);
        let dollars = extract_numbers("Balance $12", 8);
        assert!(delta(&pounds, &dollars, at, week_ago, &utc()).is_none());

        let two = extract_numbers("21 km and 3 km", 8);
        let one = extract_numbers("19 km", 8);
        assert!(delta(&two, &one, at, week_ago, &utc()).is_none());

        let pct = delta(
            &extract_numbers("Uptime 99.5%", 8),
            &extract_numbers("Uptime 99.9%", 8),
            at,
            at - Duration::days(3),
            &utc(),
        )
        .expect("points");
        assert!(pct.text.starts_with("down 0.4 points on "), "{}", pct.text);
        let same = delta(
            &extract_numbers("12 sign-ups", 8),
            &extract_numbers("12 signups", 8),
            at,
            at - Duration::hours(24),
            &utc(),
        )
        .expect("same");
        assert_eq!(same.text, "same as the day before");
        assert!(delta(
            &extract_numbers("5 runs", 8),
            &extract_numbers("0 runs", 8),
            at,
            week_ago,
            &utc()
        )
        .is_none());
    }
}
