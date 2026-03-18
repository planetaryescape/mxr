use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, Utc, Weekday};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnoozeOption {
    TomorrowMorning,
    NextMonday,
    Weekend,
    Tonight,
    Custom(DateTime<Utc>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnoozeConfig {
    pub morning_hour: u8,
    pub evening_hour: u8,
    pub weekend_day: String,
    pub weekend_hour: u8,
}

impl Default for SnoozeConfig {
    fn default() -> Self {
        Self {
            morning_hour: 9,
            evening_hour: 18,
            weekend_day: "saturday".into(),
            weekend_hour: 10,
        }
    }
}

/// Resolve a snooze option to a concrete wake time.
pub fn resolve_snooze_time(option: SnoozeOption, config: &SnoozeConfig) -> DateTime<Utc> {
    let now = Local::now();

    match option {
        SnoozeOption::TomorrowMorning => {
            let tomorrow = now.date_naive() + Duration::days(1);
            let time = NaiveTime::from_hms_opt(config.morning_hour as u32, 0, 0).unwrap();
            tomorrow
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&Utc)
        }
        SnoozeOption::NextMonday => {
            let days_until_monday = (Weekday::Mon.num_days_from_monday() as i64
                - now.weekday().num_days_from_monday() as i64
                + 7)
                % 7;
            let days = if days_until_monday == 0 {
                7
            } else {
                days_until_monday
            };
            let monday = now.date_naive() + Duration::days(days);
            let time = NaiveTime::from_hms_opt(config.morning_hour as u32, 0, 0).unwrap();
            monday
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&Utc)
        }
        SnoozeOption::Weekend => {
            let target_day = match config.weekend_day.as_str() {
                "sunday" => Weekday::Sun,
                _ => Weekday::Sat,
            };
            let days_until = (target_day.num_days_from_monday() as i64
                - now.weekday().num_days_from_monday() as i64
                + 7)
                % 7;
            let days = if days_until == 0 { 7 } else { days_until };
            let weekend = now.date_naive() + Duration::days(days);
            let time = NaiveTime::from_hms_opt(config.weekend_hour as u32, 0, 0).unwrap();
            weekend
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&Utc)
        }
        SnoozeOption::Tonight => {
            let today = now.date_naive();
            let time = NaiveTime::from_hms_opt(config.evening_hour as u32, 0, 0).unwrap();
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
        SnoozeOption::Custom(dt) => dt,
    }
}

/// Parse a snooze "until" string from CLI.
pub fn parse_snooze_until(until: &str, config: &SnoozeConfig) -> Option<DateTime<Utc>> {
    match until.to_lowercase().as_str() {
        "tomorrow" => Some(resolve_snooze_time(SnoozeOption::TomorrowMorning, config)),
        "monday" => Some(resolve_snooze_time(SnoozeOption::NextMonday, config)),
        "weekend" => Some(resolve_snooze_time(SnoozeOption::Weekend, config)),
        "tonight" => Some(resolve_snooze_time(SnoozeOption::Tonight, config)),
        _ => {
            // Try to parse as ISO 8601 datetime
            DateTime::parse_from_rfc3339(until)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }
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
        // Should be roughly 1 day from now (within 48h)
        assert!((wake - now).num_hours() <= 48);
    }

    #[test]
    fn resolve_next_monday() {
        let config = SnoozeConfig::default();
        let wake = resolve_snooze_time(SnoozeOption::NextMonday, &config);
        let now = Utc::now();
        assert!(wake > now);
        // Should be within 7 days
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
    fn resolve_custom() {
        let config = SnoozeConfig::default();
        let custom = Utc::now() + Duration::hours(3);
        let wake = resolve_snooze_time(SnoozeOption::Custom(custom), &config);
        assert_eq!(wake, custom);
    }

    #[test]
    fn parse_snooze_until_keywords() {
        let config = SnoozeConfig::default();
        assert!(parse_snooze_until("tomorrow", &config).is_some());
        assert!(parse_snooze_until("monday", &config).is_some());
        assert!(parse_snooze_until("weekend", &config).is_some());
        assert!(parse_snooze_until("tonight", &config).is_some());
    }

    #[test]
    fn parse_snooze_until_iso8601() {
        let config = SnoozeConfig::default();
        assert!(parse_snooze_until("2026-12-25T09:00:00Z", &config).is_some());
    }

    #[test]
    fn parse_snooze_until_invalid() {
        let config = SnoozeConfig::default();
        assert!(parse_snooze_until("not-a-date", &config).is_none());
    }
}
