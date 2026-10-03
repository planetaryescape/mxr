//! The one link a to-do is about, picked so the email can be opened with
//! it highlighted.
//!
//! A to-do never opens the link itself. A pay link is the classic phishing
//! payload, and two review rounds found ways around every gate tried
//! (forged authentication headers, primed sender history, subdomains,
//! lookalikes, redirects), so the row's action opens the email in mxr and
//! points at the link there. The one-click link is deferred until a design
//! survives review: docs/issues/one-click-pay-link.md.

use crate::text::floor_char_boundary;
use crate::TodoKind;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkCandidate {
    pub url: String,
    /// The link's text, or the words before a bare URL.
    pub anchor: String,
    pub host: String,
}

/// The registrable domain ("camden.gov.uk" for "www.camden.gov.uk"),
/// lowercased. Falls back to the host itself when the public suffix list
/// doesn't know it (an internal or test host).
pub fn registrable_domain(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some(psl::domain_str(&host).map_or_else(|| host.clone(), str::to_string))
}

/// The registrable domain of an email address.
pub fn email_domain(email: &str) -> Option<String> {
    registrable_domain(email.rsplit_once('@')?.1)
}

static ANCHOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<a\b[^>]*?\bhref\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a>"#)
        .expect("valid anchor regex")
});
static TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<[^>]*>").expect("valid tag regex"));
static BARE_URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"https?://[^\s<>"')\]]+"#).expect("valid url regex"));
static NOT_ACTION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)unsubscribe|opt[\s-]?out|e?mail\s+preferences|privacy|terms|cookie|facebook\.|twitter\.|instagram\.|linkedin\.|youtube\.|tiktok\.|\bx\.com|apps\.apple\.com|play\.google\.com|view\s+(this\s+)?(email\s+)?in\s+(your\s+)?browser",
    )
    .expect("valid not-action regex")
});

fn verb_regex(kind: TodoKind) -> &'static Regex {
    static BILL: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\b(pay(\s+(now|online|here|it|your\s+bill|(your\s+)?invoice|bill))?|make\s+a\s+payment|view\s+(and\s+pay\s+)?(your\s+)?(bill|invoice)|settle)\b")
            .expect("valid bill link regex")
    });
    static FIX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\b(update|fix|retry|try\s+again|payment\s+(method|details)|billing|card\s+details)\b")
            .expect("valid fix link regex")
    });
    static RENEW: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\b(renew|renewal|get\s+(a\s+)?quote|review\s+your\s+(policy|renewal|plan|cover))\b")
            .expect("valid renew link regex")
    });
    static VERIFY: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\b(verify|confirm|activate|validate)\b").expect("valid verify link regex")
    });
    static SIGN: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?i)\b(sign|review\s+(and|&)\s+sign|review\s+document|view\s+document|docusign)\b",
        )
        .expect("valid sign link regex")
    });
    static RSVP: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\b(rsvp|accept|respond|reply|register|attend|let\s+us\s+know)\b")
            .expect("valid rsvp link regex")
    });
    static OTHER: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)\b(confirm|manage|view)\b").expect("valid other link regex"));
    match kind {
        TodoKind::Bill => &BILL,
        TodoKind::PaymentFailed => &FIX,
        TodoKind::Renewal | TodoKind::Document | TodoKind::Lease => &RENEW,
        TodoKind::Verify => &VERIFY,
        TodoKind::Sign => &SIGN,
        TodoKind::Rsvp => &RSVP,
        TodoKind::Return | TodoKind::Promise | TodoKind::Other => &OTHER,
    }
}

/// How far before a bare URL its describing words may sit.
const NEARBY: usize = 80;

/// The link whose words best match what the to-do asks, if any. Links in
/// the HTML win over bare URLs in the text, since their anchor text says
/// what they do.
pub fn pick_link(html: Option<&str>, text: &str, kind: TodoKind) -> Option<LinkCandidate> {
    let verb = verb_regex(kind);
    let mut best: Option<(u8, LinkCandidate)> = None;
    let mut consider = |score: u8, url: &str, anchor: String| {
        if score == 0 || NOT_ACTION.is_match(url) || NOT_ACTION.is_match(&anchor) {
            return;
        }
        let Some(host) = url::Url::parse(url)
            .ok()
            .filter(|parsed| matches!(parsed.scheme(), "https" | "http"))
            .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
        else {
            return;
        };
        if best.as_ref().is_none_or(|(top, _)| score > *top) {
            best = Some((
                score,
                LinkCandidate {
                    url: url.to_string(),
                    anchor,
                    host,
                },
            ));
        }
    };
    if let Some(html) = html {
        for caps in ANCHOR.captures_iter(html) {
            let url = decode_entities(caps.get(1).map_or("", |m| m.as_str()).trim());
            let anchor = TAG.replace_all(caps.get(2).map_or("", |m| m.as_str()), " ");
            let anchor = decode_entities(&anchor.split_whitespace().collect::<Vec<_>>().join(" "));
            let score = if verb.is_match(&anchor) {
                3
            } else {
                u8::from(verb.is_match(&url))
            };
            consider(score, &url, anchor);
        }
    }
    for found in BARE_URL.find_iter(text) {
        let from = floor_char_boundary(text, found.start().saturating_sub(NEARBY));
        let before = last_clause(&text[from..found.start()]);
        let score = if verb.is_match(&before) {
            2
        } else {
            u8::from(verb.is_match(found.as_str()))
        };
        consider(score, found.as_str(), before);
    }
    best.map(|(_, candidate)| candidate)
}

/// The words just before a bare URL, from the last sentence break: "Update
/// your payment details to keep listening".
fn last_clause(before: &str) -> String {
    let clause = before
        .rsplit(['.', '\n', '!', '?'])
        .find(|part| !part.trim().is_empty())
        .unwrap_or(before);
    clause.trim().trim_end_matches(':').trim().to_string()
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

/// The action's words: "Open email to pay". The email opens with the
/// link highlighted; the link is never opened from the row.
pub fn action_label(kind: TodoKind) -> String {
    let verb = match kind {
        TodoKind::Bill => "pay",
        TodoKind::PaymentFailed => "update payment",
        TodoKind::Renewal | TodoKind::Document => "renew",
        TodoKind::Verify => "verify",
        TodoKind::Sign => "sign",
        TodoKind::Rsvp => "reply",
        TodoKind::Lease | TodoKind::Return | TodoKind::Promise | TodoKind::Other => "do it",
    };
    format!("Open email to {verb}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrable_domains_follow_the_public_suffix_list() {
        assert_eq!(
            registrable_domain("www.camden.gov.uk").as_deref(),
            Some("camden.gov.uk")
        );
        assert_eq!(
            registrable_domain("mail.spotify.com").as_deref(),
            Some("spotify.com")
        );
        assert_eq!(
            email_domain("billing@email.octopus.energy").as_deref(),
            Some("octopus.energy")
        );
    }

    #[test]
    fn picks_the_link_whose_words_match_the_verb() {
        let html = r#"<p><a href="https://www.camden.gov.uk/help">Help</a>
            <a href="https://www.camden.gov.uk/pay?ref=1&amp;y=2"><b>Pay now</b></a>
            <a href="https://camden.gov.uk/unsubscribe">Pay less by unsubscribing</a></p>"#;
        let link = pick_link(Some(html), "", TodoKind::Bill).expect("link");
        assert_eq!(link.url, "https://www.camden.gov.uk/pay?ref=1&y=2");
        assert_eq!(link.anchor, "Pay now");
        assert_eq!(link.host, "www.camden.gov.uk");
    }

    #[test]
    fn bare_urls_count_by_the_words_before_them() {
        let text = "To verify your email, open https://octopus.energy/verify/abc within 24 hours.";
        let link = pick_link(None, text, TodoKind::Verify).expect("link");
        assert_eq!(link.url, "https://octopus.energy/verify/abc");
        assert_eq!(
            pick_link(None, "See https://example.com/blog", TodoKind::Bill),
            None
        );
    }

    #[test]
    fn the_action_always_opens_the_email() {
        assert_eq!(action_label(TodoKind::Bill), "Open email to pay");
        assert_eq!(action_label(TodoKind::Verify), "Open email to verify");
        assert_eq!(
            action_label(TodoKind::PaymentFailed),
            "Open email to update payment"
        );
    }
}
