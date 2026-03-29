use crate::types::SnoozeConfig;
use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Utc, Weekday};
use serde::{Deserialize, Serialize};

/// Named snooze options for preset-based snoozing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnoozeOption {
    TomorrowMorning,
    Tonight,
    Weekend,
    NextMonday,
    Custom,
}

/// A displayable snooze preset with label and description.
#[derive(Debug, Clone)]
pub struct SnoozePreset {
    pub option: SnoozeOption,
    pub label: &'static str,
}

/// The four standard snooze presets, in display order.
pub const SNOOZE_PRESETS: [SnoozePreset; 4] = [
    SnoozePreset {
        option: SnoozeOption::TomorrowMorning,
        label: "Tomorrow morning",
    },
    SnoozePreset {
        option: SnoozeOption::Tonight,
        label: "Tonight",
    },
    SnoozePreset {
        option: SnoozeOption::Weekend,
        label: "Weekend",
    },
    SnoozePreset {
        option: SnoozeOption::NextMonday,
        label: "Next Monday",
    },
];

/// Resolve a snooze option to a concrete wake time using the user's config.
pub fn resolve_snooze_time(option: SnoozeOption, config: &SnoozeConfig) -> DateTime<Utc> {
    let now = Local::now();

    match option {
        SnoozeOption::TomorrowMorning => {
            let tomorrow = now.date_naive() + Duration::days(1);
            let time = NaiveTime::from_hms_opt(u32::from(config.morning_hour), 0, 0).unwrap();
            tomorrow
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&Utc)
        }
        SnoozeOption::Tonight => {
            let today = now.date_naive();
            let time = NaiveTime::from_hms_opt(u32::from(config.evening_hour), 0, 0).unwrap();
            let tonight = today
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&Utc);
            if tonight <= Utc::now() {
                tonight + Duration::days(1)
            } else {
                tonight
            }
        }
        SnoozeOption::Weekend => {
            let target_day = match config.weekend_day.as_str() {
                "sunday" => Weekday::Sun,
                _ => Weekday::Sat,
            };
            let days_until = (i64::from(target_day.num_days_from_monday())
                - i64::from(now.weekday().num_days_from_monday())
                + 7)
                % 7;
            let days = if days_until == 0 { 7 } else { days_until };
            let weekend = now.date_naive() + Duration::days(days);
            let time = NaiveTime::from_hms_opt(u32::from(config.weekend_hour), 0, 0).unwrap();
            weekend
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&Utc)
        }
        SnoozeOption::NextMonday => {
            let days_until_monday = (i64::from(Weekday::Mon.num_days_from_monday())
                - i64::from(now.weekday().num_days_from_monday())
                + 7)
                % 7;
            let days = if days_until_monday == 0 {
                7
            } else {
                days_until_monday
            };
            let monday = now.date_naive() + Duration::days(days);
            let time = NaiveTime::from_hms_opt(u32::from(config.morning_hour), 0, 0).unwrap();
            monday
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&Utc)
        }
        SnoozeOption::Custom => Utc::now(), // caller should use Custom datetime directly
    }
}

/// Compute the next occurrence of a weekday at a given hour, using the
/// user's snooze config for the hour when a named keyword is used.
pub fn next_weekday_at(from: DateTime<Utc>, target: Weekday, hour: u32) -> DateTime<Utc> {
    let current = from.weekday().num_days_from_monday();
    let target_day = target.num_days_from_monday();
    let days_ahead = if target_day <= current {
        7 - (current - target_day)
    } else {
        target_day - current
    };
    let date = (from + Duration::days(i64::from(days_ahead))).date_naive();
    let time = NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
    Utc.from_utc_datetime(&date.and_time(time))
}

/// Parse a snooze "until" string (from CLI or API) into a concrete wake time.
///
/// Accepts keywords: "tomorrow", "tonight", "monday", "weekend", plus
/// additional weekday names ("tuesday"..."sunday") and ISO 8601 datetimes.
pub fn parse_snooze_until(until: &str, config: &SnoozeConfig) -> Option<DateTime<Utc>> {
    let lower = until.trim().to_ascii_lowercase();
    match lower.as_str() {
        "tomorrow" | "tomorrow_morning" => {
            Some(resolve_snooze_time(SnoozeOption::TomorrowMorning, config))
        }
        "tonight" => Some(resolve_snooze_time(SnoozeOption::Tonight, config)),
        "weekend" | "saturday" => Some(resolve_snooze_time(SnoozeOption::Weekend, config)),
        "monday" | "next_monday" => Some(resolve_snooze_time(SnoozeOption::NextMonday, config)),
        "tuesday" => Some(next_weekday_at(
            Utc::now(),
            Weekday::Tue,
            u32::from(config.morning_hour),
        )),
        "wednesday" => Some(next_weekday_at(
            Utc::now(),
            Weekday::Wed,
            u32::from(config.morning_hour),
        )),
        "thursday" => Some(next_weekday_at(
            Utc::now(),
            Weekday::Thu,
            u32::from(config.morning_hour),
        )),
        "friday" => Some(next_weekday_at(
            Utc::now(),
            Weekday::Fri,
            u32::from(config.morning_hour),
        )),
        "sunday" => Some(next_weekday_at(
            Utc::now(),
            Weekday::Sun,
            u32::from(config.morning_hour),
        )),
        _ => {
            // Try ISO 8601 (RFC 3339 with timezone)
            DateTime::parse_from_rfc3339(until)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
                .or_else(|| {
                    // Try without timezone
                    chrono::NaiveDateTime::parse_from_str(until, "%Y-%m-%dT%H:%M:%S")
                        .map(|ndt| Utc.from_utc_datetime(&ndt))
                        .ok()
                })
        }
    }
}

/// Format a snooze preset for display, including the configured hour.
pub fn format_preset(option: SnoozeOption, config: &SnoozeConfig) -> String {
    match option {
        SnoozeOption::TomorrowMorning => {
            format!("Tomorrow morning ({:02}:00)", config.morning_hour)
        }
        SnoozeOption::Tonight => format!("Tonight ({:02}:00)", config.evening_hour),
        SnoozeOption::Weekend => {
            format!(
                "{} ({:02}:00)",
                capitalize(&config.weekend_day),
                config.weekend_hour
            )
        }
        SnoozeOption::NextMonday => format!("Monday ({:02}:00)", config.morning_hour),
        SnoozeOption::Custom => "Custom time".to_string(),
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_tomorrow_morning() {
        let config = SnoozeConfig::default();
        let wake = resolve_snooze_time(SnoozeOption::TomorrowMorning, &config);
        let now = Utc::now();
        assert!(wake > now);
        assert!((wake - now).num_hours() <= 48);
    }

    #[test]
    fn resolve_next_monday() {
        let config = SnoozeConfig::default();
        let wake = resolve_snooze_time(SnoozeOption::NextMonday, &config);
        let now = Utc::now();
        assert!(wake > now);
        assert!((wake - now).num_days() <= 7);
    }

    #[test]
    fn resolve_weekend() {
        let config = SnoozeConfig::default();
        let wake = resolve_snooze_time(SnoozeOption::Weekend, &config);
        let now = Utc::now();
        assert!(wake > now);
        assert!((wake - now).num_days() <= 7);
    }

    #[test]
    fn parse_keywords() {
        let config = SnoozeConfig::default();
        assert!(parse_snooze_until("tomorrow", &config).is_some());
        assert!(parse_snooze_until("monday", &config).is_some());
        assert!(parse_snooze_until("weekend", &config).is_some());
        assert!(parse_snooze_until("tonight", &config).is_some());
        assert!(parse_snooze_until("tuesday", &config).is_some());
        assert!(parse_snooze_until("friday", &config).is_some());
    }

    #[test]
    fn parse_iso8601() {
        let config = SnoozeConfig::default();
        assert!(parse_snooze_until("2026-12-25T09:00:00Z", &config).is_some());
    }

    #[test]
    fn parse_iso8601_no_tz() {
        let config = SnoozeConfig::default();
        assert!(parse_snooze_until("2026-12-25T09:00:00", &config).is_some());
    }

    #[test]
    fn parse_invalid() {
        let config = SnoozeConfig::default();
        assert!(parse_snooze_until("not-a-date", &config).is_none());
    }

    #[test]
    fn next_weekday_at_works() {
        let now = Utc::now();
        let next_tue = next_weekday_at(now, Weekday::Tue, 9);
        assert!(next_tue > now || next_tue.weekday() == Weekday::Tue);
        assert_eq!(next_tue.weekday(), Weekday::Tue);
    }

    #[test]
    fn format_presets_include_hours() {
        let config = SnoozeConfig::default();
        let label = format_preset(SnoozeOption::TomorrowMorning, &config);
        assert!(label.contains("09:00"));
        let label = format_preset(SnoozeOption::Tonight, &config);
        assert!(label.contains("18:00"));
    }
}
