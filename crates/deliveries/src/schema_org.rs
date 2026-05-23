//! schema.org structured-data fast-path. When a shipping email embeds JSON-LD
//! (`ParcelDelivery` / `Order`), it is ground truth: we read fields directly
//! and skip the LLM. Adoption is uneven across merchants, so this is purely
//! opportunistic — absence is the common case, not an error.

use crate::data::normalize_carrier;
use crate::DeliveryStatus;
use chrono::{DateTime, Utc};
use mxr_store::DeliveryItem;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SchemaExtract {
    pub status: Option<DeliveryStatus>,
    pub carrier: Option<String>,
    pub tracking_number: Option<String>,
    pub tracking_url: Option<String>,
    pub order_number: Option<String>,
    pub merchant: Option<String>,
    pub eta_from: Option<DateTime<Utc>>,
    pub eta_until: Option<DateTime<Utc>>,
    pub items: Vec<DeliveryItem>,
}

impl SchemaExtract {
    /// True when the block carried at least one actionable shipment field.
    fn is_meaningful(&self) -> bool {
        self.tracking_number.is_some()
            || self.status.is_some()
            || self.order_number.is_some()
            || self.eta_until.is_some()
            || self.eta_from.is_some()
    }
}

static LD_JSON: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<script[^>]+type\s*=\s*["']application/ld\+json["'][^>]*>(.*?)</script>"#)
        .unwrap()
});

/// Parse the first meaningful `ParcelDelivery`/`Order` JSON-LD block from an
/// HTML body, if any.
pub fn extract(html: Option<&str>) -> Option<SchemaExtract> {
    let html = html?;
    for cap in LD_JSON.captures_iter(html) {
        let raw = match cap.get(1) {
            Some(m) => m.as_str().trim(),
            None => continue,
        };
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        if !mentions_shipment(&value) {
            continue;
        }
        let extracted = build(&value);
        if extracted.is_meaningful() {
            return Some(extracted);
        }
    }
    None
}

fn build(v: &Value) -> SchemaExtract {
    SchemaExtract {
        status: find_str(v, "orderStatus")
            .or_else(|| find_str(v, "deliveryStatus"))
            .and_then(|s| map_order_status(&s)),
        carrier: find_name(v, "carrier")
            .or_else(|| find_name(v, "provider"))
            .map(|c| normalize_carrier(&c)),
        tracking_number: find_str(v, "trackingNumber"),
        tracking_url: find_str(v, "trackingUrl").or_else(|| find_str(v, "url")),
        order_number: find_str(v, "orderNumber").or_else(|| find_str(v, "confirmationNumber")),
        merchant: find_name(v, "merchant").or_else(|| find_name(v, "seller")),
        eta_from: find_str(v, "expectedArrivalFrom").and_then(|s| parse_dt(&s)),
        eta_until: find_str(v, "expectedArrivalUntil").and_then(|s| parse_dt(&s)),
        items: collect_items(v),
    }
}

/// Map a schema.org `OrderStatus` (bare or URL form) to our lifecycle enum.
fn map_order_status(s: &str) -> Option<DeliveryStatus> {
    let suffix = s.rsplit('/').next().unwrap_or(s);
    match suffix {
        "OrderProcessing" | "OrderPaymentDue" => Some(DeliveryStatus::Ordered),
        "OrderInTransit" => Some(DeliveryStatus::InTransit),
        "OrderPickupAvailable" => Some(DeliveryStatus::AvailableForPickup),
        "OrderDelivered" => Some(DeliveryStatus::Delivered),
        "OrderProblem" => Some(DeliveryStatus::Exception),
        "OrderReturned" => Some(DeliveryStatus::Returned),
        _ => None,
    }
}

fn parse_dt(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // Date-only ("2024-05-10"): treat as end-of-day UTC for an ETA window.
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(23, 59, 59))
        .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
}

/// Recursively detect any `@type` of ParcelDelivery / Order in the document
/// (handles `@graph`, arrays, and nested objects).
fn mentions_shipment(v: &Value) -> bool {
    match v {
        Value::Object(map) => {
            if let Some(t) = map.get("@type") {
                if type_matches(t, &["ParcelDelivery", "Order"]) {
                    return true;
                }
            }
            map.values().any(mentions_shipment)
        }
        Value::Array(items) => items.iter().any(mentions_shipment),
        _ => false,
    }
}

fn type_matches(t: &Value, wanted: &[&str]) -> bool {
    match t {
        Value::String(s) => wanted.iter().any(|w| s.ends_with(w)),
        Value::Array(items) => items.iter().any(|i| type_matches(i, wanted)),
        _ => false,
    }
}

/// DFS for the first string value under `key` anywhere in the document.
fn find_str(v: &Value, key: &str) -> Option<String> {
    match v {
        Value::Object(map) => {
            if let Some(Value::String(s)) = map.get(key) {
                if !s.trim().is_empty() {
                    return Some(s.clone());
                }
            }
            // Numbers (e.g. orderNumber) coerced to string.
            if let Some(Value::Number(n)) = map.get(key) {
                return Some(n.to_string());
            }
            map.values().find_map(|val| find_str(val, key))
        }
        Value::Array(items) => items.iter().find_map(|i| find_str(i, key)),
        _ => None,
    }
}

/// DFS for `key` whose value is an entity; return its `name` (or the string
/// itself if `key` maps directly to a string).
fn find_name(v: &Value, key: &str) -> Option<String> {
    match v {
        Value::Object(map) => {
            match map.get(key) {
                Some(Value::String(s)) if !s.trim().is_empty() => return Some(s.clone()),
                Some(Value::Object(obj)) => {
                    if let Some(Value::String(name)) = obj.get("name") {
                        if !name.trim().is_empty() {
                            return Some(name.clone());
                        }
                    }
                }
                _ => {}
            }
            map.values().find_map(|val| find_name(val, key))
        }
        Value::Array(items) => items.iter().find_map(|i| find_name(i, key)),
        _ => None,
    }
}

fn collect_items(v: &Value) -> Vec<DeliveryItem> {
    let mut items = Vec::new();
    collect_items_into(v, &mut items);
    items
}

fn collect_items_into(v: &Value, out: &mut Vec<DeliveryItem>) {
    if let Value::Object(map) = v {
        for key in ["itemShipped", "orderedItem"] {
            if let Some(node) = map.get(key) {
                push_item_node(node, out);
            }
        }
    }
    match v {
        Value::Object(map) => map.values().for_each(|val| collect_items_into(val, out)),
        Value::Array(arr) => arr.iter().for_each(|val| collect_items_into(val, out)),
        _ => {}
    }
}

fn push_item_node(node: &Value, out: &mut Vec<DeliveryItem>) {
    match node {
        Value::Array(arr) => arr.iter().for_each(|i| push_item_node(i, out)),
        Value::Object(obj) => {
            // orderedItem may wrap the product under "orderedItem"/"name".
            let name = obj
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| find_str(node, "name"));
            if let Some(name) = name {
                if !name.trim().is_empty() && !out.iter().any(|i| i.name == name) {
                    let quantity = obj
                        .get("orderQuantity")
                        .or_else(|| obj.get("quantity"))
                        .and_then(Value::as_i64);
                    out.push(DeliveryItem { name, quantity });
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_parcel_delivery_block() {
        let html = r#"
          <html><head>
          <script type="application/ld+json">
          {
            "@context": "https://schema.org",
            "@type": "ParcelDelivery",
            "trackingNumber": "1Z999AA10123456784",
            "trackingUrl": "https://ups.com/track?n=1Z999AA10123456784",
            "carrier": { "@type": "Organization", "name": "UPS" },
            "expectedArrivalUntil": "2024-05-10T18:00:00Z",
            "orderStatus": "https://schema.org/OrderInTransit",
            "partOfOrder": {
              "@type": "Order",
              "orderNumber": "ABC-123",
              "merchant": { "@type": "Organization", "name": "Acme" }
            },
            "itemShipped": { "@type": "Product", "name": "Blue Widget" }
          }
          </script></head><body>hi</body></html>"#;
        let got = extract(Some(html)).expect("schema parsed");
        assert_eq!(got.tracking_number.as_deref(), Some("1Z999AA10123456784"));
        assert_eq!(got.carrier.as_deref(), Some("ups"));
        assert_eq!(got.order_number.as_deref(), Some("ABC-123"));
        assert_eq!(got.merchant.as_deref(), Some("Acme"));
        assert_eq!(got.status, Some(DeliveryStatus::InTransit));
        assert!(got.eta_until.is_some());
        assert_eq!(got.items.len(), 1);
        assert_eq!(got.items[0].name, "Blue Widget");
    }

    #[test]
    fn ignores_non_shipment_jsonld() {
        let html = r#"<script type="application/ld+json">
            {"@type":"WebSite","name":"Acme"}</script>"#;
        assert!(extract(Some(html)).is_none());
    }

    #[test]
    fn none_when_no_html() {
        assert!(extract(None).is_none());
        assert!(extract(Some("<p>no structured data</p>")).is_none());
    }
}
