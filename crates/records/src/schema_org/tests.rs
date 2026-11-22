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
