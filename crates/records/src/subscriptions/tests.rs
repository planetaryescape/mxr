use super::*;
use chrono::Datelike;
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
        &[3, 5, 4, 7, 3, 4],
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
    assert!(signals(&found.subscriptions, day(2025, 6, 15)).is_empty());

    let found = run(&inputs[..5], day(2025, 5, 12));
    assert!(matches!(
        signals(&found.subscriptions, day(2025, 5, 12)).as_slice(),
        [Signal::PriceChange { change, .. }] if change.to_minor == 1299
    ));
}

#[test]
fn a_missed_charge_is_overdue_after_the_grace_period_and_ended_after_two() {
    let inputs = monthly("Spotify", "Spotify Premium receipt", 1199, &[3, 3, 3, 3]);
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
