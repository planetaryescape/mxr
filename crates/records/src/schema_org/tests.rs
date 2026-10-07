use super::*;

fn wrap(json: &str) -> String {
    format!(
        r#"<html><head><script type="application/ld+json">{json}</script></head><body>x</body></html>"#
    )
}

fn one(json: &str) -> SchemaRecord {
    let mut records = read(Some(&wrap(json)));
    assert_eq!(records.len(), 1, "{records:?}");
    records.remove(0)
}

#[test]
fn an_order_reads_number_merchant_price_date_and_items() {
    let record = one(r#"{"@context":"http://schema.org","@type":"Order",
            "merchant":{"@type":"Organization","name":"Dell"},
            "orderNumber":"402-118","orderStatus":"http://schema.org/OrderProcessing",
            "orderDate":"2025-03-03T10:12:00+00:00",
            "priceCurrency":"GBP","price":"1249.00",
            "acceptedOffer":[{"@type":"Offer","itemOffered":{"@type":"Product","name":"XPS 14 laptop"},
                              "price":"1249.00","priceCurrency":"GBP"}]}"#);
    assert_eq!(record.kind, RecordKind::Order);
    assert_eq!(record.stage, Stage::Confirmation);
    assert_eq!(record.reference.as_deref(), Some("402-118"));
    assert_eq!(record.issuer.as_deref(), Some("Dell"));
    assert_eq!(record.title.as_deref(), Some("XPS 14 laptop"));
    let amount = record.amount.expect("amount");
    assert_eq!((amount.minor, amount.currency.as_str()), (124_900, "GBP"));
    assert_eq!(amount.text, "1249.00");
    assert_eq!(
        record
            .issued
            .map(|issued| issued.at.date_naive().to_string()),
        Some("2025-03-03".to_string())
    );
}

#[test]
fn an_order_without_a_top_level_price_uses_its_offer_and_counts_items() {
    let record = one(
        r#"{"@type":"Order","seller":{"name":"Acme"},"orderNumber":7731,
            "acceptedOffer":[
              {"@type":"Offer","itemOffered":{"name":"Kettle"},"price":25.5,"priceCurrency":"usd"},
              {"@type":"Offer","itemOffered":{"name":"Mug"},"price":5,"priceCurrency":"usd"},
              {"@type":"Offer","itemOffered":{"name":"Tea"},"price":3,"priceCurrency":"usd"}]}"#,
    );
    assert_eq!(record.reference.as_deref(), Some("7731"));
    assert_eq!(record.title.as_deref(), Some("Kettle and 2 more"));
    assert_eq!(
        record.amount.map(|a| (a.minor, a.currency)),
        Some((2550, "USD".into()))
    );
}

#[test]
fn a_parcel_is_its_order_at_the_shipped_or_delivered_stage() {
    let record = one(
        r#"{"@type":"ParcelDelivery","deliveryStatus":"http://schema.org/OrderDelivered",
            "itemShipped":{"@type":"Product","name":"XPS 14 laptop"},
            "partOfOrder":{"@type":"Order","orderNumber":"402-118","merchant":{"name":"Dell"}}}"#,
    );
    assert_eq!(record.kind, RecordKind::Order);
    assert_eq!(record.stage, Stage::Delivered);
    assert_eq!(record.reference.as_deref(), Some("402-118"));
    assert_eq!(record.issuer.as_deref(), Some("Dell"));
    let shipped =
        one(r#"{"@type":"ParcelDelivery","partOfOrder":{"@type":"Order","orderNumber":"9"}}"#);
    assert_eq!(shipped.stage, Stage::Shipped);
}

#[test]
fn an_invoice_reads_its_price_account_and_paid_state() {
    let record = one(
        r#"{"@type":"Invoice","provider":{"name":"Octopus Energy"},"accountId":"A-99312",
            "description":"Bill, Feb","price":"128.40","priceCurrency":"GBP",
            "paymentStatus":"http://schema.org/PaymentAutomaticallyApplied"}"#,
    );
    assert_eq!(record.kind, RecordKind::Invoice);
    assert_eq!(record.stage, Stage::Receipt);
    assert_eq!(record.issuer.as_deref(), Some("Octopus Energy"));
    assert_eq!(record.account_ref.as_deref(), Some("A-99312"));
    assert_eq!(record.title.as_deref(), Some("Bill, Feb"));
    assert_eq!(record.amount.map(|a| a.minor), Some(12_840));
    let due = one(r#"{"@type":"Invoice","provider":{"name":"Camden Council"},
            "totalPaymentDue":{"@type":"PriceSpecification","price":142.00,"priceCurrency":"GBP"},
            "paymentStatus":"PaymentDue"}"#);
    assert_eq!(due.stage, Stage::Invoice);
    assert_eq!(due.amount.map(|a| a.minor), Some(14_200));
}

#[test]
fn a_flight_reads_the_route_times_airline_and_destination() {
    let record = one(
        r#"{"@context":"http://schema.org","@type":"FlightReservation","reservationNumber":"K7QX2M",
            "reservationStatus":"http://schema.org/ReservationConfirmed",
            "reservationFor":{"@type":"Flight","flightNumber":"1357",
              "airline":{"@type":"Airline","name":"TAP Air Portugal","iataCode":"TP"},
              "departureAirport":{"@type":"Airport","name":"London Heathrow","iataCode":"LHR"},
              "departureTime":"2025-06-12T07:40:00+01:00",
              "arrivalAirport":{"@type":"Airport","name":"Lisbon","iataCode":"LIS",
                 "address":{"@type":"PostalAddress","addressLocality":"Lisbon"}},
              "arrivalTime":"2025-06-12T10:20:00+01:00"}}"#,
    );
    assert_eq!(record.kind, RecordKind::Booking);
    assert_eq!(record.reference.as_deref(), Some("K7QX2M"));
    assert_eq!(record.issuer.as_deref(), Some("TAP Air Portugal"));
    assert_eq!(record.title.as_deref(), Some("LHR -> LIS TP1357"));
    assert_eq!(record.place.as_deref(), Some("Lisbon"));
    assert_eq!(
        record.span_start.map(|s| s.at.to_rfc3339()),
        Some("2025-06-12T06:40:00+00:00".to_string())
    );
    assert!(record.span_end.is_some());
}

#[test]
fn a_hotel_reads_its_stay_and_city() {
    let record = one(
        r#"{"@type":"LodgingReservation","reservationNumber":"88213",
            "reservationFor":{"@type":"LodgingBusiness","name":"Hotel Lisboa Plaza",
              "address":{"@type":"PostalAddress","addressLocality":"Lisbon","addressCountry":"PT"}},
            "checkinTime":"2025-06-12T15:00:00+01:00","checkoutTime":"2025-06-15T11:00:00+01:00",
            "totalPrice":"612.00","priceCurrency":"EUR"}"#,
    );
    assert_eq!(record.kind, RecordKind::Booking);
    assert_eq!(record.issuer.as_deref(), Some("Hotel Lisboa Plaza"));
    assert_eq!(record.title.as_deref(), Some("Lisbon, 3 nights"));
    assert_eq!(
        record.amount.map(|a| (a.minor, a.currency)),
        Some((61_200, "EUR".into()))
    );
}

#[test]
fn an_event_is_a_ticket_with_its_date_and_venue_city() {
    let record = one(r#"{"@type":"EventReservation","reservationNumber":"E-2231",
            "reservationFor":{"@type":"Event","name":"Lisbon Oceanario",
              "startDate":"2025-06-13",
              "location":{"@type":"Place","name":"Oceanario",
                 "address":{"addressLocality":"Lisbon"}}}}"#);
    assert_eq!(record.kind, RecordKind::Ticket);
    assert_eq!(record.title.as_deref(), Some("Lisbon Oceanario"));
    assert_eq!(record.issuer.as_deref(), Some("Oceanario"));
    assert_eq!(record.place.as_deref(), Some("Lisbon"));
    assert_eq!(
        record.span_start.map(|s| s.at.date_naive().to_string()),
        Some("2025-06-13".to_string())
    );
}

#[test]
fn a_pending_reservation_is_a_to_do_not_a_record_and_other_reservations_are_bookings() {
    assert!(read(Some(&wrap(
        r#"{"@type":"FoodEstablishmentReservation","reservationStatus":"ReservationPending",
            "reservationFor":{"name":"Dishoom"}}"#
    )))
    .is_empty());
    let table = one(
        r#"{"@type":"FoodEstablishmentReservation","reservationNumber":"T55",
            "reservationFor":{"@type":"FoodEstablishment","name":"Dishoom",
              "startDate":"2026-10-10T19:00:00+01:00","address":"7 Boundary St, London, E2 7JE"}}"#,
    );
    assert_eq!(table.kind, RecordKind::Booking);
    assert_eq!(table.title.as_deref(), Some("Dishoom"));
    assert_eq!(table.place.as_deref(), Some("London"));
}

#[test]
fn no_markup_and_other_types_are_no_records() {
    assert!(read(None).is_empty());
    assert!(read(Some("<p>hello</p>")).is_empty());
    assert!(read(Some(&wrap(r#"{"@type":"WebSite","name":"Acme"}"#))).is_empty());
}
