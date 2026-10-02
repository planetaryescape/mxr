use super::*;
use chrono::NaiveDate;
use chrono_tz::Europe::London;

fn sent() -> DateTime<Utc> {
    // Monday 28 September 2026, 08:00 London.
    London
        .with_ymd_and_hms(2026, 9, 28, 8, 0, 0)
        .single()
        .expect("valid time")
        .with_timezone(&Utc)
}

struct Mail {
    subject: &'static str,
    body: &'static str,
    html: Option<String>,
    from_name: Option<&'static str>,
    from_email: &'static str,
    list_mail: bool,
}

impl Mail {
    fn new(from_email: &'static str, subject: &'static str, body: &'static str) -> Self {
        Self {
            subject,
            body,
            html: None,
            from_name: None,
            from_email,
            list_mail: false,
        }
    }

    fn named(mut self, name: &'static str) -> Self {
        self.from_name = Some(name);
        self
    }

    fn html(mut self, html: String) -> Self {
        self.html = Some(html);
        self
    }

    fn list(mut self) -> Self {
        self.list_mail = true;
        self
    }

    fn detect(&self) -> Option<Detection> {
        detect(
            &MessageInput {
                subject: self.subject,
                body_text: self.body,
                body_html: self.html.as_deref(),
                from_name: self.from_name,
                from_email: self.from_email,
                sent: sent(),
                list_mail: self.list_mail,
            },
            &London,
        )
    }
}

fn due_day(detection: &Detection) -> Option<NaiveDate> {
    detection
        .due
        .as_ref()
        .map(|due| due.at.with_timezone(&London).date_naive())
}

fn day(m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, d).expect("valid date")
}

#[test]
fn council_tax_invoice_reads_from_schema_org() {
    let html = r#"<script type="application/ld+json">{"@type":"Invoice",
        "provider":{"name":"Camden Council"},"description":"Council tax",
        "totalPaymentDue":{"price":"142.00","priceCurrency":"GBP"},
        "paymentDueDate":"2026-10-09","url":"https://www.camden.gov.uk/pay-council-tax"}</script>
        <p>Your council tax payment is due.</p>"#
        .to_string();
    let detection = Mail::new(
        "council.tax@camden.gov.uk",
        "Your council tax bill",
        "Your council tax payment is due.",
    )
    .named("Camden Council")
    .html(html)
    .detect()
    .expect("a to-do");
    assert_eq!(detection.kind, TodoKind::Bill);
    assert_eq!(detection.title, "Pay council tax");
    assert_eq!(detection.counterparty.as_deref(), Some("Camden Council"));
    assert_eq!(
        detection.amount.as_ref().map(Amount::display).as_deref(),
        Some("£142.00")
    );
    assert_eq!(due_day(&detection), Some(day(10, 9)));
    assert_eq!(detection.origin, Origin::Schema);
    assert_eq!(
        detection.fields.get("amount").map(|f| f.source),
        Some(FieldSource::Schema)
    );
    assert_eq!(
        detection.fields.get("due_at").map(|f| f.source),
        Some(FieldSource::Schema)
    );
    assert_eq!(
        detection.link.as_ref().map(|link| link.url.as_str()),
        Some("https://www.camden.gov.uk/pay-council-tax")
    );
    assert_eq!(detection.doc_type.as_deref(), Some("bill_link"));
}

#[test]
fn bill_from_text_needs_a_payment_word_and_an_amount_or_date() {
    let detection = Mail::new(
        "bills@thameswater.co.uk",
        "Your water bill is ready",
        "Amount due: £48.20. Please pay by 15 October 2026 at https://www.thameswater.co.uk/pay",
    )
    .named("Thames Water")
    .detect()
    .expect("a bill");
    assert_eq!(detection.title, "Pay water bill");
    assert_eq!(detection.amount.as_ref().map(|a| a.minor), Some(4820));
    assert_eq!(due_day(&detection), Some(day(10, 15)));
    assert_eq!(detection.reason, "\"pay by 15 October 2026\" (rule)");
    assert!(
        Mail::new("bills@example.com", "Your bill", "Your bill is attached.")
            .detect()
            .is_none()
    );
}

#[test]
fn failed_payment_is_a_to_do_even_by_direct_debit() {
    let detection = Mail::new(
        "no-reply@spotify.com",
        "We can't process your payment",
        "We couldn't charge your card for Spotify Premium. Update your payment details to keep listening.",
    )
    .named("Spotify")
    .detect()
    .expect("a failed payment");
    assert_eq!(detection.kind, TodoKind::PaymentFailed);
    assert_eq!(detection.title, "Fix payment for Spotify");
    let dd = Mail::new(
        "bills@octopus.energy",
        "Your Direct Debit payment failed",
        "Your direct debit was returned unpaid.",
    )
    .detect()
    .expect("a failed direct debit");
    assert_eq!(dd.kind, TodoKind::PaymentFailed);
}

#[test]
fn scheduled_collections_receipts_and_statements_are_not_to_dos() {
    let klarna = Mail::new(
        "noreply@klarna.com",
        "Your scheduled payment",
        "Your payment of £33.33 is due 5 October 2026. It will be collected automatically from your card.",
    );
    assert_eq!(klarna.detect(), None, "a scheduled payment is an update");
    let receipt = Mail::new(
        "billing@notion.so",
        "Your Notion invoice",
        "Invoice #123. Amount paid $10.00 on 1 October 2026. Thank you for your payment.",
    );
    assert_eq!(receipt.detect(), None, "a paid invoice is a record");
    let statement = Mail::new(
        "statements@amex.co.uk",
        "Your statement is ready",
        "Your October statement is available to view online.",
    );
    assert_eq!(statement.detect(), None);
    let dd_statement = Mail::new(
        "statements@bank.co.uk",
        "Your credit card statement",
        "Minimum payment £25.00 due by 20 October 2026. We'll collect it by Direct Debit.",
    );
    assert_eq!(dd_statement.detect(), None);
}

#[test]
fn renewal_needs_something_to_compare_and_a_date() {
    let detection = Mail::new(
        "renewals@admiral.com",
        "Your renewal is due",
        "Your car insurance is due for renewal. Your policy expires on 26 October 2026. Renewal premium: £412.00.",
    )
    .named("Admiral")
    .detect()
    .expect("a renewal");
    assert_eq!(detection.kind, TodoKind::Renewal);
    assert_eq!(detection.title, "Renew car insurance");
    assert_eq!(due_day(&detection), Some(day(10, 26)));
    assert_eq!(detection.amount.as_ref().map(|a| a.minor), Some(41200));
    let subscription = Mail::new(
        "billing@example.com",
        "Your plan renews on 12 October",
        "Your subscription renews on 12 October 2026.",
    );
    assert_eq!(
        subscription.detect(),
        None,
        "a subscription renewing is an update"
    );
    let promo = Mail::new(
        "offers@insurer.com",
        "Renew now and save 20%",
        "Your home insurance expires on 1 November 2026. Renew now, 20% off.",
    );
    assert_eq!(promo.detect(), None);
    let passport = Mail::new(
        "noreply@hmpo.gov.uk",
        "Your passport is due to expire",
        "Your passport expires on 3 March 2027. Renew online.",
    )
    .detect()
    .expect("a passport renewal");
    assert_eq!(passport.kind, TodoKind::Document);
    assert_eq!(passport.doc_type.as_deref(), Some("passport"));
}

#[test]
fn verify_needs_its_link_and_codes_are_not_to_dos() {
    let detection = Mail::new(
        "hello@octopus.energy",
        "Verify your new sign-in email",
        "Please verify your email address. This link expires in 24 hours: https://octopus.energy/verify/abc123",
    )
    .named("Octopus Energy")
    .detect()
    .expect("a verify to-do");
    assert_eq!(detection.kind, TodoKind::Verify);
    assert_eq!(
        detection.title,
        "Verify your email address for Octopus Energy"
    );
    assert_eq!(
        detection.due.as_ref().map(|due| due.at),
        Some(sent() + chrono::Duration::hours(24))
    );
    assert!(Mail::new(
        "hello@example.com",
        "Verify your email",
        "Please verify your email address."
    )
    .detect()
    .is_none());
    assert!(Mail::new(
        "no-reply@bank.com",
        "Your verification code",
        "Your verification code is 123456. Confirm your account at https://bank.com/x"
    )
    .detect()
    .is_none());
}

#[test]
fn sign_requests_name_the_document_and_completions_do_not_count() {
    let detection = Mail::new(
        "dse@docusign.net",
        "Please DocuSign: Engagement letter",
        "Priya Shah sent you a document to review and sign. Please sign by 9 October 2026.",
    )
    .named("Priya Shah via DocuSign")
    .detect()
    .expect("a sign request");
    assert_eq!(detection.kind, TodoKind::Sign);
    assert_eq!(detection.title, "Sign Engagement letter");
    assert_eq!(detection.counterparty.as_deref(), Some("Priya Shah"));
    assert_eq!(due_day(&detection), Some(day(10, 9)));
    let done = Mail::new(
        "dse@docusign.net",
        "Completed: Engagement letter",
        "All parties have signed. Review and sign history attached.",
    );
    assert_eq!(done.detect(), None);
}

#[test]
fn rsvp_reads_the_reply_by_date() {
    let detection = Mail::new(
        "sam@example.com",
        "Sam's leaving drinks",
        "Drinks on Thursday 15 October 2026 at the Lamb. Please RSVP by 12 October 2026.",
    )
    .detect()
    .expect("an rsvp");
    assert_eq!(detection.kind, TodoKind::Rsvp);
    assert_eq!(detection.title, "RSVP to Sam's leaving drinks");
    assert_eq!(due_day(&detection), Some(day(10, 12)));
    assert!(
        Mail::new("sam@example.com", "Party", "Please RSVP, it'll be fun.")
            .detect()
            .is_none()
    );
}

#[test]
fn mailing_list_mail_is_never_a_renewal_sign_or_verify() {
    let list = Mail::new(
        "news@insurer.com",
        "Your car insurance is due for renewal",
        "Your car insurance expires on 26 October 2026.",
    )
    .list();
    assert_eq!(list.detect(), None);
}

#[test]
fn ambiguous_numeric_due_date_is_unchecked() {
    let detection = Mail::new(
        "accounts@builder.co.uk",
        "Invoice 2291",
        "Invoice #2291. Amount due £300.00. Payment due 09/10/2026.",
    )
    .detect()
    .expect("a bill");
    assert_eq!(
        due_day(&detection),
        Some(day(10, 9)),
        "UK sender reads day first"
    );
    let provenance = detection.fields.get("due_at").expect("due provenance");
    assert!(!provenance.checked);
    assert_eq!(
        provenance.evidence.as_deref(),
        Some("Payment due 09/10/2026")
    );
}

#[test]
fn invite_rows_carry_the_invite_as_their_source() {
    let detection = from_invite(Some("Team offsite"), Some("Maya"), "maya@work.com", None);
    assert_eq!(detection.title, "RSVP to Team offsite");
    assert_eq!(detection.origin, Origin::Ics);
}

#[test]
fn counterparty_drops_roles_and_falls_back_to_the_domain() {
    assert_eq!(
        counterparty(Some("Camden Council"), Some("camden.gov.uk")).as_deref(),
        Some("Camden Council")
    );
    assert_eq!(
        counterparty(Some("no-reply"), Some("spotify.com")).as_deref(),
        Some("Spotify")
    );
    assert_eq!(
        counterparty(None, Some("klarna.com")).as_deref(),
        Some("Klarna")
    );
    assert_eq!(
        counterparty(Some("Spotify Billing"), Some("spotify.com")).as_deref(),
        Some("Spotify")
    );
}
