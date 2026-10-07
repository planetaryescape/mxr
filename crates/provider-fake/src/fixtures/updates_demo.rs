//! A day of notifications for the demo's Updates briefing.
//!
//! Dated against the latest digest cut (08:00 or 16:30 local, the default
//! `updates.cuts`), so the same mail lands in the digest and in "since"
//! whatever time the demo starts:
//!
//! * Strava's weekly summary, 21.3 km after last week's 19.0 km: a delta
//!   computed by code ("up 12% on last week").
//! * GitHub acme/api: green two days ago, failing twice before the cut, a
//!   build tracker in Needs a look.
//! * Four Vercel deploys and one from last week: routine, one line.
//! * A new Google sign-in two hours before the cut: it goes to To do on
//!   arrival and shows "already in To do". Gmail's own DMARC pass and
//!   earlier mail from Google vouch for it.
//! * A Stripe payout that failed: Needs a look, and To do, vouched for the
//!   same way.
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
pub(super) const UPDATES_DEMO_MESSAGE_COUNT: usize = 20;

fn thread(account_id: &AccountId, name: &str) -> ThreadId {
    ThreadId::from_scoped_provider_id(account_id, "fake", &format!("demo-updates-{name}"))
}

fn address(name: &str, email: &str) -> Address {
    Address {
        name: Some(name.to_string()),
        email: email.to_string(),
    }
}

/// The latest default cut at or before `now`, local time. Mirrors
/// `updates.cuts`' default (08:00 and 16:30); provider-fake can't depend
/// on the Updates crate, and the demo runs on the defaults.
fn latest_cut(now: DateTime<Utc>) -> DateTime<Utc> {
    let local = now.with_timezone(&Local);
    let today = local.date_naive();
    let at = |day: chrono::NaiveDate, h: u32, m: u32| {
        NaiveTime::from_hms_opt(h, m, 0)
            .and_then(|time| Local.from_local_datetime(&day.and_time(time)).earliest())
            .map(|at| at.with_timezone(&Utc))
    };
    let yesterday = today.pred_opt().unwrap_or(today);
    [at(today, 16, 30), at(today, 8, 0), at(yesterday, 16, 30)]
        .into_iter()
        .flatten()
        .find(|cut| *cut <= now)
        .unwrap_or(now - Duration::hours(12))
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
    first_num: usize,
) -> Vec<(Envelope, MessageBody)> {
    let cut = latest_cut(now);
    let before = |hours: i64| cut - Duration::hours(hours);
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
                before(24 * 7 + 3),
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
                before(48),
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
                before(24 * 6),
            )),
            "vercel-old",
        ),
        (
            read(message(
                google.clone(),
                self_addr,
                "Your Google Account storage summary",
                "You're using 41% of your 100 GB of Google Account storage.",
                before(24 * 20),
            )),
            "google-prior",
        ),
        (
            read(message(
                stripe.clone(),
                self_addr,
                "Payout of R 3,980.00 is on its way",
                "Your payout of R 3,980.00 to your bank account ending 4417 is on its way.",
                before(24 * 14),
            )),
            "stripe-prior",
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
                before(1),
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
                before(24 * 7 + 1),
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
    let deploys = [9_i64, 7, 5, 3].map(|hours| {
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
        let domain = message
            .from
            .email
            .rsplit_once('@')
            .map(|(_, host)| host.to_string())
            .unwrap_or_default();
        let (envelope, mut body) =
            build_demo_msg(first_num + built.len(), account_id, &thread_id, message);
        // Gmail's own verdict, as it stamps it, on the senders a
        // breakthrough must be able to vouch for.
        if name.starts_with("google") || name.starts_with("stripe") {
            body.metadata.auth_results = vec![format!(
                "mx.google.com; dkim=pass; spf=pass; dmarc=pass (p=REJECT) header.from={domain}"
            )];
        }
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
