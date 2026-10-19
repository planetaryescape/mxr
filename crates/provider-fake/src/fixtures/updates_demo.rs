//! A day of notifications for the demo's Updates briefing.
//!
//! Dated against the latest digest cut (the daemon's `updates.cuts`, 08:00
//! and 16:30 local by default), so the same mail lands in the digest and in "since"
//! whatever time the demo starts:
//!
//! * Strava's weekly summary, 21.3 km after last week's 19.0 km: a delta
//!   computed by code ("up 12% on last week").
//! * GitHub acme/api: green two days ago, failing twice before the cut, a
//!   build tracker in Needs a look.
//! * Four Vercel deploys and one from last week: routine, one line.
//! * A new Google sign-in before the cut: the top of Needs a
//!   look, as a suggested to-do; `t` makes it one.
//! * A Stripe payout that failed: Needs a look, suggested the same way.
//! * A verification code from before the cut: expired after ten minutes,
//!   so it never shows.
//! * A status page incident that was resolved: Changed.
//! * Plausible's weekly report, down on last week.
//! * A Linear update after the cut: in "since", waiting for the next one.

use super::{build_demo_msg, DemoMessage};
use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageBody, MessageFlags, UnsubscribeMethod};

/// Messages `updates_demo_messages` returns.
pub(super) const UPDATES_DEMO_MESSAGE_COUNT: usize = 18;

fn thread(account_id: &AccountId, name: &str) -> ThreadId {
    ThreadId::from_scoped_provider_id(account_id, "fake", &format!("demo-updates-{name}"))
}

fn address(name: &str, email: &str) -> Address {
    Address {
        name: Some(name.to_string()),
        email: email.to_string(),
    }
}

/// The default `updates.cuts`, for a fake provider built without the
/// daemon's config (tests, `MXR_FAKE_DATASET` alone).
const DEFAULT_CUTS: [&str; 2] = ["08:00", "16:30"];

/// Digest cuts as local times of day. provider-fake can't depend on the
/// Updates crate, so the daemon hands over its validated `updates.cuts`
/// and the fixture dates the mail against those, not a copy of them.
#[derive(Clone, Debug)]
pub struct DemoCuts(Vec<NaiveTime>);

impl DemoCuts {
    /// "HH:MM" strings; anything unparsable or empty means the defaults.
    pub fn parse(values: &[String]) -> Self {
        let parsed: Option<Vec<NaiveTime>> = values
            .iter()
            .map(|value| NaiveTime::parse_from_str(value.trim(), "%H:%M").ok())
            .collect();
        match parsed {
            Some(times) if !times.is_empty() => Self(times),
            _ => Self::default(),
        }
    }
}

impl Default for DemoCuts {
    fn default() -> Self {
        Self(
            DEFAULT_CUTS
                .iter()
                .filter_map(|value| NaiveTime::parse_from_str(value, "%H:%M").ok())
                .collect(),
        )
    }
}

/// The previous and latest cuts, using the daemon's DST gap/fold policy.
fn cut_window<Tz: TimeZone>(
    now: DateTime<Utc>,
    cuts: &DemoCuts,
    tz: &Tz,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let today = now.with_timezone(tz).date_naive();
    let yesterday = today.pred_opt().unwrap_or(today);
    let instants: Vec<_> = [yesterday.pred_opt().unwrap_or(yesterday), yesterday, today]
        .into_iter()
        .flat_map(|day| {
            cuts.0.iter().filter_map(move |time| {
                let local = day.and_time(*time);
                tz.from_local_datetime(&local)
                    .earliest()
                    .or_else(|| {
                        tz.from_local_datetime(&(local + Duration::hours(1)))
                            .earliest()
                    })
                    .map(|at| at.with_timezone(&Utc))
            })
        })
        .filter(|cut| *cut <= now)
        .collect();
    let cut = instants.iter().copied().max().unwrap_or(now);
    let previous = instants
        .into_iter()
        .filter(|at| *at < cut)
        .max()
        .unwrap_or(cut - Duration::days(1));
    (previous, cut)
}

fn message(
    from: Address,
    to: &Address,
    subject: &str,
    body: &str,
    date: DateTime<Utc>,
) -> DemoMessage {
    DemoMessage {
        from,
        to: vec![to.clone()],
        cc: Vec::new(),
        snippet: body.chars().take(120).collect(),
        subject: subject.to_string(),
        body_text: body.to_string(),
        date,
        flags: MessageFlags::empty(),
        has_attachments: false,
        category: 9,
        unsubscribe: UnsubscribeMethod::None,
        label_provider_ids: vec!["INBOX".to_string()],
        in_reply_to: None,
        references: Vec::new(),
    }
}

/// Numbered from `first_num` so provider ids follow the seeded messages
/// before them.
pub(super) fn updates_demo_messages(
    account_id: &AccountId,
    self_addr: &Address,
    now: DateTime<Utc>,
    cuts: &DemoCuts,
    first_num: usize,
) -> Vec<(Envelope, MessageBody)> {
    let (previous, cut) = cut_window(now, cuts, &Local);
    // Keep the original ordering, even when configured cuts are minutes apart.
    // Nine is the oldest fresh offset; leave a margin after the previous cut.
    let step = Duration::hours(1).min((cut - previous) / 10);
    let before = |hours: i32| cut - step * hours;
    // After the cut but never in the future.
    let since = cut + (now - cut) / 2;

    let strava = address("Strava", "no-reply@strava.com");
    let github = address("GitHub", "notifications@github.com");
    let vercel = address("Vercel", "notifications@vercel.com");
    let google = address("Google", "no-reply@accounts.google.com");
    let stripe = address("Stripe", "notifications@stripe.com");
    let acme = address("Acme Login", "verify@acme-login.example");
    let status = address("Linear Status", "notifications@linearstatus.example");
    let plausible = address("Plausible", "notifications@plausible.io");
    let linear = address("Linear", "notifications@linear.app");

    let read = |mut m: DemoMessage| {
        m.flags = MessageFlags::READ;
        m.label_provider_ids = Vec::new();
        m
    };
    let messages: Vec<(DemoMessage, &str)> = vec![
        (
            read(message(
                strava.clone(),
                self_addr,
                "Your week in running: 19.0 km over 2 runs",
                "Hi Alex,\n\nYou covered 19.0 km over 2 runs this week.\n\nView your week https://www.strava.com/athlete/training",
                cut - Duration::hours(24 * 7 + 3),
            )),
            "strava-last",
        ),
        (
            message(
                strava,
                self_addr,
                "Your week in running: 21.3 km over 3 runs",
                "Hi Alex,\n\nYou covered 21.3 km over 3 runs this week, your longest week this month.\n\nView your week https://www.strava.com/athlete/training",
                before(3),
            ),
            "strava",
        ),
        (
            read(message(
                github.clone(),
                self_addr,
                "[acme/api] Run succeeded: CI - main (5e6f7a8)",
                "All jobs have passed.\n\nView workflow run https://github.com/acme/api/actions/runs/101",
                cut - Duration::hours(48),
            )),
            "gh-green",
        ),
        (
            message(
                github.clone(),
                self_addr,
                "[acme/api] Run failed: CI - main (a1b2c3d)",
                "The CI workflow failed on main.\n\nView workflow run https://github.com/acme/api/actions/runs/102",
                before(5),
            ),
            "gh-red-1",
        ),
        (
            message(
                github,
                self_addr,
                "[acme/api] Run failed: CI - main (9f8e7d6)",
                "The CI workflow failed on main again.\n\nView workflow run https://github.com/acme/api/actions/runs/103",
                before(2),
            ),
            "gh-red-2",
        ),
        (
            read(message(
                vercel.clone(),
                self_addr,
                "Deployment succeeded for acme-web",
                "Your deployment of acme-web is live.\n\nOpen deployment https://vercel.com/acme/acme-web",
                cut - Duration::days(6),
            )),
            "vercel-old",
        ),
        (
            message(
                google,
                self_addr,
                "Security alert: New sign-in from Chrome on Windows",
                "We noticed a new sign-in to your Google Account on a Windows device in Lisbon. If this was you, you don't need to do anything.\n\nCheck activity https://myaccount.google.com/notifications",
                before(2),
            ),
            "google-signin",
        ),
        (
            message(
                stripe,
                self_addr,
                "Payout of R 4,210.00 failed: bank declined",
                "Your payout of R 4,210.00 to your bank account ending 4417 failed because the bank declined it. Update your bank details to retry.",
                before(4),
            ),
            "stripe-payout",
        ),
        (
            message(
                acme,
                self_addr,
                "Your verification code is 482913",
                "Your Acme verification code is 482913. It expires in 10 minutes.",
                cut - Duration::hours(1),
            ),
            "acme-code",
        ),
        (
            message(
                status.clone(),
                self_addr,
                "[Investigating] Degraded performance on the dashboard",
                "We are investigating reports of slow dashboard loads.\n\nView incident https://linearstatus.example/incidents/42",
                before(6),
            ),
            "incident-open",
        ),
        (
            message(
                status,
                self_addr,
                "[Resolved] Degraded performance on the dashboard",
                "This incident has been resolved. Dashboard loads are back to normal.\n\nView incident https://linearstatus.example/incidents/42",
                before(4),
            ),
            "incident-done",
        ),
        (
            read(message(
                plausible.clone(),
                self_addr,
                "Weekly report: 1,204 visitors",
                "Here's your weekly report for acme.dev: 1,204 visitors.\n\nOpen dashboard https://plausible.io/acme.dev",
                cut - Duration::hours(24 * 7 + 1),
            )),
            "plausible-last",
        ),
        (
            message(
                plausible,
                self_addr,
                "Weekly report: 998 visitors",
                "Here's your weekly report for acme.dev: 998 visitors.\n\nOpen dashboard https://plausible.io/acme.dev",
                before(1),
            ),
            "plausible",
        ),
        (
            message(
                linear,
                self_addr,
                "3 issues moved to Done in Q4 Launch",
                "Three issues in Q4 Launch moved to Done.\n\nOpen project https://linear.app/acme/project/q4-launch",
                since,
            ),
            "linear-since",
        ),
    ];
    let deploys = [9, 7, 5, 3].map(|hours| {
        (
            message(
                vercel.clone(),
                self_addr,
                "Deployment succeeded for acme-web",
                "Your deployment of acme-web is live.\n\nOpen deployment https://vercel.com/acme/acme-web",
                before(hours),
            ),
            hours,
        )
    });

    let mut built = Vec::with_capacity(UPDATES_DEMO_MESSAGE_COUNT);
    for (message, name) in messages {
        let thread_id = thread(account_id, name);
        let (envelope, body) =
            build_demo_msg(first_num + built.len(), account_id, &thread_id, message);
        built.push((envelope, body));
    }
    for (message, hours) in deploys {
        let thread_id = thread(account_id, &format!("vercel-{hours}"));
        built.push(build_demo_msg(
            first_num + built.len(),
            account_id,
            &thread_id,
            message,
        ));
    }
    built
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Local
            .with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn fresh_updates_fit_dense_and_midnight_windows_without_redating_history() {
        let account = AccountId::new();
        let recipient = address("Alex", "alex@example.com");
        for (seed, times, previous, cut) in [
            (
                at(19, 4, 44),
                ["03:14", "05:24", "03:17"],
                at(19, 3, 14),
                at(19, 3, 17),
            ),
            (
                at(20, 0, 2),
                ["23:59", "00:01", "08:00"],
                at(19, 23, 59),
                at(20, 0, 1),
            ),
        ] {
            let cuts = DemoCuts::parse(&times.map(str::to_string));
            let messages = updates_demo_messages(&account, &recipient, seed, &cuts, 1);
            assert_eq!(messages.len(), 18);
            // The twelve fresh messages, excluding history, the expired code,
            // and Linear's post-cut update. These bounds are the digest contract.
            for index in [1, 3, 4, 6, 7, 9, 10, 12, 14, 15, 16, 17] {
                let envelope = &messages[index].0;
                assert!(
                    previous < envelope.date && envelope.date < cut,
                    "{} at {} outside ({previous}, {cut})",
                    envelope.subject,
                    envelope.date
                );
            }
            for (index, hours) in [(0, 171), (2, 48), (5, 144), (11, 169)] {
                assert_eq!(messages[index].0.date, cut - Duration::hours(hours));
            }
            assert!(
                messages[3].0.date < messages[4].0.date,
                "failed runs stay ordered"
            );
            assert!(
                messages[9].0.date < messages[10].0.date,
                "resolution follows investigation"
            );
            assert!(
                messages[8].0.date + Duration::minutes(10) < cut,
                "verification stays expired"
            );
            assert!(cut < messages[13].0.date && messages[13].0.date <= seed);
        }
    }
}
