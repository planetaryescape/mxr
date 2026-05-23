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

// Contiguous alphanumeric run — catches numbers embedded in prose
// ("USPS 9400111899560438600329 ships today").
static CONTIGUOUS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)[0-9A-Z]{8,35}").unwrap());

// Space/hyphen-grouped run of short chunks — catches numbers emails wrap into
// groups ("1Z 5R8 939 03 5756 7127"). Bounded chunk length (<=6) so it does
// not greedily merge whole sentences of words.
static GROUPED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(?:[0-9A-Z]{1,6}[ \-]){2,}[0-9A-Z]{1,6}").unwrap());

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
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_uppercase();
        // Real tracking numbers contain digits and fit known formats; this
        // skips pure-word matches and absurdly long concatenations cheaply.
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
        // Valid fixtures from the tracking_number_data dataset.
        let text = "Your UPS tracking is 1Z5R89390357567127 and USPS \
                    9400111899560438600329 — thanks!";
        let found = extract(text);
        let carriers: Vec<&str> = found.iter().map(|t| t.carrier.as_str()).collect();
        assert!(carriers.contains(&"ups"), "ups not found in {found:?}");
        assert!(carriers.contains(&"usps"), "usps not found in {found:?}");
    }

    #[test]
    fn rejects_invalid_checksum_tracking() {
        // A UPS number with a corrupted check digit must be rejected — the
        // checksum gate is what keeps precision high. (Note: some bare
        // digit strings legitimately pass FedEx/USPS checksums; precision for
        // those is enforced downstream by sender/subject corroboration.)
        let valid_flipped = "ref 1Z5R89390357567128 here"; // last digit 7 -> 8
        assert!(extract(valid_flipped).is_empty(), "{:?}", extract(valid_flipped));
        assert!(extract("just some words, no numbers").is_empty());
    }

    #[test]
    fn validates_amazon_tba() {
        let found = extract("Your Amazon package TBA619632698000 ships today");
        assert!(
            found.iter().any(|t| t.number == "TBA619632698000" && t.carrier == "amazon"),
            "{found:?}"
        );
    }

    #[test]
    fn tolerates_spaces_in_grouped_numbers() {
        let text = "Tracking: 1Z 5R8 939 03 5756 7127";
        let found = extract(text);
        assert!(
            found.iter().any(|t| t.carrier == "ups"),
            "spaced UPS not recovered: {found:?}"
        );
    }

}
