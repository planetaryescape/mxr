//! Families of mail about money: an order with its dispatch and delivery, and
//! a bill. Their messages are Updates and are kept as records; a bill with an
//! open deadline also makes a to-do.
//!
//! Visible phrasings only. The held-out ones live in `scenarios/held_out.rs`.

use super::pools::{BILLER_SENDERS, STORE_SENDERS};
use super::{
    expect, AskKind, Attachments, Dating, Family, Senders, Subjects, Turn, Unsubscribe, Variant,
    FILED, NOTICE,
};
use super::{Home, Who};

const NO_SENT: &[&str] = &[];

fn receipts(_: usize) -> Option<&'static str> {
    Some("receipts")
}

fn receipt_look(_: usize) -> usize {
    5
}

pub(super) static ORDER_RECEIPT: Family = Family {
    id: "order-receipt",
    senders: Senders::Pool(&STORE_SENDERS),
    subjects: Subjects::PerTurn,
    observer_cc: false,
    reply_headers: false,
    dating: Dating::Spread,
    due_days: 0,
    ref_prefix: "ORD",
    style: receipt_look,
    attachments: Attachments::FirstTurn,
    brief_every_41: false,
    unsubscribe: Unsubscribe::None,
    provider_label: receipts,
    waiting: false,
    spam_folder: false,
    archived_every: None,
    turns: &[
        Turn::them(
            "bulk-order-confirmation",
            "A store's order confirmation with the items and the total paid",
            FILED,
        ),
        Turn::them(
            "bulk-order-shipped",
            "The same order shipping, naming the order number: joins the record",
            FILED,
        ),
        Turn::them(
            "bulk-order-delivered",
            "The same order delivered, naming the order number: joins the record",
            FILED,
        ),
        Turn::them(
            "bulk-order-survey",
            "A request to rate the purchase: a notification, nothing to keep",
            NOTICE,
        ),
        Turn::them(
            "bulk-order-invoice",
            "The invoice for the order, with its number, date and amount",
            FILED,
        ),
    ],
    sent_replies: NO_SENT,
    variants: &[
        Variant {
            id: "plain-store",
            subjects: &[
                "Order {ref} confirmed",
                "Your order {ref} has shipped",
                "Order {ref} delivered",
                "How was your order {ref}?",
                "Invoice for order {ref}",
            ],
            bodies: &[
                "Thanks for your order, Alex.\n\nOrder number: {ref}\nOrdered on: {date}\n\n1 x Standing desk mat\n1 x USB-C dock\n\nTotal paid: {amount}\n\nYou can view your order any time in your account.\n\n{org}",
                "Good news: order {ref} is on its way and should reach you in two to three working days. We'll email you again when it arrives.\n\n{org}",
                "Your order {ref} was delivered today. We hope you enjoy it.\n\n{org}",
                "You recently bought from us. Tell us how it went; it takes one minute.\n\n{org}",
                "Your invoice for order {ref} is ready.\n\nInvoice total: {amount}\nPaid: {date}\n\n{org}",
            ],
        },
        Variant {
            id: "receipt-style",
            subjects: &[
                "Your receipt from {org}",
                "Dispatch notice for {ref}",
                "Delivery confirmation for {ref}",
                "Rate your recent purchase",
                "Tax invoice {ref}",
            ],
            bodies: &[
                "Receipt for your purchase\n\nReference: {ref}\nDate: {date}\nAmount: {amount}\nPaid with: card ending 4417\n\nThank you for shopping with {org}.",
                "We've dispatched your purchase (reference {ref}). Expect it within three working days.\n\n{org}",
                "Your purchase {ref} has been delivered. If anything is wrong with it, our returns page explains what to do.\n\n{org}",
                "Thanks for shopping with {org}. We'd love to hear how your purchase went.\n\n{org}",
                "Tax invoice {ref}\nDate of issue: {date}\nTotal: {amount}\n\nThis invoice is for your records.\n\n{org}",
            ],
        },
        Variant {
            id: "warm",
            subjects: &[
                "We've got your order! ({ref})",
                "Your parcel is on the way ({ref})",
                "It's arrived! Order {ref}",
                "Got a minute? Tell us about order {ref}",
                "Your invoice is here ({ref})",
            ],
            bodies: &[
                "Hi Alex,\n\nThank you so much for your order. We're packing it now. Here's what you ordered:\n\n- Desk lamp\n- Cable organiser\n\nOrder {ref}, total {amount}. We'll let you know as soon as it ships.\n\nThe team at {org}",
                "Hi Alex,\n\nYour parcel has left our warehouse. Order {ref} should be with you in a few days.\n\nThe team at {org}",
                "Hi Alex,\n\nYour order {ref} has arrived. We hope it makes your desk a bit nicer.\n\nThe team at {org}",
                "Hi Alex, if you have a minute we'd really like to know what you thought of order {ref}.\n\nThe team at {org}",
                "Hi Alex,\n\nAttached is the invoice for order {ref} ({amount}), in case you need it for expenses.\n\nThe team at {org}",
            ],
        },
        Variant {
            id: "terse-machine",
            subjects: &[
                "Order confirmation {ref}",
                "Shipment notification {ref}",
                "Delivery notification {ref}",
                "Customer survey {ref}",
                "Invoice {ref}",
            ],
            bodies: &[
                "ORDER CONFIRMATION\nOrder: {ref}\nDate: {date}\nItems: 3\nTotal: {amount}\nStatus: processing",
                "SHIPMENT NOTIFICATION\nOrder: {ref}\nStatus: shipped\nEstimated delivery: 3 working days",
                "DELIVERY NOTIFICATION\nOrder: {ref}\nStatus: delivered",
                "CUSTOMER SURVEY\nOrder: {ref}\nPlease rate your experience from 1 to 5.",
                "INVOICE\nOrder: {ref}\nDate: {date}\nAmount: {amount}\nStatus: paid",
            ],
        },
    ],
};

pub(super) static BILL_DUE: Family = Family {
    id: "bill-due",
    senders: Senders::Pool(&BILLER_SENDERS),
    subjects: Subjects::PerTurn,
    observer_cc: false,
    reply_headers: false,
    dating: Dating::Asks,
    due_days: 9,
    ref_prefix: "INV",
    style: receipt_look,
    attachments: Attachments::FirstTurn,
    brief_every_41: false,
    unsubscribe: Unsubscribe::None,
    provider_label: receipts,
    waiting: false,
    spam_folder: false,
    archived_every: None,
    turns: &[
        Turn::them(
            "bulk-bill-issued",
            "A bill with an amount, a due date and a pay link: a to-do and a record",
            expect(Who::Machine, Home::Updates, true),
        )
        .asking(AskKind::Pay, true),
        Turn::them(
            "bulk-bill-payment-received",
            "The payment received: a receipt for the bill",
            FILED,
        ),
        Turn::them(
            "bulk-bill-statement-ready",
            "A statement is ready to view in the account: an update",
            NOTICE,
        )
        .reviewed(
            &["record"],
            "No document is attached, only a notice that a statement exists; labelled no record. Same call as todo-statement-ready.",
        ),
        Turn::them(
            "bulk-bill-price-change",
            "A notice that prices change next month, with nothing to do",
            NOTICE,
        ),
        Turn::them(
            "bulk-bill-survey",
            "A request to rate the service: a notification, nothing to keep",
            NOTICE,
        ),
    ],
    sent_replies: NO_SENT,
    variants: &[
        Variant {
            id: "utility",
            subjects: &[
                "Your {org} bill is ready",
                "Payment received, thank you",
                "Your statement is ready",
                "A change to your tariff",
                "Tell us how we're doing",
            ],
            bodies: &[
                "Hello Alex,\n\nYour bill for this period is {amount}. Payment is due by {due}.\n\nPay now: https://billing.demo.mxr.local/pay/{ref}\n\n{org}",
                "Thanks, we've received your payment of {amount} on {date}. Your account is up to date.\n\n{org}",
                "Your latest statement is now available to view in your online account.\n\n{org}",
                "From next month your rates will change. There is nothing you need to do; the new rates will be on your next statement.\n\n{org}",
                "How did we do? Rate our service in one click.\n\n{org}",
            ],
        },
        Variant {
            id: "invoice",
            subjects: &[
                "Invoice {ref} for {amount}",
                "Receipt for invoice {ref}",
                "Statement available for your account",
                "We're updating our prices",
                "Quick survey about your account",
            ],
            bodies: &[
                "Hi Alex,\n\nInvoice {ref} for {amount} was issued on {date}. Please pay by {due} to avoid interruption to your service.\n\nPay invoice: https://billing.demo.mxr.local/invoices/{ref}\n\n{org} accounts",
                "We've received {amount} against invoice {ref}. Thank you.\n\n{org} accounts",
                "Your account statement is ready in the billing portal.\n\n{org} accounts",
                "Our prices are changing next month. Your plan stays the same and no action is needed on your part.\n\n{org} accounts",
                "Help us improve: answer three short questions about your account.\n\n{org} accounts",
            ],
        },
        Variant {
            id: "reminder-voice",
            subjects: &[
                "Bill due {due}: {amount}",
                "Thanks, your payment is in",
                "Your monthly statement",
                "Price changes from next month",
                "Rate our support",
            ],
            bodies: &[
                "A friendly note that your bill of {amount} is due on {due}. You can pay online in a few clicks: https://billing.demo.mxr.local/pay/{ref}\n\n{org}",
                "Your payment of {amount} has arrived. Thank you for paying on time.\n\n{org}",
                "Your statement for this month is ready to view whenever you are.\n\n{org}",
                "We're changing some prices from next month. Nothing to do on your side.\n\n{org}",
                "How was your last chat with our support team? Rate it here.\n\n{org}",
            ],
        },
        Variant {
            id: "formal-notice",
            subjects: &[
                "Account {ref}: amount due",
                "Acknowledgement of payment {ref}",
                "Account statement {ref}",
                "Notice of revised charges",
                "Customer experience survey",
            ],
            bodies: &[
                "Dear customer,\n\nThe amount of {amount} is due on your account {ref}. We must receive payment by {due}. Pay online: https://billing.demo.mxr.local/pay/{ref}\n\nYours faithfully,\n{org}",
                "We acknowledge receipt of your payment of {amount} against account {ref} on {date}.\n\nYours faithfully,\n{org}",
                "Your statement for account {ref} is now available in your online account.\n\nYours faithfully,\n{org}",
                "We write to inform you that our charges will be revised from next month. No action is required.\n\nYours faithfully,\n{org}",
                "We would value your feedback on your recent experience with us.\n\nYours faithfully,\n{org}",
            ],
        },
    ],
};
