use crate::types::SnoozeConfig;
use chrono::{DateTime, Duration, Local, TimeZone, Utc, Weekday};
use mxr_core::natural_time::{resolve_time, resolve_time_local, TimePrefs};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnoozeOption {
    TomorrowMorning,
    Tonight,
    Weekend,
    NextMonday,
    Custom,
}

#[derive(Debug, Clone)]
pub struct SnoozePreset {
    pub option: SnoozeOption,
    pub label: &'static str,
}

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

pub fn resolve_snooze_time(option: SnoozeOption, config: &SnoozeConfig) -> DateTime<Utc> {
    let now = Local::now();
    resolve_snooze_time_from(option, config, &now)
}

fn resolve_snooze_time_from<Tz>(
    option: SnoozeOption,
    config: &SnoozeConfig,
    now: &DateTime<Tz>,
) -> DateTime<Utc>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let phrase = match option {
        SnoozeOption::TomorrowMorning => "tomorrow",
        SnoozeOption::Tonight => "tonight",
        SnoozeOption::Weekend => "weekend",
        SnoozeOption::NextMonday => "monday",
        SnoozeOption::Custom => return now.with_timezone(&Utc),
    };
    let prefs = config.time_prefs();
    resolve_time(phrase, now, &prefs)
        .or_else(|_| resolve_time("tomorrow evening", now, &prefs))
        .map_or_else(|_| now.with_timezone(&Utc) + Duration::days(1), |r| r.at)
}

pub fn resolve_snooze_preset_time(
    option: SnoozeOption,
    config: &SnoozeConfig,
) -> Option<DateTime<Utc>> {
    let now = Local::now();
    resolve_snooze_preset_time_from(option, config, &now)
}

fn resolve_snooze_preset_time_from<Tz>(
    option: SnoozeOption,
    config: &SnoozeConfig,
    now: &DateTime<Tz>,
) -> Option<DateTime<Utc>>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    if option != SnoozeOption::Tonight {
        return Some(resolve_snooze_time_from(option, config, now));
    }
    resolve_time("tonight", now, &config.time_prefs())
        .ok()
        .map(|resolution| resolution.at)
}

/// Parse a snooze "until" string (from CLI or API) into a concrete wake time.
///
/// Accepts every phrase [`mxr_core::natural_time`] does, resolved in local
/// time with the user's configured hours, plus the underscore preset names
/// ("tomorrow_morning", "next_monday").
pub fn parse_snooze_until(until: &str, config: &SnoozeConfig) -> Option<DateTime<Utc>> {
    resolve_time_local(until, &config.time_prefs())
        .ok()
        .map(|resolution| resolution.at)
}

impl SnoozeConfig {
    pub fn time_prefs(&self) -> TimePrefs {
        TimePrefs {
            morning_hour: self.morning_hour,
            evening_hour: self.evening_hour,
            weekend_day: match self.weekend_day.as_str() {
                "sunday" => Weekday::Sun,
                _ => Weekday::Sat,
            },
            weekend_hour: self.weekend_hour,
        }
    }
}

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
    use chrono::{Datelike, FixedOffset, Timelike};

    #[test]
    fn parse_accepts_every_natural_phrase() {
        let config = SnoozeConfig::default();
        for phrase in [
            "tomorrow_morning",
            "next_monday",
            "fri 3",
            "in 2h",
            "3d",
            "next week",
            "3 oct",
        ] {
            assert!(parse_snooze_until(phrase, &config).is_some(), "{phrase}");
        }
    }

    #[test]
    fn presets_use_configured_hours_in_the_callers_zone() {
        let config = SnoozeConfig {
            morning_hour: 7,
            weekend_day: "sunday".into(),
            weekend_hour: 11,
            ..SnoozeConfig::default()
        };
        let tz = FixedOffset::east_opt(2 * 3600).expect("valid test offset");
        let now = tz
            .with_ymd_and_hms(2026, 5, 11, 8, 0, 0)
            .single()
            .expect("valid test time");
        let local = |option| resolve_snooze_time_from(option, &config, &now).with_timezone(&tz);

        let tomorrow = local(SnoozeOption::TomorrowMorning);
        assert_eq!((tomorrow.day(), tomorrow.hour()), (12, 7));
        let weekend = local(SnoozeOption::Weekend);
        assert_eq!((weekend.weekday(), weekend.hour()), (Weekday::Sun, 11));
        let monday = local(SnoozeOption::NextMonday);
        assert_eq!((monday.day(), monday.hour()), (18, 7));
    }
}
