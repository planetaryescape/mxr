//! Whether a message's sender is who it says, from the receiving
//! provider's own verdict. A breakthrough to To do rests on this, so an
//! alert anyone could write ("New sign-in" from security@evil.example)
//! stays in Needs a look.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::text::registrable_domain;

static DMARC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bdmarc\s*=\s*([a-z]+)").expect("valid dmarc regex"));
static HEADER_FROM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)header\.from\s*=\s*([a-z0-9.\-]+)").expect("valid header.from regex")
});

/// Whether DMARC passed for `sender_domain`, read only from a result the
/// receiving provider added.
///
/// Anyone can put an `Authentication-Results` header in a message, so only
/// the topmost one whose authserv-id is in `trusted_ids` counts (receivers
/// prepend theirs, so a forged one sits below it); with no trusted id it
/// fails. That result must say `dmarc=pass` and name the sender's domain
/// in `header.from`. An id starting with `*.` matches any host under it.
pub fn dmarc_passes(auth_results: &[String], sender_domain: &str, trusted_ids: &[String]) -> bool {
    let Some(header) = auth_results
        .iter()
        .find(|header| trusted_ids.iter().any(|id| authserv_matches(header, id)))
    else {
        return false;
    };
    let passed = DMARC
        .captures(header)
        .and_then(|caps| caps.get(1))
        .is_some_and(|result| result.as_str().eq_ignore_ascii_case("pass"));
    passed
        && HEADER_FROM
            .captures(header)
            .and_then(|caps| caps.get(1))
            .is_some_and(|from| registrable_domain(from.as_str()).as_deref() == Some(sender_domain))
}

/// The authserv-id is the header's first token, before any `;`.
fn authserv_matches(header: &str, trusted: &str) -> bool {
    let Some(id) = header
        .split(';')
        .next()
        .and_then(|first| first.split_whitespace().next())
    else {
        return false;
    };
    let id = id.to_ascii_lowercase();
    let trusted = trusted.trim().to_ascii_lowercase();
    match trusted.strip_prefix("*.") {
        Some(parent) => id.ends_with(&format!(".{parent}")),
        None => id == trusted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn google() -> Vec<String> {
        vec!["mx.google.com".to_string()]
    }

    #[test]
    fn reads_only_the_receiving_providers_result_for_the_sender() {
        let real = "mx.google.com; dkim=pass header.i=@google.com; spf=pass; dmarc=pass (p=REJECT) header.from=accounts.google.com".to_string();
        assert!(dmarc_passes(
            std::slice::from_ref(&real),
            "google.com",
            &google()
        ));
        assert!(!dmarc_passes(
            std::slice::from_ref(&real),
            "evil.example",
            &google()
        ));
        let fail = "mx.google.com; dmarc=fail header.from=google.com".to_string();
        assert!(!dmarc_passes(&[fail], "google.com", &google()));
        assert!(!dmarc_passes(&[], "google.com", &google()));
        assert!(
            !dmarc_passes(&[real], "google.com", &[]),
            "no trusted id, no pass"
        );
    }

    #[test]
    fn a_forged_header_never_counts() {
        let provider = "mx.google.com; dmarc=fail (p=NONE) header.from=evil.example".to_string();
        let forged = "mx.google.com; dmarc=pass header.from=evil.example".to_string();
        assert!(!dmarc_passes(
            &[provider, forged],
            "evil.example",
            &google()
        ));
        let elsewhere = "evil.example; dmarc=pass header.from=evil.example".to_string();
        assert!(!dmarc_passes(&[elsewhere], "evil.example", &google()));
        let wildcard = vec!["*.fastmail.com".to_string()];
        let ok = "mx3.fastmail.com; dmarc=pass header.from=stripe.com".to_string();
        assert!(dmarc_passes(&[ok], "stripe.com", &wildcard));
    }
}
