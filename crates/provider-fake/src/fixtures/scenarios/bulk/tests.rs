use super::render::Rendered;
use super::*;
use chrono::{TimeZone, Utc};
use std::collections::HashSet;

fn me() -> mxr_core::types::Address {
    super::super::self_address()
}

fn seed() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
        .single()
        .expect("a valid date")
}

#[test]
fn every_family_is_reachable_and_ids_are_unique() {
    let mut reached = HashSet::new();
    for thread_num in 0..SLOTS * 6 {
        reached.insert(family_for(thread_num).id);
    }
    let all: HashSet<&str> = families().iter().map(|f| f.id).collect();
    assert_eq!(reached, all);
    assert_eq!(all.len(), 13);
    let mut scenarios = HashSet::new();
    for family in families() {
        for turn in family.turns {
            assert!(scenarios.insert(turn.scenario), "{} twice", turn.scenario);
            assert!(turn.scenario.starts_with("bulk-"), "{}", turn.scenario);
            assert!(!turn.description.is_empty());
        }
    }
}

/// The text, the dates in it and the label do not move with the clock: the
/// same thread built at two clocks a year apart differs only by the clock.
#[test]
fn bulk_dates_follow_the_seed_clock() {
    let now = seed();
    let later = now + chrono::Duration::days(400);
    let mut seen = HashSet::new();
    let mut checked = 0;
    for thread_num in 0..20_000 {
        let family = family_for(thread_num);
        let (variant, _) = variant_for(family, thread_num);
        let len = thread_len(thread_num);
        let fresh: Vec<usize> = (0..len)
            .filter(|turn| seen.insert((family.id, variant, *turn)))
            .collect();
        for reply_idx in fresh {
            let a = render(&me(), now, thread_num, reply_idx, len);
            let b = render(&me(), later, thread_num, reply_idx, len);
            assert_eq!(b.date - a.date, later - now);
            let expected = label(thread_num, reply_idx);
            for clock in clocks() {
                let rendered = render(&me(), clock, thread_num, reply_idx, len);
                let again = label(thread_num, reply_idx);
                assert_eq!(
                    again.map(|l| (l.expect, l.held_out, l.from_email, l.turn.scenario)),
                    expected.map(|l| (l.expect, l.held_out, l.from_email, l.turn.scenario)),
                    "{} variant {variant} position {reply_idx} label moved at {clock}",
                    family.id
                );
                assert_eq!(
                    rendered.from.email, a.from.email,
                    "{} sender moved",
                    family.id
                );
                match expected {
                    Some(l) => assert_eq!(rendered.from.email, l.from_email),
                    None => assert_eq!(rendered.from.email, me().email),
                }
            }
            let (ta, tb) = (written_dates(&a), written_dates(&b));
            let fixed: Vec<_> = ta.iter().filter(|d| tb.contains(d)).collect();
            assert!(
                fixed.is_empty(),
                "{} variant {variant} position {reply_idx} writes {fixed:?} at both clocks",
                family.id
            );
            checked += 1;
        }
    }
    let total: usize = families()
        .iter()
        .map(|f| f.variant_count() * f.turns.len())
        .sum();
    assert_eq!(
        checked, total,
        "every family x variant x position was checked"
    );
}

/// The seed clocks the labels must not depend on: now, a year and more away,
/// and either side of a clock change.
fn clocks() -> [chrono::DateTime<Utc>; 4] {
    let now = seed();
    [
        now,
        now + chrono::Duration::days(400),
        at_hour(2026, 10, 24),
        at_hour(2026, 10, 26),
    ]
}

fn at_hour(year: i32, month: u32, day: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, 12, 0, 0)
        .single()
        .expect("a valid date")
}

/// A live ask is the mail that arrived in the last hours at every clock, and
/// every other ask thread arrived at least two months back, for every ask
/// thread and not only the first of each phrasing.
#[test]
fn every_ask_thread_is_dated_to_its_label_at_every_clock() {
    let mut checked = 0;
    for thread_num in 0..20_000 {
        let family = family_for(thread_num);
        if family.dating != Dating::Asks {
            continue;
        }
        let len = thread_len(thread_num);
        let live = asks_recent(thread_num);
        assert!(!items(thread_num, 0).is_empty() <= live);
        for clock in clocks() {
            let age = clock - render(&me(), clock, thread_num, 0, len).date;
            if live {
                assert!(age < chrono::Duration::hours(11), "{thread_num} at {clock}");
            } else {
                assert!(age > chrono::Duration::days(59), "{thread_num} at {clock}");
            }
        }
        checked += 1;
    }
    assert!(checked > 1_000);
}

fn written_dates(rendered: &Rendered) -> Vec<String> {
    super::super::date_finder::written_dates(&format!("{}\n{}", rendered.subject, rendered.body))
}
