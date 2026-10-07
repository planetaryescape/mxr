//! The edition's order: three time bands, and inside each the sources you
//! read most first (research reading.md §5).
//!
//! - Since you were last here: arrived after your previous visit.
//! - Earlier: still inside its source's window.
//! - Fading: in its last day.
//!
//! Affinity is finished issues (weighted) plus opened ones, over the issues
//! the source sent; a source with three or fewer issues is new and gets one
//! of the lead slots, so ranking by what you already read never hides
//! something you just subscribed to.

use crate::fade;
use chrono::{DateTime, Duration, Utc};
use std::cmp::Ordering;

/// Lead items at the top of the first band.
pub const LEAD_SLOTS: usize = 3;
/// A source this new still earns a lead slot.
pub const NEW_SOURCE_ISSUES: u32 = 3;
/// The unsubscribe offer: at least this many recent issues...
pub const UNSUBSCRIBE_MIN_ISSUES: u32 = 8;
/// A visit ends after this long without opening Reading again.
pub const VISIT_GAP: Duration = Duration::minutes(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Band {
    Since,
    Earlier,
    Fading,
}

impl Band {
    pub const ALL: [Self; 3] = [Self::Since, Self::Earlier, Self::Fading];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Since => "since_last_visit",
            Self::Earlier => "earlier",
            Self::Fading => "fading",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Since => "Since you were last here",
            Self::Earlier => "Earlier this week",
            Self::Fading => "Fading",
        }
    }
}

/// Which band an item is in at `now`, or `None` once it has expired.
/// `boundary` is the end of your previous visit; with none, everything
/// current is new to you.
pub fn band(
    arrived: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    boundary: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<Band> {
    if fade::is_expired(now, expires_at) {
        None
    } else if fade::is_fading(now, expires_at) {
        Some(Band::Fading)
    } else if boundary.is_none_or(|boundary| arrived > boundary) {
        Some(Band::Since)
    } else {
        Some(Band::Earlier)
    }
}

/// How you read one source, from local engagement and the provider's read
/// flag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceStats {
    /// Issues it sent in the window the counts cover.
    pub issues: u32,
    /// Of those, opened (in mxr, or read elsewhere).
    pub opened: u32,
    /// Of those, read to the end in mxr.
    pub finished: u32,
    /// Every issue it ever sent that is still stored.
    pub total_issues: u32,
}

impl SourceStats {
    /// Finished counts three times an open.
    pub fn affinity(&self) -> f64 {
        if self.issues == 0 {
            return 0.0;
        }
        f64::from(self.finished * 3 + self.opened) / f64::from(self.issues)
    }

    pub const fn is_new(&self) -> bool {
        self.total_issues <= NEW_SOURCE_ISSUES
    }

    /// "You opened 0 of the last 11 issues."
    pub fn evidence(&self) -> String {
        if self.issues == 1 {
            return format!("You opened {} of its 1 issue", self.opened);
        }
        format!("You opened {} of the last {} issues", self.opened, self.issues)
    }

    /// The quiet "Unsubscribe?" offer: enough issues and none opened.
    pub const fn suggests_unsubscribe(&self) -> bool {
        self.issues >= UNSUBSCRIBE_MIN_ISSUES && self.opened == 0
    }
}

/// What ranking needs to know about one item.
#[derive(Debug, Clone, Copy)]
pub struct Rankable {
    pub arrived: DateTime<Utc>,
    pub affinity: f64,
    pub new_source: bool,
}

/// Affinity first, then newest. Ties in affinity go to the newer item.
pub fn by_affinity(a: &Rankable, b: &Rankable) -> Ordering {
    b.affinity
        .partial_cmp(&a.affinity)
        .unwrap_or(Ordering::Equal)
        .then_with(|| b.arrived.cmp(&a.arrived))
}

/// Order `items` within one band and return how many lead. When no new
/// source made the leads, the best new-source item takes the last lead
/// slot.
pub fn rank<T>(items: &mut [T], key: impl Fn(&T) -> Rankable, lead: bool) -> usize {
    items.sort_by(|a, b| by_affinity(&key(a), &key(b)));
    if !lead || items.is_empty() {
        return 0;
    }
    let leads = items.len().min(LEAD_SLOTS);
    let has_new = items[..leads].iter().any(|item| key(item).new_source);
    if !has_new {
        if let Some(offset) = items[leads..].iter().position(|item| key(item).new_source) {
            let from = leads + offset;
            items[leads - 1..=from].rotate_right(1);
        }
    }
    leads
}

/// The edition's visit: when you were last here, so "Since you were last
/// here" stays the same through one sitting and moves on at the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Visit {
    /// The end of the previous visit: the band boundary.
    pub boundary: Option<DateTime<Utc>>,
    /// The last time Reading was opened.
    pub last_seen: Option<DateTime<Utc>>,
}

impl Visit {
    /// Opening Reading at `now`: a gap of `VISIT_GAP` starts a new visit,
    /// whose boundary is where the last one ended.
    pub fn open(self, now: DateTime<Utc>) -> Self {
        match self.last_seen {
            Some(last) if now - last < VISIT_GAP => Self {
                boundary: self.boundary,
                last_seen: Some(now),
            },
            last => Self {
                boundary: last,
                last_seen: Some(now),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hours: i64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + Duration::days(20_000) + Duration::hours(hours)
    }

    #[test]
    fn bands_follow_the_visit_and_the_window() {
        let boundary = Some(at(10));
        let now = at(50);
        assert_eq!(band(at(20), at(200), boundary, now), Some(Band::Since));
        assert_eq!(band(at(5), at(200), boundary, now), Some(Band::Earlier));
        assert_eq!(band(at(5), at(60), boundary, now), Some(Band::Fading));
        assert_eq!(band(at(20), at(60), boundary, now), Some(Band::Fading));
        assert_eq!(band(at(5), at(50), boundary, now), None);
        assert_eq!(band(at(5), at(200), None, now), Some(Band::Since));
    }

    #[test]
    fn affinity_weighs_finishing_over_opening() {
        let finisher = SourceStats { issues: 10, opened: 9, finished: 9, total_issues: 40 };
        let opener = SourceStats { issues: 10, opened: 10, finished: 0, total_issues: 40 };
        assert!(finisher.affinity() > opener.affinity());
        assert!(SourceStats::default().affinity().abs() < f64::EPSILON);
    }

    #[test]
    fn evidence_names_the_count_and_the_offer_needs_eight_unopened() {
        let ignored = SourceStats { issues: 11, opened: 0, finished: 0, total_issues: 30 };
        assert_eq!(ignored.evidence(), "You opened 0 of the last 11 issues");
        assert!(ignored.suggests_unsubscribe());
        let read = SourceStats { issues: 11, opened: 1, ..ignored };
        assert!(!read.suggests_unsubscribe());
        let few = SourceStats { issues: 5, opened: 0, ..ignored };
        assert!(!few.suggests_unsubscribe());
        let one = SourceStats { issues: 1, opened: 1, finished: 0, total_issues: 1 };
        assert_eq!(one.evidence(), "You opened 1 of its 1 issue");
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Item(&'static str, f64, bool, i64);

    fn key(item: &Item) -> Rankable {
        Rankable { arrived: at(item.3), affinity: item.1, new_source: item.2 }
    }

    #[test]
    fn ranking_puts_the_sources_you_read_first_and_saves_a_lead_for_a_new_one() {
        let mut items = [
            Item("rarely", 0.1, false, 9),
            Item("loved", 3.5, false, 1),
            Item("new", 0.0, true, 5),
            Item("often", 2.0, false, 3),
            Item("sometimes", 1.0, false, 8),
        ];
        let leads = rank(&mut items, key, true);
        assert_eq!(leads, 3);
        let order: Vec<_> = items.iter().map(|i| i.0).collect();
        assert_eq!(order, ["loved", "often", "new", "sometimes", "rarely"]);
    }

    #[test]
    fn ties_go_to_the_newer_item_and_later_bands_have_no_leads() {
        let mut items = [Item("old", 1.0, false, 1), Item("new", 1.0, false, 2)];
        assert_eq!(rank(&mut items, key, false), 0);
        assert_eq!(items[0].0, "new");
    }

    #[test]
    fn a_visit_keeps_its_boundary_until_a_half_hour_gap() {
        let first = Visit::default().open(at(0));
        assert_eq!(first.boundary, None);
        let same = first.open(at(0) + Duration::minutes(10));
        assert_eq!(same.boundary, None);
        let next = same.open(at(5));
        assert_eq!(next.boundary, Some(at(0) + Duration::minutes(10)));
        assert_eq!(next.last_seen, Some(at(5)));
    }
}
