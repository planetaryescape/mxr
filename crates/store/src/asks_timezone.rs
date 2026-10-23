//! Immutable TZif/POSIX rules shared by judging and freshness tokens.
use chrono::{Datelike, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use jiff::tz::{AmbiguousOffset, TimeZone as Rules};
use sha2::{Digest, Sha256};
use std::{io, path::Path};

#[derive(Clone, Debug)]
pub struct AsksTimezone {
    rules: Rules,
    token: String,
}

impl AsksTimezone {
    pub fn resolve() -> io::Result<Self> {
        Self::from_spec(std::env::var("TZ").ok().as_deref())
    }

    // Read TZif bytes once: both the resolver and token own this snapshot.
    fn from_spec(spec: Option<&str>) -> io::Result<Self> {
        let spec = match spec {
            Some("") => "UTC0",
            Some(spec) => spec,
            None => "",
        };
        let zone = spec.trim_start_matches(':');
        let path = if zone.is_empty() {
            Path::new("/etc/localtime").to_path_buf()
        } else if Path::new(zone).is_absolute() {
            Path::new(zone).to_path_buf()
        } else {
            Path::new("/usr/share/zoneinfo").join(zone)
        };
        match std::fs::read(&path) {
            Ok(bytes) => Self::from_tzif(spec, &bytes),
            Err(error) if !zone.is_empty() && !Path::new(zone).is_absolute() => {
                let rules = Rules::posix(zone).map_err(|_| error)?;
                Ok(Self {
                    rules,
                    token: hash(spec.as_bytes()),
                })
            }
            Err(error) => Err(error),
        }
    }

    fn from_tzif(name: &str, bytes: &[u8]) -> io::Result<Self> {
        let rules = Rules::tzif(name, bytes).map_err(io::Error::other)?;
        let mut source = name.as_bytes().to_vec();
        source.push(0);
        source.extend_from_slice(bytes);
        Ok(Self {
            rules,
            token: hash(&source),
        })
    }

    pub fn validate_timestamp(&self, at: chrono::DateTime<chrono::Utc>) -> io::Result<()> {
        jiff::Timestamp::from_second(at.timestamp())
            .map(|_| ())
            .map_err(io::Error::other)
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

fn hash(bytes: &[u8]) -> String {
    base16ct::lower::encode_string(&Sha256::digest(bytes))
}

/// Chrono reconstructs a timezone from its offset during date arithmetic.
/// Carry the immutable rules through that round trip, rather than becoming fixed-offset.
#[derive(Clone, Debug)]
pub struct AsksOffset {
    fixed: FixedOffset,
    timezone: AsksTimezone,
}
impl chrono::Offset for AsksOffset {
    fn fix(&self) -> FixedOffset {
        self.fixed
    }
}
impl std::fmt::Display for AsksOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fixed.fmt(f)
    }
}

impl TimeZone for AsksTimezone {
    type Offset = AsksOffset;

    fn from_offset(offset: &Self::Offset) -> Self {
        offset.timezone.clone()
    }

    fn offset_from_local_date(&self, local: &NaiveDate) -> LocalResult<Self::Offset> {
        self.offset_from_local_datetime(&local.and_time(chrono::NaiveTime::MIN))
    }

    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<Self::Offset> {
        let Ok(year) = i16::try_from(local.year()) else {
            return LocalResult::None;
        };
        let Ok(dt) = jiff::civil::DateTime::new(
            year,
            local.month() as i8,
            local.day() as i8,
            local.hour() as i8,
            local.minute() as i8,
            local.second() as i8,
            local.nanosecond() as i32,
        ) else {
            return LocalResult::None;
        };
        let fixed = |offset: jiff::tz::Offset| {
            FixedOffset::east_opt(offset.seconds()).map(|fixed| AsksOffset {
                fixed,
                timezone: self.clone(),
            })
        };
        match self.rules.to_ambiguous_timestamp(dt).offset() {
            AmbiguousOffset::Unambiguous { offset } => {
                fixed(offset).map_or(LocalResult::None, LocalResult::Single)
            }
            AmbiguousOffset::Gap { .. } => LocalResult::None,
            AmbiguousOffset::Fold { before, after } => match (fixed(before), fixed(after)) {
                (Some(a), Some(b)) => LocalResult::Ambiguous(a, b),
                _ => LocalResult::None,
            },
        }
    }

    fn offset_from_utc_date(&self, utc: &NaiveDate) -> Self::Offset {
        self.offset_from_utc_datetime(&utc.and_time(chrono::NaiveTime::MIN))
    }

    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> Self::Offset {
        let stamp = jiff::Timestamp::from_second(utc.and_utc().timestamp())
            .expect("asks message date is within Jiff timestamp range");
        AsksOffset {
            fixed: FixedOffset::east_opt(self.rules.to_offset(stamp).seconds())
                .expect("Jiff offset is within chrono range"),
            timezone: self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Offset;
    #[test]
    fn snapshots_keep_dst_rules_and_distinguish_future_offsets() {
        let london = AsksTimezone::from_spec(Some("Europe/London")).unwrap();
        let utc = AsksTimezone::from_spec(Some("UTC0")).unwrap();
        let winter = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let summer = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap();
        assert_eq!(
            london.offset_from_utc_date(&winter).fix(),
            utc.offset_from_utc_date(&winter).fix()
        );
        assert_ne!(london.token(), utc.token());
        assert_ne!(
            london.offset_from_utc_date(&summer).fix(),
            utc.offset_from_utc_date(&summer).fix()
        );
        let anchor = london.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
        assert_eq!(anchor.timezone().token(), london.token());
        assert_eq!(
            anchor
                .timezone()
                .with_ymd_and_hms(2026, 7, 15, 12, 0, 0)
                .unwrap()
                .offset()
                .fix()
                .local_minus_utc(),
            3600
        );
        assert!(matches!(
            london.with_ymd_and_hms(2026, 3, 29, 1, 30, 0),
            LocalResult::None
        ));
        assert!(matches!(
            london.with_ymd_and_hms(2026, 10, 25, 1, 30, 0),
            LocalResult::Ambiguous(..)
        ));
        let bytes = std::fs::read("/usr/share/zoneinfo/Europe/London").unwrap();
        let path = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(path.path(), &bytes).unwrap();
        let first = AsksTimezone::from_spec(path.path().to_str()).unwrap();
        std::fs::write(
            path.path(),
            std::fs::read("/usr/share/zoneinfo/UTC").unwrap(),
        )
        .unwrap();
        let second = AsksTimezone::from_spec(path.path().to_str()).unwrap();
        assert_ne!(first.token(), second.token());
        assert_eq!(
            first.offset_from_utc_date(&summer).fix(),
            london.offset_from_utc_date(&summer).fix()
        );
        assert_eq!(
            second.offset_from_utc_date(&summer).fix(),
            utc.offset_from_utc_date(&summer).fix()
        );
        assert!(AsksTimezone::from_spec(Some(":Europe/London")).is_ok());
        assert!(AsksTimezone::from_spec(Some("EST5EDT,M3.2.0,M11.1.0")).is_ok());
        assert!(AsksTimezone::from_spec(Some("missing/zone")).is_err());
    }
}
