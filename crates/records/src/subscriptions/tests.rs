use super::*;
use chrono::{Datelike, TimeZone};
use chrono_tz::Europe::London;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid day")
}

fn charge(id: &str, issuer: &str, title: &str, minor: i64, on: NaiveDate) -> ChargeInput {
    ChargeInput {
        record_id: id.to_string(),
        kind: RecordKind::Receipt,
        issuer_key: issuer.to_lowercase(),
        issuer: Some(issuer.to_string()),
        title: Some(title.to_string()),
        title_stated: false,
        amount_minor: Some(minor),
        currency: Some("GBP".to_string()),
        at: day_at(on),
        checked: false,
    }
}

fn run(inputs: &[ChargeInput], today: NaiveDate) -> Detection {
    detect(inputs, &[], today, &London)
}

/// Monthly charges on `days` (year 2025, month = index + 1).
fn monthly(issuer: &str, title: &str, minor: i64, days: &[u32]) -> Vec<ChargeInput> {
    days.iter()
        .enumerate()
        .map(|(i, d)| {
            charge(
                &format!("{}-{i:02}", issuer.to_lowercase()),
                issuer,
                title,
                minor,
                day(2025, i as u32 + 1, *d),
            )
        })
        .collect()
}

#[test]
fn monthly_charges_with_a_few_days_of_jitter_are_a_subscription() {
    let inputs = monthly(
        "Spotify",
        "Spotify Premium receipt",
        1199,
        &[3, 5, 2, 7, 3, 4],
    );
    let found = run(&inputs, day(2025, 6, 20));
    assert_eq!(found.subscriptions.len(), 1);
    let sub = &found.subscriptions[0];
    assert_eq!(sub.cadence, Cadence::Monthly);
    assert_eq!(sub.product.as_deref(), Some("Premium"));
    assert_eq!(sub.amount_minor, Some(1199));
    assert_eq!(sub.yearly_cost_minor(), Some(14_388));
    assert_eq!(sub.charges.len(), 6);
    assert_eq!(sub.status, Status::Active);
    assert_eq!(sub.next_expected, Some(day(2025, 7, 4)));
    assert_eq!(sub.start(), day(2025, 1, 3));
    assert!(sub.confirmed());
    assert!(found.one_offs.is_empty());
}

#[test]
fn two_charges_are_not_enough_for_a_monthly_subscription() {
    let inputs = monthly("Spotify", "Spotify Premium receipt", 1199, &[3, 3]);
    assert!(run(&inputs, day(2025, 2, 10)).subscriptions.is_empty());
}

#[test]
fn yearly_counts_from_two_charges_and_is_unconfirmed_until_the_third() {
    let mut inputs = vec![
        charge(
            "y1",
            "Admiral",
            "Admiral receipt",
            41_200,
            day(2023, 10, 26),
        ),
        charge(
            "y2",
            "Admiral",
            "Admiral receipt",
            41_200,
            day(2024, 10, 24),
        ),
    ];
    let found = run(&inputs, day(2025, 3, 1));
    assert_eq!(found.subscriptions.len(), 1);
    let sub = &found.subscriptions[0];
    assert_eq!(sub.cadence, Cadence::Yearly);
    assert!(!sub.confirmed());
    assert_eq!(sub.next_expected, Some(day(2025, 10, 24)));
    assert_eq!(sub.yearly_cost_minor(), Some(41_200));

    inputs.push(charge(
        "y3",
        "Admiral",
        "Admiral receipt",
        41_200,
        day(2025, 10, 30),
    ));
    let found = run(&inputs, day(2025, 11, 1));
    assert!(found.subscriptions[0].confirmed());
}

#[test]
fn a_price_change_that_sticks_is_kept_with_its_date_and_signalled() {
    let mut inputs = monthly("Netflix", "Netflix", 1099, &[10, 10, 10, 10]);
    inputs.extend((4..6).map(|i| {
        charge(
            &format!("netflix-{i:02}"),
            "Netflix",
            "Netflix",
            1299,
            day(2025, i + 1, 10),
        )
    }));
    let found = run(&inputs, day(2025, 6, 15));
    assert_eq!(found.subscriptions.len(), 1, "{found:?}");
    let sub = &found.subscriptions[0];
    assert_eq!(sub.charges.len(), 6, "the two bands are one subscription");
    assert_eq!(sub.amount_minor, Some(1299));
    assert_eq!(
        sub.price_changes,
        vec![PriceChange {
            day: day(2025, 5, 10),
            record_id: "netflix-04".to_string(),
            from_minor: 1099,
            to_minor: 1299,
        }]
    );
    // The newest charge is at the old new price by now: no signal.
    assert!(signals(&found.subscriptions, day(2025, 6, 15)).is_empty());

    // On the first charge at the new price, it is a signal.
    let found = run(&inputs[..5], day(2025, 5, 12));
    assert!(matches!(
        signals(&found.subscriptions, day(2025, 5, 12)).as_slice(),
        [Signal::PriceChange { change, .. }] if change.to_minor == 1299
    ));
}

#[test]
fn a_missed_charge_is_overdue_after_the_grace_period_and_ended_after_two() {
    let inputs = monthly("Spotify", "Spotify Premium receipt", 1199, &[3, 3, 3, 3]);
    // Expected 3 May; within a week it is still active.
    assert_eq!(
        run(&inputs, day(2025, 5, 9)).subscriptions[0].status,
        Status::Active
    );
    let overdue = run(&inputs, day(2025, 5, 12));
    assert_eq!(overdue.subscriptions[0].status, Status::Overdue);
    assert!(matches!(
        signals(&overdue.subscriptions, day(2025, 5, 12)).as_slice(),
        [Signal::MissedCharge { expected, .. }] if *expected == day(2025, 5, 3)
    ));
    let ended = run(&inputs, day(2025, 6, 12));
    assert_eq!(ended.subscriptions[0].status, Status::Ended);
    assert_eq!(ended.subscriptions[0].next_expected, None);
    assert!(signals(&ended.subscriptions, day(2025, 6, 12)).is_empty());
}

#[test]
fn a_yearly_subscriptions_next_charge_within_two_weeks_is_a_renewal_signal() {
    let inputs = [
        charge("y1", "Admiral", "Admiral receipt", 41_200, day(2023, 10, 26)),
        charge("y2", "Admiral", "Admiral receipt", 41_200, day(2024, 10, 24)),
    ];
    // Due 24 Oct 2025; two weeks out is 10 Oct.
    let found = run(&inputs, day(2025, 10, 12));
    assert_eq!(found.subscriptions[0].next_expected, Some(day(2025, 10, 24)));
    assert!(matches!(
        signals(&found.subscriptions, day(2025, 10, 12)).as_slice(),
        [Signal::RenewalApproaching { due, .. }] if *due == day(2025, 10, 24)
    ));
}

#[test]
fn a_yearly_subscriptions_next_charge_months_away_is_not_yet_a_signal() {
    let inputs = [
        charge("y1", "Admiral", "Admiral receipt", 41_200, day(2023, 10, 26)),
        charge("y2", "Admiral", "Admiral receipt", 41_200, day(2024, 10, 24)),
    ];
    let found = run(&inputs, day(2025, 3, 1));
    assert!(signals(&found.subscriptions, day(2025, 3, 1)).is_empty());
}

#[test]
fn one_missed_month_between_charges_still_counts() {
    let inputs = [
        charge("a", "Spotify", "Premium", 1199, day(2025, 1, 3)),
        charge("b", "Spotify", "Premium", 1199, day(2025, 2, 3)),
        charge("c", "Spotify", "Premium", 1199, day(2025, 4, 3)),
        charge("d", "Spotify", "Premium", 1199, day(2025, 5, 3)),
    ];
    let found = run(&inputs, day(2025, 5, 10));
    let sub = &found.subscriptions[0];
    assert_eq!(sub.charges.len(), 4);
    assert_eq!((sub.single_gaps, sub.double_gaps), (2, 1));
}

#[test]
fn two_products_from_one_issuer_are_two_subscriptions() {
    // Named in the title.
    let mut inputs = monthly("Apple", "iCloud+ 200GB receipt", 299, &[1, 1, 1]);
    inputs.extend((0..3).map(|i| {
        charge(
            &format!("music-{i}"),
            "Apple",
            "Apple Music receipt",
            1099,
            day(2025, i + 1, 15),
        )
    }));
    let found = run(&inputs, day(2025, 3, 20));
    let products: Vec<_> = found
        .subscriptions
        .iter()
        .map(|s| (s.product.clone(), s.amount_minor))
        .collect();
    assert_eq!(
        products,
        vec![
            (Some("Music".to_string()), Some(1099)),
            (Some("iCloud+ 200GB".to_string()), Some(299)),
        ]
    );

    // No product in the title: the amounts tell them apart.
    let mut inputs = monthly("Google", "Google receipt", 159, &[2, 2, 2]);
    inputs.extend((0..3).map(|i| {
        charge(
            &format!("g2-{i}"),
            "Google",
            "Google receipt",
            1899,
            day(2025, i + 1, 20),
        )
    }));
    let found = run(&inputs, day(2025, 3, 25));
    let mut amounts: Vec<_> = found.subscriptions.iter().map(|s| s.amount_minor).collect();
    amounts.sort();
    assert_eq!(amounts, vec![Some(159), Some(1899)]);
}

#[test]
fn one_off_orders_from_a_shop_are_not_a_subscription() {
    let days = [
        (1, 3, 2599),
        (1, 19, 1350),
        (2, 2, 899),
        (2, 27, 4500),
        (3, 4, 1350),
        (3, 30, 2210),
        (4, 11, 1350),
        (5, 23, 640),
        (6, 1, 12999),
    ];
    let inputs: Vec<ChargeInput> = days
        .iter()
        .enumerate()
        .map(|(i, (m, d, minor))| {
            charge(
                &format!("amz-{i}"),
                "Amazon",
                "Amazon.co.uk receipt",
                *minor,
                day(2025, *m, *d),
            )
        })
        .collect();
    let found = run(&inputs, day(2025, 6, 10));
    assert!(found.subscriptions.is_empty(), "{:?}", found.subscriptions);
}

#[test]
fn one_offs_beside_a_subscription_are_marked() {
    let mut inputs = monthly("Amazon", "Amazon Prime receipt", 899, &[14, 14, 14, 14]);
    inputs.push(charge(
        "kettle",
        "Amazon",
        "Amazon receipt",
        3499,
        day(2025, 2, 20),
    ));
    inputs.push(charge(
        "book",
        "Amazon",
        "Amazon receipt",
        1299,
        day(2025, 3, 2),
    ));
    let found = run(&inputs, day(2025, 4, 20));
    assert_eq!(found.subscriptions.len(), 1);
    assert_eq!(found.subscriptions[0].product.as_deref(), Some("Prime"));
    assert_eq!(
        found.one_offs.get("amazon"),
        Some(&vec!["book".to_string(), "kettle".to_string()])
    );
}

#[test]
fn a_cancellation_email_after_the_last_charge_ends_it() {
    let inputs = monthly("Spotify", "Spotify Premium receipt", 1199, &[3, 3, 3]);
    let cancel = |subject: &str, on: NaiveDate| CancellationInput {
        issuer_key: "spotify".to_string(),
        subject: subject.to_string(),
        at: day_at(on),
        message_id: "m-cancel".to_string(),
    };
    let found = detect(
        &inputs,
        &[cancel(
            "Your Premium subscription has been cancelled",
            day(2025, 3, 20),
        )],
        day(2025, 3, 25),
        &London,
    );
    let sub = &found.subscriptions[0];
    assert_eq!(sub.status, Status::Ended);
    assert_eq!(sub.status_reason, "Cancellation email on 20 Mar 2025");

    // A threat is not an ending, and an old cancellation doesn't end a
    // subscription that started again.
    let found = detect(
        &inputs,
        &[
            cancel(
                "Your subscription will be cancelled unless you pay",
                day(2025, 3, 20),
            ),
            cancel("Your subscription has been cancelled", day(2024, 12, 1)),
        ],
        day(2025, 3, 25),
        &London,
    );
    assert_eq!(found.subscriptions[0].status, Status::Active);
}

#[test]
fn a_cancellation_naming_one_plan_ends_only_that_one() {
    let mut inputs = monthly("Apple", "iCloud+ receipt", 299, &[1, 1, 1]);
    inputs.extend((0..3).map(|i| {
        charge(
            &format!("music-{i}"),
            "Apple",
            "Apple Music receipt",
            1099,
            day(2025, i + 1, 15),
        )
    }));
    let found = detect(
        &inputs,
        &[CancellationInput {
            issuer_key: "apple".to_string(),
            subject: "Your Apple Music subscription has ended".to_string(),
            at: day_at(day(2025, 3, 20)),
            message_id: "m".to_string(),
        }],
        day(2025, 3, 25),
        &London,
    );
    let status: Vec<_> = found
        .subscriptions
        .iter()
        .map(|s| (s.product.clone().unwrap_or_default(), s.status))
        .collect();
    assert_eq!(
        status,
        vec![
            ("iCloud+".to_string(), Status::Active),
            ("Music".to_string(), Status::Ended),
        ]
    );
}

#[test]
fn a_receipt_and_an_invoice_for_one_payment_are_one_charge() {
    let mut inputs = monthly("Linear", "Linear receipt", 800, &[1, 1, 1]);
    let mut invoice = charge("inv-feb", "Linear", "Linear invoice", 800, day(2025, 2, 1));
    invoice.kind = RecordKind::Invoice;
    inputs.push(invoice);
    let found = run(&inputs, day(2025, 3, 10));
    let sub = &found.subscriptions[0];
    assert_eq!(sub.charges.len(), 3);
    assert_eq!(sub.charges[1].record_ids, vec!["inv-feb", "linear-01"]);
    assert!(found.one_offs.is_empty());
}

#[test]
fn the_result_does_not_depend_on_storage_order() {
    let mut inputs = monthly("Spotify", "Spotify Premium receipt", 1199, &[3, 5, 2, 7]);
    inputs.extend(monthly("Netflix", "Netflix", 1099, &[10, 10, 10]));
    inputs.push(charge("x", "Netflix", "Netflix", 1099, day(2025, 2, 11)));
    let forward = run(&inputs, day(2025, 4, 12));
    inputs.reverse();
    let backward = run(&inputs, day(2025, 4, 12));
    assert_eq!(forward, backward);
    inputs.rotate_left(3);
    assert_eq!(forward, run(&inputs, day(2025, 4, 12)));
}

#[test]
fn a_late_evening_charge_counts_on_the_users_day() {
    // 23:30 on 31 Mar in London is 22:30 UTC: still the 31st. 23:30 UTC
    // on 30 Jun is 00:30 on 1 Jul in London (BST).
    let at = |y, m, d, h, min| {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0)
            .single()
            .expect("time")
    };
    assert_eq!(local_day(at(2025, 6, 30, 23, 30), &London), day(2025, 7, 1));
    assert_eq!(
        local_day(at(2025, 1, 31, 23, 30), &London),
        day(2025, 1, 31)
    );
    // A day stored at midday UTC stays that day in any zone.
    assert_eq!(
        local_day(day_at(day(2025, 6, 30)), &chrono_tz::Pacific::Auckland),
        day(2025, 6, 30)
    );
}

#[test]
fn totals_are_per_currency_and_never_converted() {
    let mut inputs = monthly("Spotify", "Spotify Premium receipt", 1199, &[3, 3, 3]);
    let mut dollars = monthly("GitHub", "GitHub Copilot receipt", 1000, &[9, 9, 9]);
    for input in &mut dollars {
        input.currency = Some("USD".to_string());
    }
    inputs.extend(dollars);
    inputs.extend([
        charge("y1", "Admiral", "Admiral receipt", 41_200, day(2024, 3, 20)),
        charge("y2", "Admiral", "Admiral receipt", 41_200, day(2025, 3, 20)),
    ]);
    // Ended: not counted.
    inputs.extend(
        monthly("Old", "Old receipt", 500, &[1, 1, 1])
            .into_iter()
            .map(|mut c| {
                c.at = day_at(day(2023, c.at.month(), 1));
                c
            }),
    );
    let found = run(&inputs, day(2025, 3, 25));
    assert_eq!(
        totals(&found.subscriptions),
        vec![
            ("GBP".to_string(), 4_632, 55_588),
            ("USD".to_string(), 1_000, 12_000),
        ]
    );
}

#[test]
fn a_stated_plan_may_change_amount_but_a_wandering_amount_is_not_a_subscription() {
    let amounts = [4210, 6105, 3890, 5522];
    let wandering: Vec<ChargeInput> = amounts
        .iter()
        .enumerate()
        .map(|(i, minor)| {
            charge(
                &format!("e{i}"),
                "Octopus",
                "Electricity",
                *minor,
                day(2025, i as u32 + 1, 12),
            )
        })
        .collect();
    assert!(run(&wandering, day(2025, 4, 20)).subscriptions.is_empty());

    let stated: Vec<ChargeInput> = wandering
        .into_iter()
        .map(|mut c| {
            c.title_stated = true;
            c
        })
        .collect();
    assert_eq!(run(&stated, day(2025, 4, 20)).subscriptions.len(), 1);
}

#[test]
fn weekly_and_quarterly_cadences_are_found() {
    let weekly: Vec<ChargeInput> = (0..5)
        .map(|i| {
            charge(
                &format!("w{i}"),
                "Guardian",
                "Guardian receipt",
                450,
                day(2025, 1, 6) + Duration::days(7 * i),
            )
        })
        .collect();
    assert_eq!(
        run(&weekly, day(2025, 2, 6)).subscriptions[0].cadence,
        Cadence::Weekly
    );
    let quarterly: Vec<ChargeInput> = [(1, 15), (4, 14), (7, 16)]
        .iter()
        .enumerate()
        .map(|(i, (m, d))| {
            charge(
                &format!("q{i}"),
                "Water",
                "Water receipt",
                9000,
                day(2025, *m, *d),
            )
        })
        .collect();
    let found = run(&quarterly, day(2025, 8, 1));
    assert_eq!(found.subscriptions[0].cadence, Cadence::Quarterly);
    assert_eq!(found.subscriptions[0].yearly_cost_minor(), Some(36_000));
}

#[test]
fn a_daily_habit_at_one_price_is_not_a_weekly_subscription() {
    // A £3.20 coffee most weekdays for two months.
    let inputs: Vec<ChargeInput> = (0..60)
        .filter(|i| i % 7 < 5)
        .map(|i| {
            charge(
                &format!("c{i:02}"),
                "Pret",
                "Pret receipt",
                320,
                day(2025, 3, 3) + Duration::days(i),
            )
        })
        .collect();
    assert!(run(&inputs, day(2025, 5, 5)).subscriptions.is_empty());
}

#[test]
fn a_shops_many_prices_never_form_one_wide_band() {
    // Amounts 1% apart from £10 to £30: neighbours are close, the ends are
    // not, so no band spans them.
    let inputs: Vec<ChargeInput> = (0..110)
        .map(|i| {
            let minor = (1000.0 * 1.01_f64.powi(i)) as i64;
            charge(
                &format!("s{i:03}"),
                "Shop",
                "Shop receipt",
                minor,
                day(2025, 1, 1) + Duration::days(i64::from(i) * 3),
            )
        })
        .collect();
    assert!(run(&inputs, day(2026, 1, 1)).subscriptions.is_empty());
}
