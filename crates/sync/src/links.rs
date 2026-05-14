//! Extract external link counts from message bodies for the tri-state link
//! indicator (`Envelope::link_density`) and the `has:link*` search filters.
//!
//! Counts URLs in `text_plain` and `text_html` and subtracts hostnames that
//! look like list-management / unsubscribe / tracking infrastructure — those
//! show up in nearly every newsletter and would make the indicator useless if
//! left in.
//!
//! Also computes a coarse body word count (whitespace-split, plain-text
//! preferred) used as the denominator for the heavy-density tier.

use mxr_core::types::MessageBody;
use std::sync::OnceLock;

/// Hostname suffixes that should NOT be counted toward the visible link
/// total. Suffix-matched against the URL host, lowercased. Keep the list
/// short and conservative — false positives here mean genuine links get
/// hidden from the indicator.
const TRACKER_HOSTNAME_SUFFIXES: &[&str] = &[
    "unsubscribe.",
    "list-manage.",
    "list-unsubscribe.",
    "mailtrack.",
    "click.",
    "links.",
    "tracking.",
    "track.",
    "open.",
    "pixel.",
    "beacon.",
    // Common ESP infrastructure:
    "sendgrid.",
    "mailgun.",
    "mailchimp.",
    "constantcontact.",
    "campaign-archive.",
    "amazonses.",
    "hubspotemail.",
    "hubspotlinks.",
    "hs-mail.",
];

#[derive(Debug, Clone, Copy, Default)]
pub struct BodyLinkMetrics {
    /// External URLs that survived the deny-list filter.
    pub link_count: u32,
    /// Whitespace-split word count of the plain-text body (or stripped HTML).
    pub body_word_count: u32,
}

/// Walk the message body and produce link + word counts. Prefers `text_plain`
/// for both passes; when only HTML is present, counts URLs against the raw
/// HTML (so `href="..."` attribute URLs are visible) and counts words against
/// a cheap tag-stripped version.
pub fn body_link_metrics(body: &MessageBody) -> BodyLinkMetrics {
    let plain = body.text_plain.as_deref().unwrap_or("");
    if !plain.is_empty() {
        return BodyLinkMetrics {
            link_count: count_external_urls(plain),
            body_word_count: word_count(plain),
        };
    }
    let Some(html) = body.text_html.as_deref() else {
        return BodyLinkMetrics::default();
    };
    let link_count = count_external_urls(html);
    let stripped = strip_html_tags(html);
    BodyLinkMetrics {
        link_count,
        body_word_count: word_count(&stripped),
    }
}

fn word_count(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

fn count_external_urls(text: &str) -> u32 {
    let mut count: u32 = 0;
    let lower = text.to_ascii_lowercase();
    let mut cursor = 0usize;
    while cursor < lower.len() {
        let Some(rel) = lower[cursor..].find("http") else {
            break;
        };
        let start = cursor + rel;
        let after_http = &lower[start..];
        let scheme_len = if after_http.starts_with("https://") {
            8
        } else if after_http.starts_with("http://") {
            7
        } else {
            cursor = start + 4;
            continue;
        };
        let host_start = start + scheme_len;
        // Find the end of the host (first delimiter).
        let host_end = lower[host_start..]
            .find(|ch: char| {
                ch.is_ascii_whitespace()
                    || matches!(
                        ch,
                        '/' | '?' | '#' | '>' | '<' | '"' | '\'' | ')' | ']' | '}'
                    )
            })
            .map(|offset| host_start + offset)
            .unwrap_or(lower.len());
        if host_end > host_start {
            let host = &lower[host_start..host_end];
            if !is_tracker_host(host) {
                count = count.saturating_add(1);
            }
        }
        cursor = host_end.max(host_start + 1);
    }
    count
}

fn is_tracker_host(host: &str) -> bool {
    static SUFFIXES: OnceLock<Vec<&'static str>> = OnceLock::new();
    let suffixes = SUFFIXES.get_or_init(|| TRACKER_HOSTNAME_SUFFIXES.to_vec());
    suffixes.iter().any(|suffix| host.contains(suffix))
}

/// Crude HTML-to-text stripper: drops everything between `<` and `>`, keeps
/// the rest. Used only for word-count and URL-count estimation; not for any
/// rendered output.
fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::types::MessageBody;

    fn body_with_text(text: &str) -> MessageBody {
        MessageBody {
            message_id: mxr_core::id::MessageId::new(),
            text_plain: Some(text.to_string()),
            text_html: None,
            attachments: vec![],
            metadata: Default::default(),
            fetched_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn no_urls_means_zero() {
        let body = body_with_text("Just a plain message. Nothing to click.");
        let metrics = body_link_metrics(&body);
        assert_eq!(metrics.link_count, 0);
        assert!(metrics.body_word_count >= 6);
    }

    #[test]
    fn counts_two_distinct_urls() {
        let body = body_with_text(
            "Check out https://example.com/doc and http://blog.example.com/post — both relevant.",
        );
        let metrics = body_link_metrics(&body);
        assert_eq!(metrics.link_count, 2);
    }

    #[test]
    fn excludes_tracker_hostnames() {
        let body = body_with_text(
            "Unsubscribe: https://unsubscribe.example.com/abc \
             Tracking: https://click.example.com/xyz \
             Real link: https://docs.example.com/page",
        );
        let metrics = body_link_metrics(&body);
        // Only the real link survives the deny list.
        assert_eq!(metrics.link_count, 1);
    }

    #[test]
    fn html_body_is_stripped_then_counted() {
        let body = MessageBody {
            message_id: mxr_core::id::MessageId::new(),
            text_plain: None,
            text_html: Some(
                r#"<p>Hi. See <a href="https://example.com/x">this</a> and <a href="https://other.example.com/y">that</a>.</p>"#
                    .to_string(),
            ),
            attachments: vec![],
            metadata: Default::default(),
            fetched_at: chrono::Utc::now(),
        };
        let metrics = body_link_metrics(&body);
        assert_eq!(metrics.link_count, 2);
        assert!(metrics.body_word_count >= 4);
    }
}
