//! Mail that gives the demo's Archive records to file, group and answer
//! about.
//!
//! * Dell: an order confirmation with schema.org `Order` markup and its
//!   invoice PDF, then a shipping email and a carrier's delivery email that
//!   name the same order number: one record, three emails.
//! * Lisbon: a TAP flight, a hotel and an Oceanario ticket, each with
//!   reservation markup, starting three days from now: one trip, coming up.
//! * Octopus Energy: three monthly bills with the account number and amount
//!   in the text and a PDF each: a series, filed by rule with unchecked
//!   amounts.
//! * Apple: an iCloud+ receipt by rule.
//! * Airbnb Porto: last year's booking by rule, so the ledger has a year to
//!   step back to.
//! * Bose: a headphone order whose two-year warranty runs out in three
//!   weeks, so it comes up.

use super::{build_demo_msg, DemoMessage};
use chrono::{DateTime, Duration, Months, Utc};
use mxr_core::id::{AccountId, AttachmentId, ThreadId};
use mxr_core::types::{
    Address, AttachmentDisposition, AttachmentMeta, Envelope, MessageBody, MessageFlags,
    UnsubscribeMethod,
};

/// Messages `records_demo_messages` returns.
pub(super) const RECORDS_DEMO_MESSAGE_COUNT: usize = 12;

fn thread(account_id: &AccountId, name: &str) -> ThreadId {
    ThreadId::from_scoped_provider_id(account_id, "fake", &format!("demo-records-{name}"))
}

fn address(name: &str, email: &str) -> Address {
    Address {
        name: Some(name.to_string()),
        email: email.to_string(),
    }
}

fn message(from: Address, to: &Address, subject: &str, body: String, date: DateTime<Utc>) -> DemoMessage {
    DemoMessage {
        from,
        to: vec![to.clone()],
        cc: Vec::new(),
        snippet: body.chars().take(120).collect(),
        subject: subject.to_string(),
        body_text: body,
        date,
        flags: MessageFlags::READ,
        has_attachments: false,
        category: 9,
        unsubscribe: UnsubscribeMethod::None,
        label_provider_ids: vec!["INBOX".to_string()],
        in_reply_to: None,
        references: Vec::new(),
    }
}

fn ld(json: &str, text: &str) -> String {
    format!(
        r#"<!doctype html><html><head><script type="application/ld+json">{json}</script></head><body><p>{}</p></body></html>"#,
        text.replace('\n', "<br>")
    )
}

/// Numbered from `first_num` so provider ids follow the seeded messages
/// before them.
pub(super) fn records_demo_messages(
    account_id: &AccountId,
    self_addr: &Address,
    now: DateTime<Utc>,
    first_num: usize,
) -> Vec<(Envelope, MessageBody)> {
    let mut built = Vec::with_capacity(RECORDS_DEMO_MESSAGE_COUNT);
    let mut push = |message: DemoMessage, thread_id: ThreadId, html: Option<String>, pdf: Option<(&str, u64)>| {
        let num = first_num + built.len();
        let (mut envelope, mut body) = build_demo_msg(num, account_id, &thread_id, message);
        if let Some(html) = html {
            body.text_html = Some(html);
        }
        if let Some((filename, size_bytes)) = pdf {
            let provider_id = format!("demo-record-att-{num}");
            body.attachments.push(AttachmentMeta {
                id: AttachmentId::from_scoped_provider_id(account_id, "fake", &provider_id),
                message_id: envelope.id.clone(),
                filename: filename.to_string(),
                mime_type: "application/pdf".to_string(),
                disposition: AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes,
                local_path: None,
                provider_id,
            });
            envelope.has_attachments = true;
        }
        built.push((envelope, body));
    };

    // Dell: one order, three emails.
    let ordered = now - Duration::days(40);
    let dell = address("Dell", "orders@dell.co.uk");
    let order_text = "Thanks for your order.\nOrder number: 402-118\nXPS 14 laptop\nOrder total: £1,249.00\nYour invoice is attached.";
    push(
        message(dell.clone(), self_addr, "Your Dell order confirmation", order_text.to_string(), ordered),
        thread(account_id, "dell-order"),
        Some(ld(
            &format!(
                r#"{{"@context":"https://schema.org","@type":"Order","merchant":{{"@type":"Organization","name":"Dell"}},"orderNumber":"402-118","orderStatus":"https://schema.org/OrderProcessing","orderDate":"{}","price":"1249.00","priceCurrency":"GBP","acceptedOffer":{{"@type":"Offer","itemOffered":{{"@type":"Product","name":"XPS 14 laptop"}},"price":"1249.00","priceCurrency":"GBP"}}}}"#,
                ordered.format("%Y-%m-%d")
            ),
            order_text,
        )),
        Some(("Invoice-402118.pdf", 186_000)),
    );
    push(
        message(
            dell,
            self_addr,
            "Your order 402-118 has shipped",
            "Good news: order #402-118 is on its way with DPD.".to_string(),
            ordered + Duration::days(2),
        ),
        thread(account_id, "dell-order"),
        None,
        None,
    );
    push(
        message(
            address("DPD", "notifications@dpd.co.uk"),
            self_addr,
            "Delivered: your Dell parcel",
            format!(
                "Order 402-118 was delivered today and left with your neighbour.\nReturn by {} if it isn't right. Covered by a 2-year limited warranty.",
                (ordered + Duration::days(34)).format("%-d %B %Y")
            ),
            ordered + Duration::days(4),
        ),
        thread(account_id, "dell-dpd"),
        None,
        None,
    );

    // Lisbon: a trip of three bookings, three days away.
    let flight_out = now + Duration::days(3);
    let booked = now - Duration::days(60);
    let lisbon = r#"{"@type":"PostalAddress","addressLocality":"Lisbon","addressCountry":"PT"}"#;
    push(
        message(
            address("TAP Air Portugal", "booking@flytap.com"),
            self_addr,
            "Your booking is confirmed",
            "Booking reference: K7QX2M\nLondon Heathrow to Lisbon, TP1357.".to_string(),
            booked,
        ),
        thread(account_id, "tap"),
        Some(ld(
            &format!(
                r#"{{"@context":"https://schema.org","@type":"FlightReservation","reservationNumber":"K7QX2M","reservationStatus":"https://schema.org/ReservationConfirmed","totalPrice":"212.40","priceCurrency":"GBP","reservationFor":{{"@type":"Flight","flightNumber":"1357","airline":{{"@type":"Airline","name":"TAP Air Portugal","iataCode":"TP"}},"departureAirport":{{"@type":"Airport","name":"London Heathrow","iataCode":"LHR"}},"departureTime":"{}","arrivalAirport":{{"@type":"Airport","name":"Lisbon","iataCode":"LIS","address":{lisbon}}},"arrivalTime":"{}"}}}}"#,
                flight_out.to_rfc3339(),
                (flight_out + Duration::hours(3)).to_rfc3339()
            ),
            "Booking reference: K7QX2M",
        )),
        Some(("e-ticket-K7QX2M.pdf", 92_000)),
    );
    push(
        message(
            address("Hotel Lisboa Plaza", "reservations@lisboaplaza.pt"),
            self_addr,
            "Your reservation at Hotel Lisboa Plaza",
            "Reservation number: 88213\nThree nights in Lisbon.".to_string(),
            booked + Duration::days(1),
        ),
        thread(account_id, "hotel"),
        Some(ld(
            &format!(
                r#"{{"@context":"https://schema.org","@type":"LodgingReservation","reservationNumber":"88213","reservationStatus":"https://schema.org/ReservationConfirmed","totalPrice":"612.00","priceCurrency":"EUR","reservationFor":{{"@type":"LodgingBusiness","name":"Hotel Lisboa Plaza","address":{lisbon}}},"checkinTime":"{}","checkoutTime":"{}"}}"#,
                (flight_out + Duration::hours(6)).to_rfc3339(),
                (flight_out + Duration::days(3)).to_rfc3339()
            ),
            "Reservation number: 88213",
        )),
        None,
    );
    push(
        message(
            address("Oceanario de Lisboa", "tickets@oceanario.pt"),
            self_addr,
            "Your tickets for the Oceanario",
            "Ticket ref: OC-55120\n2 adults.".to_string(),
            booked + Duration::days(2),
        ),
        thread(account_id, "oceanario"),
        Some(ld(
            &format!(
                r#"{{"@context":"https://schema.org","@type":"EventReservation","reservationNumber":"OC-55120","reservationStatus":"https://schema.org/ReservationConfirmed","totalPrice":"44.00","priceCurrency":"EUR","reservationFor":{{"@type":"Event","name":"Lisbon Oceanario","startDate":"{}","location":{{"@type":"Place","name":"Oceanario de Lisboa","address":{lisbon}}}}}}}"#,
                (flight_out + Duration::days(1)).format("%Y-%m-%d")
            ),
            "Ticket ref: OC-55120",
        )),
        None,
    );

    // Octopus Energy: three months of bills, a series.
    for (months_back, amount) in [(3u32, "118.20"), (2, "121.75"), (1, "128.40")] {
        let date = now
            .checked_sub_months(Months::new(months_back))
            .unwrap_or(now)
            + Duration::days(2);
        push(
            message(
                address("Octopus Energy", "hello@octopus.energy"),
                self_addr,
                "Your bill is ready",
                format!(
                    "Hi Alex,\n\nYour latest bill is attached.\nAccount number: A-99312\nAmount due: £{amount}\nWe'll collect it by direct debit, so there's nothing to do."
                ),
                date,
            ),
            thread(account_id, &format!("octopus-{months_back}")),
            None,
            Some(("octopus-bill.pdf", 54_000)),
        );
    }

    push(
        message(
            address("Apple", "no_reply@email.apple.com"),
            self_addr,
            "Your receipt from Apple.",
            "Receipt\nDocument No. MSXK21\niCloud+ with 200 GB storage £2.99\nTotal £2.99".to_string(),
            now - Duration::days(12),
        ),
        thread(account_id, "apple"),
        None,
        None,
    );

    push(
        message(
            address("Airbnb", "automated@airbnb.com"),
            self_addr,
            "Booking confirmation - Porto",
            "Booking reference: HMX8Q2\nCheck-in: 14 May\nCheck-out: 16 May\nTotal £412.00".to_string(),
            now - Duration::days(400),
        ),
        thread(account_id, "airbnb"),
        None,
        None,
    );

    let bose_ordered = now - Duration::days(710);
    push(
        message(
            address("Bose", "orders@bose.co.uk"),
            self_addr,
            "Your Bose order confirmation",
            format!(
                "Order number: BO-77120\nQuietComfort headphones\nOrder total: £299.95\nWarranty valid until {}.",
                (now + Duration::days(20)).format("%-d %B %Y")
            ),
            bose_ordered,
        ),
        thread(account_id, "bose"),
        None,
        None,
    );

    built
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_count_matches_the_mail_built_and_pdfs_are_attached() {
        let account_id = AccountId::from_provider_id("fake", "alex@demo.mxr.local");
        let me = address("Alex Demo", "alex@demo.mxr.local");
        let built = records_demo_messages(&account_id, &me, Utc::now(), 1);
        assert_eq!(built.len(), RECORDS_DEMO_MESSAGE_COUNT);
        let pdfs = built
            .iter()
            .flat_map(|(_, body)| &body.attachments)
            .filter(|a| a.mime_type == "application/pdf")
            .count();
        assert_eq!(pdfs, 5);
    }
}
