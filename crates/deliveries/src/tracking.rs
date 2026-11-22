//! Tracking-number extraction. Wraps the `tracking-numbers` crate, which
//! embeds the canonical carrier dataset and validates checksums (Mod10, Mod7,
//! Luhn, S10). We surface only numbers that pass validation — the biggest
//! precision lever against order/invoice/phone-number collisions.

use crate::data::normalize_carrier;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedTracking {
    pub number: String,
    pub carrier: String,
    pub tracking_url: Option<String>,
}

static CONTIGUOUS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)[0-9A-Z]{8,35}").expect("valid contiguous-token regex"));

static GROUPED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:[0-9A-Z]{1,6}[ \-]){2,}[0-9A-Z]{1,6}").expect("valid grouped-token regex")
});

/// Extract and validate tracking numbers from free text (subject + body).
/// Returns only checksum-valid numbers, de-duplicated by normalized number.
pub fn extract(text: &str) -> Vec<ValidatedTracking> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let candidates = CONTIGUOUS
        .find_iter(text)
        .chain(GROUPED.find_iter(text))
        .map(|m| m.as_str());
    for raw in candidates {
        let normalized: String = raw
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_uppercase();
        if normalized.len() < 8
            || normalized.len() > 35
            || !normalized.chars().any(|c| c.is_ascii_digit())
            || !seen.insert(normalized.clone())
        {
            continue;
        }
        if let Some(r) = tracking_numbers::track(&normalized) {
            out.push(ValidatedTracking {
                number: r.tracking_number,
                carrier: normalize_carrier(&r.courier),
                tracking_url: non_empty(r.tracking_url),
            });
        }
    }
    out
}

fn non_empty(s: String) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_real_ups_and_usps_numbers() {
        let text = "Your UPS tracking is 1Z5R89390357567127 and USPS \
                    9400111899560438600329 — thanks!";
        let found = extract(text);
        let carriers: Vec<&str> = found.iter().map(|t| t.carrier.as_str()).collect();
        assert!(carriers.contains(&"ups"), "ups not found in {found:?}");
        assert!(carriers.contains(&"usps"), "usps not found in {found:?}");
    }

    #[test]
    fn rejects_invalid_checksum_tracking() {
        let valid_flipped = "ref 1Z5R89390357567128 here";
        assert!(
            extract(valid_flipped).is_empty(),
            "{:?}",
            extract(valid_flipped)
        );
        assert!(extract("just some words, no numbers").is_empty());
    }
}
