//! Records with a moment: a trip starting in the next 72 hours, a ticket
//! today, a return window closing in 3 days, a warranty ending within 30
//! days. They show in Archive's "Coming up" strip and as one line on Now,
//! the way a Wallet pass surfaces on its date. Return windows and
//! warranties never become to-dos by themselves: most pass on purpose.

use chrono::{DateTime, Duration, TimeZone, Utc};
use mxr_store::{ArchiveRecord, RecordGroup};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MomentKind {
    Trip,
    Booking,
    Ticket,
    ReturnWindow,
    Warranty,
}

impl MomentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trip => "trip",
            Self::Booking => "booking",
            Self::Ticket => "ticket",
            Self::ReturnWindow => "return_window",
            Self::Warranty => "warranty",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moment {
    pub kind: MomentKind,
    pub record_id: String,
    pub group_id: Option<String>,
    pub at: DateTime<Utc>,
    /// "Lisbon, June 2025 · in 3 days".
    pub label: String,
}

const TRIP_AHEAD_HOURS: i64 = 72;
const TICKET_AHEAD_HOURS: i64 = 24;
const RETURN_AHEAD_DAYS: i64 = 3;
const WARRANTY_AHEAD_DAYS: i64 = 30;

pub fn moments<Tz>(
    records: &[ArchiveRecord],
    groups: &HashMap<String, RecordGroup>,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Vec<Moment>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let mut out = Vec::new();
    let mut trips_seen: HashSet<&str> = HashSet::new();
    let within = |at: DateTime<Utc>, ahead: Duration| at >= now && at <= now + ahead;
    for record in records.iter().filter(|r| r.dismissed_at.is_none()) {
        let name = record
            .title
            .clone()
            .or_else(|| record.issuer.clone())
            .unwrap_or_else(|| record.kind.clone());
        if let Some(group) = record
            .group_id
            .as_deref()
            .and_then(|id| groups.get(id))
            .filter(|group| group.kind == "trip")
        {
            if let Some(start) = group.span_start {
                let ongoing = start <= now && group.span_end.is_some_and(|end| end >= now);
                if (within(start, Duration::hours(TRIP_AHEAD_HOURS)) || ongoing)
                    && trips_seen.insert(group.id.as_str())
                {
                    out.push(Moment {
                        kind: MomentKind::Trip,
                        record_id: record.id.clone(),
                        group_id: Some(group.id.clone()),
                        at: start,
                        label: format!(
                            "{} · {}",
                            group.title,
                            if ongoing {
                                "now".to_string()
                            } else {
                                relative(start, now, tz)
                            }
                        ),
                    });
                }
            }
        } else if let Some(start) = record.span_start {
            let (kind, ahead) = if record.kind == "ticket" {
                (MomentKind::Ticket, Duration::hours(TICKET_AHEAD_HOURS))
            } else if record.kind == "booking" {
                (MomentKind::Booking, Duration::hours(TRIP_AHEAD_HOURS))
            } else {
                (MomentKind::Booking, Duration::zero())
            };
            if ahead > Duration::zero() && within(start, ahead) {
                out.push(Moment {
                    kind,
                    record_id: record.id.clone(),
                    group_id: None,
                    at: start,
                    label: format!("{name} · {}", relative(start, now, tz)),
                });
            }
        }
        if let Some(by) = record
            .return_by
            .filter(|by| within(*by, Duration::days(RETURN_AHEAD_DAYS)))
        {
            out.push(Moment {
                kind: MomentKind::ReturnWindow,
                record_id: record.id.clone(),
                group_id: None,
                at: by,
                label: format!("Return {name} by {}", day_label(by, now, tz)),
            });
        }
        if let Some(until) = record
            .warranty_until
            .filter(|until| within(*until, Duration::days(WARRANTY_AHEAD_DAYS)))
        {
            out.push(Moment {
                kind: MomentKind::Warranty,
                record_id: record.id.clone(),
                group_id: None,
                at: until,
                label: format!("{name} warranty ends {}", day_label(until, now, tz)),
            });
        }
    }
    out.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.record_id.cmp(&b.record_id)));
    out
}

/// "today", "tomorrow", "in 3 days".
fn relative<Tz>(at: DateTime<Utc>, now: DateTime<Utc>, tz: &Tz) -> String
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let days = (at.with_timezone(tz).date_naive() - now.with_timezone(tz).date_naive()).num_days();
    match days {
        ..=0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        n => format!("in {n} days"),
    }
}

/// "today", "tomorrow", "Wed 12 Jun".
fn day_label<Tz>(at: DateTime<Utc>, now: DateTime<Utc>, tz: &Tz) -> String
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let days = (at.with_timezone(tz).date_naive() - now.with_timezone(tz).date_naive()).num_days();
    match days {
        ..=0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        _ => at.with_timezone(tz).format("%a %-d %b").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::AccountId;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 9, 9, 0, 0)
            .single()
            .expect("time")
    }

    fn record(id: &str, kind: &str) -> ArchiveRecord {
        ArchiveRecord {
            id: id.to_string(),
            account_id: AccountId::new(),
            dedup_key: id.to_string(),
            kind: kind.to_string(),
            issuer: Some("Dell".into()),
            issuer_key: Some("dell".into()),
            title: Some("XPS 14 laptop".into()),
            reference: None,
            amount_minor: None,
            currency: None,
            issued_at: None,
            span_start: None,
            span_end: None,
            place: None,
            delivered_at: None,
            return_by: None,
            warranty_until: None,
            valid_until: None,
            checked: false,
            group_id: None,
            origin: "rule".into(),
            reason: String::new(),
            thread_id: None,
            last_message_at: None,
            rules_version: 1,
            dismissed_at: None,
            created_at: now(),
            updated_at: now(),
        }
    }

    #[test]
    fn a_trip_in_three_days_a_closing_return_and_an_ending_warranty_come_up() {
        let mut flight = record("flight", "booking");
        flight.group_id = Some("g".into());
        flight.span_start = Some(now() + Duration::days(3));
        let mut hotel = flight.clone();
        hotel.id = "hotel".into();
        let mut laptop = record("laptop", "order");
        laptop.return_by = Some(now() + Duration::days(2));
        laptop.warranty_until = Some(now() + Duration::days(20));
        let mut far = record("far", "order");
        far.return_by = Some(now() + Duration::days(10));
        let groups = HashMap::from([(
            "g".to_string(),
            RecordGroup {
                id: "g".into(),
                account_id: AccountId::new(),
                kind: "trip".into(),
                group_key: "trip|flight".into(),
                title: "Lisbon, June 2025".into(),
                span_start: Some(now() + Duration::days(3)),
                span_end: Some(now() + Duration::days(6)),
            },
        )]);
        let got = moments(&[flight, hotel, laptop, far], &groups, now(), &Utc);
        let kinds: Vec<_> = got.iter().map(|m| m.kind).collect();
        assert_eq!(
            kinds,
            vec![
                MomentKind::ReturnWindow,
                MomentKind::Trip,
                MomentKind::Warranty
            ]
        );
        assert_eq!(got[1].label, "Lisbon, June 2025 · in 3 days");
        assert_eq!(got[0].label, "Return XPS 14 laptop by Wed 11 Jun");
    }
}
