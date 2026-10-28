//! Immutable TZif/POSIX rules shared by judging and freshness tokens.
use chrono::{Datelike, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use jiff::tz::{AmbiguousOffset, TimeZone as Rules};
use sha2::{Digest, Sha256};
use std::{
    io,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone, Debug)]
pub struct AsksTimezone {
    rules: Rules,
    token: Arc<str>,
}

struct CachedTimezone {
    raw_spec: Option<String>,
    bytes: Option<Vec<u8>>,
    timezone: AsksTimezone,
}

type SnapshotCache = Mutex<Option<CachedTimezone>>;
static SNAPSHOT_CACHE: OnceLock<SnapshotCache> = OnceLock::new();

impl AsksTimezone {
    pub fn resolve() -> io::Result<Self> {
        Self::from_spec(std::env::var("TZ").ok().as_deref())
    }

    fn from_spec(spec: Option<&str>) -> io::Result<Self> {
        Self::from_spec_using(spec, SNAPSHOT_CACHE.get_or_init(|| Mutex::new(None)))
    }

    fn from_spec_using(raw_spec: Option<&str>, cache: &SnapshotCache) -> io::Result<Self> {
        let spec = match raw_spec {
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
        // Always read current bytes before consulting the bounded parse cache.
        // Metadata and elapsed time cannot establish that future offset rules agree.
        let source = match std::fs::read(&path) {
            Ok(bytes) => Ok(bytes),
            Err(error) if !zone.is_empty() && !Path::new(zone).is_absolute() => Err(error),
            Err(error) => return Err(error),
        };
        let mut cached = cache
            .lock()
            .map_err(|_| io::Error::other("timezone snapshot cache poisoned"))?;
        if let Some(previous) = cached.as_ref() {
            if previous.raw_spec.as_deref() == raw_spec
                && previous.bytes.as_deref() == source.as_ref().ok().map(Vec::as_slice)
            {
                return Ok(previous.timezone.clone());
            }
        }
        let (timezone, bytes) = match source {
            Ok(bytes) => (Self::from_tzif(spec, &bytes)?, Some(bytes)),
            Err(error) => {
                let rules = Rules::posix(zone).map_err(|_| error)?;
                (
                    Self {
                        rules,
                        token: hash(spec.as_bytes()).into(),
                    },
                    None,
                )
            }
        };
        *cached = Some(CachedTimezone {
            raw_spec: raw_spec.map(str::to_owned),
            bytes,
            timezone: timezone.clone(),
        });
        Ok(timezone)
    }

    fn from_tzif(name: &str, bytes: &[u8]) -> io::Result<Self> {
        let rules = Rules::tzif(name, bytes).map_err(io::Error::other)?;
        let mut source = name.as_bytes().to_vec();
        source.push(0);
        source.extend_from_slice(bytes);
        Ok(Self {
            rules,
            token: hash(&source).into(),
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
    #[test]
    fn repeated_resolution_reports_native_parse_work() {
        let bytes = std::fs::read("/usr/share/zoneinfo/Europe/London").unwrap();
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), bytes).unwrap();
        let started = std::time::Instant::now();
        for _ in 0..500 {
            AsksTimezone::from_spec(file.path().to_str()).unwrap();
        }
        eprintln!(
            "500 fresh-source timezone resolutions: {:?}",
            started.elapsed()
        );
    }
    #[test]
    fn exact_source_reuses_one_parsed_snapshot_and_raw_overrides_remain_distinct() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            file.path(),
            std::fs::read("/usr/share/zoneinfo/Europe/London").unwrap(),
        )
        .unwrap();
        let cache = Mutex::new(None);
        let first = AsksTimezone::from_spec_using(file.path().to_str(), &cache).unwrap();
        for _ in 0..500 {
            let next = AsksTimezone::from_spec_using(file.path().to_str(), &cache).unwrap();
            assert!(
                Arc::ptr_eq(&first.token, &next.token),
                "reuse the parsed snapshot, not only its token value"
            );
        }
        let utc = AsksTimezone::from_spec_using(Some("UTC0"), &cache).unwrap();
        let empty = AsksTimezone::from_spec_using(Some(""), &cache).unwrap();
        assert_eq!(utc.token(), empty.token());
        assert!(
            !Arc::ptr_eq(&utc.token, &empty.token),
            "raw override changes must miss the cache"
        );
        let eastern =
            AsksTimezone::from_spec_using(Some("EST5EDT,M3.2.0,M11.1.0"), &cache).unwrap();
        assert_ne!(utc.token(), eastern.token());
        assert!(AsksTimezone::from_spec_using(Some("missing/zone"), &cache).is_err());
    }

    #[test]
    fn current_complete_bytes_detect_hot_replacement_and_errors_without_changing_frozen_rules() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let cache = Mutex::new(None);
        let spec = file.path().to_str();
        let london = std::fs::read("/usr/share/zoneinfo/Europe/London").unwrap();
        std::fs::write(file.path(), &london).unwrap();
        let frozen = AsksTimezone::from_spec_using(spec, &cache).unwrap();
        let summer = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap();
        assert_eq!(
            frozen.offset_from_utc_date(&summer).fix().local_minus_utc(),
            3600
        );
        std::fs::write(
            file.path(),
            std::fs::read("/usr/share/zoneinfo/UTC").unwrap(),
        )
        .unwrap();
        let utc = AsksTimezone::from_spec_using(spec, &cache).unwrap();
        assert_ne!(frozen.token(), utc.token());
        assert_eq!(utc.offset_from_utc_date(&summer).fix().local_minus_utc(), 0);
        let one = std::fs::read("/usr/share/zoneinfo/Etc/GMT+1").unwrap();
        let two = std::fs::read("/usr/share/zoneinfo/Etc/GMT+2").unwrap();
        assert_eq!(one.len(), two.len());
        std::fs::write(file.path(), &one).unwrap();
        let first = AsksTimezone::from_spec_using(spec, &cache).unwrap();
        std::fs::write(file.path(), &two).unwrap();
        let second = AsksTimezone::from_spec_using(spec, &cache).unwrap();
        assert_ne!(first.token(), second.token());
        assert_ne!(
            first.offset_from_utc_date(&summer).fix(),
            second.offset_from_utc_date(&summer).fix()
        );
        std::fs::write(file.path(), vec![0; two.len()]).unwrap();
        assert!(AsksTimezone::from_spec_using(spec, &cache).is_err());
        std::fs::remove_file(file.path()).unwrap();
        assert!(AsksTimezone::from_spec_using(spec, &cache).is_err());
        std::fs::write(file.path(), &london).unwrap();
        assert_eq!(
            AsksTimezone::from_spec_using(spec, &cache).unwrap().token(),
            frozen.token()
        );
        assert_eq!(
            frozen.offset_from_utc_date(&summer).fix().local_minus_utc(),
            3600
        );
    }
}
