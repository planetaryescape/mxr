use chrono::{
    DateTime, Datelike, Duration, FixedOffset, MappedLocalTime, NaiveDate, NaiveDateTime, TimeZone,
    Utc, Weekday,
};

use super::*;

/// Europe/London without pulling chrono-tz into the workspace: GMT, with
/// BST (+01:00) from 01:00 UTC on the last Sunday of March to 01:00 UTC on
/// the last Sunday of October.
#[derive(Debug, Clone, Copy)]
struct London;

fn last_sunday(year: i32, month: u32) -> NaiveDate {
    (1..=31)
        .rev()
        .filter_map(|day| NaiveDate::from_ymd_opt(year, month, day))
        .find(|date| date.weekday() == Weekday::Sun)
        .unwrap()
}

fn gmt() -> FixedOffset {
    FixedOffset::east_opt(0).unwrap()
}

fn bst() -> FixedOffset {
    FixedOffset::east_opt(3600).unwrap()
}

impl TimeZone for London {
    type Offset = FixedOffset;

    fn from_offset(_: &FixedOffset) -> Self {
        Self
    }

    fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<FixedOffset> {
        self.offset_from_local_datetime(&local.and_hms_opt(12, 0, 0).unwrap())
    }

    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> MappedLocalTime<FixedOffset> {
        // BST first so an ambiguous time lists the earlier instant first.
        let valid: Vec<FixedOffset> = [bst(), gmt()]
            .into_iter()
            .filter(|offset| {
                let utc = *local - Duration::seconds(i64::from(offset.local_minus_utc()));
                self.offset_from_utc_datetime(&utc) == *offset
            })
            .collect();
        match valid.as_slice() {
            [one] => MappedLocalTime::Single(*one),
            [first, second] => MappedLocalTime::Ambiguous(*first, *second),
            _ => MappedLocalTime::None,
        }
    }

    fn offset_from_utc_date(&self, utc: &NaiveDate) -> FixedOffset {
        self.offset_from_utc_datetime(&utc.and_hms_opt(0, 0, 0).unwrap())
    }

    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> FixedOffset {
        let year = utc.year();
        let start = last_sunday(year, 3).and_hms_opt(1, 0, 0).unwrap();
        let end = last_sunday(year, 10).and_hms_opt(1, 0, 0).unwrap();
        if *utc >= start && *utc < end {
            bst()
        } else {
            gmt()
        }
    }
}

fn london(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<London> {
    London
        .with_ymd_and_hms(y, mo, d, h, mi, 0)
        .single()
        .unwrap()
}

/// Tuesday 7 May 2024, 14:00 BST (13:00 UTC). A Tuesday gives every weekday
/// name a distinct answer.
fn anchor() -> DateTime<London> {
    london(2024, 5, 7, 14, 0)
}

fn resolve_at(input: &str, now: &DateTime<London>) -> TimeResolution {
    resolve_time(input, now, &TimePrefs::default()).expect(input)
}

fn local(resolution: &TimeResolution) -> String {
    resolution
        .at
        .with_timezone(&London)
        .format("%a %Y-%m-%d %H:%M")
        .to_string()
}

#[test]
fn phrase_table() {
    let cases: &[(&str, &str)] = &[
        // Offsets.
        ("in 30m", "Tue 2024-05-07 14:30"),
        ("in 2h", "Tue 2024-05-07 16:00"),
        ("in 5d", "Sun 2024-05-12 14:00"),
        ("in 2w", "Tue 2024-05-21 14:00"),
        ("in 1w", "Tue 2024-05-14 14:00"),
        ("3d", "Fri 2024-05-10 14:00"),
        ("2w", "Tue 2024-05-21 14:00"),
        ("in an hour", "Tue 2024-05-07 15:00"),
        ("in 2 hours", "Tue 2024-05-07 16:00"),
        ("in 2h 30m", "Tue 2024-05-07 16:30"),
        ("in 2 months", "Sun 2024-07-07 14:00"),
        // Presets the old config parser accepted.
        ("tomorrow", "Wed 2024-05-08 09:00"),
        ("tomorrow_morning", "Wed 2024-05-08 09:00"),
        ("tonight", "Tue 2024-05-07 18:00"),
        ("weekend", "Sat 2024-05-11 10:00"),
        ("monday", "Mon 2024-05-13 09:00"),
        ("next_monday", "Mon 2024-05-13 09:00"),
        ("next monday", "Mon 2024-05-13 09:00"),
        ("saturday", "Sat 2024-05-11 10:00"),
        ("sunday", "Sun 2024-05-12 10:00"),
        // Weekdays: the next one, never today.
        ("tuesday", "Tue 2024-05-14 09:00"),
        ("tue", "Tue 2024-05-14 09:00"),
        ("thu", "Thu 2024-05-09 09:00"),
        ("thurs", "Thu 2024-05-09 09:00"),
        ("sat", "Sat 2024-05-11 10:00"),
        ("fri", "Fri 2024-05-10 09:00"),
        ("tom", "Wed 2024-05-08 09:00"),
        // Day plus time, either order, with fillers.
        ("tomorrow 9am", "Wed 2024-05-08 09:00"),
        ("tomorrow 5", "Wed 2024-05-08 17:00"),
        ("monday 17:00", "Mon 2024-05-13 17:00"),
        ("monday 5pm", "Mon 2024-05-13 17:00"),
        ("monday 5 pm", "Mon 2024-05-13 17:00"),
        ("monday 09:30am", "Mon 2024-05-13 09:30"),
        ("fri 3", "Fri 2024-05-10 15:00"),
        ("fri 3pm", "Fri 2024-05-10 15:00"),
        ("3pm fri", "Fri 2024-05-10 15:00"),
        ("fri at 3pm", "Fri 2024-05-10 15:00"),
        ("on fri at 3", "Fri 2024-05-10 15:00"),
        ("fri 8", "Fri 2024-05-10 08:00"),
        ("fri 7", "Fri 2024-05-10 19:00"),
        ("fri 12", "Fri 2024-05-10 12:00"),
        ("fri morning", "Fri 2024-05-10 09:00"),
        ("fri afternoon", "Fri 2024-05-10 14:00"),
        ("fri evening", "Fri 2024-05-10 18:00"),
        ("tonight 9", "Tue 2024-05-07 21:00"),
        ("today 17:00", "Tue 2024-05-07 17:00"),
        ("today 5", "Tue 2024-05-07 17:00"),
        ("tomorrow 12am", "Wed 2024-05-08 00:00"),
        ("tomorrow 12pm", "Wed 2024-05-08 12:00"),
        // A time alone means its next occurrence.
        ("5pm", "Tue 2024-05-07 17:00"),
        ("9am", "Wed 2024-05-08 09:00"),
        ("noon", "Wed 2024-05-08 12:00"),
        ("midnight", "Wed 2024-05-08 00:00"),
        ("eod", "Tue 2024-05-07 17:00"),
        ("end of day", "Tue 2024-05-07 17:00"),
        ("end of the day", "Tue 2024-05-07 17:00"),
        ("eow", "Fri 2024-05-10 17:00"),
        // Calendar phrases.
        ("next week", "Mon 2024-05-13 09:00"),
        ("next week 3pm", "Mon 2024-05-13 15:00"),
        ("next month", "Sat 2024-06-01 09:00"),
        ("3 oct", "Thu 2024-10-03 09:00"),
        ("oct 3", "Thu 2024-10-03 09:00"),
        ("3 october", "Thu 2024-10-03 09:00"),
        ("october 3rd", "Thu 2024-10-03 09:00"),
        ("3rd of october", "Thu 2024-10-03 09:00"),
        ("3 oct 5pm", "Thu 2024-10-03 17:00"),
        ("oct 3, 2025", "Fri 2025-10-03 09:00"),
        ("3 may", "Sat 2025-05-03 09:00"),
        ("2024-10-03", "Thu 2024-10-03 09:00"),
        ("2024-10-03 15:00", "Thu 2024-10-03 15:00"),
        // Absolute forms.
        ("2026-06-01T15:00:00Z", "Mon 2026-06-01 16:00"),
        ("2026-12-25T09:00:00", "Fri 2026-12-25 09:00"),
        // Case and whitespace.
        ("MONDAY 5PM", "Mon 2024-05-13 17:00"),
        ("   tomorrow   9am  ", "Wed 2024-05-08 09:00"),
        ("Fri 3PM", "Fri 2024-05-10 15:00"),
    ];
    let mut failures = Vec::new();
    for (input, expected) in cases {
        match resolve_time(input, &anchor(), &TimePrefs::default()) {
            Ok(resolution) if local(&resolution) == *expected => {}
            Ok(resolution) => failures.push(format!(
                "{input:?}: got {}, want {expected}",
                local(&resolution)
            )),
            Err(err) => failures.push(format!("{input:?}: error {err}")),
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn tomorrow_9am_is_local_not_utc() {
    // The old parser built "tomorrow 9am" as 09:00 UTC. In London in May
    // that is 10:00 on the wall clock.
    let resolution = resolve_at("tomorrow 9am", &anchor());
    assert_eq!(
        resolution.at,
        Utc.with_ymd_and_hms(2024, 5, 8, 8, 0, 0).unwrap()
    );

    let plus_two = FixedOffset::east_opt(2 * 3600).unwrap();
    let now = plus_two.with_ymd_and_hms(2024, 5, 7, 14, 0, 0).unwrap();
    let resolution = resolve_time("tomorrow 9am", &now, &TimePrefs::default()).unwrap();
    assert_eq!(
        resolution.at,
        Utc.with_ymd_and_hms(2024, 5, 8, 7, 0, 0).unwrap()
    );
    assert_eq!(resolution.choices[0].local, "2024-05-08T09:00:00+02:00");
}

#[test]
fn fri_3_offers_afternoon_first_then_early_morning() {
    let resolution = resolve_at("fri 3", &anchor());
    assert!(resolution.is_ambiguous());
    let labels: Vec<&str> = resolution
        .choices
        .iter()
        .map(|c| c.label.as_str())
        .collect();
    assert_eq!(labels, ["15:00", "03:00"]);
    assert_eq!(resolution.choices[0].implied, [ImpliedPart::Meridiem]);
    assert_eq!(resolution.description, "Friday 10 May, 15:00");
    assert_eq!(resolution.choices[0].relative_label, "in 3 days");
    assert_eq!(resolution.at, resolution.choices[0].at);
}

#[test]
fn past_half_of_an_ambiguous_time_is_dropped() {
    // At 14:00, "today 9" can only be 21:00.
    let resolution = resolve_at("today 9", &anchor());
    assert_eq!(resolution.choices.len(), 1);
    assert_eq!(local(&resolution), "Tue 2024-05-07 21:00");
}

#[test]
fn bare_hour_rolls_each_reading_to_its_next_occurrence() {
    let resolution = resolve_at("3", &anchor());
    let choices: Vec<&str> = resolution
        .choices
        .iter()
        .map(|c| c.label.as_str())
        .collect();
    assert_eq!(choices, ["Tue 7 May, 15:00", "Wed 8 May, 03:00"]);
    assert!(resolution.choices[1].implied.contains(&ImpliedPart::Date));
}

#[test]
fn implied_parts_are_reported() {
    let tomorrow = resolve_at("tomorrow", &anchor());
    assert_eq!(tomorrow.choices[0].implied, [ImpliedPart::Time]);

    let five = resolve_at("5pm", &anchor());
    assert_eq!(five.choices[0].implied, [ImpliedPart::Date]);

    let october = resolve_at("3 oct", &anchor());
    assert_eq!(
        october.choices[0].implied,
        [ImpliedPart::Year, ImpliedPart::Time]
    );

    let exact = resolve_at("fri 3pm", &anchor());
    assert!(exact.choices[0].implied.is_empty());
    assert!(!exact.is_ambiguous());
}

#[test]
fn labels_and_relative_text() {
    let next_year = resolve_at("3 may", &anchor());
    assert_eq!(next_year.choices[0].date_label, "Saturday 3 May 2025");
    assert_eq!(
        resolve_at("in 2h", &anchor()).choices[0].relative_label,
        "in 2 hours"
    );
    assert_eq!(
        resolve_at("in 30m", &anchor()).choices[0].relative_label,
        "in 30 minutes"
    );
    assert_eq!(
        resolve_at("tomorrow", &anchor()).choices[0].relative_label,
        "tomorrow"
    );
    assert_eq!(
        resolve_at("in 3w", &anchor()).choices[0].relative_label,
        "in 3 weeks"
    );
    assert_eq!(
        resolve_at("5pm", &anchor()).choices[0].relative_label,
        "in 3 hours"
    );
}

#[test]
fn spans_cover_what_was_understood() {
    let resolution = resolve_at("  fri at 3 ", &anchor());
    assert_eq!(resolution.spans, [TimeSpan { start: 2, end: 10 }]);

    let err = resolve_time("fri frday", &anchor(), &TimePrefs::default()).unwrap_err();
    assert_eq!(err.understood, [TimeSpan { start: 0, end: 3 }]);
    assert_eq!(err.token.as_deref(), Some("frday"));
}

#[test]
fn errors_are_specific() {
    let err = |input: &str| resolve_time(input, &anchor(), &TimePrefs::default()).unwrap_err();

    assert_eq!(err("   ").kind, TimeResolveErrorKind::Empty);
    assert_eq!(err("today").kind, TimeResolveErrorKind::NeedsTime);
    assert_eq!(err("asdf").kind, TimeResolveErrorKind::Unrecognized);
    assert_eq!(
        err("frday").message,
        "Didn't catch \"frday\". Try \"fri 3pm\" or \"in 2d\"."
    );
    assert_eq!(
        err("tomorrow 25:00").kind,
        TimeResolveErrorKind::Unrecognized
    );
    assert_eq!(err("fri tomorrow").kind, TimeResolveErrorKind::Conflict);
    assert_eq!(err("in 2h fri").kind, TimeResolveErrorKind::Conflict);
    assert_eq!(err("31 feb").kind, TimeResolveErrorKind::InvalidDate);
    assert_eq!(err("in").kind, TimeResolveErrorKind::Unrecognized);
    assert_eq!(err("at").kind, TimeResolveErrorKind::Unrecognized);

    let past = err("today 9am");
    assert_eq!(past.kind, TimeResolveErrorKind::InPast);
    assert_eq!(
        past.message,
        "Tuesday 7 May, 09:00 has already passed. Try \"tomorrow 9am\" or \"in 2h\"."
    );
    assert_eq!(err("in 0h").kind, TimeResolveErrorKind::InPast);
    assert_eq!(err("in -1h").kind, TimeResolveErrorKind::InPast);
    assert_eq!(
        err("2020-01-01T00:00:00Z").kind,
        TimeResolveErrorKind::InPast
    );
    assert_eq!(err("today 1").kind, TimeResolveErrorKind::InPast);
}

#[test]
fn no_error_message_uses_an_em_dash() {
    for input in ["", "frday", "today", "today 9am", "fri tomorrow", "31 feb"] {
        let err = resolve_time(input, &anchor(), &TimePrefs::default()).unwrap_err();
        assert!(
            !err.message.contains('\u{2014}'),
            "{input:?}: {}",
            err.message
        );
    }
}

#[test]
fn tonight_after_the_evening_hour_is_in_the_past() {
    let late = london(2024, 5, 7, 20, 0);
    let err = resolve_time("tonight", &late, &TimePrefs::default()).unwrap_err();
    assert_eq!(err.kind, TimeResolveErrorKind::InPast);
    assert_eq!(
        local(&resolve_at("tonight 11pm", &late)),
        "Tue 2024-05-07 23:00"
    );
}

#[test]
fn eod_after_five_rolls_to_tomorrow_and_eow_to_next_week() {
    let friday_evening = london(2024, 5, 10, 18, 0);
    assert_eq!(
        local(&resolve_at("eod", &friday_evening)),
        "Sat 2024-05-11 17:00"
    );
    assert_eq!(
        local(&resolve_at("eow", &friday_evening)),
        "Fri 2024-05-17 17:00"
    );
}

#[test]
fn preferred_hours_come_from_prefs() {
    let prefs = TimePrefs {
        morning_hour: 7,
        evening_hour: 20,
        weekend_day: Weekday::Sun,
        weekend_hour: 11,
    };
    let at = |input: &str| {
        let resolution = resolve_time(input, &anchor(), &prefs).unwrap();
        local(&resolution)
    };
    assert_eq!(at("tomorrow"), "Wed 2024-05-08 07:00");
    assert_eq!(at("tonight"), "Tue 2024-05-07 20:00");
    assert_eq!(at("weekend"), "Sun 2024-05-12 11:00");
}

#[test]
fn spring_forward_gap_moves_forward_by_the_gap() {
    // Clocks go forward at 01:00 GMT on Sunday 31 March 2024.
    let saturday = london(2024, 3, 30, 12, 0);
    let resolution = resolve_at("tomorrow 1:30am", &saturday);
    assert_eq!(
        resolution.at,
        Utc.with_ymd_and_hms(2024, 3, 31, 1, 30, 0).unwrap()
    );
    assert_eq!(resolution.choices[0].time_label, "02:30");
    assert!(resolution.choices[0].note.is_some());

    // A day keeps the wall-clock time; 24 hours doesn't.
    assert_eq!(
        local(&resolve_at("in 1d", &saturday)),
        "Sun 2024-03-31 12:00"
    );
    assert_eq!(
        local(&resolve_at("in 24h", &saturday)),
        "Sun 2024-03-31 13:00"
    );
}

#[test]
fn fall_back_overlap_takes_the_first_occurrence() {
    // Clocks go back at 02:00 BST on Sunday 27 October 2024.
    let saturday = london(2024, 10, 26, 12, 0);
    let resolution = resolve_at("tomorrow 1:30am", &saturday);
    assert_eq!(
        resolution.at,
        Utc.with_ymd_and_hms(2024, 10, 27, 0, 30, 0).unwrap()
    );
    assert!(resolution.choices[0].note.is_some());

    let nine = resolve_at("tomorrow 9am", &saturday);
    assert_eq!(
        nine.at,
        Utc.with_ymd_and_hms(2024, 10, 27, 9, 0, 0).unwrap()
    );
    assert!(nine.choices[0].note.is_none());
}
