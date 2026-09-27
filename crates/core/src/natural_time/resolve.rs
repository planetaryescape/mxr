//! Turn a parsed [`Phrase`] into instants in a time zone, and describe them.

use chrono::{
    DateTime, Datelike, Duration, MappedLocalTime, Months, NaiveDate, NaiveDateTime, NaiveTime,
    TimeZone, Utc, Weekday,
};

use super::grammar::{DateSpec, DurationUnit, GrammarError, Parsed, Period, Phrase, TimeSpec};
use super::{
    ImpliedPart, TimeChoice, TimePrefs, TimeResolution, TimeResolveError, TimeResolveErrorKind,
    TimeSpan,
};

/// Hours treated as working hours when a phrase leaves out am/pm. An hour
/// from 1 to 12 always has exactly one reading inside this window: 1 to 7
/// read as afternoon or evening, 8 to 12 as morning or noon.
const WORKING_HOURS: std::ops::Range<u32> = 8..20;
const AFTERNOON_HOUR: u32 = 14;
const END_OF_DAY_HOUR: u32 = 17;
const MAX_CHOICES: usize = 3;

const EXAMPLES: &str = "Try \"fri 3pm\" or \"in 2d\".";

pub(super) fn grammar_error(error: GrammarError) -> TimeResolveError {
    match error {
        GrammarError::Empty => TimeResolveError {
            kind: TimeResolveErrorKind::Empty,
            message: "Type a time, like \"tomorrow 9am\" or \"in 2h\".".into(),
            token: None,
            understood: Vec::new(),
        },
        GrammarError::Unrecognized { token, understood } => TimeResolveError {
            kind: TimeResolveErrorKind::Unrecognized,
            message: format!("Didn't catch \"{token}\". {EXAMPLES}"),
            token: Some(token),
            understood,
        },
        GrammarError::Conflict { token, understood } => TimeResolveError {
            kind: TimeResolveErrorKind::Conflict,
            message: format!(
                "\"{token}\" clashes with an earlier part. Give one day and one time, like \"fri 3pm\"."
            ),
            token: Some(token),
            understood,
        },
    }
}

fn error(kind: TimeResolveErrorKind, message: String, spans: &[TimeSpan]) -> TimeResolveError {
    TimeResolveError {
        kind,
        message,
        token: None,
        understood: spans.to_vec(),
    }
}

/// How to move a date forward when the first reading is already past.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Roll {
    Never,
    Day,
    Week,
    Year,
}

struct Candidate<Tz: TimeZone> {
    at: DateTime<Tz>,
    implied: Vec<ImpliedPart>,
    note: Option<String>,
}

pub(super) fn resolve<Tz>(
    input: &str,
    parsed: Parsed,
    now: &DateTime<Tz>,
    prefs: &TimePrefs,
) -> Result<TimeResolution, TimeResolveError>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let spans = parsed.spans;
    let candidates = match parsed.phrase {
        Phrase::Absolute(at) => {
            let at = at.with_timezone(&now.timezone());
            ensure_future(&at, now, &spans)?;
            vec![Candidate {
                at,
                implied: Vec::new(),
                note: None,
            }]
        }
        Phrase::LocalIso(naive) => {
            let (at, note) = localize(&now.timezone(), naive).ok_or_else(|| invalid(&spans))?;
            ensure_future(&at, now, &spans)?;
            vec![Candidate {
                at,
                implied: Vec::new(),
                note,
            }]
        }
        Phrase::Offset(parts) => vec![offset(&parts, now, &spans)?],
        Phrase::Calendar { date, time, period } => {
            calendar(date, time, period, now, prefs, &spans)?
        }
    };
    let choices = describe(&candidates, now);
    let Some(first) = choices.first() else {
        return Err(in_past(None, &spans));
    };
    Ok(TimeResolution {
        input: input.to_string(),
        at: first.at,
        description: format!("{}, {}", first.date_label, first.time_label),
        spans,
        choices,
    })
}

fn ensure_future<Tz: TimeZone>(
    at: &DateTime<Tz>,
    now: &DateTime<Tz>,
    spans: &[TimeSpan],
) -> Result<(), TimeResolveError> {
    if at > now {
        Ok(())
    } else {
        Err(in_past(Some(at.naive_local()), spans))
    }
}

fn in_past(at: Option<NaiveDateTime>, spans: &[TimeSpan]) -> TimeResolveError {
    let message = match at {
        Some(at) => format!(
            "{} has already passed. Try \"tomorrow 9am\" or \"in 2h\".",
            at.format("%A %-d %B, %H:%M")
        ),
        None => "That time has already passed. Try \"tomorrow 9am\" or \"in 2h\".".into(),
    };
    error(TimeResolveErrorKind::InPast, message, spans)
}

fn invalid(spans: &[TimeSpan]) -> TimeResolveError {
    error(
        TimeResolveErrorKind::InvalidDate,
        format!("That date doesn't exist. {EXAMPLES}"),
        spans,
    )
}

/// Map a wall-clock time to an instant. A time skipped by a forward clock
/// change moves forward by the gap (01:30 becomes 02:30 in London); a time
/// that happens twice when clocks go back takes the first occurrence.
fn localize<Tz: TimeZone>(tz: &Tz, naive: NaiveDateTime) -> Option<(DateTime<Tz>, Option<String>)> {
    match tz.from_local_datetime(&naive) {
        MappedLocalTime::Single(single) => return Some((single, None)),
        MappedLocalTime::Ambiguous(earliest, _) => {
            return Some((
                earliest,
                Some(format!(
                    "{} happens twice as clocks go back; this is the first.",
                    naive.format("%H:%M")
                )),
            ))
        }
        MappedLocalTime::None => {}
    }
    [60, 30, 120].into_iter().find_map(|minutes| {
        let shifted = naive + Duration::minutes(minutes);
        let at = tz.from_local_datetime(&shifted).earliest()?;
        let note = format!(
            "{} is skipped as clocks go forward, so this is {}.",
            naive.format("%H:%M"),
            shifted.format("%H:%M")
        );
        Some((at, Some(note)))
    })
}

fn offset<Tz: TimeZone>(
    parts: &[(i64, DurationUnit)],
    now: &DateTime<Tz>,
    spans: &[TimeSpan],
) -> Result<Candidate<Tz>, TimeResolveError> {
    if parts.iter().any(|(amount, _)| *amount <= 0) {
        return Err(in_past(None, spans));
    }
    // Days, weeks and months move the calendar and keep the wall-clock time,
    // so "in 1d" across a clock change is still the same time tomorrow.
    // Hours and minutes are elapsed time.
    let mut wall = now.naive_local();
    let mut elapsed = Duration::zero();
    for (amount, unit) in parts {
        let amount = *amount;
        match unit {
            DurationUnit::Minutes => elapsed += Duration::minutes(amount),
            DurationUnit::Hours => elapsed += Duration::hours(amount),
            DurationUnit::Days => wall += Duration::days(amount),
            DurationUnit::Weeks => wall += Duration::weeks(amount),
            DurationUnit::Months => {
                let months = u32::try_from(amount).map_err(|_| invalid(spans))?;
                wall = wall
                    .checked_add_months(Months::new(months))
                    .ok_or_else(|| invalid(spans))?;
            }
        }
    }
    let (base, note) = if wall == now.naive_local() {
        (now.clone(), None)
    } else {
        localize(&now.timezone(), wall).ok_or_else(|| invalid(spans))?
    };
    let at = base + elapsed;
    ensure_future(&at, now, spans)?;
    Ok(Candidate {
        at,
        implied: Vec::new(),
        note,
    })
}

fn calendar<Tz: TimeZone>(
    date: Option<DateSpec>,
    time: Option<TimeSpec>,
    period: Option<Period>,
    now: &DateTime<Tz>,
    prefs: &TimePrefs,
    spans: &[TimeSpan],
) -> Result<Vec<Candidate<Tz>>, TimeResolveError> {
    let today = now.date_naive();
    let times = times(date, time, period, prefs, spans)?;
    let (base, roll, mut date_implied) = base_date(date, today, prefs, spans)?;
    if date.is_none() {
        date_implied.push(ImpliedPart::Date);
    }

    let tz = now.timezone();
    // Named in the "already passed" message when no reading is ahead.
    let first_attempt = times.first().map(|(clock, _)| base.and_time(*clock));
    let mut candidates: Vec<Candidate<Tz>> = Vec::new();
    for (clock, time_implied) in times {
        let mut day = base;
        let mut attempt = localize(&tz, day.and_time(clock));
        let past = attempt.as_ref().is_none_or(|(at, _)| at <= now);
        if past && roll != Roll::Never {
            day = rolled(day, roll).ok_or_else(|| invalid(spans))?;
            attempt = localize(&tz, day.and_time(clock));
        }
        let Some((at, note)) = attempt else { continue };
        if at <= *now || candidates.iter().any(|existing| existing.at == at) {
            continue;
        }
        let mut implied = date_implied.clone();
        implied.extend(time_implied);
        candidates.push(Candidate { at, implied, note });
    }
    if candidates.is_empty() {
        return Err(in_past(first_attempt, spans));
    }
    candidates.truncate(MAX_CHOICES);
    Ok(candidates)
}

/// Candidate wall-clock times, preferred first, each with what was assumed.
fn times(
    date: Option<DateSpec>,
    time: Option<TimeSpec>,
    period: Option<Period>,
    prefs: &TimePrefs,
    spans: &[TimeSpan],
) -> Result<Vec<(NaiveTime, Vec<ImpliedPart>)>, TimeResolveError> {
    let at = |hour: u32, minute: u32| NaiveTime::from_hms_opt(hour.min(23), minute.min(59), 0);
    let hours: Vec<(u32, u32, Vec<ImpliedPart>)> = match (time, period) {
        (Some(TimeSpec::Exact { hour, minute }), _) => vec![(hour, minute, Vec::new())],
        (Some(TimeSpec::EndOfDay), _) => vec![(END_OF_DAY_HOUR, 0, Vec::new())],
        (Some(TimeSpec::Ambiguous { hour, minute }), Some(period)) => {
            let hour = match period {
                Period::Morning => hour % 12,
                Period::Afternoon | Period::Evening => hour % 12 + 12,
            };
            vec![(hour, minute, Vec::new())]
        }
        (Some(TimeSpec::Ambiguous { hour, minute }), None) => {
            let morning = hour % 12;
            let evening = morning + 12;
            let (preferred, other) = if WORKING_HOURS.contains(&morning) {
                (morning, evening)
            } else {
                (evening, morning)
            };
            vec![
                (preferred, minute, vec![ImpliedPart::Meridiem]),
                (other, minute, vec![ImpliedPart::Meridiem]),
            ]
        }
        (None, Some(period)) => {
            let hour = match period {
                Period::Morning => u32::from(prefs.morning_hour),
                Period::Afternoon => AFTERNOON_HOUR,
                Period::Evening => u32::from(prefs.evening_hour),
            };
            vec![(hour, 0, vec![ImpliedPart::Time])]
        }
        (None, None) => {
            let hour = match date {
                Some(DateSpec::Today) | None => {
                    return Err(error(
                        TimeResolveErrorKind::NeedsTime,
                        "Add a time, like \"today 5pm\" or \"in 2h\".".into(),
                        spans,
                    ))
                }
                Some(DateSpec::EndOfWeek) => END_OF_DAY_HOUR,
                Some(DateSpec::Weekend | DateSpec::Weekday(Weekday::Sat | Weekday::Sun)) => {
                    u32::from(prefs.weekend_hour)
                }
                Some(_) => u32::from(prefs.morning_hour),
            };
            vec![(hour, 0, vec![ImpliedPart::Time])]
        }
    };
    Ok(hours
        .into_iter()
        .filter_map(|(hour, minute, implied)| Some((at(hour, minute)?, implied)))
        .collect())
}

/// The first date a day phrase can mean, how to roll it forward if that is
/// already past, and what was assumed.
fn base_date(
    date: Option<DateSpec>,
    today: NaiveDate,
    prefs: &TimePrefs,
    spans: &[TimeSpan],
) -> Result<(NaiveDate, Roll, Vec<ImpliedPart>), TimeResolveError> {
    let next = |target: Weekday, include_today: bool| {
        let start = if include_today { 0 } else { 1 };
        (start..start + 7)
            .map(|offset| today + Duration::days(offset))
            .find(|day| day.weekday() == target)
            .unwrap_or(today)
    };
    let resolved = match date {
        None => (today, Roll::Day, Vec::new()),
        Some(DateSpec::Today) => (today, Roll::Never, Vec::new()),
        Some(DateSpec::Tomorrow) => (today + Duration::days(1), Roll::Never, Vec::new()),
        Some(DateSpec::Weekday(day)) => (next(day, false), Roll::Never, Vec::new()),
        Some(DateSpec::Weekend) => (next(prefs.weekend_day, false), Roll::Never, Vec::new()),
        Some(DateSpec::NextWeek) => (next(Weekday::Mon, false), Roll::Never, Vec::new()),
        Some(DateSpec::EndOfWeek) => (next(Weekday::Fri, true), Roll::Week, Vec::new()),
        Some(DateSpec::NextMonth) => {
            let first = today
                .with_day(1)
                .and_then(|day| day.checked_add_months(Months::new(1)))
                .ok_or_else(|| invalid(spans))?;
            (first, Roll::Never, Vec::new())
        }
        Some(DateSpec::MonthDay {
            month,
            day,
            year: Some(year),
        }) => {
            let date = NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| invalid(spans))?;
            (date, Roll::Never, Vec::new())
        }
        Some(DateSpec::MonthDay {
            month,
            day,
            year: None,
        }) => {
            // Feb 29 means the next leap year that has it.
            let date = (today.year()..today.year() + 8)
                .find_map(|year| {
                    NaiveDate::from_ymd_opt(year, month, day).filter(|date| *date >= today)
                })
                .ok_or_else(|| invalid(spans))?;
            (date, Roll::Year, vec![ImpliedPart::Year])
        }
        Some(DateSpec::Date(date)) => (date, Roll::Never, Vec::new()),
    };
    Ok(resolved)
}

fn rolled(day: NaiveDate, roll: Roll) -> Option<NaiveDate> {
    match roll {
        Roll::Never => Some(day),
        Roll::Day => Some(day + Duration::days(1)),
        Roll::Week => Some(day + Duration::weeks(1)),
        Roll::Year => (1..=8)
            .find_map(|years| NaiveDate::from_ymd_opt(day.year() + years, day.month(), day.day())),
    }
}

fn describe<Tz>(candidates: &[Candidate<Tz>], now: &DateTime<Tz>) -> Vec<TimeChoice>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let date_label = |at: &DateTime<Tz>| {
        if at.year() == now.year() {
            at.format("%A %-d %B").to_string()
        } else {
            at.format("%A %-d %B %Y").to_string()
        }
    };
    let same_day = candidates
        .windows(2)
        .all(|pair| pair[0].at.date_naive() == pair[1].at.date_naive());
    candidates
        .iter()
        .map(|candidate| {
            let time_label = candidate.at.format("%H:%M").to_string();
            let label = if same_day {
                time_label.clone()
            } else {
                candidate.at.format("%a %-d %b, %H:%M").to_string()
            };
            TimeChoice {
                at: candidate.at.with_timezone(&Utc),
                local: candidate.at.to_rfc3339(),
                label,
                date_label: date_label(&candidate.at),
                time_label,
                relative_label: relative_label(&candidate.at, now),
                implied: candidate.implied.clone(),
                note: candidate.note.clone(),
            }
        })
        .collect()
}

fn plural(count: i64, unit: &str) -> String {
    if count == 1 {
        format!("in 1 {unit}")
    } else {
        format!("in {count} {unit}s")
    }
}

fn relative_label<Tz: TimeZone>(at: &DateTime<Tz>, now: &DateTime<Tz>) -> String {
    let minutes = (at.clone() - now.clone()).num_minutes();
    if minutes < 1 {
        return "in under a minute".into();
    }
    if minutes < 60 {
        return plural(minutes, "minute");
    }
    let days = (at.date_naive() - now.date_naive()).num_days();
    if minutes < 12 * 60 || days == 0 {
        return plural((minutes + 30) / 60, "hour");
    }
    match days {
        1 => "tomorrow".into(),
        2..=13 => plural(days, "day"),
        14..=59 => plural(days / 7, "week"),
        _ => plural(days / 30, "month"),
    }
}
