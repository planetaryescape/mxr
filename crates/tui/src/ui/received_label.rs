use chrono::{DateTime, Datelike, FixedOffset, Local, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayZone {
    Local,
    Fixed(FixedOffset),
}

impl DisplayZone {
    pub fn at(self, instant: DateTime<Utc>) -> DateTime<FixedOffset> {
        match self {
            Self::Local => instant.with_timezone(&Local).fixed_offset(),
            Self::Fixed(offset) => instant.with_timezone(&offset),
        }
    }
}

pub fn received_label(at: DateTime<Utc>, now: DateTime<Utc>, zone: DisplayZone) -> String {
    let local = zone.at(at);
    let today = zone.at(now);
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
    use chrono::{FixedOffset, TimeZone};

    fn zone() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).unwrap()
    }

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn the_local_date_decides_the_day_not_utc() {
        let at = utc(2026, 10, 21, 23, 30);
        let now = utc(2026, 10, 22, 6, 0);
        assert_eq!(
            received_label(at, now, DisplayZone::Fixed(zone())),
            "Today 01:30 \u{b7} 6h ago"
        );
    }
}
