//! Which kind of update a message is, from its subject alone: the subject
//! with numbers, ids, dates, times, quoted strings, addresses and links
//! masked. Two weekly summaries from Strava share a template; a weekly
//! summary and a new-follower note don't. No model.

use once_cell::sync::Lazy;
use regex::Regex;

/// "Re:", "Fwd:" and notification prefixes, repeated.
static PREFIX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*(?:(?:re|fwd?|aw|sv|notification|alert|reminder)\s*:\s*)+")
        .expect("valid prefix regex")
});

/// A leading "[acme/api]" or "[Strava]" tag.
static TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*\[[^\]]{1,60}\]\s*").expect("valid tag regex"));

/// The subject a person would read: prefixes and leading tags gone, a
/// brand prefix ("Strava: ") or suffix (" - Strava", " | Vercel") naming
/// the source dropped, whitespace collapsed.
pub fn clean_subject(subject: &str, source_name: &str) -> String {
    let mut text = PREFIX.replace(subject, "").to_string();
    loop {
        let stripped = TAG.replace(&text, "").to_string();
        let stripped = PREFIX.replace(&stripped, "").to_string();
        if stripped == text {
            break;
        }
        text = stripped;
    }
    let brand = source_name
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase();
    if !brand.is_empty() {
        let lower = text.to_lowercase();
        if let Some(rest) = lower.strip_prefix(&brand) {
            if let Some(after) = rest.strip_prefix(':').or_else(|| rest.strip_prefix(" -")) {
                // Lowercasing can change byte lengths; cut only when it didn't.
                let start = text.len() - after.len();
                if lower.len() == text.len() && text.is_char_boundary(start) {
                    text = text[start..].to_string();
                }
            }
        }
        let lower = text.to_lowercase();
        for separator in [" - ", " | ", " · ", " — "] {
            let suffix = format!("{separator}{brand}");
            if lower.ends_with(&suffix) && lower.len() == text.len() && text.len() > suffix.len() {
                let keep = text.len() - suffix.len();
                if text.is_char_boundary(keep) {
                    text.truncate(keep);
                }
                break;
            }
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

static MASKS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    let month = r"(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)";
    let rules: Vec<(String, &'static str)> = vec![
        (r#"https?://\S+"#.into(), "<url>"),
        (r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+".into(), "<email>"),
        (r#""[^"]{1,80}"|“[^”]{1,80}”|‘[^’]{1,80}’"#.into(), "<q>"),
        (r"\b\d{4}-\d{2}-\d{2}(?:[t ]\d{2}:\d{2}(?::\d{2})?z?)?\b".into(), "<date>"),
        (
            r"\b\d{1,2}/\d{1,2}(?:/\d{2,4})?\b|\b\d{1,2}\.\d{1,2}\.\d{2,4}\b".into(),
            "<date>",
        ),
        (
            format!(r"\b(?:\d{{1,2}}(?:st|nd|rd|th)?\s+(?:of\s+)?{month}|{month}\.?\s+\d{{1,2}}(?:st|nd|rd|th)?)(?:,?\s+\d{{4}})?\b"),
            "<date>",
        ),
        (format!(r"\b{month}\b(?:\s+\d{{4}})?"), "<date>"),
        (
            r"\b(?:mon|tue|wed|thu|fri|sat|sun)(?:day|sday|nesday|rsday|urday)?\b".into(),
            "<day>",
        ),
        (r"\b\d{1,2}:\d{2}(?::\d{2})?\s*(?:[ap]\.?m\.?)?\b".into(), "<time>"),
        (r"\b\d{1,2}\s*[ap]\.?m\.?\b".into(), "<time>"),
        // Ids: hashes, long numbers and tokens mixing letters and digits.
        (r"#\s?[\w-]*\d[\w-]*".into(), "<id>"),
        (r"\b(?:[a-z]+\d|\d+[a-z])[a-z0-9-]*\b".into(), "<id>"),
        (r"\b\d{5,}\b".into(), "<id>"),
        // Money and plain numbers, with a sign.
        (r"[-+]?[£$€¥₹]\s?\d[\d,]*(?:\.\d+)?".into(), "<n>"),
        (r"[-+]?\b\d[\d,]*(?:\.\d+)?\b".into(), "<n>"),
    ];
    rules
        .into_iter()
        .map(|(pattern, mask)| {
            (
                Regex::new(&format!("(?i){pattern}")).expect("valid mask regex"),
                mask,
            )
        })
        .collect()
});

/// The subject with everything that varies between two messages of the
/// same kind masked, lowercased: "your week: <n> runs, <n> km".
pub fn template_key(subject: &str) -> String {
    let mut text = PREFIX.replace(subject, "").to_string();
    loop {
        let stripped = TAG.replace(&text, "").to_string();
        let stripped = PREFIX.replace(&stripped, "").to_string();
        if stripped == text {
            break;
        }
        text = stripped;
    }
    let mut text = text.to_lowercase();
    for (pattern, mask) in MASKS.iter() {
        text = pattern.replace_all(&text, *mask).into_owned();
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_prefixes_tags_and_the_brand() {
        assert_eq!(
            clean_subject("Re: [acme/api] Run failed: CI - main", "GitHub acme/api"),
            "Run failed: CI - main"
        );
        assert_eq!(clean_subject("Strava: Your week in review", "Strava"), "Your week in review");
        assert_eq!(
            clean_subject("Deployment succeeded | Vercel", "Vercel"),
            "Deployment succeeded"
        );
        assert_eq!(clean_subject("Notification: New sign-in", "Google"), "New sign-in");
    }

    // Fixtures shaped on the automated senders the research names: the
    // same kind of update must share a template (merge), different kinds
    // must not (split).
    #[test]
    fn same_kind_merges_across_numbers_ids_and_dates() {
        let merge = [
            ("Your week: 3 runs, 21.3 km", "Your week: 5 runs, 40.1 km"),
            (
                "[acme/api] Run failed: CI - main (a1b2c3d)",
                "[acme/api] Run failed: CI - main (9f8e7d6)",
            ),
            (
                "Your order #112-3345 has shipped",
                "Your order #987-0001 has shipped",
            ),
            (
                "Payout of R 4,210.00 scheduled for 12 Oct",
                "Payout of R 980.50 scheduled for October 3, 2026",
            ),
            ("Weekly report: 1,204 visitors", "Weekly report: 998 visitors"),
            (
                "Your verification code is 482913",
                "Your verification code is 100293",
            ),
            (
                "New sign-in from Chrome on Windows at 06:12",
                "New sign-in from Chrome on Windows at 11:40pm",
            ),
            (
                "Statement for \"Main account\" ready Monday",
                "Statement for \"Savings\" ready Friday",
            ),
            (
                "Invoice INV-2026-0042 for alice@example.com",
                "Invoice INV-2026-0043 for bob@example.com",
            ),
        ];
        for (a, b) in merge {
            assert_eq!(template_key(a), template_key(b), "{a} vs {b}");
        }
    }

    #[test]
    fn different_kinds_split() {
        let split = [
            ("Your week: 3 runs, 21.3 km", "New follower on Strava"),
            ("[acme/api] Run failed: CI - main", "[acme/api] Run succeeded: CI - main"),
            ("Your order has shipped", "Your order was delivered"),
            ("Deployment succeeded", "Deployment failed"),
        ];
        for (a, b) in split {
            assert_ne!(template_key(a), template_key(b), "{a} vs {b}");
        }
        assert_eq!(template_key("Your week: 3 runs, 21.3 km"), "your week: <n> runs, <n> km");
    }
}
