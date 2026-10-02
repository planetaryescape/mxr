//! schema.org markup that says a to-do exists: an `Invoice` with payment
//! due, an `Order` whose status is `OrderPaymentDue`, a `*Reservation`
//! still pending. Markup is the sender's own statement, so its fields win
//! over patterns in the text. It also settles the other way: an invoice
//! marked paid or collected automatically is a record, not a to-do.
//!
//! Adoption is uneven, so absence is the common case, not an error.

use chrono::{DateTime, NaiveDate, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaAmount {
    pub minor: i64,
    pub currency: String,
    /// The value as the markup wrote it, for provenance.
    pub text: String,
}

/// A due date from markup: a calendar day, or an exact instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaDate {
    Day(NaiveDate),
    At(DateTime<Utc>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaTodo {
    /// `Invoice` (or `Order` with `OrderPaymentDue`) still to pay.
    Bill {
        provider: Option<String>,
        description: Option<String>,
        amount: Option<SchemaAmount>,
        due: Option<SchemaDate>,
        due_text: Option<String>,
        url: Option<String>,
    },
    /// An `Invoice` whose payment was declined: fix it.
    PaymentFailed {
        provider: Option<String>,
        amount: Option<SchemaAmount>,
        url: Option<String>,
    },
    /// A `*Reservation` waiting for the user to confirm it.
    PendingReservation {
        name: Option<String>,
        provider: Option<String>,
        starts: Option<SchemaDate>,
        starts_text: Option<String>,
        url: Option<String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaVerdict {
    pub todo: Option<SchemaTodo>,
    /// The markup says nothing is owed: paid, collected automatically, or
    /// a confirmed booking. Text patterns must not make a to-do of it.
    pub settled: bool,
}

static LD_JSON: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<script[^>]+type\s*=\s*["']application/ld\+json["'][^>]*>(.*?)</script>"#)
        .expect("valid ld+json regex")
});

/// Read every JSON-LD block in `html` for to-do markup.
pub fn read(html: Option<&str>) -> SchemaVerdict {
    let mut verdict = SchemaVerdict::default();
    let Some(html) = html else {
        return verdict;
    };
    for cap in LD_JSON.captures_iter(html) {
        let Some(raw) = cap.get(1) else { continue };
        let Ok(value) = serde_json::from_str::<Value>(raw.as_str().trim()) else {
            continue;
        };
        let mut entities = Vec::new();
        collect_entities(&value, &mut entities);
        for entity in entities {
            match judge(entity) {
                Judgement::Todo(todo) if verdict.todo.is_none() => verdict.todo = Some(todo),
                Judgement::Settled => verdict.settled = true,
                _ => {}
            }
        }
    }
    if verdict.todo.is_some() {
        verdict.settled = false;
    }
    verdict
}

enum Judgement {
    Todo(SchemaTodo),
    Settled,
    Nothing,
}

fn collect_entities<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            if map.get("@type").is_some_and(|t| {
                type_matches(t, |name| {
                    name == "Invoice" || name == "Order" || name.ends_with("Reservation")
                })
            }) {
                out.push(value);
            }
            map.values().for_each(|child| collect_entities(child, out));
        }
        Value::Array(items) => items.iter().for_each(|child| collect_entities(child, out)),
        _ => {}
    }
}

fn type_matches(t: &Value, wanted: impl Fn(&str) -> bool + Copy) -> bool {
    match t {
        Value::String(s) => wanted(s.rsplit('/').next().unwrap_or(s)),
        Value::Array(items) => items.iter().any(|item| type_matches(item, wanted)),
        _ => false,
    }
}

fn type_name(entity: &Value) -> Option<String> {
    match entity.get("@type")? {
        Value::String(s) => Some(s.rsplit('/').next().unwrap_or(s).to_string()),
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .next()
            .map(|s| s.rsplit('/').next().unwrap_or(s).to_string()),
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

fn judge(entity: &Value) -> Judgement {
    let Some(name) = type_name(entity) else {
        return Judgement::Nothing;
    };
    match name.as_str() {
        "Invoice" => match enum_suffix(entity, "paymentStatus").as_deref() {
            Some("PaymentComplete" | "PaymentAutomaticallyApplied") => Judgement::Settled,
            Some("PaymentDeclined") => Judgement::Todo(SchemaTodo::PaymentFailed {
                provider: name_field(entity, &["provider", "broker", "seller"]),
                amount: ["totalPaymentDue", "minimumPaymentDue"]
                    .iter()
                    .find_map(|key| entity.get(*key).and_then(|node| amount(node, entity))),
                url: action_url(entity),
            }),
            _ => Judgement::Todo(bill(
                entity,
                &["provider", "broker", "seller"],
                &["totalPaymentDue", "minimumPaymentDue"],
            )),
        },
        "Order" => match enum_suffix(entity, "orderStatus").as_deref() {
            Some("OrderPaymentDue") => Judgement::Todo(bill(
                entity,
                &["seller", "merchant", "broker"],
                &["totalPaymentDue", "price", "acceptedOffer"],
            )),
            _ => Judgement::Nothing,
        },
        _ if name.ends_with("Reservation") => {
            match enum_suffix(entity, "reservationStatus").as_deref() {
                Some("ReservationPending" | "ReservationHold") => {
                    let starts_text = entity
                        .get("reservationFor")
                        .and_then(|target| {
                            str_field(target, &["startDate", "startTime", "departureTime"])
                        })
                        .or_else(|| str_field(entity, &["checkinTime", "startTime"]));
                    Judgement::Todo(SchemaTodo::PendingReservation {
                        name: entity
                            .get("reservationFor")
                            .and_then(|target| str_field(target, &["name"])),
                        provider: name_field(entity, &["provider", "broker"]),
                        starts: starts_text.as_deref().and_then(parse_date),
                        starts_text,
                        url: action_url(entity),
                    })
                }
                Some("ReservationConfirmed" | "ReservationCancelled") => Judgement::Settled,
                _ => Judgement::Nothing,
            }
        }
        _ => Judgement::Nothing,
    }
}

fn bill(entity: &Value, provider_keys: &[&str], amount_keys: &[&str]) -> SchemaTodo {
    let due_text = str_field(entity, &["paymentDueDate", "paymentDue"]);
    SchemaTodo::Bill {
        provider: name_field(entity, provider_keys),
        description: str_field(entity, &["description", "name"]),
        amount: amount_keys
            .iter()
            .find_map(|key| entity.get(*key).and_then(|node| amount(node, entity))),
        due: due_text.as_deref().and_then(parse_date),
        due_text,
        url: action_url(entity),
    }
}

fn str_field(entity: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match entity.get(*key)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn name_field(entity: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match entity.get(*key)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Object(obj) => obj
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string),
        _ => None,
    })
}

/// A `PriceSpecification`, `MonetaryAmount`, `Offer` or a bare number with
/// the currency on the parent.
fn amount(node: &Value, parent: &Value) -> Option<SchemaAmount> {
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
    let number: f64 = text.replace(',', "").parse().ok()?;
    if !number.is_finite() || number <= 0.0 {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "bounded money amount rounded to minor units"
    )]
    let minor = (number * 100.0).round() as i64;
    Some(SchemaAmount {
        minor,
        currency,
        text,
    })
}

fn action_url(entity: &Value) -> Option<String> {
    let from_action = |action: &Value| -> Option<String> {
        match action.get("target")? {
            Value::String(s) => Some(s.clone()),
            Value::Object(obj) => obj
                .get("urlTemplate")
                .and_then(Value::as_str)
                .map(str::to_string),
            _ => None,
        }
    };
    let action = match entity.get("potentialAction") {
        Some(Value::Array(items)) => items.iter().find_map(from_action),
        Some(action @ Value::Object(_)) => from_action(action),
        _ => None,
    };
    action
        .or_else(|| str_field(entity, &["url", "modifyReservationUrl"]))
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
}

fn parse_date(text: &str) -> Option<SchemaDate> {
    if let Ok(at) = DateTime::parse_from_rfc3339(text) {
        return Some(SchemaDate::At(at.with_timezone(&Utc)));
    }
    NaiveDate::parse_from_str(text.get(..10).unwrap_or(text), "%Y-%m-%d")
        .ok()
        .map(SchemaDate::Day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(json: &str) -> String {
        format!(r#"<html><head><script type="application/ld+json">{json}</script></head></html>"#)
    }

    #[test]
    fn invoice_with_payment_due_is_a_bill() {
        let html = wrap(
            r#"{"@context":"https://schema.org","@type":"Invoice",
                "provider":{"@type":"Organization","name":"Camden Council"},
                "description":"Council tax",
                "totalPaymentDue":{"@type":"PriceSpecification","price":142.00,"priceCurrency":"GBP"},
                "paymentDueDate":"2026-10-09",
                "paymentStatus":"https://schema.org/PaymentDue",
                "url":"https://www.camden.gov.uk/pay"}"#,
        );
        let verdict = read(Some(&html));
        let Some(SchemaTodo::Bill {
            provider,
            amount,
            due,
            url,
            ..
        }) = verdict.todo
        else {
            panic!("expected a bill, got {verdict:?}");
        };
        assert_eq!(provider.as_deref(), Some("Camden Council"));
        assert_eq!(
            amount.map(|amount| (amount.minor, amount.currency)),
            Some((14200, "GBP".to_string()))
        );
        assert_eq!(
            due,
            Some(SchemaDate::Day(
                NaiveDate::from_ymd_opt(2026, 10, 9).expect("date")
            ))
        );
        assert_eq!(url.as_deref(), Some("https://www.camden.gov.uk/pay"));
        assert!(!verdict.settled);
    }

    #[test]
    fn paid_or_automatic_invoices_settle() {
        for status in ["PaymentComplete", "PaymentAutomaticallyApplied"] {
            let html = wrap(&format!(
                r#"{{"@type":"Invoice","paymentStatus":"https://schema.org/{status}",
                    "totalPaymentDue":{{"price":"9.99","priceCurrency":"GBP"}}}}"#
            ));
            let verdict = read(Some(&html));
            assert_eq!(verdict.todo, None, "{status}");
            assert!(verdict.settled, "{status}");
        }
    }

    #[test]
    fn a_declined_payment_is_a_to_do_not_a_settled_invoice() {
        let html = wrap(
            r#"{"@type":"Invoice","provider":{"name":"Spotify"},"paymentStatus":"https://schema.org/PaymentDeclined",
                "totalPaymentDue":{"price":"11.99","priceCurrency":"GBP"}}"#,
        );
        let verdict = read(Some(&html));
        assert!(!verdict.settled);
        assert!(matches!(
            verdict.todo,
            Some(SchemaTodo::PaymentFailed {
                amount: Some(SchemaAmount { minor: 1199, .. }),
                ..
            })
        ));
    }

    #[test]
    fn order_awaiting_payment_is_a_bill_and_shipped_order_is_nothing() {
        let due = wrap(
            r#"{"@type":"Order","orderStatus":"OrderPaymentDue","seller":{"name":"Acme"},
                "price":"25.50","priceCurrency":"usd","paymentDueDate":"2026-10-03T17:00:00Z"}"#,
        );
        assert!(matches!(
            read(Some(&due)).todo,
            Some(SchemaTodo::Bill {
                amount: Some(SchemaAmount { minor: 2550, .. }),
                ..
            })
        ));
        let shipped = wrap(r#"{"@type":"Order","orderStatus":"OrderInTransit"}"#);
        assert_eq!(read(Some(&shipped)), SchemaVerdict::default());
    }

    #[test]
    fn pending_reservation_needs_confirming_and_confirmed_settles() {
        let pending = wrap(
            r#"{"@type":"FoodEstablishmentReservation","reservationStatus":"ReservationPending",
                "reservationFor":{"@type":"FoodEstablishment","name":"Dishoom","startDate":"2026-10-10T19:00:00+01:00"},
                "potentialAction":{"@type":"ConfirmAction","target":"https://book.example.com/confirm/1"}}"#,
        );
        let Some(SchemaTodo::PendingReservation {
            name, starts, url, ..
        }) = read(Some(&pending)).todo
        else {
            panic!("expected a pending reservation");
        };
        assert_eq!(name.as_deref(), Some("Dishoom"));
        assert!(matches!(starts, Some(SchemaDate::At(_))));
        assert_eq!(url.as_deref(), Some("https://book.example.com/confirm/1"));
        let confirmed =
            wrap(r#"{"@type":"LodgingReservation","reservationStatus":"ReservationConfirmed"}"#);
        assert!(read(Some(&confirmed)).settled);
    }

    #[test]
    fn no_markup_is_no_verdict() {
        assert_eq!(read(None), SchemaVerdict::default());
        assert_eq!(read(Some("<p>hello</p>")), SchemaVerdict::default());
    }
}
