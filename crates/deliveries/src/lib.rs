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

/// How a delivery was detected. Persisted as a string.
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

/// What Stage 1 decided to do with a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Persist/merge a delivery directly from deterministic signals.
    Create,
    /// Plausible but uncertain — hand to the LLM to confirm and enrich.
    ShortlistLlm,
    /// Not shipment-related (or a post-delivery survey). Do nothing.
    Reject,
}

/// Everything Stage 1 needs from a message. The daemon builds this from the
/// envelope + (reader-cleaned) body.
#[derive(Debug, Clone)]
pub struct DetectInput<'a> {
    /// Sender display name (e.g. "Acme Orders").
    pub from_name: &'a str,
    /// Lowercased sender domain (e.g. "mail.acme.com").
    pub from_domain: &'a str,
    pub subject: &'a str,
    /// Reader-cleaned plain text (quotes/signatures stripped).
    pub body_text: &'a str,
    /// Raw HTML body, for schema.org JSON-LD only.
    pub body_html: Option<&'a str>,
    pub link_count: u32,
    pub body_word_count: u32,
    /// Whether the message carried a List-Unsubscribe header.
    pub has_unsubscribe: bool,
}

/// The result of Stage 1 detection over one message.
#[derive(Debug, Clone, PartialEq)]
pub struct DeliverySignal {
    pub decision: Decision,
    pub confidence: f32,
    pub source: DetectionSource,
    pub stage: Option<DeliveryStatus>,
    pub carrier: Option<String>,
    /// Checksum-validated tracking numbers (may be empty).
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
    /// Human-readable signals that fired, for `scan --dry-run` explainability.
    pub matched_signals: Vec<&'static str>,
}

impl DeliverySignal {
    /// The single best tracking number, if any (schema-derived or validated).
    pub fn primary_tracking_number(&self) -> Option<String> {
        self.tracking_numbers.first().map(|t| t.number.clone())
    }
}

/// Run the deterministic Stage-1 detector over a message. Pure and local.
pub fn detect(input: &DetectInput) -> DeliverySignal {
    let assessment = heuristics::assess(input);
    let schema = schema_org::extract(input.body_html);

    // Tracking numbers come from subject + body. The tracking-numbers crate
    // validates carrier checksums (incl. Amazon TBA), so junk is dropped here.
    let combined = format!("{}\n{}", input.subject, input.body_text);
    let tracking = tracking::extract(&combined);

    // Combine heuristic score with tracking/schema contributions.
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

    // Field resolution: schema (ground truth) wins, then deterministic signals.
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

    // Decision ladder (precision over recall).
    let raw = if schema_present {
        Decision::Create
    } else if carrier_sender && valid_tracking {
        Decision::Create
    } else if score >= 0.8 && (valid_tracking || carrier_sender) {
        Decision::Create
    } else if score >= 0.5 {
        Decision::ShortlistLlm
    } else if stage.is_some() && (order_number.is_some() || known_sender) {
        // Order confirmations from unknown merchant domains (the common case)
        // score low but are worth an LLM look when there's a lifecycle stage
        // plus an order number to correlate on.
        Decision::ShortlistLlm
    } else {
        Decision::Reject
    };

    // A delivery must have a shipping context, not just a string that happens
    // to validate as a tracking number. Loose carrier formats (e.g. DHL
    // eCommerce) match verification codes/hashes in unrelated mail (finance,
    // crypto, SaaS receipts), so never auto-create on a tracking match alone:
    // require a carrier/merchant sender, a shipping-stage subject/body, or
    // schema.org. Uncorroborated candidates are handed to the LLM to judge.
    let corroborated = is_corroborated(&signals);

    // Hard gates: a post-delivery survey never creates; a Create needs a
    // dedup key, otherwise let the LLM try to recover identifiers.
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

/// True if any corroborating shipping-context signal fired.
pub fn is_corroborated(signals: &[&str]) -> bool {
    signals
        .iter()
        .any(|s| CORROBORATING_SIGNALS.contains(s))
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

    fn input<'a>(name: &'a str, domain: &'a str, subject: &'a str, body: &'a str) -> DetectInput<'a> {
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
    fn bare_tracking_match_from_unknown_sender_is_not_auto_created() {
        // A string that validates as a tracking number inside an unrelated
        // email (no carrier/merchant sender, no shipping subject) must not
        // auto-create — it goes to the LLM to confirm. Guards against the
        // CoinTracker/Hostinger-style false positives (loose formats matching
        // codes/hashes).
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
        // Amazon TBA is a high-precision format the checksum library validates,
        // so it is trusted regardless of sender.
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
