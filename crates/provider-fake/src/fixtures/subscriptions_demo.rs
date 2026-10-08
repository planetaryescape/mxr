//! Receipts that recur, so the demo's Archive has subscriptions to show.
//!
//! * Spotify Premium: four monthly receipts at £11.99, active, and one
//!   gift card from Spotify that is a one-off.
//! * Netflix: three monthly receipts at £10.99, then the newest at £12.99:
//!   a price change, signalled.
//! * Headspace: three monthly receipts, the last one 40 days ago: the
//!   expected charge is overdue, signalled as missed.
//! * 1Password: two yearly receipts in dollars, the newest a month ago:
//!   yearly, unconfirmed, and totalled in its own currency.
//! * Disney+: three monthly receipts and then a cancellation email: ended.

use super::{build_demo_msg, DemoMessage};
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageBody, MessageFlags, UnsubscribeMethod};

/// Messages `subscriptions_demo_messages` returns.
pub(super) const SUBSCRIPTIONS_DEMO_MESSAGE_COUNT: usize = 18;

fn address(name: &str, email: &str) -> Address {
    Address {
        name: Some(name.to_string()),
        email: email.to_string(),
    }
}

/// Numbered from `first_num` so provider ids follow the seeded messages
/// before them.
pub(super) fn subscriptions_demo_messages(
    account_id: &AccountId,
    self_addr: &Address,
    now: DateTime<Utc>,
    first_num: usize,
) -> Vec<(Envelope, MessageBody)> {
    let mut built = Vec::with_capacity(SUBSCRIPTIONS_DEMO_MESSAGE_COUNT);
    let mut push = |from: &Address, subject: &str, body: String, days_ago: i64| {
        let num = first_num + built.len();
        let thread_id = ThreadId::from_scoped_provider_id(
            account_id,
            "fake",
            &format!("demo-subscription-{num}"),
        );
        let message = DemoMessage {
            from: from.clone(),
            to: vec![self_addr.clone()],
            cc: Vec::new(),
            snippet: body.chars().take(120).collect(),
            subject: subject.to_string(),
            body_text: body,
            date: now - Duration::days(days_ago),
            flags: MessageFlags::READ,
            has_attachments: false,
            category: 9,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec!["INBOX".to_string()],
            in_reply_to: None,
            references: Vec::new(),
        };
        built.push(build_demo_msg(num, account_id, &thread_id, message));
    };
    let receipt = |total: &str| format!("Thanks for your payment.\nTotal: {total}");

    let spotify = address("Spotify", "no-reply@spotify.com");
    for days in [92, 61, 31, 1] {
        push(
            &spotify,
            "Your Spotify Premium receipt",
            receipt("£11.99"),
            days,
        );
    }
    push(&spotify, "Your Spotify receipt", receipt("£30.00"), 45);

    let netflix = address("Netflix", "info@account.netflix.com");
    for (days, total) in [
        (93, "£10.99"),
        (62, "£10.99"),
        (32, "£10.99"),
        (2, "£12.99"),
    ] {
        push(&netflix, "Your Netflix receipt", receipt(total), days);
    }

    let headspace = address("Headspace", "no-reply@headspace.com");
    for days in [100, 70, 40] {
        push(&headspace, "Your Headspace receipt", receipt("£9.99"), days);
    }

    let onepassword = address("1Password", "billing@1password.com");
    for days in [395, 30] {
        push(
            &onepassword,
            "Your 1Password receipt",
            receipt("$35.88"),
            days,
        );
    }

    let disney = address("Disney+", "disneyplus@mail.disneyplus.com");
    for days in [120, 90, 60] {
        push(&disney, "Your Disney+ receipt", receipt("£7.99"), days);
    }
    push(
        &disney,
        "Your Disney+ subscription has been cancelled",
        "We're sorry to see you go. You can come back any time.".to_string(),
        50,
    );

    built
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_count_matches_the_mail_built() {
        let account_id = AccountId::from_provider_id("fake", "alex@demo.mxr.local");
        let me = address("Alex Demo", "alex@demo.mxr.local");
        let built = subscriptions_demo_messages(&account_id, &me, Utc::now(), 1);
        assert_eq!(built.len(), SUBSCRIPTIONS_DEMO_MESSAGE_COUNT);
    }
}
