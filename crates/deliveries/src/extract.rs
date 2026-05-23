//! Optional local-LLM confirm + enrich for shortlisted candidates.
//!
//! Runs only on `Decision::ShortlistLlm` candidates (and to enrich items/ETA on
//! high-confidence heuristic creates). Mirrors the codebase's extraction
//! handlers: a strict-JSON system prompt, `serde_json` parse, field validation.
//! Anti-hallucination: any LLM-emitted tracking number is re-validated against
//! the checksum library before it is trusted. Disabled/blocked LLM degrades to
//! the original heuristic signal — never errors the caller.

use crate::{tracking, Decision, DeliverySignal, DeliveryStatus, DetectionSource};
use chrono::{DateTime, Utc};
use mxr_llm::{ChatMessage, CompletionRequest, FeatureLlmRuntime, LlmError};
use mxr_store::DeliveryItem;
use serde::Deserialize;

const SYSTEM_PROMPT: &str = "You classify and extract package-delivery details from a single email. \
Output STRICT JSON and nothing else, with this schema:\n\
{\"is_shipment_related\": bool, \
\"status\": \"ordered|info_received|in_transit|out_for_delivery|attempt_fail|available_for_pickup|delivered|exception|returned|expired|null\", \
\"merchant\": str|null, \"carrier\": str|null, \"tracking_number\": str|null, \
\"tracking_url\": str|null, \"order_number\": str|null, \
\"items\": [{\"name\": str, \"quantity\": int|null}], \
\"eta_from\": \"RFC3339|null\", \"eta_until\": \"RFC3339|null\", \
\"delivered_at\": \"RFC3339|null\", \"confidence\": 0.0}\n\n\
Rules: set is_shipment_related=true only for emails about a physical package/order \
shipment (order confirmation, shipped, out-for-delivery, delivered, delay/exception). \
Marketing/deals, digital receipts, and subscription emails are NOT shipment-related. \
A code-like string that resembles a tracking number is NOT enough on its own — \
finance, crypto, hosting, security/verification, and SaaS-receipt emails often \
contain reference codes or hashes that look like tracking numbers; these are NOT \
shipments. Only set is_shipment_related=true when the email is clearly from a \
merchant or carrier about a physical parcel the recipient is receiving. \
Copy the tracking number verbatim if present, else null. Do not invent values. \
Return JSON only.";

#[derive(Debug, Deserialize)]
struct LlmDelivery {
    #[serde(default)]
    is_shipment_related: bool,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    merchant: Option<String>,
    #[serde(default)]
    carrier: Option<String>,
    #[serde(default)]
    tracking_number: Option<String>,
    #[serde(default)]
    tracking_url: Option<String>,
    #[serde(default)]
    order_number: Option<String>,
    #[serde(default)]
    items: Vec<LlmItem>,
    #[serde(default)]
    eta_from: Option<String>,
    #[serde(default)]
    eta_until: Option<String>,
    #[serde(default)]
    confidence: f32,
}

#[derive(Debug, Deserialize)]
struct LlmItem {
    name: String,
    #[serde(default)]
    quantity: Option<i64>,
}

/// The text the LLM sees. Built by the caller from the envelope + cleaned body.
pub struct LlmInput<'a> {
    pub from_name: &'a str,
    pub from_domain: &'a str,
    pub subject: &'a str,
    pub body_text: &'a str,
}

/// Confirm + enrich a shortlisted candidate. On confirmation, returns a signal
/// with `decision = Create`, deterministic fields preferred and gaps filled
/// from the model. On rejection, returns `Reject`. If the LLM is
/// disabled/blocked/erroring, returns `base` unchanged so the caller can fall
/// back to heuristic-only handling.
pub async fn enrich(
    runtime: &FeatureLlmRuntime,
    input: &LlmInput<'_>,
    base: DeliverySignal,
) -> DeliverySignal {
    let req = CompletionRequest {
        max_tokens: Some(450),
        temperature: Some(0.0),
        messages: vec![
            ChatMessage::system(SYSTEM_PROMPT),
            ChatMessage::user(format!(
                "From: {} <{}>\nSubject: {}\n\nBody:\n{}\n\nReturn JSON only.",
                input.from_name,
                input.from_domain,
                input.subject,
                truncate(input.body_text, 6000),
            )),
        ],
    };

    let resp = match runtime.complete_background(req).await {
        Ok(r) => r,
        Err(LlmError::Disabled | LlmError::PrivacyBlocked(_)) => return base,
        Err(error) => {
            tracing::warn!(%error, "delivery LLM enrich failed; using heuristic");
            return base;
        }
    };

    let parsed: LlmDelivery = match serde_json::from_str(extract_json(&resp.content)) {
        Ok(p) => p,
        Err(error) => {
            tracing::warn!(%error, "delivery LLM returned non-JSON; using heuristic");
            return base;
        }
    };

    if !parsed.is_shipment_related {
        return DeliverySignal {
            decision: Decision::Reject,
            ..base
        };
    }

    merge(base, parsed)
}

/// Merge LLM output into the heuristic signal: deterministic/schema fields win,
/// the LLM only fills gaps. The LLM tracking number is re-validated.
fn merge(mut base: DeliverySignal, llm: LlmDelivery) -> DeliverySignal {
    base.decision = Decision::Create;
    // Keep schema as the source of record; otherwise credit the LLM.
    if base.source != DetectionSource::Schema {
        base.source = DetectionSource::Llm;
    }
    base.confidence = base.confidence.max(llm.confidence.clamp(0.0, 1.0));

    // Re-validate any LLM tracking number; only adopt if it passes checksum.
    if base.tracking_numbers.is_empty() {
        if let Some(raw) = llm.tracking_number.as_deref() {
            let validated = tracking::extract(raw);
            if !validated.is_empty() {
                base.tracking_numbers = validated;
            }
        }
    }

    if base.stage.is_none() {
        base.stage = llm.status.as_deref().and_then(DeliveryStatus::parse);
    }
    if base.carrier.is_none() {
        base.carrier = llm
            .carrier
            .filter(|c| !c.trim().is_empty())
            .map(|c| crate::data::normalize_carrier(&c));
    }
    if base.tracking_url.is_none() {
        base.tracking_url = llm.tracking_url.filter(|s| !s.trim().is_empty());
    }
    if base.order_number.is_none() {
        base.order_number = llm.order_number.filter(|s| !s.trim().is_empty());
    }
    if base.merchant.is_none() {
        base.merchant = llm.merchant.filter(|s| !s.trim().is_empty());
    }
    if base.eta_from.is_none() {
        base.eta_from = llm.eta_from.as_deref().and_then(parse_dt);
    }
    if base.eta_until.is_none() {
        base.eta_until = llm.eta_until.as_deref().and_then(parse_dt);
    }
    if base.items.is_empty() {
        base.items = llm
            .items
            .into_iter()
            .filter(|i| !i.name.trim().is_empty())
            .map(|i| DeliveryItem {
                name: i.name,
                quantity: i.quantity,
            })
            .collect();
    }

    // Re-derive the dedup key now that fields may be filled in.
    base.dedup_key = crate::compute_dedup_key(
        base.primary_tracking_number().as_deref(),
        base.merchant.as_deref(),
        base.order_number.as_deref(),
    );
    base
}

fn parse_dt(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(23, 59, 59))
                .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
        })
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

/// Pull the JSON object out of a completion that may be wrapped in prose or
/// ```json fences.
fn extract_json(content: &str) -> &str {
    let trimmed = content.trim();
    let start = trimmed.find('{');
    let end = trimmed.rfind('}');
    match (start, end) {
        (Some(s), Some(e)) if e >= s => &trimmed[s..=e],
        _ => trimmed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_unwraps_fences() {
        let c = "Here you go:\n```json\n{\"is_shipment_related\": true}\n```\nDone.";
        assert_eq!(extract_json(c), "{\"is_shipment_related\": true}");
    }

    fn base_signal() -> DeliverySignal {
        DeliverySignal {
            decision: Decision::ShortlistLlm,
            confidence: 0.6,
            source: DetectionSource::Heuristic,
            stage: None,
            carrier: None,
            tracking_numbers: vec![],
            tracking_url: None,
            order_number: None,
            merchant: Some("Acme".into()),
            eta_from: None,
            eta_until: None,
            items: vec![],
            dedup_key: None,
            post_delivery_noise: false,
            matched_signals: vec![],
        }
    }

    #[test]
    fn merge_fills_gaps_and_validates_tracking() {
        let llm = LlmDelivery {
            is_shipment_related: true,
            status: Some("in_transit".into()),
            merchant: Some("Ignored".into()),
            carrier: Some("UPS".into()),
            tracking_number: Some("1Z5R89390357567127".into()),
            tracking_url: None,
            order_number: Some("A-9".into()),
            items: vec![LlmItem {
                name: "Cable".into(),
                quantity: Some(2),
            }],
            eta_from: None,
            eta_until: Some("2024-05-10T12:00:00Z".into()),
            confidence: 0.8,
        };
        let merged = merge(base_signal(), llm);
        assert_eq!(merged.decision, Decision::Create);
        assert_eq!(merged.source, DetectionSource::Llm);
        assert_eq!(merged.stage, Some(DeliveryStatus::InTransit));
        assert_eq!(merged.merchant.as_deref(), Some("Acme"), "base merchant kept");
        assert_eq!(merged.carrier.as_deref(), Some("ups"));
        assert_eq!(
            merged.primary_tracking_number().as_deref(),
            Some("1Z5R89390357567127")
        );
        assert_eq!(merged.dedup_key.as_deref(), Some("1Z5R89390357567127"));
        assert_eq!(merged.items.len(), 1);
        assert!(merged.eta_until.is_some());
    }

    #[test]
    fn merge_drops_hallucinated_tracking_number() {
        let llm = LlmDelivery {
            is_shipment_related: true,
            status: Some("in_transit".into()),
            merchant: None,
            carrier: None,
            tracking_number: Some("not-a-real-number-123".into()),
            tracking_url: None,
            order_number: Some("A-9".into()),
            items: vec![],
            eta_from: None,
            eta_until: None,
            confidence: 0.5,
        };
        let merged = merge(base_signal(), llm);
        assert!(merged.tracking_numbers.is_empty(), "checksum-invalid dropped");
        // Falls back to merchant|order dedup.
        assert_eq!(merged.dedup_key.as_deref(), Some("acme|a-9"));
    }
}
