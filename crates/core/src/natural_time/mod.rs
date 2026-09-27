//! Natural-language time for snooze, send later, reminders and every other
//! place mxr asks for a time.
//!
//! One parser serves the CLI, TUI, daemon and web bridge so a phrase
//! resolves to the same instant everywhere. It works in the caller's time
//! zone (production passes [`chrono::Local`]; tests inject a zone) and takes
//! the user's preferred hours as plain data so this crate does not depend on
//! config.
//!
//! The result says what was understood (byte spans), what was assumed
//! ([`ImpliedPart`]), and offers up to three [`TimeChoice`]s when a phrase
//! has more than one sensible reading, such as "fri 3" (15:00 or 03:00). The
//! first choice is the default: the next future occurrence, preferring
//! working hours when the am/pm is missing.
//!
//! Accepted phrases:
//!
//! * Offsets: "in 2h", "in 5d", "in 30m", "in 1w", "in 2 months", "3d",
//!   "2w", "in an hour", "in 2h 30m".
//! * Days: "today" (needs a time), "tomorrow"/"tom", "tonight", "weekend",
//!   weekday names and abbreviations ("fri", "tue", "thurs"), "next monday",
//!   "next week" (Monday), "next month" (the 1st), "end of week"/"eow"
//!   (Friday 17:00), "3 oct", "oct 3", "3rd october 2027", "2026-10-03".
//! * Times: "9am", "9 am", "5pm", "9:30am", "17:00", "09:30", "noon",
//!   "midnight", "eod"/"end of day" (17:00), "morning", "afternoon",
//!   "evening"/"night", and a bare hour after a day ("fri 3").
//! * Absolute: RFC3339 ("2026-06-01T15:00:00Z") and ISO 8601 without an
//!   offset, read as local time.
//!
//! Days and times combine in either order with filler words: "fri at 3pm",
//! "3pm fri", "tomorrow morning", "tonight 9". A named weekday always means
//! the next one, never today. A time with no day means its next occurrence.

mod grammar;
mod resolve;

#[cfg(test)]
mod tests;

use chrono::{DateTime, Local, TimeZone, Utc, Weekday};
use serde::{Deserialize, Serialize};

/// The user's preferred hours, taken from the snooze config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimePrefs {
    /// Used when a phrase names a day but no time ("tomorrow", "fri").
    pub morning_hour: u8,
    /// Used for "tonight" and "evening".
    pub evening_hour: u8,
    /// The day "weekend" means.
    pub weekend_day: Weekday,
    /// Used for "weekend", "sat" and "sun" without a time.
    pub weekend_hour: u8,
}

impl Default for TimePrefs {
    fn default() -> Self {
        Self {
            morning_hour: 9,
            evening_hour: 18,
            weekend_day: Weekday::Sat,
            weekend_hour: 10,
        }
    }
}

/// A byte range of the input the parser understood.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TimeSpan {
    pub start: usize,
    pub end: usize,
}

/// A part of the result the user did not type and the parser assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ImpliedPart {
    /// No day was given ("3pm" means today or tomorrow).
    Date,
    /// No year was given ("3 oct").
    Year,
    /// No time was given, so a preferred hour was used.
    Time,
    /// An hour without am/pm ("fri 3").
    Meridiem,
}

impl ImpliedPart {
    /// How the CLI and TUI name the assumption: "Assumed am or pm".
    pub const fn label(self) -> &'static str {
        match self {
            Self::Date => "the day",
            Self::Year => "the year",
            Self::Time => "the time",
            Self::Meridiem => "am or pm",
        }
    }
}

/// One reading of the phrase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TimeChoice {
    /// The instant to store. Send this back to the mutation unchanged so
    /// what was previewed is what is saved.
    pub at: DateTime<Utc>,
    /// The same instant as RFC3339 in the resolving zone.
    pub local: String,
    /// Short text for a choice chip, such as "15:00".
    pub label: String,
    /// "Friday 3 October", with the year only when it isn't this year.
    pub date_label: String,
    /// "15:00".
    pub time_label: String,
    /// "in 6 days", "tomorrow", "in 2 hours".
    pub relative_label: String,
    pub implied: Vec<ImpliedPart>,
    /// Set when a clock change moved or doubled the requested wall time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl TimeChoice {
    /// "Friday 3 October, 15:00 (in 6 days)", the one-line form terminal
    /// clients print.
    pub fn summary(&self) -> String {
        format!(
            "{}, {} ({})",
            self.date_label, self.time_label, self.relative_label
        )
    }

    /// "the day, am or pm", or empty when nothing was assumed.
    pub fn assumed(&self) -> String {
        self.implied
            .iter()
            .map(|part| part.label())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A phrase resolved to at least one instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TimeResolution {
    pub input: String,
    /// The default choice's instant.
    pub at: DateTime<Utc>,
    /// "Friday 3 October, 15:00" for the default choice.
    pub description: String,
    /// Byte ranges of `input` that were understood.
    pub spans: Vec<TimeSpan>,
    /// Every reading, default first. More than one means the phrase was
    /// ambiguous.
    pub choices: Vec<TimeChoice>,
}

impl TimeResolution {
    pub fn is_ambiguous(&self) -> bool {
        self.choices.len() > 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TimeResolveErrorKind {
    Empty,
    /// A word the parser doesn't know.
    Unrecognized,
    /// Two parts that set the same thing ("fri tomorrow").
    Conflict,
    /// A day with no time where the time can't be assumed ("today").
    NeedsTime,
    /// A date that doesn't exist ("31 feb").
    InvalidDate,
    InPast,
}

/// Why a phrase didn't resolve, phrased for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TimeResolveError {
    pub kind: TimeResolveErrorKind,
    /// A calm, specific message with an example that works.
    pub message: String,
    /// The word that wasn't understood, for `unrecognized` and `conflict`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Byte ranges understood before the problem.
    pub understood: Vec<TimeSpan>,
}

impl std::fmt::Display for TimeResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TimeResolveError {}

/// Resolve `input` against `now` in `now`'s time zone.
pub fn resolve_time<Tz>(
    input: &str,
    now: &DateTime<Tz>,
    prefs: &TimePrefs,
) -> Result<TimeResolution, TimeResolveError>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let parsed = grammar::parse(input).map_err(resolve::grammar_error)?;
    resolve::resolve(input, parsed, now, prefs)
}

/// Resolve `input` against the current time in the local time zone.
pub fn resolve_time_local(
    input: &str,
    prefs: &TimePrefs,
) -> Result<TimeResolution, TimeResolveError> {
    resolve_time(input, &Local::now(), prefs)
}
