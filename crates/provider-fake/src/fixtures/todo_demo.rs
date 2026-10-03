//! Mail that gives the demo's To do something to show, and something it
//! must not show.
//!
//! * Camden Council's council tax bill, due in five days, with schema.org
//!   `Invoice` markup, a pay link on the council's own domain and a DMARC
//!   pass, after three statements over 80 days: an "Open email to pay"
//!   row that points at the council's link.
//! * Spotify's failed payment yesterday: a to-do at once.
//! * Sam's leaving drinks, which ended yesterday: already over, so never
//!   shown.
//! * A message Alex sent promising the signed engagement form, with no
//!   date; the demo command keeps it as a promise.
//! * A bank statement and a Klarna payment collected automatically: an
//!   update and a scheduled payment, neither a to-do.
//! * A water bill already past its due date and an undated tenancy renewal
//!   to sign: backlog from before the first run, so they wait in the
//!   one-time catch-up instead of landing in Now.

use super::{build_demo_msg, DemoMessage};
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageBody, MessageFlags, UnsubscribeMethod};

/// Messages `todo_demo_messages` returns.
pub(super) const TODO_DEMO_MESSAGE_COUNT: usize = 11;

/// The sent message the demo command records as an undated promise.
const TODO_DEMO_PROMISE_SUBJECT: &str = "Re: Engagement form";
/// Its position among the messages `todo_demo_messages` returns.
pub(super) const TODO_DEMO_PROMISE_POSITION: usize = 6;

fn thread(account_id: &AccountId, name: &str) -> ThreadId {
    ThreadId::from_scoped_provider_id(account_id, "fake", &format!("demo-todo-{name}"))
}

fn address(name: &str, email: &str) -> Address {
    Address {
        name: Some(name.to_string()),
        email: email.to_string(),
    }
}

fn message(
    from: Address,
    to: &Address,
    subject: &str,
    body: String,
    date: DateTime<Utc>,
) -> DemoMessage {
    DemoMessage {
        from,
        to: vec![to.clone()],
        cc: Vec::new(),
        snippet: body.chars().take(120).collect(),
        subject: subject.to_string(),
        body_text: body,
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

const CAMDEN_AUTH: &str = "mx.google.com; dkim=pass header.i=@camden.gov.uk header.s=s1; spf=pass (google.com: domain of council.tax@camden.gov.uk designates 192.0.2.1 as permitted sender) smtp.mailfrom=council.tax@camden.gov.uk; dmarc=pass (p=REJECT sp=REJECT dis=NONE) header.from=camden.gov.uk";

/// Numbered from `first_num` so provider ids follow the seeded messages
/// before them.
pub(super) fn todo_demo_messages(
    account_id: &AccountId,
    self_addr: &Address,
    now: DateTime<Utc>,
    first_num: usize,
) -> Vec<(Envelope, MessageBody)> {
    let camden = address("Camden Council", "council.tax@camden.gov.uk");
    let due = now + Duration::days(5);
    let due_words = due.format("%-d %B %Y").to_string();
    let due_iso = due.format("%Y-%m-%d").to_string();
    let event = now - Duration::days(1);
    let reply_by = now - Duration::days(4);
    let collected = now + Duration::days(3);
    let water_due = now - Duration::days(2);

    let mut built = Vec::with_capacity(TODO_DEMO_MESSAGE_COUNT);
    let mut push = |message: DemoMessage, thread_id: ThreadId, html: Option<String>, auth: bool| {
        let (envelope, mut body) =
            build_demo_msg(first_num + built.len(), account_id, &thread_id, message);
        if let Some(html) = html {
            body.text_html = Some(html);
        }
        if auth {
            body.metadata.auth_results = vec![CAMDEN_AUTH.to_string()];
        }
        built.push((envelope, body));
    };

    // Three statements over 80 days: a biller you have a history with.
    for days in [120, 80, 40] {
        let mut statement = message(
            camden.clone(),
            self_addr,
            "Your council tax statement",
            "Your council tax statement is attached. You don't need to do anything yet; we'll write when your next instalment is due.".to_string(),
            now - Duration::days(days),
        );
        statement.flags = MessageFlags::READ;
        push(
            statement,
            thread(account_id, &format!("camden-statement-{days}")),
            None,
            true,
        );
    }

    let bill_text = format!(
        "Dear Alex,\n\nYour council tax payment of £142.00 is due on {due_words}.\n\nPay online: https://www.camden.gov.uk/pay-council-tax\n\nAccount reference: 4471 0092 18\n\nCamden Council"
    );
    let bill_html = format!(
        r#"<!doctype html><html><head><script type="application/ld+json">{{"@context":"https://schema.org","@type":"Invoice","provider":{{"@type":"Organization","name":"Camden Council"}},"description":"Council tax","accountId":"4471 0092 18","totalPaymentDue":{{"@type":"PriceSpecification","price":"142.00","priceCurrency":"GBP"}},"paymentDueDate":"{due_iso}","paymentStatus":"https://schema.org/PaymentDue","url":"https://www.camden.gov.uk/pay-council-tax"}}</script></head><body><p>Dear Alex,</p><p>Your council tax payment of £142.00 is due on {due_words}.</p><p><a href="https://www.camden.gov.uk/pay-council-tax">Pay now</a></p><p>Account reference: 4471 0092 18</p><p><a href="https://www.camden.gov.uk/privacy">Privacy</a></p></body></html>"#
    );
    push(
        message(
            camden,
            self_addr,
            "Your council tax bill",
            bill_text,
            now - Duration::hours(20),
        ),
        thread(account_id, "camden-bill"),
        Some(bill_html),
        true,
    );

    push(
        message(
            address("Spotify", "no-reply@spotify.com"),
            self_addr,
            "We can't process your payment",
            "Hi Alex,\n\nWe couldn't charge your card for Spotify Premium (£11.99). Update your payment details to keep listening: https://www.spotify.com/account/update-payment\n\nThe Spotify team".to_string(),
            now - Duration::days(1),
        ),
        thread(account_id, "spotify"),
        None,
        false,
    );

    push(
        message(
            address("Sam Okafor", "sam@work.com"),
            self_addr,
            "Sam's leaving drinks",
            format!(
                "Hi all,\n\nDrinks for my last day on {}, at the Lamb from 6pm. Please RSVP by {} so I can book the room.\n\nSam",
                event.format("%-d %B %Y"),
                reply_by.format("%-d %B %Y")
            ),
            now - Duration::days(9),
        ),
        thread(account_id, "drinks"),
        None,
        false,
    );

    let mut promise = message(
        self_addr.clone(),
        &address("Priya Shah", "priya@work.com"),
        TODO_DEMO_PROMISE_SUBJECT,
        "Hi Priya,\n\nThanks for the call. I'll send you the signed engagement form.\n\nAlex"
            .to_string(),
        now - Duration::days(2),
    );
    promise.flags = MessageFlags::READ | MessageFlags::SENT;
    promise.label_provider_ids = vec!["SENT".to_string()];
    push(promise, thread(account_id, "engagement"), None, false);

    push(
        message(
            address("Monzo", "statements@monzo.com"),
            self_addr,
            "Your October statement is ready",
            "Your October statement is ready to view in the app. Balance at the end of the month: £1,204.33.".to_string(),
            now - Duration::days(3),
        ),
        thread(account_id, "statement"),
        None,
        false,
    );

    push(
        message(
            address("Klarna", "noreply@klarna.com"),
            self_addr,
            "Your scheduled payment",
            format!(
                "Hi Alex,\n\nYour next payment of £33.33 for your order is scheduled for {}. It will be collected automatically from your card, so you don't need to do anything.\n\nKlarna",
                collected.format("%-d %B %Y")
            ),
            now - Duration::days(2),
        ),
        thread(account_id, "klarna"),
        None,
        false,
    );

    push(
        message(
            address("Thames Water", "billing@thameswater.co.uk"),
            self_addr,
            "Reminder: your water bill",
            format!(
                "Hi Alex,\n\nYour water bill of £48.20 was due on {}. Please pay now to avoid a late payment charge: https://www.thameswater.co.uk/pay-my-bill\n\nThames Water",
                water_due.format("%-d %B %Y")
            ),
            now - Duration::days(9),
        ),
        thread(account_id, "water"),
        None,
        false,
    );

    push(
        message(
            address("Foxtons via DocuSign", "dse@docusign.net"),
            self_addr,
            "Please DocuSign: Tenancy renewal",
            "Foxtons sent you a document to review and sign: Tenancy renewal for Flat 2, 14 Albert Street.\n\nREVIEW DOCUMENT https://www.docusign.net/Signing/?ti=demo-tenancy".to_string(),
            now - Duration::days(3),
        ),
        thread(account_id, "tenancy"),
        None,
        false,
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
        let built = todo_demo_messages(&account_id, &me, Utc::now(), 1);
        assert_eq!(built.len(), TODO_DEMO_MESSAGE_COUNT);
        assert_eq!(
            built[TODO_DEMO_PROMISE_POSITION].0.subject,
            TODO_DEMO_PROMISE_SUBJECT
        );
    }
}
