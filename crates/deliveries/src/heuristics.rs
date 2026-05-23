//! Deterministic, fully-local signal extraction and scoring. No network, no
//! LLM. Produces an [`Assessment`] that `detect()` combines with tracking-number
//! and schema.org signals to decide create / shortlist-for-LLM / reject and to
//! fill fields without a model.

use crate::data::{
    self, carrier_from_domain, ECOMMERCE_DOMAINS, PROMO_KEYWORDS, REVIEW_KEYWORDS, STAGE_KEYWORDS,
    SUBSCRIPTION_KEYWORDS,
};
use crate::{DeliveryStatus, DetectInput};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenderClass {
    Carrier,
    Ecommerce,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Assessment {
    /// Heuristic-only score (sender/subject/body/exclusions). Tracking-number
    /// and schema contributions are added by `detect()`.
    pub score: f32,
    pub signals: Vec<&'static str>,
    pub sender: SenderClass,
    pub carrier_from_sender: Option<&'static str>,
    pub subject_stage: Option<DeliveryStatus>,
    pub body_stage: Option<DeliveryStatus>,
    pub is_promo: bool,
    pub is_subscription: bool,
    /// Post-delivery review/survey email — must never create or resurrect.
    pub post_delivery_noise: bool,
    pub order_number: Option<String>,
    pub merchant: Option<String>,
    pub eta_until: Option<DateTime<Utc>>,
}

static ORDER_NUMBER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\border\s*(?:#|no\.?|number)?\s*[:#]?\s*([A-Z0-9][A-Z0-9\-]{4,})")
        .expect("valid order-number regex")
});

// ISO date near an ETA cue → eta_until. Conservative on purpose; schema/LLM
// cover the messier phrasings.
static ETA_ISO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(?:arriv\w*|expected|estimated delivery|deliver(?:y|ed)?\s+by|arrives)\D{0,30}(\d{4}-\d{2}-\d{2})",
    )
    .expect("valid eta-iso regex")
});

// Display-name fragments that are roles, not merchants.
const ROLE_NOISE: &[&str] = &[
    "no-reply",
    "noreply",
    "no_reply",
    "notification",
    "shipment",
    "shipping",
    "delivery",
    "orders",
    "order",
    "support",
    "team",
    "info",
    "mailer",
    "auto",
    "do-not-reply",
];

pub fn assess(input: &DetectInput) -> Assessment {
    let subject_l = input.subject.to_lowercase();
    let body_l = input.body_text.to_lowercase();
    let mut score = 0.0_f32;
    let mut signals: Vec<&'static str> = Vec::new();

    // --- sender classification ---
    let (sender, carrier_from_sender) = classify_sender(input.from_domain);
    match sender {
        SenderClass::Carrier => {
            score += 0.5;
            signals.push("carrier_sender");
        }
        SenderClass::Ecommerce => {
            score += 0.3;
            signals.push("ecommerce_sender");
        }
        SenderClass::Unknown => {}
    }

    // --- lifecycle stage from subject, then body ---
    let subject_stage = match_stage(&subject_l);
    let body_stage = match_stage(&body_l);
    match subject_stage {
        Some(DeliveryStatus::Ordered) => {
            score += 0.2;
            signals.push("order_confirmation_subject");
        }
        Some(DeliveryStatus::InfoReceived) => {
            score += 0.15;
            signals.push("info_received_subject");
        }
        Some(_) => {
            score += 0.3;
            signals.push("shipping_stage_subject");
        }
        None => {
            if body_stage.is_some() {
                score += 0.15;
                signals.push("body_stage");
            }
        }
    }

    // --- order number ---
    let order_number = ORDER_NUMBER
        .captures(input.subject)
        .or_else(|| ORDER_NUMBER.captures(input.body_text))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim_end_matches(['.', ',']).to_string())
        // Real order numbers contain a digit; this rejects the next word after
        // "order" ("...your order. Thank you..." → "Thank").
        .filter(|s| s.chars().any(|c| c.is_ascii_digit()));
    if order_number.is_some() {
        score += 0.1;
        signals.push("order_number");
    }

    // --- exclusions (negative) ---
    let is_promo =
        contains_any(&subject_l, PROMO_KEYWORDS) || contains_any(&body_l, PROMO_KEYWORDS);
    if is_promo {
        score -= 0.4;
        signals.push("promo");
    }
    let is_subscription = contains_any(&subject_l, SUBSCRIPTION_KEYWORDS)
        || contains_any(&body_l, SUBSCRIPTION_KEYWORDS);
    if is_subscription {
        score -= 0.4;
        signals.push("subscription");
    }
    let post_delivery_noise =
        contains_any(&subject_l, REVIEW_KEYWORDS) || contains_any(&body_l, REVIEW_KEYWORDS);
    if post_delivery_noise {
        score -= 0.5;
        signals.push("review_survey");
    }
    if input.has_unsubscribe {
        score -= 0.1;
        signals.push("list_unsubscribe");
    }
    if input.link_count >= 15 && input.body_word_count < 120 {
        score -= 0.15;
        signals.push("newsletter_shape");
    }

    let merchant = merchant_from_sender(input.from_name, input.from_domain, sender);
    let eta_until = ETA_ISO
        .captures(input.body_text)
        .or_else(|| ETA_ISO.captures(input.subject))
        .and_then(|c| c.get(1))
        .and_then(|m| parse_iso_eod(m.as_str()));

    Assessment {
        score,
        signals,
        sender,
        carrier_from_sender,
        subject_stage,
        body_stage,
        is_promo,
        is_subscription,
        post_delivery_noise,
        order_number,
        merchant,
        eta_until,
    }
}

fn classify_sender(domain: &str) -> (SenderClass, Option<&'static str>) {
    let d = domain.trim_start_matches("www.").to_lowercase();
    if data::CARRIER_DOMAINS.iter().any(|c| domain_matches(&d, c)) {
        return (SenderClass::Carrier, carrier_from_domain(&d));
    }
    if ECOMMERCE_DOMAINS.iter().any(|c| domain_matches(&d, c)) {
        // Amazon is both a marketplace and (often) the carrier-of-record.
        return (SenderClass::Ecommerce, carrier_from_domain(&d));
    }
    (SenderClass::Unknown, None)
}

/// Suffix match: the From domain equals the needle or is a subdomain of it.
fn domain_matches(domain: &str, needle: &str) -> bool {
    domain == needle || domain.ends_with(&format!(".{needle}"))
}

fn match_stage(haystack_lower: &str) -> Option<DeliveryStatus> {
    for (stage, keywords) in STAGE_KEYWORDS {
        if keywords.iter().any(|k| haystack_lower.contains(k)) {
            return Some(*stage);
        }
    }
    None
}

fn contains_any(haystack_lower: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack_lower.contains(n))
}

fn merchant_from_sender(name: &str, domain: &str, sender: SenderClass) -> Option<String> {
    // A carrier sender isn't the merchant — leave it for schema/LLM.
    if sender == SenderClass::Carrier {
        return None;
    }
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        let lower = trimmed.to_lowercase();
        if !ROLE_NOISE.iter().any(|r| lower.contains(r)) {
            // Take the part before a separator ("Acme Orders <...>" → "Acme").
            let clean = trimmed
                .split(['(', ',', '|'])
                .next()
                .unwrap_or(trimmed)
                .trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }
    // Fall back to the registrable label of the domain ("mail.acme.com" → "acme").
    domain_root(domain)
}

/// The second-level label of a domain, capitalized ("shop.acme.co.uk"→"Acme").
fn domain_root(domain: &str) -> Option<String> {
    let parts: Vec<&str> = domain.trim_start_matches("www.").split('.').collect();
    let label = if parts.len() >= 2 {
        parts[parts.len() - 2]
    } else {
        parts.first().copied().unwrap_or("")
    };
    if label.is_empty() {
        return None;
    }
    let mut chars = label.chars();
    Some(match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => return None,
    })
}

fn parse_iso_eod(s: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(23, 59, 59))
        .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(
        name: &'a str,
        domain: &'a str,
        subject: &'a str,
        body: &'a str,
    ) -> DetectInput<'a> {
        DetectInput {
            from_name: name,
            from_domain: domain,
            subject,
            body_text: body,
            body_html: None,
            link_count: 1,
            body_word_count: 80,
            has_unsubscribe: false,
        }
    }

    #[test]
    fn carrier_shipped_scores_high() {
        let a = assess(&input(
            "UPS",
            "ups.com",
            "Your package has shipped",
            "Tracking number inside.",
        ));
        assert_eq!(a.sender, SenderClass::Carrier);
        assert_eq!(a.carrier_from_sender, Some("ups"));
        assert_eq!(a.subject_stage, Some(DeliveryStatus::InTransit));
        assert!(a.score >= 0.8, "score={}", a.score);
    }

    #[test]
    fn delivered_beats_shipped_in_priority() {
        let a = assess(&input(
            "Acme",
            "acme.com",
            "Your shipped order was delivered",
            "",
        ));
        assert_eq!(a.subject_stage, Some(DeliveryStatus::Delivered));
    }

    #[test]
    fn promo_is_penalized_and_flagged() {
        let a = assess(&input(
            "Acme",
            "acme.com",
            "Flash sale — 50% off + free shipping!",
            "Shop now.",
        ));
        assert!(a.is_promo);
        assert!(a.score < 0.3, "score={}", a.score);
    }

    #[test]
    fn review_email_is_noise() {
        let a = assess(&input(
            "Acme",
            "acme.com",
            "How was your delivery?",
            "Leave a review.",
        ));
        assert!(a.post_delivery_noise);
    }

    #[test]
    fn extracts_order_number_and_merchant() {
        let a = assess(&input(
            "Acme Store",
            "mail.acme.com",
            "Order #AB-12345 confirmed",
            "",
        ));
        assert_eq!(a.order_number.as_deref(), Some("AB-12345"));
        assert_eq!(a.merchant.as_deref(), Some("Acme Store"));
    }

    #[test]
    fn order_number_requires_a_digit() {
        // "...your order. Thank you..." must not yield "Thank" as an order #.
        let a = assess(&input(
            "Acme",
            "acme.com",
            "thanks for your order",
            "Thank you for your order.",
        ));
        assert!(a.order_number.is_none(), "got {:?}", a.order_number);
    }

    #[test]
    fn merchant_falls_back_to_domain_for_role_senders() {
        let a = assess(&input(
            "no-reply",
            "shop.acme.com",
            "Order confirmation",
            "",
        ));
        assert_eq!(a.merchant.as_deref(), Some("Acme"));
    }

    #[test]
    fn parses_iso_eta() {
        let a = assess(&input(
            "Acme",
            "acme.com",
            "Shipped",
            "Estimated delivery 2024-05-10. Thanks.",
        ));
        assert!(a.eta_until.is_some());
    }
}
