//! Tokenise a time phrase and parse it into a [`Phrase`].
//!
//! The grammar is a flat sequence of parts (a day, a clock time, a part of
//! the day, or a duration) in any order, with filler words allowed between
//! them. Keeping it flat means "3pm fri" and "fri at 3pm" both work without
//! a real parser generator, and every consumed token has a byte span the UI
//! can highlight.

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, Weekday};

use super::TimeSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DateSpec {
    Today,
    Tomorrow,
    Weekday(Weekday),
    Weekend,
    NextWeek,
    NextMonth,
    EndOfWeek,
    MonthDay {
        month: u32,
        day: u32,
        year: Option<i32>,
    },
    Date(NaiveDate),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TimeSpec {
    /// A wall-clock time with no doubt about am/pm.
    Exact {
        hour: u32,
        minute: u32,
    },
    /// An hour from 1 to 12 with no am/pm, such as the "3" in "fri 3".
    Ambiguous {
        hour: u32,
        minute: u32,
    },
    EndOfDay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Period {
    Morning,
    Afternoon,
    Evening,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DurationUnit {
    Minutes,
    Hours,
    Days,
    Weeks,
    Months,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Phrase {
    /// RFC3339 with an explicit offset.
    Absolute(DateTime<FixedOffset>),
    /// ISO 8601 without an offset, read as local time.
    LocalIso(NaiveDateTime),
    Offset(Vec<(i64, DurationUnit)>),
    Calendar {
        date: Option<DateSpec>,
        time: Option<TimeSpec>,
        period: Option<Period>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Parsed {
    pub phrase: Phrase,
    pub spans: Vec<TimeSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GrammarError {
    Empty,
    Unrecognized {
        token: String,
        understood: Vec<TimeSpan>,
    },
    Conflict {
        token: String,
        understood: Vec<TimeSpan>,
    },
}

#[derive(Debug, Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    span: TimeSpan,
}

const FILLERS: &[&str] = &["at", "on", "this", "the", "by", "@", "and"];

pub(super) fn parse(input: &str) -> Result<Parsed, GrammarError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(GrammarError::Empty);
    }
    let lead = input.len() - input.trim_start().len();
    let whole = vec![TimeSpan {
        start: lead,
        end: lead + trimmed.len(),
    }];
    if let Ok(absolute) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(Parsed {
            phrase: Phrase::Absolute(absolute),
            spans: whole,
        });
    }
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, format) {
            return Ok(Parsed {
                phrase: Phrase::LocalIso(naive),
                spans: whole,
            });
        }
    }

    // ASCII-only rewrites keep byte offsets identical to the input, so spans
    // computed here index straight into what the user typed.
    let normalized: String = input
        .chars()
        .map(|c| match c {
            '_' | ',' => ' ',
            c => c.to_ascii_lowercase(),
        })
        .collect();
    Parser::new(tokenize(&normalized)).run()
}

fn tokenize(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(begin) = start.take() {
                tokens.push(Token {
                    text: &text[begin..index],
                    span: TimeSpan {
                        start: begin,
                        end: index,
                    },
                });
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(begin) = start {
        tokens.push(Token {
            text: &text[begin..],
            span: TimeSpan {
                start: begin,
                end: text.len(),
            },
        });
    }
    tokens
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    index: usize,
    date: Option<DateSpec>,
    time: Option<TimeSpec>,
    period: Option<Period>,
    offsets: Vec<(i64, DurationUnit)>,
}

impl<'a> Parser<'a> {
    fn new(tokens: Vec<Token<'a>>) -> Self {
        Self {
            tokens,
            index: 0,
            date: None,
            time: None,
            period: None,
            offsets: Vec::new(),
        }
    }

    fn peek(&self, ahead: usize) -> Option<&'a str> {
        self.tokens.get(self.index + ahead).map(|token| token.text)
    }

    fn consume(&mut self, count: usize) {
        self.index += count;
    }

    /// Tokens are consumed strictly in order and parsing stops at the first
    /// unknown one, so what was understood is one run from the first token
    /// to the last consumed one. "fri at 3" highlights as a single phrase.
    fn understood(&self) -> Vec<TimeSpan> {
        match (self.tokens.first(), self.index.checked_sub(1)) {
            (Some(first), Some(last)) => vec![TimeSpan {
                start: first.span.start,
                end: self.tokens[last].span.end,
            }],
            _ => Vec::new(),
        }
    }

    fn unrecognized(&self) -> GrammarError {
        GrammarError::Unrecognized {
            token: self.tokens[self.index].text.to_string(),
            understood: self.understood(),
        }
    }

    fn conflict(&self) -> GrammarError {
        GrammarError::Conflict {
            token: self.tokens[self.index].text.to_string(),
            understood: self.understood(),
        }
    }

    fn set_date(&mut self, date: DateSpec, count: usize) -> Result<(), GrammarError> {
        if self.date.is_some() || !self.offsets.is_empty() {
            return Err(self.conflict());
        }
        self.date = Some(date);
        self.consume(count);
        Ok(())
    }

    fn set_time(&mut self, time: TimeSpec, count: usize) -> Result<(), GrammarError> {
        if self.time.is_some() || !self.offsets.is_empty() {
            return Err(self.conflict());
        }
        self.time = Some(time);
        self.consume(count);
        Ok(())
    }

    fn set_period(&mut self, period: Period, count: usize) -> Result<(), GrammarError> {
        if self.period.is_some() || !self.offsets.is_empty() {
            return Err(self.conflict());
        }
        self.period = Some(period);
        self.consume(count);
        Ok(())
    }

    fn run(mut self) -> Result<Parsed, GrammarError> {
        while self.index < self.tokens.len() {
            self.step()?;
        }
        let spans = self.understood();
        let phrase = if !self.offsets.is_empty() {
            Phrase::Offset(self.offsets)
        } else if self.date.is_none() && self.time.is_none() && self.period.is_none() {
            // Only filler words such as "at" or "this".
            return Err(GrammarError::Unrecognized {
                token: self
                    .tokens
                    .last()
                    .map(|token| token.text.to_string())
                    .unwrap_or_default(),
                understood: Vec::new(),
            });
        } else {
            Phrase::Calendar {
                date: self.date,
                time: self.time,
                period: self.period,
            }
        };
        Ok(Parsed { phrase, spans })
    }

    fn step(&mut self) -> Result<(), GrammarError> {
        let Some(word) = self.peek(0) else {
            return Ok(());
        };
        if FILLERS.contains(&word) {
            self.consume(1);
            return Ok(());
        }
        if word == "in" {
            return self.offsets_after_in();
        }
        if let Some((amount, unit, used)) = self.duration_at(0, false) {
            return self.push_offset(amount, unit, used);
        }
        if word == "next" {
            let date = match self.peek(1) {
                Some("week") => Some(DateSpec::NextWeek),
                Some("month") => Some(DateSpec::NextMonth),
                Some("weekend") => Some(DateSpec::Weekend),
                Some(other) => weekday(other).map(DateSpec::Weekday),
                None => None,
            };
            return match date {
                Some(date) => self.set_date(date, 2),
                None => Err(self.unrecognized()),
            };
        }
        if word == "end" {
            let skip_the = usize::from(self.peek(2) == Some("the"));
            if self.peek(1) == Some("of") {
                match self.peek(2 + skip_the) {
                    Some("day") => return self.set_time(TimeSpec::EndOfDay, 3 + skip_the),
                    Some("week") => return self.set_date(DateSpec::EndOfWeek, 3 + skip_the),
                    _ => {}
                }
            }
            return Err(self.unrecognized());
        }
        match word {
            "eod" | "cob" => return self.set_time(TimeSpec::EndOfDay, 1),
            "eow" => return self.set_date(DateSpec::EndOfWeek, 1),
            "today" => return self.set_date(DateSpec::Today, 1),
            "tomorrow" | "tom" | "tmr" | "tmrw" | "tomorow" => {
                return self.set_date(DateSpec::Tomorrow, 1)
            }
            "tonight" => {
                self.set_date(DateSpec::Today, 0)?;
                return self.set_period(Period::Evening, 1);
            }
            "weekend" => return self.set_date(DateSpec::Weekend, 1),
            "morning" => return self.set_period(Period::Morning, 1),
            "afternoon" => return self.set_period(Period::Afternoon, 1),
            "evening" | "night" => return self.set_period(Period::Evening, 1),
            "noon" | "midday" => {
                return self.set_time(
                    TimeSpec::Exact {
                        hour: 12,
                        minute: 0,
                    },
                    1,
                )
            }
            "midnight" => return self.set_time(TimeSpec::Exact { hour: 0, minute: 0 }, 1),
            _ => {}
        }
        if let Some(day) = weekday(word) {
            return self.set_date(DateSpec::Weekday(day), 1);
        }
        if let Some((date, used)) = self.month_day() {
            return self.set_date(date, used);
        }
        if let Ok(date) = NaiveDate::parse_from_str(word, "%Y-%m-%d") {
            return self.set_date(DateSpec::Date(date), 1);
        }
        if let Some((time, used)) = self.clock() {
            return self.set_time(time, used);
        }
        Err(self.unrecognized())
    }

    fn offsets_after_in(&mut self) -> Result<(), GrammarError> {
        let Some((amount, unit, used)) = self.duration_at(1, true) else {
            return Err(self.unrecognized());
        };
        self.push_offset(amount, unit, used + 1)
    }

    fn push_offset(
        &mut self,
        amount: i64,
        unit: DurationUnit,
        used: usize,
    ) -> Result<(), GrammarError> {
        if self.date.is_some() || self.time.is_some() || self.period.is_some() {
            return Err(self.conflict());
        }
        self.offsets.push((amount, unit));
        self.consume(used);
        Ok(())
    }

    /// A duration starting `ahead` tokens from here: "3d", "3 days", and after
    /// "in" also "a day" / "an hour".
    fn duration_at(&self, ahead: usize, after_in: bool) -> Option<(i64, DurationUnit, usize)> {
        let word = self.peek(ahead)?;
        if let Some((amount, unit)) = glued_duration(word) {
            return Some((amount, unit, 1));
        }
        let amount = match word {
            "a" | "an" | "one" if after_in => 1,
            _ => signed_number(word)?,
        };
        let unit = duration_unit(self.peek(ahead + 1)?)?;
        Some((amount, unit, 2))
    }

    /// "oct 3", "october 3rd 2027", "3 oct", "3rd of october".
    fn month_day(&self) -> Option<(DateSpec, usize)> {
        let first = self.peek(0)?;
        let (month, day, mut used) = if let Some(month) = month(first) {
            (month, day_of_month(self.peek(1)?)?, 2)
        } else {
            let day = day_of_month(first)?;
            let skip_of = usize::from(self.peek(1) == Some("of"));
            (month(self.peek(1 + skip_of)?)?, day, 2 + skip_of)
        };
        let year = self.peek(used).and_then(year);
        if year.is_some() {
            used += 1;
        }
        Some((DateSpec::MonthDay { month, day, year }, used))
    }

    /// "9am", "9 am", "9:30pm", "17:00", "09:30", "3".
    fn clock(&self) -> Option<(TimeSpec, usize)> {
        let word = self.peek(0)?;
        if let Some(meridiem) = self.peek(1).and_then(meridiem) {
            if let Some(time) = clock_with_meridiem(word, meridiem) {
                return Some((time, 2));
            }
        }
        for (suffix, pm) in [("am", false), ("pm", true), ("a", false), ("p", true)] {
            if let Some(stem) = word.strip_suffix(suffix) {
                return clock_with_meridiem(stem, pm).map(|time| (time, 1));
            }
        }
        let (hour_text, minute) = split_clock(word)?;
        let hour: u32 = hour_text.parse().ok()?;
        if hour > 23 {
            return None;
        }
        let zero_padded = hour_text.len() == 2 && hour_text.starts_with('0');
        let time = if hour == 0 || hour >= 13 || zero_padded {
            TimeSpec::Exact { hour, minute }
        } else {
            TimeSpec::Ambiguous { hour, minute }
        };
        Some((time, 1))
    }
}

fn clock_with_meridiem(stem: &str, pm: bool) -> Option<TimeSpec> {
    let (hour_text, minute) = split_clock(stem)?;
    let hour: u32 = hour_text.parse().ok()?;
    if !(1..=12).contains(&hour) {
        return None;
    }
    // 12am is midnight and 12pm is noon.
    let hour = match (hour, pm) {
        (12, false) => 0,
        (12, true) => 12,
        (hour, true) => hour + 12,
        (hour, false) => hour,
    };
    Some(TimeSpec::Exact { hour, minute })
}

/// Split "9" or "9:30" into the hour text and the minute.
fn split_clock(text: &str) -> Option<(&str, u32)> {
    let (hour, minute) = match text.split_once(':') {
        Some((hour, minute)) if minute.len() == 2 => (hour, minute.parse().ok()?),
        Some(_) => return None,
        None => (text, 0),
    };
    let digits_only =
        !hour.is_empty() && hour.len() <= 2 && hour.bytes().all(|b| b.is_ascii_digit());
    (digits_only && minute <= 59).then_some((hour, minute))
}

fn meridiem(word: &str) -> Option<bool> {
    match word {
        "am" | "a.m." => Some(false),
        "pm" | "p.m." => Some(true),
        _ => None,
    }
}

fn signed_number(word: &str) -> Option<i64> {
    let digits = word.strip_prefix('-').unwrap_or(word);
    if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    word.parse().ok()
}

fn glued_duration(word: &str) -> Option<(i64, DurationUnit)> {
    let split = word
        .char_indices()
        .find(|(index, c)| !(c.is_ascii_digit() || (*index == 0 && *c == '-')))
        .map(|(index, _)| index)?;
    let (number, unit) = word.split_at(split);
    Some((signed_number(number)?, duration_unit(unit)?))
}

fn duration_unit(word: &str) -> Option<DurationUnit> {
    match word {
        "m" | "min" | "mins" | "minute" | "minutes" => Some(DurationUnit::Minutes),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(DurationUnit::Hours),
        "d" | "day" | "days" => Some(DurationUnit::Days),
        "w" | "wk" | "wks" | "week" | "weeks" => Some(DurationUnit::Weeks),
        "mo" | "mos" | "month" | "months" => Some(DurationUnit::Months),
        _ => None,
    }
}

fn weekday(word: &str) -> Option<Weekday> {
    match word {
        "monday" | "mon" => Some(Weekday::Mon),
        "tuesday" | "tue" | "tues" => Some(Weekday::Tue),
        "wednesday" | "wed" | "weds" => Some(Weekday::Wed),
        "thursday" | "thu" | "thur" | "thurs" => Some(Weekday::Thu),
        "friday" | "fri" => Some(Weekday::Fri),
        "saturday" | "sat" => Some(Weekday::Sat),
        "sunday" | "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

fn month(word: &str) -> Option<u32> {
    let month = match word {
        "jan" | "january" => 1,
        "feb" | "february" => 2,
        "mar" | "march" => 3,
        "apr" | "april" => 4,
        "may" => 5,
        "jun" | "june" => 6,
        "jul" | "july" => 7,
        "aug" | "august" => 8,
        "sep" | "sept" | "september" => 9,
        "oct" | "october" => 10,
        "nov" | "november" => 11,
        "dec" | "december" => 12,
        _ => return None,
    };
    Some(month)
}

/// "3", "03", "3rd", "21st".
fn day_of_month(word: &str) -> Option<u32> {
    let digits = ["st", "nd", "rd", "th"]
        .iter()
        .find_map(|suffix| word.strip_suffix(suffix))
        .unwrap_or(word);
    if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let day: u32 = digits.parse().ok()?;
    (1..=31).contains(&day).then_some(day)
}

fn year(word: &str) -> Option<i32> {
    if word.len() != 4 || !word.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    word.parse()
        .ok()
        .filter(|year| (1970..=9999).contains(year))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_index_into_the_original_input() {
        let parsed = parse("  Fri at 3  ").unwrap();
        assert_eq!(parsed.spans, vec![TimeSpan { start: 2, end: 10 }]);
    }

    #[test]
    fn unrecognized_token_is_reported_with_understood_spans() {
        let err = parse("fri frday").unwrap_err();
        assert_eq!(
            err,
            GrammarError::Unrecognized {
                token: "frday".into(),
                understood: vec![TimeSpan { start: 0, end: 3 }],
            }
        );
    }

    #[test]
    fn two_days_conflict() {
        assert!(matches!(
            parse("fri tomorrow"),
            Err(GrammarError::Conflict { .. })
        ));
    }
}
