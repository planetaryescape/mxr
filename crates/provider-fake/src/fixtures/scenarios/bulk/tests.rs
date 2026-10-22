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

/// `locate` answers without generating anything, so it must agree with
/// walking the threads the way `DemoFixtureStream::page` does.
#[test]
fn locate_agrees_with_walking_the_threads() {
    let mut index = 0;
    for thread_num in 0..2_000 {
        assert_eq!(first_index(thread_num), index, "thread {thread_num}");
        for reply_idx in 0..thread_len(thread_num) {
            assert_eq!(locate(index), (thread_num, reply_idx), "index {index}");
            index += 1;
        }
    }
}

#[test]
fn thread_lengths_and_the_mix_are_what_the_demo_always_had() {
    // Thirteen threads, 35.5 messages on average: the old categories.
    let lens: Vec<usize> = (0..52).map(thread_len).collect();
    assert_eq!(lens.iter().sum::<usize>(), 142);
    assert_eq!(thread_len(1), 5);
    assert_eq!(thread_len(4), 5);
    assert_eq!(thread_len(8), 3);
    assert_eq!(thread_len(2), 1);
    assert_eq!(thread_len(12), 1);
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

/// Each family's script covers the longest thread its slots produce, and
/// each variant has the text the script reads.
#[test]
fn every_variant_has_the_text_its_family_reads() {
    for family in families() {
        let longest = (0..PERIOD * SLOTS)
            .filter(|t| family_for(*t).id == family.id)
            .map(thread_len)
            .max()
            .expect("the family has threads");
        assert!(
            family.turns.len() >= longest,
            "{} has {} turns for threads of {longest}",
            family.id,
            family.turns.len()
        );
        let inbound = family
            .turns
            .iter()
            .filter(|t| t.from == Speaker::Them)
            .count();
        let needs_sent = family.turns.iter().any(|t| t.from == Speaker::You);
        assert_eq!(needs_sent, !family.sent_replies.is_empty(), "{}", family.id);
        assert!(family.variant_count() >= 6, "{}", family.id);
        assert!(
            !held_out::variants(family.id).is_empty(),
            "{} holds nothing out",
            family.id
        );
        // A quarter to a half of the variants are held out.
        let held = held_out::variants(family.id).len();
        assert!(
            held * 4 >= family.variant_count() && held * 2 <= family.variant_count(),
            "{}: {held} of {} held out",
            family.id,
            family.variant_count()
        );
        let mut ids = HashSet::new();
        for index in 0..family.variant_count() {
            let (variant, _) = family.variant(index);
            assert!(
                ids.insert(variant.id),
                "{} repeats {}",
                family.id,
                variant.id
            );
            assert_eq!(
                variant.bodies.len(),
                inbound,
                "{} {}",
                family.id,
                variant.id
            );
            let subjects = match family.subjects {
                Subjects::Pool => 0,
                Subjects::Thread => 1,
                Subjects::PerTurn => family.turns.len(),
            };
            assert_eq!(
                variant.subjects.len(),
                subjects,
                "{} {}",
                family.id,
                variant.id
            );
        }
    }
}

/// A placeholder left in the text would reach the demo as written.
#[test]
fn no_text_keeps_a_placeholder_after_filling() {
    for thread_num in 0..2_000 {
        for reply_idx in 0..thread_len(thread_num) {
            let rendered = render(&me(), seed(), thread_num, reply_idx, thread_len(thread_num));
            for text in [&rendered.subject, &rendered.body] {
                assert!(
                    !text.contains('{') && !text.contains('}'),
                    "thread {thread_num} message {reply_idx}: {text}"
                );
            }
        }
    }
}

/// Every phrasing of every family is reachable at every position, so the
/// sampled seed-clock check and the full eval see them all.
#[test]
fn every_family_variant_and_position_is_reached() {
    let mut wanted = HashSet::new();
    for family in families() {
        for variant in 0..family.variant_count() {
            for turn in 0..family.turns.len() {
                wanted.insert((family.id, variant, turn));
            }
        }
    }
    let mut seen = HashSet::new();
    for thread_num in 0..20_000 {
        let family = family_for(thread_num);
        let (variant, _) = variant_for(family, thread_num);
        for turn in 0..thread_len(thread_num) {
            seen.insert((family.id, variant, turn));
        }
        if seen.len() == wanted.len() {
            break;
        }
    }
    let missing: Vec<_> = wanted.difference(&seen).collect();
    assert!(missing.is_empty(), "never generated: {missing:?}");
}

/// A label names the sender of the mail it labels, and gives no label to
/// mail you wrote.
#[test]
fn a_label_names_the_sender_of_the_mail_it_labels() {
    let me = me();
    let mut labelled = 0;
    for thread_num in 0..2_000 {
        let len = thread_len(thread_num);
        for reply_idx in 0..len {
            let rendered = render(&me, seed(), thread_num, reply_idx, len);
            match label(thread_num, reply_idx) {
                Some(label) => {
                    labelled += 1;
                    assert_eq!(rendered.from.email, label.from_email);
                    assert_ne!(rendered.from.email, me.email);
                }
                None => assert_eq!(rendered.from.email, me.email),
            }
        }
    }
    assert!(labelled > 4_000);
}

#[test]
fn held_out_is_the_variants_past_the_visible_ones() {
    for thread_num in 0..2_000 {
        let family = family_for(thread_num);
        let (index, held) = variant_for(family, thread_num);
        assert_eq!(held, index >= family.variants.len());
        assert_eq!(family.variant(index).1, held);
    }
    // Over a long run the held-out share is the share of held-out variants,
    // within a few points.
    let (mut held, mut total) = (0, 0);
    for thread_num in 0..50_000 {
        total += 1;
        held += usize::from(variant_for(family_for(thread_num), thread_num).1);
    }
    let share = held as f64 / total as f64;
    assert!((0.25..=0.40).contains(&share), "held-out share {share}");
}

/// Mail that asks for something by a date is dated to match its label: a
/// live ask arrives in the last hours, the others at least two months back.
#[test]
fn an_ask_is_dated_to_match_whether_its_deadline_is_open() {
    let now = seed();
    let mut recent = 0;
    for thread_num in 0..13 * ASKS_RECENT_EVERY * 4 {
        let family = family_for(thread_num);
        if family.dating != Dating::Asks {
            continue;
        }
        let rendered = render(&me(), now, thread_num, 0, thread_len(thread_num));
        let age = now - rendered.date;
        if asks_recent(thread_num) {
            recent += 1;
            assert!(age < chrono::Duration::hours(11), "{thread_num}: {age}");
        } else {
            assert!(age > chrono::Duration::days(59), "{thread_num}: {age}");
        }
        let expected_todo = label(thread_num, 0).expect("the ask").expect.todo;
        assert_eq!(
            expected_todo,
            asks_recent(thread_num) && family.turns[0].expect.todo
        );
    }
    assert!(recent > 4, "only {recent} live asks");
    // The first threads, which the web e2e suite reads, carry no live ask.
    for thread_num in 0..13 * 2 {
        assert!(!asks_recent(thread_num));
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
        // Only threads that add a (family, variant, position) not yet checked.
        let fresh: Vec<usize> = (0..len)
            .filter(|turn| seen.insert((family.id, variant, *turn)))
            .collect();
        for reply_idx in fresh {
            let a = render(&me(), now, thread_num, reply_idx, len);
            let b = render(&me(), later, thread_num, reply_idx, len);
            assert_eq!(b.date - a.date, later - now);
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

fn written_dates(rendered: &Rendered) -> Vec<String> {
    super::super::date_finder::written_dates(&format!("{}\n{}", rendered.subject, rendered.body))
}
