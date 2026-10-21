//! Optional local-LLM confirm + enrich for shortlisted candidates.
//!
//! Runs only on `Decision::ShortlistLlm` candidates (and to enrich items/ETA on
//! high-confidence heuristic creates). Mirrors the codebase's extraction
//! handlers: a strict-JSON system prompt, `serde_json` parse, field validation.
//! Anti-hallucination: any LLM-emitted tracking number is re-validated against
//! the checksum library before it is trusted. Disabled/blocked LLM degrades to
//! the original heuristic signal — never errors the caller.

use crate::{tracking, Decision, DeliverySignal, DeliveryStatus, DetectionSource};
use chrono::{DateTime, Local, Utc};
use mxr_core::natural_time::{due_words, TimePrefs};
use mxr_llm::{
    guarded_system_prompt, wrap_untrusted_mail, ChatMessage, CompletionRequest, FeatureLlmRuntime,
    LlmError,
};
use mxr_store::DeliveryItem;
use serde::Deserialize;

const SYSTEM_PROMPT: &str = "You classify and extract package-delivery details from a single email. \
Output STRICT JSON and nothing else, with this schema:\n\
{\"is_shipment_related\": bool, \
\"status\": \"ordered|info_received|in_transit|out_for_delivery|attempt_fail|available_for_pickup|delivered|exception|returned|expired|null\", \
\"merchant\": str|null, \"carrier\": str|null, \"tracking_number\": str|null, \
\"tracking_url\": str|null, \"order_number\": str|null, \
\"items\": [{\"name\": str, \"quantity\": int|null}], \
\"eta_from\": str|null, \"eta_until\": str|null, \
\"confidence\": 0.0}\n\n\
Rules: set is_shipment_related=true only for emails about a physical package/order \
shipment (order confirmation, shipped, out-for-delivery, delivered, delay/exception). \
Marketing/deals, digital receipts, and subscription emails are NOT shipment-related. \
A code-like string that resembles a tracking number is NOT enough on its own — \
finance, crypto, hosting, security/verification, and SaaS-receipt emails often \
contain reference codes or hashes that look like tracking numbers; these are NOT \
shipments. Only set is_shipment_related=true when the email is clearly from a \
merchant or carrier about a physical parcel the recipient is receiving. \
Copy the tracking number verbatim if present, else null. Do not invent values. \
eta_from and eta_until are the words in the email that say when the parcel \
arrives, copied exactly as written, or null: for example \"arriving Thursday\". \
For a window such as \"between 2pm and 4pm\", copy the start into eta_from and \
the end into eta_until. Never work out a date or time yourself. Return JSON only.";

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
    /// Words the email copies for when, resolved by `due_words`. Never a date.
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
    /// When the email was sent. "Thursday" in it means the Thursday of that
    /// week, not the next one after today.
    pub received_at: DateTime<Utc>,
    /// The user's snooze hours, which the time parser uses for a day named
    /// without a time.
    pub time_prefs: TimePrefs,
}

/// Confirm + enrich a shortlisted candidate. On confirmation, returns a signal
/// with `decision = Create`, deterministic fields preferred and gaps filled
/// from the model. On rejection, returns `Reject`. If the LLM is
/// disabled/blocked/erroring, returns `base` unchanged so the caller can fall
/// back to heuristic-only handling.
///
/// The model copies the words that say when the parcel arrives; those are
/// checked against the text it read and resolved against the send time. Words
/// the text doesn't say leave the field unset.
///
/// `message_key` names the message for the background gate: a message that
/// already failed is not re-sent while it backs off, and one that succeeded
/// is not re-sent at all. `force` (explicit user backfill) skips the gate.
pub async fn enrich(
    runtime: &FeatureLlmRuntime,
    message_key: &str,
    force: bool,
    input: &LlmInput<'_>,
    base: DeliverySignal,
) -> DeliverySignal {
    // A message's content doesn't change, so the key alone identifies it.
    const FINGERPRINT: &str = "";
    if !force && !runtime.background_attempt_due(message_key, FINGERPRINT) {
        return base;
    }
    // From/subject/body are all attacker-controllable; wrap the whole
    // block in untrusted-content delimiters. Strict-JSON parsing plus the
    // checksum re-validation of any tracking number are the boundary.
    let email_block = format!(
        "From: {} <{}>\nSubject: {}\n\nBody:\n{}",
        input.from_name,
        input.from_domain,
        input.subject,
        truncate(input.body_text, 6000),
    );
    let req = CompletionRequest {
        max_tokens: Some(450),
        temperature: Some(0.0),
        messages: vec![
            ChatMessage::system(guarded_system_prompt(SYSTEM_PROMPT)),
            ChatMessage::user(format!(
                "{}\n\nReturn JSON only.",
                wrap_untrusted_mail(&email_block)
            )),
        ],
    };

    let resp = match runtime.complete_background(req).await {
        Ok(r) => r,
        Err(LlmError::Disabled | LlmError::PrivacyBlocked(_) | LlmError::CircuitOpen { .. }) => {
            return base
        }
        Err(error) => {
            runtime.record_background_attempt(message_key, FINGERPRINT, false);
            tracing::warn!(message = message_key, %error, "delivery LLM enrich failed; using heuristic");
            return base;
        }
    };

    let parsed: LlmDelivery = match serde_json::from_str(extract_json(&resp.content)) {
        Ok(p) => p,
        Err(error) => {
            runtime.record_background_attempt(message_key, FINGERPRINT, false);
            tracing::warn!(
                message = message_key,
                %error,
                chars = resp.content.len(),
                finish_reason = ?resp.finish_reason,
                "delivery LLM returned non-JSON; using heuristic"
            );
            return base;
        }
    };
    runtime.record_background_attempt(message_key, FINGERPRINT, true);

    if !parsed.is_shipment_related {
        return DeliverySignal {
            decision: Decision::Reject,
            ..base
        };
    }

    merge(
        base,
        parsed,
        &email_block,
        input.received_at,
        input.time_prefs,
    )
}

/// Merge LLM output into the heuristic signal: deterministic/schema fields win,
/// the LLM only fills gaps. The LLM tracking number is re-validated, and its
/// time words are checked against `text` (the email the model read).
fn merge(
    mut base: DeliverySignal,
    llm: LlmDelivery,
    text: &str,
    received_at: DateTime<Utc>,
    prefs: TimePrefs,
) -> DeliverySignal {
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
    let when = |words: Option<&str>| resolve_when(words, text, received_at, prefs);
    if base.eta_from.is_none() {
        base.eta_from = when(llm.eta_from.as_deref());
    }
    if base.eta_until.is_none() {
        // The parser reads one phrase at a time, so a window's end is resolved
        // on its own. "Thursday" then "4pm" gives Thursday's start and the next
        // 4pm, which is before it; an end before its start is that mismatch, so
        // it is dropped.
        base.eta_until = when(llm.eta_until.as_deref())
            .filter(|until| base.eta_from.is_none_or(|from| *until >= from));
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

/// The instant the model's copied words say, or `None` when the email doesn't
/// contain them or the parser can't read them. The email's own send time is
/// "now" for the words, and the daemon's local zone is used, as the snooze
/// phrases are.
fn resolve_when(
    words: Option<&str>,
    text: &str,
    received_at: DateTime<Utc>,
    prefs: TimePrefs,
) -> Option<DateTime<Utc>> {
    let words = words?;
    due_words(text, words, &received_at.with_timezone(&Local), &prefs)?
        .resolution
        .map(|resolution| resolution.at)
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
    use chrono::{NaiveDate, TimeZone};

    #[test]
    fn extract_json_unwraps_fences() {
        let c = "Here you go:\n```json\n{\"is_shipment_related\": true}\n```\nDone.";
        assert_eq!(extract_json(c), "{\"is_shipment_related\": true}");
    }

    #[tokio::test]
    async fn enrich_prompt_wraps_email_and_guards_system() {
        use mxr_llm::{CompletionResponse, LlmCapabilities, LlmFeature, LlmProvider, LlmRuntime};
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct Capture {
            msgs: Mutex<Vec<ChatMessage>>,
        }
        #[async_trait::async_trait]
        impl LlmProvider for Capture {
            async fn complete(
                &self,
                req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                *self.msgs.lock().expect("msgs lock") = req.messages.clone();
                Ok(CompletionResponse {
                    content: r#"{"is_shipment_related": false}"#.into(),
                    model: "stub".into(),
                    finish_reason: Some("stop".into()),
                })
            }
            fn capabilities(&self) -> LlmCapabilities {
                LlmCapabilities {
                    context_window: 8192,
                    supports_streaming: false,
                }
            }
            fn model_name(&self) -> &str {
                "stub"
            }
        }

        let cap = Arc::new(Capture::default());
        let runtime = Arc::new(LlmRuntime::new(cap.clone() as Arc<dyn LlmProvider>));
        let feature = runtime.for_feature(LlmFeature::DeliveryExtraction);
        let input = LlmInput {
            from_name: "Shop",
            from_domain: "shop.example",
            subject: "Your order",
            body_text: "PACKAGE-BODY-MARKER shipped today",
            received_at: sent(),
            time_prefs: TimePrefs::default(),
        };

        let _ = enrich(&feature, "msg-1", false, &input, base_signal()).await;

        let msgs = cap.msgs.lock().expect("msgs lock");
        assert!(
            msgs[0].content.contains(mxr_llm::UNTRUSTED_MAIL_GUARD),
            "system prompt must carry the shared injection guard"
        );
        let user = &msgs[1].content;
        let begin = user
            .find(mxr_llm::UNTRUSTED_MAIL_BEGIN)
            .expect("begin marker present");
        let end = user
            .find(mxr_llm::UNTRUSTED_MAIL_END)
            .expect("end marker present");
        let body = user
            .find("PACKAGE-BODY-MARKER")
            .expect("email body present");
        assert!(
            begin < body && body < end,
            "email content must sit between the untrusted-content markers"
        );
    }

    /// Monday 6 May 2024, 10:00 UTC: the email's send time in the tests below.
    fn sent() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 5, 6, 10, 0, 0).unwrap()
    }

    fn local_date(at: DateTime<Utc>) -> NaiveDate {
        at.with_timezone(&Local).date_naive()
    }

    fn llm_when(eta_from: Option<&str>, eta_until: Option<&str>) -> LlmDelivery {
        LlmDelivery {
            is_shipment_related: true,
            status: None,
            merchant: None,
            carrier: None,
            tracking_number: None,
            tracking_url: None,
            order_number: None,
            items: vec![],
            eta_from: eta_from.map(str::to_string),
            eta_until: eta_until.map(str::to_string),
            confidence: 0.8,
        }
    }

    fn when(llm: LlmDelivery, text: &str) -> DeliverySignal {
        merge(base_signal(), llm, text, sent(), TimePrefs::default())
    }

    #[test]
    fn eta_words_the_email_says_resolve_against_the_email_date() {
        // An old email's "Thursday" is the Thursday of its own week.
        let old = Utc.with_ymd_and_hms(2024, 3, 5, 10, 0, 0).unwrap();
        let merged = merge(
            base_signal(),
            llm_when(Some("arriving Thursday"), None),
            "Your parcel is arriving Thursday.",
            old,
            TimePrefs::default(),
        );
        assert_eq!(
            local_date(merged.eta_from.expect("resolved")),
            NaiveDate::from_ymd_opt(2024, 3, 7).expect("valid date")
        );
        assert_eq!(merged.eta_until, None);
    }

    #[test]
    fn words_the_email_does_not_contain_leave_the_field_unset() {
        let merged = when(
            llm_when(Some("arriving Thursday"), None),
            "Your parcel is on its way.",
        );
        assert_eq!(merged.eta_from, None);
    }

    #[test]
    fn an_rfc3339_date_the_email_does_not_contain_is_not_used() {
        let merged = when(
            llm_when(None, Some("2024-05-10T12:00:00Z")),
            "Your parcel is arriving Friday.",
        );
        assert_eq!(merged.eta_until, None);
    }

    #[test]
    fn words_the_parser_cannot_read_leave_the_field_unset() {
        let merged = when(
            llm_when(Some("once the warehouse reopens"), None),
            "Your parcel ships once the warehouse reopens.",
        );
        assert_eq!(merged.eta_from, None);
    }

    #[test]
    fn a_window_resolves_each_end_from_its_own_words() {
        let merged = when(
            llm_when(Some("2pm"), Some("4pm")),
            "Arriving today between 2pm and 4pm.",
        );
        let (from, until) = (
            merged.eta_from.expect("start"),
            merged.eta_until.expect("end"),
        );
        assert_eq!(until - from, chrono::Duration::hours(2));
    }

    #[test]
    fn a_window_end_before_its_start_is_dropped() {
        // "4pm" alone reads as the next 4pm, which is before Thursday's
        // start: the day did not carry across, so the end is not stored.
        let merged = when(
            llm_when(Some("arriving Thursday"), Some("4pm")),
            "Arriving Thursday between 2pm and 4pm.",
        );
        assert!(merged.eta_from.is_some());
        assert_eq!(merged.eta_until, None);
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
            eta_until: Some("arriving Friday".into()),
            confidence: 0.8,
        };
        let merged = merge(
            base_signal(),
            llm,
            "Your parcel is arriving Friday.",
            sent(),
            TimePrefs::default(),
        );
        assert_eq!(merged.decision, Decision::Create);
        assert_eq!(merged.source, DetectionSource::Llm);
        assert_eq!(merged.stage, Some(DeliveryStatus::InTransit));
        assert_eq!(
            merged.merchant.as_deref(),
            Some("Acme"),
            "base merchant kept"
        );
        assert_eq!(merged.carrier.as_deref(), Some("ups"));
        assert_eq!(
            merged.primary_tracking_number().as_deref(),
            Some("1Z5R89390357567127")
        );
        assert_eq!(merged.dedup_key.as_deref(), Some("1Z5R89390357567127"));
        assert_eq!(merged.items.len(), 1);
        assert_eq!(
            local_date(merged.eta_until.expect("eta from the email's words")),
            NaiveDate::from_ymd_opt(2024, 5, 10).expect("valid date"),
            "Friday of the week the email was sent"
        );
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
        let merged = merge(base_signal(), llm, "", sent(), TimePrefs::default());
        assert!(
            merged.tracking_numbers.is_empty(),
            "checksum-invalid dropped"
        );
        // Falls back to merchant|order dedup.
        assert_eq!(merged.dedup_key.as_deref(), Some("acme|a-9"));
    }

    #[tokio::test]
    async fn failed_message_is_not_resent_until_forced() {
        use mxr_llm::{CompletionResponse, LlmCapabilities, LlmFeature, LlmProvider, LlmRuntime};
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        #[derive(Default)]
        struct NotJson {
            calls: AtomicU32,
        }
        #[async_trait::async_trait]
        impl LlmProvider for NotJson {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> std::result::Result<CompletionResponse, LlmError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(CompletionResponse {
                    content: "Sorry, I can't tell.".into(),
                    model: "stub".into(),
                    finish_reason: Some("stop".into()),
                })
            }
            fn capabilities(&self) -> LlmCapabilities {
                LlmCapabilities {
                    context_window: 8192,
                    supports_streaming: false,
                }
            }
            fn model_name(&self) -> &str {
                "stub"
            }
        }

        let provider = Arc::new(NotJson::default());
        let runtime = Arc::new(LlmRuntime::new(provider.clone() as Arc<dyn LlmProvider>));
        let feature = runtime.for_feature(LlmFeature::DeliveryExtraction);
        let input = LlmInput {
            from_name: "Shop",
            from_domain: "shop.example",
            subject: "Your order",
            body_text: "shipped today",
            received_at: sent(),
            time_prefs: TimePrefs::default(),
        };

        for _ in 0..3 {
            let signal = enrich(&feature, "msg-1", false, &input, base_signal()).await;
            assert_eq!(
                signal.decision,
                Decision::ShortlistLlm,
                "falls back to heuristic"
            );
        }
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);

        let _ = enrich(&feature, "msg-2", false, &input, base_signal()).await;
        let _ = enrich(&feature, "msg-1", true, &input, base_signal()).await;
        assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    }
}
