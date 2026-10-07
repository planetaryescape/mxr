//! What a charge is for, and whether an email ends a subscription.
//!
//! The product is what is left of a record's title once the issuer, the
//! dates, the reference numbers and the receipt words are gone: "Spotify
//! Premium receipt, March 2025" is "Premium". Nothing left means the title
//! names no product, and the amount tells charges apart instead.

use once_cell::sync::Lazy;
use regex::Regex;

/// Words that say what kind of email it is, not what was bought.
const NOT_PRODUCT: &[&str] = &[
    "a",
    "an",
    "and",
    "account",
    "annual",
    "annually",
    "available",
    "been",
    "bill",
    "billing",
    "charge",
    "charged",
    "confirmation",
    "confirmed",
    "dated",
    "due",
    "for",
    "from",
    "has",
    "id",
    "inv",
    "is",
    "invoice",
    "invoices",
    "membership",
    "month",
    "monthly",
    "new",
    "no",
    "number",
    "of",
    "on",
    "order",
    "paid",
    "payment",
    "payments",
    "plan",
    "processed",
    "purchase",
    "quarterly",
    "ready",
    "receipt",
    "receipts",
    "ref",
    "reference",
    "renewal",
    "renewed",
    "renews",
    "statement",
    "subscription",
    "subscriptions",
    "thank",
    "thanks",
    "the",
    "to",
    "week",
    "weekly",
    "year",
    "yearly",
    "you",
    "your",
];

const MONTHS: &[&str] = &[
    "jan",
    "january",
    "feb",
    "february",
    "mar",
    "march",
    "apr",
    "april",
    "may",
    "jun",
    "june",
    "jul",
    "july",
    "aug",
    "august",
    "sep",
    "sept",
    "september",
    "oct",
    "october",
    "nov",
    "november",
    "dec",
    "december",
    "mon",
    "monday",
    "tue",
    "tues",
    "tuesday",
    "wed",
    "wednesday",
    "thu",
    "thurs",
    "thursday",
    "fri",
    "friday",
    "sat",
    "saturday",
    "sun",
    "sunday",
];

static TOKEN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[\p{L}\p{N}][\p{L}\p{N}+&'.]*").expect("valid token regex"));

/// The product a title names, as shown ("Premium") and as a key
/// ("premium"), or `None` when the title names only the issuer and the
/// kind of email.
pub fn product(title: Option<&str>, issuer: Option<&str>) -> Option<(String, String)> {
    let title = title?;
    let issuer_words: Vec<String> = issuer
        .map(|issuer| {
            TOKEN
                .find_iter(issuer)
                .map(|m| normalise(m.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let kept: Vec<&str> = TOKEN
        .find_iter(title)
        .map(|m| m.as_str().trim_end_matches(['.', '\'']))
        .filter(|word| {
            let key = normalise(word);
            !key.is_empty()
                && !NOT_PRODUCT.contains(&key.as_str())
                && !MONTHS.contains(&key.as_str())
                && !issuer_words.contains(&key)
                && !looks_like_a_number(&key)
        })
        .collect();
    if kept.is_empty() {
        return None;
    }
    let shown = kept.join(" ");
    let key = kept
        .iter()
        .map(|word| normalise(word))
        .collect::<Vec<_>>()
        .join(" ");
    Some((shown, key))
}

fn normalise(word: &str) -> String {
    word.trim_end_matches(['.', '\'']).to_lowercase()
}

/// A year, a day, an invoice number: digits that change every month.
/// "200gb" stays, since it names the plan.
fn looks_like_a_number(word: &str) -> bool {
    let digits = word.chars().filter(char::is_ascii_digit).count();
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    digits > 0 && (letters == 0 || digits >= 4 || (letters <= 2 && is_ordinal(word)))
}

fn is_ordinal(word: &str) -> bool {
    ["st", "nd", "rd", "th"]
        .iter()
        .any(|suffix| word.ends_with(suffix))
}

/// A subject that says a subscription has been cancelled or has ended.
/// Threats ("will be cancelled unless") and failed payments are not
/// endings: the charge may still come.
static CANCELLED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(?:subscription|membership|plan|trial)\b[^.]{0,40}\b(?:has been |was |is )?(?:cancell?ed|ended|has ended|terminated)\b|\bcancell?ation (?:confirmed|confirmation|complete)\b|\byou(?:'ve| have) cancell?ed\b|\bsorry to see you go\b|\bwe'?ve cancell?ed your\b",
    )
    .expect("valid cancellation regex")
});

static NOT_AN_ENDING: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\bwill be\b|\bunless\b|\bfail|\bdeclin|\bupdate your payment\b|\baction required\b|\babout to\b|\bsoon\b|\breminder\b|\bundo\b|\bbooking\b|\breservation\b|\border\b",
    )
    .expect("valid not-an-ending regex")
});

pub fn is_cancellation(subject: &str) -> bool {
    CANCELLED.is_match(subject) && !NOT_AN_ENDING.is_match(subject)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_is_the_title_without_issuer_dates_and_receipt_words() {
        assert_eq!(
            product(Some("Spotify Premium receipt, March 2025"), Some("Spotify")),
            Some(("Premium".to_string(), "premium".to_string()))
        );
        assert_eq!(
            product(Some("iCloud+ 200GB invoice 2025-03-01"), Some("Apple")),
            Some(("iCloud+ 200GB".to_string(), "icloud+ 200gb".to_string()))
        );
        assert_eq!(product(Some("Netflix"), Some("Netflix")), None);
        assert_eq!(
            product(Some("Invoice INV-20250301 for 1st March"), Some("Linear")),
            None
        );
    }

    #[test]
    fn a_cancellation_is_an_ending_not_a_threat() {
        assert!(is_cancellation("Your subscription has been cancelled"));
        assert!(is_cancellation("Your Premium membership has ended"));
        assert!(is_cancellation("Cancellation confirmed"));
        assert!(!is_cancellation(
            "Your subscription will be cancelled unless you update your payment"
        ));
        assert!(!is_cancellation(
            "Payment failed: subscription cancelled soon"
        ));
        assert!(!is_cancellation("Your booking was cancelled"));
    }
}
