//! When an email arrived, as the TUI shows it on an open email: local time,
//! never UTC and never a day alone (blueprint 27, "When an email arrived";
//! D148). Lists keep `mail_list::format_date_relative`'s compact form.

use chrono::{DateTime, Datelike, Local, TimeZone, Utc};
use std::fmt::Display;

/// "Today 14:02 · 2h ago", "Tue 21 Oct 14:02", "Tue 21 Oct 2025 14:02".
/// The clock is passed in so the render doesn't read it.
pub fn received_label(at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    received_label_in(at, now, &Local)
}

pub fn received_label_in<Tz>(at: DateTime<Utc>, now: DateTime<Utc>, zone: &Tz) -> String
where
    Tz: TimeZone,
    Tz::Offset: Display,
{
    let local = at.with_timezone(zone);
    let today = now.with_timezone(zone);
    let clock = local.format("%H:%M");
    if local.date_naive() == today.date_naive() {
        return format!("Today {clock} \u{b7} {}", ago(now - at));
    }
    if local.year() == today.year() {
        format!("{} {clock}", local.format("%a %-d %b"))
    } else {
        format!("{} {clock}", local.format("%a %-d %b %Y"))
    }
}

/// "just now", "40m ago", "2h ago". An arrival after `now` (clock skew) is
/// "just now" too.
fn ago(elapsed: chrono::Duration) -> String {
    let minutes = elapsed.num_minutes();
    if minutes < 1 {
        "just now".into()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else {
        format!("{}h ago", elapsed.num_hours())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    // Fixed offsets keep these tests independent of the machine's zone.
    fn zone() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).unwrap()
    }

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn today_shows_local_clock_and_how_long_ago() {
        // 12:02 UTC is 14:02 at +02:00.
        let at = utc(2026, 10, 21, 12, 2);
        let now = utc(2026, 10, 21, 14, 2);
        assert_eq!(
            received_label_in(at, now, &zone()),
            "Today 14:02 \u{b7} 2h ago"
        );
    }

    #[test]
    fn a_minute_old_or_future_arrival_is_just_now() {
        let at = utc(2026, 10, 21, 12, 2);
        assert_eq!(
            received_label_in(at, utc(2026, 10, 21, 12, 2), &zone()),
            "Today 14:02 \u{b7} just now"
        );
        assert_eq!(
            received_label_in(at, utc(2026, 10, 21, 11, 0), &zone()),
            "Today 14:02 \u{b7} just now"
        );
    }

    #[test]
    fn this_year_gives_weekday_day_month_and_time() {
        let at = utc(2026, 10, 21, 12, 2);
        let now = utc(2026, 11, 3, 9, 0);
        assert_eq!(received_label_in(at, now, &zone()), "Wed 21 Oct 14:02");
    }

    #[test]
    fn another_year_adds_the_year() {
        let at = utc(2025, 10, 21, 12, 2);
        let now = utc(2026, 10, 22, 9, 0);
        assert_eq!(received_label_in(at, now, &zone()), "Tue 21 Oct 2025 14:02");
    }

    #[test]
    fn the_local_date_decides_the_day_not_utc() {
        // 23:30 UTC on the 21st is already the 22nd at +02:00.
        let at = utc(2026, 10, 21, 23, 30);
        let now = utc(2026, 10, 22, 6, 0);
        assert_eq!(
            received_label_in(at, now, &zone()),
            "Today 01:30 \u{b7} 6h ago"
        );
    }
}
