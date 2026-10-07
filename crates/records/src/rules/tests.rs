use super::*;
use chrono::Utc;

fn sent() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 3, 3, 9, 30, 0)
        .single()
        .expect("time")
}

fn read_one(subject: &str, body: &str) -> Option<RuleRead> {
    read(
        &RuleInput {
            subject,
            body_text: body,
            from_email: "orders@dell.co.uk",
            sent: sent(),
            list_mail: false,
        },
        &Utc,
    )
}

fn field(read: &RuleRead, name: FieldName) -> Option<&Found> {
    read.fields.iter().find(|found| found.field == name)
}

#[test]
fn an_order_confirmation_reads_its_number_total_and_date_unchecked() {
    let read = read_one(
        "Your Dell order confirmation",
        "Thanks for shopping with Dell.\nOrder number: 402-118\nOrder date: 3 March 2025\nSubtotal £1,040.83\nVAT £208.17\nOrder total: £1,249.00\nDelivery £0.00",
    )
    .expect("an order");
    assert_eq!(read.kind, RecordKind::Order);
    assert_eq!(read.stage, Stage::Confirmation);
    assert_eq!(read.reference.as_deref(), Some("402-118"));
    let amount = field(&read, FieldName::Amount).expect("amount");
    assert_eq!(
        amount.value,
        crate::fields::FieldValue::Money {
            minor: 124_900,
            currency: "GBP".into()
        }
    );
    assert!(!amount.checked, "a rule's money is unchecked");
    assert_eq!(amount.evidence.as_deref(), Some("£1,249.00"));
    let issued = field(&read, FieldName::IssuedAt).expect("issued");
    assert!(!issued.checked);
    assert_eq!(issued.evidence.as_deref(), Some("Order date: 3 March 2025"));
}

#[test]
fn shipping_and_delivery_emails_are_stages_of_the_order() {
    let shipped = read_one(
        "Your order 402-118 has shipped",
        "Order #402-118 is on its way.",
    )
    .expect("shipped");
    assert_eq!(
        (shipped.kind, shipped.stage),
        (RecordKind::Order, Stage::Shipped)
    );
    assert_eq!(shipped.reference.as_deref(), Some("402-118"));
    let delivered = read_one(
        "Delivered: XPS 14 laptop",
        "Order 402-118 was delivered today.",
    )
    .expect("delivered");
    assert_eq!(delivered.stage, Stage::Delivered);
    // A shipping note with nothing to file is an update, not a record.
    assert!(read_one("Your parcel is on its way", "Track it in the app.").is_none());
}

#[test]
fn receipts_statements_and_invoices_need_a_reference_or_an_amount() {
    let receipt = read_one(
        "Your receipt from Apple.",
        "Receipt\nDocument No. MSXK21\niCloud+ 200GB  £2.99\nTotal £2.99",
    )
    .expect("receipt");
    assert_eq!(receipt.kind, RecordKind::Receipt);
    assert_eq!(receipt.reference.as_deref(), Some("MSXK21"));
    let statement = read_one(
        "Your bill is ready",
        "Account number: A-99312\nAmount due: £128.40",
    )
    .expect("statement");
    assert_eq!(statement.kind, RecordKind::Statement);
    assert_eq!(statement.reference.as_deref(), Some("A-99312"));
    let invoice = read_one("Invoice INV-2025-0042", "Total due: $450.00").expect("invoice");
    assert_eq!(invoice.kind, RecordKind::Invoice);
    assert!(read_one("Your statement", "Log in to see it.").is_none());
}

#[test]
fn bookings_tickets_and_warranties_read_their_dates() {
    let booking = read_one(
        "Booking confirmation - Lisbon",
        "Booking reference: K7QX2M\nCheck-in: 12 June 2025\nCheck-out: 15 June 2025",
    )
    .expect("booking");
    assert_eq!(booking.kind, RecordKind::Booking);
    assert_eq!(booking.reference.as_deref(), Some("K7QX2M"));
    assert!(field(&booking, FieldName::SpanStart).is_some());
    assert!(field(&booking, FieldName::SpanEnd).is_some());
    let ticket = read_one(
        "Your tickets for Hamlet",
        "Ticket ref: TK-88213\nTotal £64.00",
    )
    .expect("ticket");
    assert_eq!(ticket.kind, RecordKind::Ticket);
    let warranty = read_one(
        "Your order confirmation",
        "Order number 77-12. Covered by a 2-year limited warranty. Return by 2 April 2025.",
    )
    .expect("order");
    let until = field(&warranty, FieldName::WarrantyUntil).expect("warranty");
    assert!(!until.checked);
    assert_eq!(until.evidence.as_deref(), Some("2-year limited warranty"));
    assert_eq!(
        field(&warranty, FieldName::ReturnBy).map(|f| f.value.clone()),
        Some(crate::fields::FieldValue::At(day_at(
            chrono::NaiveDate::from_ymd_opt(2025, 4, 2).expect("day")
        )))
    );
}

#[test]
fn marketing_and_list_mail_without_both_never_file() {
    assert!(read_one("20% off your next order", "Order number 1234 Total £10").is_none());
    assert!(read_one("Complete your order", "Order number 1234 Total £10").is_none());
    let list = read(
        &RuleInput {
            subject: "Your tickets for the AGM",
            body_text: "Ticket ref TK-1234",
            from_email: "news@club.org",
            sent: sent(),
            list_mail: true,
        },
        &Utc,
    );
    assert!(list.is_none(), "a list must give a reference and an amount");
}

#[test]
fn the_total_beats_subtotals_and_unlabelled_amounts_pick_none() {
    let total = find_total("Subtotal £10.00\nShipping £2.00\nGrand total £12.00").expect("total");
    assert_eq!(total.minor, 1200);
    assert!(find_total("Item one £5.00, item two £7.00").is_none());
}

#[test]
fn issuers_come_from_the_display_name_or_the_domain() {
    assert_eq!(issuer_from_sender(Some("Dell"), "orders@dell.com"), "Dell");
    assert_eq!(
        issuer_from_sender(Some("no-reply"), "no-reply@octopus.energy"),
        "Octopus"
    );
    assert_eq!(issuer_from_sender(None, "billing@mail.bt.co.uk"), "Bt");
    assert_eq!(
        issuer_from_sender(Some("\"Airbnb\" via Mailchimp"), "x@mc.us"),
        "Airbnb"
    );
}

#[test]
fn titles_drop_the_boilerplate() {
    assert_eq!(
        title_from_subject("Your Dell order #402-118 has shipped").as_deref(),
        Some("Dell order")
    );
    assert_eq!(
        title_from_subject("Re: Receipt").as_deref(),
        Some("Receipt")
    );
    assert_eq!(title_from_subject("#12345"), None);
}

#[test]
fn past_days_read_with_or_without_a_year() {
    let anchor = sent().date_naive();
    let day = |y, m, d| chrono::NaiveDate::from_ymd_opt(y, m, d).expect("day");
    assert_eq!(past_day("3 March 2025", anchor), Some(day(2025, 3, 3)));
    assert_eq!(past_day("Mar 1st, 2025", anchor), Some(day(2025, 3, 1)));
    assert_eq!(
        past_day("Friday 28 February", anchor),
        Some(day(2025, 2, 28))
    );
    // A yearless day after the email is last year's.
    assert_eq!(past_day("12 December", anchor), Some(day(2024, 12, 12)));
    assert_eq!(past_day("2025-02-01", anchor), Some(day(2025, 2, 1)));
    assert_eq!(past_day("Friday", anchor), None);
}
