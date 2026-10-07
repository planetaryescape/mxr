//! schema.org markup that says an email is a record: an `Order` or
//! `ParcelDelivery` (an order and its stages), an `Invoice` with its price,
//! and the reservations: `FlightReservation`, `LodgingReservation`,
//! `EventReservation` and the rest of the `*Reservation` family.
//!
//! Markup is the sender's own statement of the transaction, so its money
//! and dates file as checked. Senders must register with Google to send it,
//! so it comes mainly from large merchants and airlines; absence is the
//! common case, not an error. A pending reservation is a to-do (confirm
//! it), not a record, so it is left to To do.

use crate::fields::day_at;
use crate::{RecordKind, Stage};
use chrono::{DateTime, NaiveDate, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaMoney {
    pub minor: i64,
    pub currency: String,
    /// The value as the markup wrote it.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaWhen {
    pub at: DateTime<Utc>,
    /// The value as the markup wrote it.
    pub text: String,
}

/// One record the markup describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaRecord {
    pub kind: RecordKind,
    pub stage: Stage,
    /// The `@type`, for the why line: "FlightReservation".
    pub schema_type: String,
    pub issuer: Option<String>,
    pub title: Option<String>,
    pub reference: Option<String>,
    pub amount: Option<SchemaMoney>,
    pub issued: Option<SchemaWhen>,
    pub span_start: Option<SchemaWhen>,
    pub span_end: Option<SchemaWhen>,
    pub place: Option<String>,
    /// An invoice's `accountId`: bills to one account form a series.
    pub account_ref: Option<String>,
}

static LD_JSON: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<script[^>]+type\s*=\s*["']application/ld\+json["'][^>]*>(.*?)</script>"#)
        .expect("valid ld+json regex")
});

/// Every record the JSON-LD blocks in `html` describe, in document order.
/// An `Order` nested in a `ParcelDelivery` is read once, as the delivery.
pub fn read(html: Option<&str>) -> Vec<SchemaRecord> {
    let Some(html) = html else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for cap in LD_JSON.captures_iter(html) {
        let Some(raw) = cap.get(1) else { continue };
        let Ok(value) = serde_json::from_str::<Value>(raw.as_str().trim()) else {
            continue;
        };
        collect(&value, &mut out);
    }
    out
}

fn collect(value: &Value, out: &mut Vec<SchemaRecord>) {
    match value {
        Value::Object(map) => {
            if let Some(record) = type_name(value).and_then(|name| judge(&name, value)) {
                out.push(record);
                // The record read its nested entities (partOfOrder,
                // reservationFor) already.
                return;
            }
            map.values().for_each(|child| collect(child, out));
        }
        Value::Array(items) => items.iter().for_each(|child| collect(child, out)),
        _ => {}
    }
}

fn type_name(entity: &Value) -> Option<String> {
    let suffix = |s: &str| s.rsplit('/').next().unwrap_or(s).to_string();
    match entity.get("@type")? {
        Value::String(s) => Some(suffix(s)),
        Value::Array(items) => items.iter().filter_map(Value::as_str).next().map(suffix),
        _ => None,
    }
}

fn enum_suffix(entity: &Value, key: &str) -> Option<String> {
    let raw = match entity.get(key)? {
        Value::String(s) => s.clone(),
        Value::Object(obj) => obj
            .get("@id")
            .or_else(|| obj.get("name"))
            .and_then(Value::as_str)?
            .to_string(),
        _ => return None,
    };
    Some(raw.rsplit('/').next().unwrap_or(&raw).to_string())
}

fn judge(name: &str, entity: &Value) -> Option<SchemaRecord> {
    match name {
        "Order" => Some(order(entity)),
        "ParcelDelivery" => Some(parcel(entity)),
        "Invoice" => Some(invoice(entity)),
        _ if name.ends_with("Reservation") => reservation(name, entity),
        _ => None,
    }
}

fn empty(kind: RecordKind, stage: Stage, schema_type: &str) -> SchemaRecord {
    SchemaRecord {
        kind,
        stage,
        schema_type: schema_type.to_string(),
        issuer: None,
        title: None,
        reference: None,
        amount: None,
        issued: None,
        span_start: None,
        span_end: None,
        place: None,
        account_ref: None,
    }
}

fn order_stage(status: Option<&str>) -> Stage {
    match status {
        Some("OrderInTransit" | "OrderPickupAvailable") => Stage::Shipped,
        Some("OrderDelivered") => Stage::Delivered,
        Some("OrderReturned") => Stage::Return,
        Some("OrderCancelled") => Stage::Cancellation,
        _ => Stage::Confirmation,
    }
}

fn order(entity: &Value) -> SchemaRecord {
    let mut record = empty(
        RecordKind::Order,
        order_stage(enum_suffix(entity, "orderStatus").as_deref()),
        "Order",
    );
    record.reference = str_field(entity, &["orderNumber", "confirmationNumber"]);
    record.issuer = name_field(entity, &["merchant", "seller", "broker"]);
    record.amount = ["totalPaymentDue", "price", "totalPrice"]
        .iter()
        .find_map(|key| entity.get(*key).and_then(|node| money(node, entity)))
        .or_else(|| offers(entity).iter().find_map(|offer| money(offer, offer)));
    record.issued = when(entity, &["orderDate"]);
    record.title = item_title(
        &offers(entity)
            .iter()
            .filter_map(|offer| offered_name(offer))
            .collect::<Vec<_>>(),
    )
    .or_else(|| str_field(entity, &["name", "description"]));
    record
}

fn parcel(entity: &Value) -> SchemaRecord {
    let status = enum_suffix(entity, "deliveryStatus").or_else(|| {
        entity
            .get("partOfOrder")
            .and_then(|order| enum_suffix(order, "orderStatus"))
    });
    let stage = match order_stage(status.as_deref()) {
        Stage::Confirmation => Stage::Shipped,
        other => other,
    };
    let mut record = empty(RecordKind::Order, stage, "ParcelDelivery");
    if let Some(order) = entity.get("partOfOrder") {
        record.reference = str_field(order, &["orderNumber", "confirmationNumber"]);
        record.issuer = name_field(order, &["merchant", "seller", "broker"]);
        record.issued = when(order, &["orderDate"]);
    }
    let items: Vec<String> = match entity.get("itemShipped") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| str_field(item, &["name"]))
            .collect(),
        Some(item) => str_field(item, &["name"]).into_iter().collect(),
        None => Vec::new(),
    };
    record.title = item_title(&items);
    record
}

fn invoice(entity: &Value) -> SchemaRecord {
    let paid = matches!(
        enum_suffix(entity, "paymentStatus").as_deref(),
        Some("PaymentComplete" | "PaymentAutomaticallyApplied")
    );
    let mut record = empty(
        RecordKind::Invoice,
        if paid { Stage::Receipt } else { Stage::Invoice },
        "Invoice",
    );
    record.issuer = name_field(entity, &["provider", "broker", "seller"]);
    record.reference = str_field(
        entity,
        &["confirmationNumber", "identifier", "invoiceNumber"],
    );
    record.account_ref = str_field(entity, &["accountId"]);
    record.amount = ["totalPaymentDue", "price", "minimumPaymentDue"]
        .iter()
        .find_map(|key| entity.get(*key).and_then(|node| money(node, entity)));
    record.issued = when(entity, &["dateCreated", "dateIssued", "datePublished"]);
    record.title = str_field(entity, &["description", "name"]).or_else(|| {
        entity
            .get("referencesOrder")
            .and_then(|order| str_field(order, &["description", "name"]))
    });
    record
}

fn reservation(name: &str, entity: &Value) -> Option<SchemaRecord> {
    let status = enum_suffix(entity, "reservationStatus");
    let stage = match status.as_deref() {
        // Waiting for the user is a to-do, not a record yet.
        Some("ReservationPending" | "ReservationHold") => return None,
        Some("ReservationCancelled") => Stage::Cancellation,
        _ => Stage::Booking,
    };
    let target = entity.get("reservationFor");
    let kind = if name == "EventReservation" {
        RecordKind::Ticket
    } else {
        RecordKind::Booking
    };
    let mut record = empty(kind, stage, name);
    record.reference = str_field(entity, &["reservationNumber", "confirmationNumber"]);
    record.amount = ["totalPrice", "price"]
        .iter()
        .find_map(|key| entity.get(*key).and_then(|node| money(node, entity)));
    record.issued = when(entity, &["bookingTime"]);
    match name {
        "FlightReservation" => {
            let target = target?;
            let airline = target.get("airline");
            record.issuer = airline
                .and_then(|airline| str_field(airline, &["name"]))
                .or_else(|| name_field(entity, &["provider", "broker"]));
            let from = airport(target.get("departureAirport"));
            let to = airport(target.get("arrivalAirport"));
            let flight = match (
                airline.and_then(|airline| str_field(airline, &["iataCode"])),
                str_field(target, &["flightNumber"]),
            ) {
                (Some(code), Some(number)) if !number.starts_with(&code) => {
                    Some(format!("{code}{number}"))
                }
                (_, Some(number)) => Some(number),
                _ => None,
            };
            record.title = match (from.as_ref(), to.as_ref()) {
                (Some(from), Some(to)) => Some(match &flight {
                    Some(flight) => format!("{} -> {} {flight}", from.code, to.code),
                    None => format!("{} -> {}", from.code, to.code),
                }),
                _ => flight,
            };
            record.span_start = when(target, &["departureTime"]);
            record.span_end = when(target, &["arrivalTime"]);
            record.place = to.and_then(|to| to.city);
        }
        "LodgingReservation" => {
            let target = target?;
            record.issuer = str_field(target, &["name"])
                .or_else(|| name_field(entity, &["provider", "broker"]));
            record.span_start = when(entity, &["checkinTime", "checkinDate"]);
            record.span_end = when(entity, &["checkoutTime", "checkoutDate"]);
            record.place = locality(target);
            record.title = match (&record.span_start, &record.span_end) {
                (Some(start), Some(end)) => {
                    let nights = (end.at.date_naive() - start.at.date_naive()).num_days();
                    let stay = match nights {
                        1 => "1 night".to_string(),
                        n if n > 1 => format!("{n} nights"),
                        _ => "stay".to_string(),
                    };
                    Some(match &record.place {
                        Some(place) => format!("{place}, {stay}"),
                        None => stay,
                    })
                }
                _ => record.place.clone(),
            };
        }
        "EventReservation" => {
            let target = target?;
            record.title = str_field(target, &["name"]);
            record.issuer = target
                .get("organizer")
                .and_then(name_field_value)
                .or_else(|| name_field(entity, &["provider", "broker"]))
                .or_else(|| {
                    target
                        .get("location")
                        .and_then(|location| str_field(location, &["name"]))
                });
            record.span_start = when(target, &["startDate"]);
            record.span_end = when(target, &["endDate"]);
            record.place = target.get("location").and_then(locality);
        }
        _ => {
            record.issuer = name_field(entity, &["provider", "broker"])
                .or_else(|| target.and_then(|target| name_field(target, &["provider"])));
            record.title = target.and_then(|target| str_field(target, &["name"]));
            record.span_start = target
                .and_then(|target| when(target, &["startDate", "startTime", "departureTime"]))
                .or_else(|| when(entity, &["startTime", "pickupTime"]));
            record.span_end = target
                .and_then(|target| when(target, &["endDate", "endTime", "arrivalTime"]))
                .or_else(|| when(entity, &["endTime", "dropoffTime"]));
            record.place = target.and_then(locality);
        }
    }
    Some(record)
}

struct Airport {
    code: String,
    city: Option<String>,
}

fn airport(node: Option<&Value>) -> Option<Airport> {
    let node = node?;
    let code = str_field(node, &["iataCode", "name"])?;
    Some(Airport {
        city: locality(node).or_else(|| str_field(node, &["name"]).filter(|name| *name != code)),
        code,
    })
}

/// The city of a place: its address's locality, else the place's name.
fn locality(node: &Value) -> Option<String> {
    match node.get("address") {
        Some(Value::Object(_)) => node
            .get("address")
            .and_then(|address| str_field(address, &["addressLocality", "addressRegion"])),
        Some(Value::String(address)) => address
            .split(',')
            .rev()
            .nth(1)
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty()),
        _ => None,
    }
}

fn offers(entity: &Value) -> Vec<&Value> {
    match entity.get("acceptedOffer") {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(offer @ Value::Object(_)) => vec![offer],
        _ => Vec::new(),
    }
}

fn offered_name(offer: &Value) -> Option<String> {
    match offer.get("itemOffered") {
        Some(item @ Value::Object(_)) => str_field(item, &["name"]),
        Some(Value::String(name)) => Some(name.trim().to_string()),
        _ => str_field(offer, &["name"]),
    }
}

/// "XPS 14 laptop", or "XPS 14 laptop and 2 more".
fn item_title(names: &[String]) -> Option<String> {
    let mut unique: Vec<&String> = Vec::new();
    for name in names {
        if !name.trim().is_empty() && !unique.contains(&name) {
            unique.push(name);
        }
    }
    let first = unique.first()?;
    Some(match unique.len() {
        1 => (*first).clone(),
        n => format!("{first} and {} more", n - 1),
    })
}

fn str_field(entity: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match entity.get(*key)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn name_field_value(node: &Value) -> Option<String> {
    match node {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Object(obj) => obj
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

fn name_field(entity: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| entity.get(*key).and_then(name_field_value))
}

/// A `PriceSpecification`, `MonetaryAmount`, `Offer` or a bare number with
/// the currency on the parent.
fn money(node: &Value, parent: &Value) -> Option<SchemaMoney> {
    let (value, currency) = match node {
        Value::Object(obj) => (
            obj.get("price").or_else(|| obj.get("value"))?,
            obj.get("priceCurrency")
                .or_else(|| obj.get("currency"))
                .or_else(|| parent.get("priceCurrency")),
        ),
        Value::Number(_) | Value::String(_) => (node, parent.get("priceCurrency")),
        _ => return None,
    };
    let text = match value {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.trim().to_string(),
        _ => return None,
    };
    let currency = currency?.as_str()?.trim().to_ascii_uppercase();
    if currency.len() != 3 {
        return None;
    }
    let minor = mxr_todo::money::parse_minor(text.trim())?;
    (minor > 0).then_some(SchemaMoney {
        minor,
        currency,
        text,
    })
}

fn when(entity: &Value, keys: &[&str]) -> Option<SchemaWhen> {
    keys.iter().find_map(|key| {
        let text = str_field(entity, &[key])?;
        Some(SchemaWhen {
            at: parse_when(&text)?,
            text,
        })
    })
}

/// RFC 3339 instants as given; a date alone as that day.
fn parse_when(text: &str) -> Option<DateTime<Utc>> {
    if let Ok(at) = DateTime::parse_from_rfc3339(text) {
        return Some(at.with_timezone(&Utc));
    }
    if let Ok(at) = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S") {
        return Some(DateTime::from_naive_utc_and_offset(at, Utc));
    }
    NaiveDate::parse_from_str(text.get(..10).unwrap_or(text), "%Y-%m-%d")
        .ok()
        .map(day_at)
}

#[cfg(test)]
mod tests;
