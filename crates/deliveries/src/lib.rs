//! Package/delivery detection and lifecycle for mxr.
//!
//! Pipeline:
//!   1. [`detect`] — a deterministic, fully-local heuristic. Classifies the
//!      sender, scans subject/body for lifecycle keywords, reads schema.org
//!      JSON-LD when present, and extracts checksum-validated tracking numbers.
//!      Returns a [`DeliverySignal`] with a create / shortlist-for-LLM / reject
//!      decision and as many fields as it can fill without a model.
//!   2. `extract` (Phase 4) — optional local-LLM confirm+enrich for shortlisted
//!      candidates; re-validates any LLM tracking number against the checksum
//!      library.
//!   3. `lifecycle` (Phase 4) — collapses the many emails of one shipment into
//!      one row, advances status monotonically, resolves on delivered.
//!
//! The crate is provider-agnostic and does no network I/O in Stage 1.

pub mod data;
pub mod extract;
pub mod heuristics;
pub mod lifecycle;
pub mod schema_org;
pub mod tracking;

use chrono::{DateTime, Utc};
use mxr_store::DeliveryItem;
use serde::{Deserialize, Serialize};

pub use tracking::ValidatedTracking;

/// Normalized lifecycle status. Stored as the snake_case string at the
/// store/protocol boundary; the typed enum lives here so `core`/`protocol`
/// stay free of feature-specific vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Ordered,
    InfoReceived,
    InTransit,
    OutForDelivery,
    AttemptFail,
    AvailableForPickup,
    Delivered,
    Exception,
    Returned,
    Expired,
}

impl DeliveryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ordered => "ordered",
            Self::InfoReceived => "info_received",
            Self::InTransit => "in_transit",
            Self::OutForDelivery => "out_for_delivery",
            Self::AttemptFail => "attempt_fail",
            Self::AvailableForPickup => "available_for_pickup",
            Self::Delivered => "delivered",
            Self::Exception => "exception",
            Self::Returned => "returned",
            Self::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "ordered" => Self::Ordered,
            "info_received" => Self::InfoReceived,
            "in_transit" => Self::InTransit,
            "out_for_delivery" => Self::OutForDelivery,
            "attempt_fail" => Self::AttemptFail,
            "available_for_pickup" => Self::AvailableForPickup,
            "delivered" => Self::Delivered,
            "exception" => Self::Exception,
            "returned" => Self::Returned,
            "expired" => Self::Expired,
            _ => return None,
        })
    }

    /// Monotonic ordering for advancement. Terminal states rank highest so a
    /// late, stale email cannot regress a delivered/returned/expired parcel.
    pub fn rank(self) -> u8 {
        match self {
            Self::Ordered => 10,
            Self::InfoReceived => 20,
            Self::InTransit => 30,
            Self::Exception => 35,
            Self::AttemptFail => 40,
            Self::AvailableForPickup => 45,
            Self::OutForDelivery => 50,
            Self::Returned => 90,
            Self::Expired => 95,
            Self::Delivered => 100,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Delivered | Self::Returned | Self::Expired)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionSource {
    Schema,
    Llm,
    Heuristic,
}

impl DetectionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Llm => "llm",
            Self::Heuristic => "heuristic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Create,
    ShortlistLlm,
    Reject,
}

/// Everything Stage 1 needs from a message. The daemon builds this from the
/// envelope + (reader-cleaned) body.
#[derive(Debug, Clone)]
pub struct DetectInput<'a> {
    /// Sender display name (e.g. "Acme Orders").
    pub from_name: &'a str,
    pub from_domain: &'a str,
    pub subject: &'a str,
    pub body_text: &'a str,
    /// Raw HTML body, for schema.org JSON-LD only.
    pub body_html: Option<&'a str>,
    pub link_count: u32,
    pub body_word_count: u32,
    pub has_unsubscribe: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeliverySignal {
    pub decision: Decision,
    pub confidence: f32,
    pub source: DetectionSource,
    pub stage: Option<DeliveryStatus>,
    pub carrier: Option<String>,
    pub tracking_numbers: Vec<ValidatedTracking>,
    pub tracking_url: Option<String>,
    pub order_number: Option<String>,
    pub merchant: Option<String>,
    pub eta_from: Option<DateTime<Utc>>,
    pub eta_until: Option<DateTime<Utc>>,
    pub items: Vec<DeliveryItem>,
    /// Correlation key: tracking number, else "merchant|order".
    pub dedup_key: Option<String>,
    /// Post-delivery review/survey — never create or resurrect from this.
    pub post_delivery_noise: bool,
    pub matched_signals: Vec<&'static str>,
}

impl DeliverySignal {
    pub fn primary_tracking_number(&self) -> Option<String> {
        self.tracking_numbers.first().map(|t| t.number.clone())
    }
}

pub fn detect(input: &DetectInput) -> DeliverySignal {
    let assessment = heuristics::assess(input);
    let schema = schema_org::extract(input.body_html);

    let combined = format!("{}\n{}", input.subject, input.body_text);
    let tracking = tracking::extract(&combined);

    let mut score = assessment.score;
    let mut signals = assessment.signals.clone();
    let schema_present = schema.is_some();
    let valid_tracking = !tracking.is_empty();
    if schema_present {
        score += 1.0;
        signals.push("schema_org");
    }
    if valid_tracking {
        score += 0.5;
        signals.push("valid_tracking_number");
    }

    let tracking_url = schema
        .as_ref()
        .and_then(|s| s.tracking_url.clone())
        .or_else(|| tracking.iter().find_map(|t| t.tracking_url.clone()));
    if tracking_url.is_some() {
        score += 0.3;
        signals.push("tracking_url");
    }

    let stage = schema
        .as_ref()
        .and_then(|s| s.status)
        .or(assessment.subject_stage)
        .or(assessment.body_stage)
        .or(if valid_tracking {
            Some(DeliveryStatus::InTransit)
        } else {
            None
        });

    let carrier = schema
        .as_ref()
        .and_then(|s| s.carrier.clone())
        .or_else(|| tracking.first().map(|t| t.carrier.clone()))
        .or_else(|| assessment.carrier_from_sender.map(str::to_string));

    let tracking_number = schema
        .as_ref()
        .and_then(|s| s.tracking_number.clone())
        .or_else(|| tracking.first().map(|t| t.number.clone()));
    let order_number = schema
        .as_ref()
        .and_then(|s| s.order_number.clone())
        .or_else(|| assessment.order_number.clone());
    let merchant = schema
        .as_ref()
        .and_then(|s| s.merchant.clone())
        .or_else(|| assessment.merchant.clone());
    let eta_from = schema.as_ref().and_then(|s| s.eta_from);
    let eta_until = schema
        .as_ref()
        .and_then(|s| s.eta_until)
        .or(assessment.eta_until);
    let items = schema.as_ref().map(|s| s.items.clone()).unwrap_or_default();

    let dedup_key = compute_dedup_key(
        tracking_number.as_deref(),
        merchant.as_deref(),
        order_number.as_deref(),
    );

    let confidence = score.clamp(0.0, 1.0);
    let known_sender = matches!(
        assessment.sender,
        heuristics::SenderClass::Carrier | heuristics::SenderClass::Ecommerce
    );
    let carrier_sender = assessment.sender == heuristics::SenderClass::Carrier;

    let create = schema_present
        || (carrier_sender && valid_tracking)
        || (score >= 0.8 && (valid_tracking || carrier_sender));
    let shortlist = score >= 0.5 || (stage.is_some() && (order_number.is_some() || known_sender));
    let raw = if create {
        Decision::Create
    } else if shortlist {
        Decision::ShortlistLlm
    } else {
        Decision::Reject
    };

    let corroborated = is_corroborated(&signals);

    let decision = if assessment.post_delivery_noise {
        Decision::Reject
    } else if raw == Decision::Create && dedup_key.is_none() {
        if known_sender {
            Decision::ShortlistLlm
        } else {
            Decision::Reject
        }
    } else if raw == Decision::Create && !schema_present && !corroborated {
        Decision::ShortlistLlm
    } else {
        raw
    };

    let source = if schema_present {
        DetectionSource::Schema
    } else {
        DetectionSource::Heuristic
    };

    DeliverySignal {
        decision,
        confidence,
        source,
        stage,
        carrier,
        tracking_numbers: tracking,
        tracking_url,
        order_number,
        merchant,
        eta_from,
        eta_until,
        items,
        dedup_key,
        post_delivery_noise: assessment.post_delivery_noise,
        matched_signals: signals,
    }
}

/// Signals that establish a real shipping context (vs. a bare tracking-number
/// match). A delivery should not be created from a tracking number alone.
pub const CORROBORATING_SIGNALS: &[&str] = &[
    "carrier_sender",
    "ecommerce_sender",
    "shipping_stage_subject",
    "order_confirmation_subject",
    "info_received_subject",
    "body_stage",
    "schema_org",
];

pub fn is_corroborated(signals: &[&str]) -> bool {
    signals.iter().any(|s| CORROBORATING_SIGNALS.contains(s))
}

/// Correlation key: the tracking number when known, else "merchant|order".
pub fn compute_dedup_key(
    tracking_number: Option<&str>,
    merchant: Option<&str>,
    order_number: Option<&str>,
) -> Option<String> {
    if let Some(t) = tracking_number {
        let t = t.trim();
        if !t.is_empty() {
            return Some(t.to_uppercase());
        }
    }
    if let (Some(m), Some(o)) = (merchant, order_number) {
        let (m, o) = (m.trim(), o.trim());
        if !m.is_empty() && !o.is_empty() {
            return Some(format!("{}|{}", m.to_lowercase(), o.to_lowercase()));
        }
    }
    None
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
    fn carrier_email_with_valid_tracking_creates() {
        let sig = detect(&input(
            "UPS",
            "ups.com",
            "Your package has shipped",
            "Tracking: 1Z5R89390357567127",
        ));
        assert_eq!(sig.decision, Decision::Create);
        assert_eq!(sig.source, DetectionSource::Heuristic);
        assert_eq!(sig.stage, Some(DeliveryStatus::InTransit));
        assert_eq!(sig.carrier.as_deref(), Some("ups"));
        assert_eq!(sig.dedup_key.as_deref(), Some("1Z5R89390357567127"));
        assert!(sig.tracking_url.is_some());
    }

    #[test]
    fn schema_email_creates_from_schema_source() {
        let html = r#"<script type="application/ld+json">
            {"@type":"ParcelDelivery","trackingNumber":"1Z999AA10123456784",
             "carrier":{"name":"UPS"},"orderStatus":"OrderInTransit"}</script>"#;
        let mut inp = input("Acme", "acme.com", "Update", "see attachment");
        inp.body_html = Some(html);
        let sig = detect(&inp);
        assert_eq!(sig.decision, Decision::Create);
        assert_eq!(sig.source, DetectionSource::Schema);
        assert_eq!(sig.dedup_key.as_deref(), Some("1Z999AA10123456784"));
    }

    #[test]
    fn order_confirmation_without_tracking_is_shortlisted() {
        let sig = detect(&input(
            "Acme Store",
            "mail.acme.com",
            "Order #AB-12345 confirmation — thanks for your order",
            "We received your order.",
        ));
        assert_eq!(sig.decision, Decision::ShortlistLlm);
        assert_eq!(sig.dedup_key.as_deref(), Some("acme store|ab-12345"));
        assert_eq!(sig.stage, Some(DeliveryStatus::Ordered));
    }

    #[test]
    fn order_is_confirmed_phrasing_is_shortlisted_not_rejected() {
        let sig = detect(&input(
            "Google Store",
            "google.com",
            "Your order is confirmed",
            "Order confirmed. Order number: GS.9175-8733-9584. Delivers 31 May - \
             5 Jun. Google Fitbit Air (Obsidian). For your VAT invoice, visit \
             payments.google.com.",
        ));
        assert_eq!(sig.stage, Some(DeliveryStatus::Ordered));
        assert_eq!(sig.decision, Decision::ShortlistLlm);
        assert_eq!(
            sig.dedup_key.as_deref(),
            Some("google store|gs.9175-8733-9584")
        );
    }

    #[test]
    fn bare_tracking_match_from_unknown_sender_is_not_auto_created() {
        let sig = detect(&input(
            "CoinTracker",
            "cointracker.io",
            "Your 2024 tax report is ready",
            "Reference 1Z5R89390357567127 for your records.",
        ));
        assert!(!sig.tracking_numbers.is_empty(), "tracking still detected");
        assert_eq!(sig.decision, Decision::ShortlistLlm, "not auto-created");
    }

    #[test]
    fn promotional_email_is_rejected() {
        let sig = detect(&input(
            "Acme",
            "acme.com",
            "Flash sale: 50% off everything + free shipping",
            "Shop now, limited time.",
        ));
        assert_eq!(sig.decision, Decision::Reject);
    }

    #[test]
    fn review_survey_is_rejected_and_flagged_noise() {
        let sig = detect(&input(
            "Acme",
            "acme.com",
            "How was your delivery?",
            "Leave a review of your purchase.",
        ));
        assert_eq!(sig.decision, Decision::Reject);
        assert!(sig.post_delivery_noise);
    }

    #[test]
    fn unknown_sender_plain_text_rejected() {
        let sig = detect(&input(
            "Bob",
            "personal.example",
            "lunch tomorrow?",
            "are you free at noon",
        ));
        assert_eq!(sig.decision, Decision::Reject);
    }

    #[test]
    fn amazon_tba_is_detected_and_creates() {
        let sig = detect(&input(
            "Amazon",
            "amazon.com",
            "Your package is out for delivery",
            "Tracking TBA619632698000",
        ));
        assert!(sig
            .tracking_numbers
            .iter()
            .any(|t| t.number == "TBA619632698000"));
        assert_eq!(sig.decision, Decision::Create);
        assert_eq!(sig.carrier.as_deref(), Some("amazon"));
    }
}
