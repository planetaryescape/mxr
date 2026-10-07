//! Composite records: a trip is bookings whose dates overlap or touch, a
//! series is a run of bills from one issuer. Orders are composite already:
//! their emails share one row by order number (see `pass`).
//!
//! Grouping is recomputed from the records each time, so splitting a trip
//! is a correction to a booking's dates, never a list to maintain.

use crate::RecordKind;
use chrono::{DateTime, Datelike, Duration, Utc};
use std::collections::{BTreeMap, BTreeSet};

/// What grouping reads of a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupInput {
    pub id: String,
    pub kind: RecordKind,
    pub issuer_key: Option<String>,
    pub issuer: Option<String>,
    pub date: Option<DateTime<Utc>>,
    pub span_start: Option<DateTime<Utc>>,
    pub span_end: Option<DateTime<Utc>>,
    pub place: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Trip,
    Series,
}

impl GroupKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trip => "trip",
            Self::Series => "series",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupPlan {
    pub kind: GroupKind,
    /// Stable across regroups while the group's first member stays.
    pub key: String,
    pub title: String,
    pub span_start: Option<DateTime<Utc>>,
    pub span_end: Option<DateTime<Utc>>,
    pub members: Vec<String>,
}

/// Bookings closer than this are one trip: a flight that lands the
/// evening before the hotel's check-in is still the same trip.
const TRIP_GAP_HOURS: i64 = 24;
/// A series needs this many bills, in this many different months.
const SERIES_MIN: usize = 3;

pub fn plan(records: &[GroupInput]) -> Vec<GroupPlan> {
    let mut plans = trips(records);
    plans.extend(series(records));
    plans
}

/// A booking and the span it covers.
type Span<'a> = (&'a GroupInput, DateTime<Utc>, DateTime<Utc>);

fn trips(records: &[GroupInput]) -> Vec<GroupPlan> {
    let mut bookings: Vec<Span<'_>> = records
        .iter()
        .filter(|record| matches!(record.kind, RecordKind::Booking | RecordKind::Ticket))
        .filter_map(|record| {
            let start = record.span_start?;
            let end = record.span_end.filter(|end| *end >= start).unwrap_or(start);
            Some((record, start, end))
        })
        .collect();
    bookings.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.id.cmp(&b.0.id)));

    let mut clusters: Vec<Vec<Span<'_>>> = Vec::new();
    let mut cluster_end: Option<DateTime<Utc>> = None;
    for booking in bookings {
        let joins =
            cluster_end.is_some_and(|end| booking.1 <= end + Duration::hours(TRIP_GAP_HOURS));
        if joins {
            if let Some(last) = clusters.last_mut() {
                last.push(booking);
            }
        } else {
            clusters.push(vec![booking]);
        }
        cluster_end = Some(
            cluster_end
                .filter(|_| joins)
                .map_or(booking.2, |end| end.max(booking.2)),
        );
    }

    clusters
        .into_iter()
        .filter(|cluster| cluster.len() >= 2)
        .map(|cluster| {
            let start = cluster.iter().map(|b| b.1).min();
            let end = cluster.iter().map(|b| b.2).max();
            let place = trip_place(&cluster.iter().map(|b| b.0).collect::<Vec<_>>());
            let when = start.map(|start| format!("{} {}", month_name(start.month()), start.year()));
            let title = match (place, when) {
                (Some(place), Some(when)) => format!("{place}, {when}"),
                (Some(place), None) => place,
                (None, Some(when)) => format!("Trip, {when}"),
                (None, None) => "Trip".to_string(),
            };
            GroupPlan {
                kind: GroupKind::Trip,
                key: format!("trip|{}", cluster[0].0.id),
                title,
                span_start: start,
                span_end: end,
                members: cluster.iter().map(|b| b.0.id.clone()).collect(),
            }
        })
        .collect()
}

/// Where the trip goes: the place most bookings name, preferring a hotel's
/// city, which is where the nights are.
fn trip_place(members: &[&GroupInput]) -> Option<String> {
    let mut counts: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for member in members {
        if let Some(place) = member.place.as_deref().filter(|p| !p.trim().is_empty()) {
            let entry = counts
                .entry(place.trim().to_lowercase())
                .or_insert_with(|| (0, place.trim().to_string()));
            entry.0 += 1;
        }
    }
    counts
        .into_values()
        .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)))
        .map(|(_, place)| place)
}

fn series(records: &[GroupInput]) -> Vec<GroupPlan> {
    let mut by_issuer: BTreeMap<&str, Vec<&GroupInput>> = BTreeMap::new();
    for record in records {
        if !matches!(
            record.kind,
            RecordKind::Invoice | RecordKind::Statement | RecordKind::Receipt
        ) {
            continue;
        }
        if let Some(issuer) = record.issuer_key.as_deref().filter(|key| !key.is_empty()) {
            by_issuer.entry(issuer).or_default().push(record);
        }
    }
    by_issuer
        .into_iter()
        .filter_map(|(issuer_key, mut members)| {
            let months: BTreeSet<(i32, u32)> = members
                .iter()
                .filter_map(|m| m.date.map(|d| (d.year(), d.month())))
                .collect();
            if members.len() < SERIES_MIN || months.len() < SERIES_MIN {
                return None;
            }
            members.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
            let issuer = members
                .iter()
                .rev()
                .find_map(|m| m.issuer.clone())
                .unwrap_or_else(|| issuer_key.to_string());
            let noun = if members.iter().all(|m| m.kind == RecordKind::Receipt) {
                "receipts"
            } else {
                "bills"
            };
            Some(GroupPlan {
                kind: GroupKind::Series,
                key: format!("series|{issuer_key}"),
                title: format!("{issuer} {noun}"),
                span_start: members.iter().filter_map(|m| m.date).min(),
                span_end: members.iter().filter_map(|m| m.date).max(),
                members: members.iter().map(|m| m.id.clone()).collect(),
            })
        })
        .collect()
}

pub fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        _ => "December",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn day(m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, m, d, h, 0, 0)
            .single()
            .expect("time")
    }

    fn booking(
        id: &str,
        kind: RecordKind,
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
        place: Option<&str>,
    ) -> GroupInput {
        GroupInput {
            id: id.to_string(),
            kind,
            issuer_key: Some(id.to_string()),
            issuer: None,
            date: Some(day(1, 20, 9)),
            span_start: Some(start),
            span_end: end,
            place: place.map(str::to_string),
        }
    }

    fn bill(id: &str, issuer: &str, date: DateTime<Utc>) -> GroupInput {
        GroupInput {
            id: id.to_string(),
            kind: RecordKind::Statement,
            issuer_key: Some(issuer.to_lowercase()),
            issuer: Some(issuer.to_string()),
            date: Some(date),
            span_start: None,
            span_end: None,
            place: None,
        }
    }

    #[test]
    fn overlapping_bookings_are_one_trip_named_by_where_the_nights_are() {
        let plans = plan(&[
            booking(
                "flight-out",
                RecordKind::Booking,
                day(6, 12, 6),
                Some(day(6, 12, 9)),
                Some("Lisbon"),
            ),
            booking(
                "hotel",
                RecordKind::Booking,
                day(6, 12, 14),
                Some(day(6, 15, 10)),
                Some("Lisbon"),
            ),
            booking(
                "oceanario",
                RecordKind::Ticket,
                day(6, 13, 10),
                None,
                Some("Lisbon"),
            ),
            booking(
                "flight-back",
                RecordKind::Booking,
                day(6, 15, 18),
                Some(day(6, 15, 20)),
                Some("London"),
            ),
            // A month later: a different trip, alone, so no group.
            booking(
                "dinner",
                RecordKind::Booking,
                day(7, 20, 19),
                None,
                Some("London"),
            ),
        ]);
        assert_eq!(plans.len(), 1);
        let trip = &plans[0];
        assert_eq!(trip.kind, GroupKind::Trip);
        assert_eq!(trip.title, "Lisbon, June 2025");
        assert_eq!(trip.members.len(), 4);
        assert_eq!(trip.key, "trip|flight-out");
    }

    #[test]
    fn bookings_more_than_a_day_apart_are_separate() {
        let plans = plan(&[
            booking(
                "a",
                RecordKind::Booking,
                day(6, 1, 9),
                Some(day(6, 2, 9)),
                None,
            ),
            booking(
                "b",
                RecordKind::Booking,
                day(6, 4, 9),
                Some(day(6, 5, 9)),
                None,
            ),
        ]);
        assert!(plans.is_empty());
    }

    #[test]
    fn three_months_of_bills_from_one_issuer_are_a_series() {
        let plans = plan(&[
            bill("jan", "Octopus Energy", day(1, 11, 9)),
            bill("feb", "Octopus Energy", day(2, 11, 9)),
            bill("mar", "Octopus Energy", day(3, 11, 9)),
            bill("bt1", "BT", day(1, 2, 9)),
            bill("bt2", "BT", day(1, 20, 9)),
            bill("bt3", "BT", day(2, 2, 9)),
        ]);
        assert_eq!(plans.len(), 1, "BT's three bills fall in two months");
        assert_eq!(plans[0].title, "Octopus Energy bills");
        assert_eq!(plans[0].members, vec!["jan", "feb", "mar"]);
    }
}
