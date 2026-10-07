//! Digest cuts: the fixed times a day the briefing is gathered, 08:00 and
//! 16:30 by default, one to four (blueprint 22, Updates' rhythm). A cut
//! is a stable set: everything that arrived by the cut and wasn't let go.
//! Mail after the latest cut waits in the "since" section.

use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Timelike, Utc, Weekday};

/// The default cuts, local time.
pub const DEFAULT_CUTS: [&str; 2] = ["08:00", "16:30"];
pub const MAX_CUTS: usize = 4;

/// A bad `updates.cuts` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutError(pub String);

impl std::fmt::Display for CutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CutError {}

/// The day's cuts, sorted, local time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cuts(Vec<NaiveTime>);

impl Default for Cuts {
    fn default() -> Self {
        Self::parse(&DEFAULT_CUTS.map(str::to_string)).unwrap_or_else(|_| Self(Vec::new()))
    }
}

/// Where the clock sits between cuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutWindow {
    /// The latest cut at or before now: the digest holds mail up to here.
    pub at: DateTime<Utc>,
    /// The cut before it.
    pub previous: DateTime<Utc>,
    /// The next cut after now.
    pub next: DateTime<Utc>,
}

impl Cuts {
    /// One to four "HH:MM" times, each once.
    pub fn parse(values: &[String]) -> Result<Self, CutError> {
        if values.is_empty() || values.len() > MAX_CUTS {
            return Err(CutError(format!(
                "updates.cuts takes 1 to {MAX_CUTS} times, like [\"08:00\", \"16:30\"]; got {}",
                values.len()
            )));
        }
        let mut times = Vec::with_capacity(values.len());
        for value in values {
            let time = NaiveTime::parse_from_str(value.trim(), "%H:%M").map_err(|_| {
                CutError(format!(
                    "updates.cuts: \"{value}\" is not a time like 08:00"
                ))
            })?;
            if times.contains(&time) {
                return Err(CutError(format!("updates.cuts: {value} is listed twice")));
            }
            times.push(time);
        }
        times.sort_unstable();
        Ok(Self(times))
    }

    /// "08:00" and "16:30".
    pub fn labels(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|t| t.format("%H:%M").to_string())
            .collect()
    }

    /// Every cut instant from `days_back` days ago through tomorrow.
    fn instants<Tz: TimeZone>(
        &self,
        now: DateTime<Utc>,
        tz: &Tz,
        days_back: i64,
    ) -> Vec<DateTime<Utc>> {
        let today = now.with_timezone(tz).date_naive();
        let mut out = Vec::new();
        for offset in -days_back..=1 {
            let Some(day) = today.checked_add_signed(Duration::days(offset)) else {
                continue;
            };
            for time in &self.0 {
                // A cut inside a DST gap moves to the next valid instant.
                let local = day.and_time(*time);
                let at = tz.from_local_datetime(&local).earliest().or_else(|| {
                    tz.from_local_datetime(&(local + Duration::hours(1)))
                        .earliest()
                });
                if let Some(at) = at {
                    out.push(at.with_timezone(&Utc));
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The cuts around `now`.
    pub fn window<Tz: TimeZone>(&self, now: DateTime<Utc>, tz: &Tz) -> CutWindow {
        let instants = self.instants(now, tz, 2);
        let latest = instants.iter().rposition(|at| *at <= now);
        match latest {
            Some(index) => CutWindow {
                at: instants[index],
                previous: index
                    .checked_sub(1)
                    .map_or(instants[index] - Duration::days(1), |i| instants[i]),
                next: instants
                    .get(index + 1)
                    .copied()
                    .unwrap_or(instants[index] + Duration::days(1)),
            },
            // Unreachable with at least one cut a day; a safe fallback.
            None => CutWindow {
                at: now,
                previous: now - Duration::days(1),
                next: now + Duration::days(1),
            },
        }
    }
}

/// "This morning's digest", "Yesterday afternoon's digest".
pub fn digest_title<Tz: TimeZone>(cut: DateTime<Utc>, now: DateTime<Utc>, tz: &Tz) -> String {
    let local = cut.with_timezone(tz);
    let part = match local.hour() {
        0..=11 => "morning",
        12..=16 => "afternoon",
        _ => "evening",
    };
    let today = now.with_timezone(tz).date_naive();
    let day = local.date_naive();
    if day == today {
        if part == "evening" {
            "This evening's digest".to_string()
        } else {
            format!("This {part}'s digest")
        }
    } else if today.pred_opt() == Some(day) {
        format!("Yesterday {part}'s digest")
    } else {
        let weekday = match local.weekday() {
            Weekday::Mon => "Monday",
            Weekday::Tue => "Tuesday",
            Weekday::Wed => "Wednesday",
            Weekday::Thu => "Thursday",
            Weekday::Fri => "Friday",
            Weekday::Sat => "Saturday",
            Weekday::Sun => "Sunday",
        };
        format!("{weekday} {part}'s digest")
    }
}

/// "08:00" in the user's zone.
pub fn time_label<Tz: TimeZone>(at: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    at.with_timezone(tz).format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn parses_one_to_four_unique_times() {
        assert_eq!(Cuts::default().labels(), vec!["08:00", "16:30"]);
        let three = Cuts::parse(&["18:00".into(), "07:30".into(), "12:00".into()]).unwrap();
        assert_eq!(three.labels(), vec!["07:30", "12:00", "18:00"]);
        assert!(Cuts::parse(&[]).is_err());
        assert!(
            Cuts::parse(&["1".into(), "2".into(), "3".into(), "4".into(), "5".into()]).is_err()
        );
        assert!(Cuts::parse(&["8am".into()]).is_err());
        assert!(Cuts::parse(&["08:00".into(), "08:00".into()]).is_err());
    }

    #[test]
    fn the_window_is_the_latest_cut_its_previous_and_the_next() {
        let cuts = Cuts::default();
        let noon = cuts.window(at(7, 12, 0), &utc());
        assert_eq!(noon.at, at(7, 8, 0));
        assert_eq!(noon.previous, at(6, 16, 30));
        assert_eq!(noon.next, at(7, 16, 30));
        let early = cuts.window(at(7, 6, 0), &utc());
        assert_eq!(early.at, at(6, 16, 30));
        assert_eq!(early.next, at(7, 8, 0));
        // Exactly on a cut belongs to that cut.
        assert_eq!(cuts.window(at(7, 16, 30), &utc()).at, at(7, 16, 30));
        let one = Cuts::parse(&["09:00".into()])
            .unwrap()
            .window(at(7, 12, 0), &utc());
        assert_eq!(
            (one.previous, one.at, one.next),
            (at(6, 9, 0), at(7, 9, 0), at(8, 9, 0))
        );
    }

    #[test]
    fn titles_name_the_part_of_day() {
        assert_eq!(
            digest_title(at(7, 8, 0), at(7, 12, 0), &utc()),
            "This morning's digest"
        );
        assert_eq!(
            digest_title(at(6, 16, 30), at(7, 6, 0), &utc()),
            "Yesterday afternoon's digest"
        );
        assert_eq!(time_label(at(7, 16, 30), &utc()), "16:30");
    }
}
