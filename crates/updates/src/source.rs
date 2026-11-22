//! The source an update belongs to: the stream a sender's mail forms.
//!
//! The key is the sender's registrable domain, narrowed by a repository or
//! project token when the subject carries one ("[acme/api]" from GitHub),
//! else by the `List-Id`, so one GitHub account with three repositories is
//! three sources and one bank is one.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::text::{capitalise, registrable_domain};

/// "[acme/api]" or "[acme/api] " at the start of a subject: a repository.
static REPO_TAG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*(?:(?:re|fwd?):\s*)*\[([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)\]")
        .expect("valid repo tag regex")
});

/// The registrable domain of an address: "github.com" for
/// "notifications@github.com", "camden.gov.uk" for a subdomain sender.
pub fn email_domain(email: &str) -> Option<String> {
    registrable_domain(email.trim().rsplit_once('@')?.1.trim_end_matches('>'))
}

pub fn source_key(from_email: &str, list_id: Option<&str>, subject: &str) -> String {
    let domain = email_domain(from_email).unwrap_or_else(|| from_email.trim().to_ascii_lowercase());
    if let Some(repo) = REPO_TAG.captures(subject).and_then(|c| c.get(1)) {
        return format!("{domain}/{}", repo.as_str().to_ascii_lowercase());
    }
    match list_id.map(list_label).filter(|label| !label.is_empty()) {
        Some(label) if label != domain && !label.ends_with(&format!(".{domain}")) => {
            format!("{domain}#{label}")
        }
        _ => domain,
    }
}

/// "github.com" from "<acme/api.github.com>" style ids: the id inside the
/// angle brackets, lowercased.
fn list_label(list_id: &str) -> String {
    let inner = list_id
        .rsplit_once('<')
        .map_or(list_id, |(_, rest)| rest.trim_end_matches('>'));
    inner.trim().to_ascii_lowercase()
}

/// How the source is named: the sender's display name when it is a name
/// rather than an address, else the domain's first label capitalised; a
/// repository is added after the name ("GitHub acme/api").
pub fn source_name(from_name: Option<&str>, from_email: &str, key: &str) -> String {
    let base = from_name
        .map(str::trim)
        .map(|name| name.trim_matches('"'))
        .filter(|name| !name.is_empty() && !name.contains('@'))
        .map(strip_notification_suffix)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            let domain = email_domain(from_email).unwrap_or_default();
            capitalise(domain.split('.').next().unwrap_or(&domain))
        });
    match key.split_once('/') {
        Some((_, repo)) if !base.contains(repo) => format!("{base} {repo}"),
        _ => base,
    }
}

fn strip_notification_suffix(name: &str) -> String {
    const SUFFIXES: &[&str] = &[
        " notifications",
        " notification",
        " alerts",
        " no-reply",
        " noreply",
        " updates",
    ];
    let lower = name.to_ascii_lowercase();
    SUFFIXES
        .iter()
        .find(|suffix| lower.ends_with(*suffix) && name.len() > suffix.len())
        .map_or_else(
            || name.to_string(),
            |suffix| name[..name.len() - suffix.len()].trim().to_string(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_by_registrable_domain_narrowed_by_repo_or_list() {
        assert_eq!(
            source_key("noreply@strava.com", None, "Your week"),
            "strava.com"
        );
        assert_eq!(
            source_key("alerts@mail.camden.gov.uk", None, "Council tax"),
            "camden.gov.uk"
        );
        assert_eq!(
            source_key(
                "notifications@github.com",
                Some("acme/api <api.acme.github.com>"),
                "[acme/api] Run failed: CI"
            ),
            "github.com/acme/api"
        );
        assert_eq!(
            source_key("Re@x.io", None, "Re: [Acme/Web] PR merged"),
            "x.io/acme/web"
        );
        assert_eq!(
            source_key(
                "news@sender.example",
                Some("<weekly.lists.other.example>"),
                "Hello"
            ),
            "sender.example#weekly.lists.other.example"
        );
        assert_eq!(
            source_key("news@stripe.com", Some("<alerts.stripe.com>"), "Hello"),
            "stripe.com"
        );
    }
}
