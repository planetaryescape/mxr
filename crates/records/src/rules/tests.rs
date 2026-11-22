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
    assert_eq!(statement.reference, None);
    assert_eq!(statement.account_ref.as_deref(), Some("A-99312"));
    assert!(field(&statement, FieldName::Reference).is_some());
    let invoice = read_one("Invoice INV-2025-0042", "Total due: $450.00").expect("invoice");
    assert_eq!(invoice.kind, RecordKind::Invoice);
    assert!(read_one("Your statement", "Log in to see it.").is_none());
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
fn people_writing_about_contracts_and_failed_payments_are_not_records() {
    assert!(read_one(
        "Contract renewal details",
        "Reference REF-2231. Total £40,000.00"
    )
    .is_none());
    assert!(read_one(
        "We can't process your payment",
        "Amount due £11.99. Ref 77123"
    )
    .is_none());
    let signed = read_one(
        "Completed: Tenancy renewal",
        "Envelope ID: 4471-AB. Reference 99812",
    )
    .expect("signed");
    assert_eq!(signed.kind, RecordKind::Contract);
}

#[test]
fn the_total_beats_subtotals_and_unlabelled_amounts_pick_none() {
    let total = find_total("Subtotal £10.00\nShipping £2.00\nGrand total £12.00").expect("total");
    assert_eq!(total.minor, 1200);
    assert!(find_total("Item one £5.00, item two £7.00").is_none());
}
