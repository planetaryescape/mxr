//! Parse `List-Unsubscribe` (RFC 2369) and `List-Unsubscribe-Post` (RFC 8058)
//! email headers into a typed action enum.
//!
//! See the [README] for the rationale and coverage matrix.
//!
//! [README]: https://github.com/planetaryescape/list-unsubscribe#readme
//!
//! ```
//! use list_unsubscribe::{parse_with_post, UnsubscribeMethod};
//!
//! let header = "<mailto:u@example.com>, <https://example.com/unsub?u=abc>";
//! let post = Some("List-Unsubscribe=One-Click");
//!
//! match parse_with_post(header, post) {
//!     UnsubscribeMethod::OneClick { url } => {
//!         // POST to `url` with body `List-Unsubscribe=One-Click`
//!         let _ = url;
//!     }
//!     UnsubscribeMethod::Mailto { address, subject } => {
//!         let _ = (address, subject);
//!     }
//!     UnsubscribeMethod::HttpLink { url } => {
//!         let _ = url;
//!     }
//!     UnsubscribeMethod::None => {}
//! }
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unsafe_code, unused_must_use)]

use url::Url;

/// The unsubscribe action a message exposes via its headers.
///
/// Returned by [`parse`] and [`parse_with_post`]. The `OneClick` variant is
/// only produced when an RFC 8058 `List-Unsubscribe-Post` header is present
/// and an HTTPS or HTTP URL is available in the main header.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind"))]
pub enum UnsubscribeMethod {
    /// RFC 8058 one-click. The caller should POST to `url` with body
    /// `List-Unsubscribe=One-Click` and `Content-Type: application/x-www-form-urlencoded`.
    OneClick { url: Url },
    /// A plain HTTP(S) link the user should open in a browser. May require
    /// further interaction (preference page, confirmation click).
    HttpLink { url: Url },
    /// Send an email to `address` with optional `subject`. The body, if any
    /// was specified in the `mailto:` query, is intentionally dropped — see
    /// the crate README for the rationale.
    Mailto {
        address: String,
        subject: Option<String>,
    },
    /// No `List-Unsubscribe` header was found, or every candidate was
    /// unparseable.
    None,
}

/// Parse a raw `List-Unsubscribe` header value into an [`UnsubscribeMethod`].
///
/// Equivalent to [`parse_with_post`] with `post_header_value = None`. Use
/// this when you have not been able to retrieve `List-Unsubscribe-Post` —
/// for example because the message store discarded it. RFC 8058 one-click
/// detection requires both headers, so this entry point will never return
/// [`UnsubscribeMethod::OneClick`].
pub fn parse(header_value: &str) -> UnsubscribeMethod {
    parse_with_post(header_value, None)
}

/// Parse `List-Unsubscribe` together with the optional `List-Unsubscribe-Post`
/// header.
///
/// `post_header_value` is matched case-insensitively against the substring
/// `list-unsubscribe=one-click`, per [RFC 8058 §3.2][rfc8058-3-2]. When
/// present and accompanied by a valid HTTP(S) URL in the main header, the
/// returned variant is [`UnsubscribeMethod::OneClick`].
///
/// Preference order when multiple methods are present:
///
/// 1. RFC 8058 one-click (Post header + HTTP(S) URL)
/// 2. `mailto:` (first encountered)
/// 3. HTTP(S) link (first encountered)
///
/// Malformed URLs are skipped silently and fall through to the next
/// candidate. If no candidate parses, [`UnsubscribeMethod::None`] is
/// returned.
///
/// [rfc8058-3-2]: https://www.rfc-editor.org/rfc/rfc8058#section-3.2
pub fn parse_with_post(header_value: &str, post_header_value: Option<&str>) -> UnsubscribeMethod {
    let entries = split_entries(header_value);
    if entries.is_empty() {
        return UnsubscribeMethod::None;
    }

    let one_click_requested = post_header_value
        .map(|value| {
            value
                .to_ascii_lowercase()
                .contains("list-unsubscribe=one-click")
        })
        .unwrap_or(false);

    if one_click_requested {
        for entry in &entries {
            if is_http(entry) {
                if let Ok(url) = Url::parse(entry) {
                    return UnsubscribeMethod::OneClick { url };
                }
            }
        }
    }

    for entry in &entries {
        if let Some(rest) = strip_mailto(entry) {
            return parse_mailto(rest);
        }
    }

    for entry in &entries {
        if is_http(entry) {
            if let Ok(url) = Url::parse(entry) {
                return UnsubscribeMethod::HttpLink { url };
            }
        }
    }

    UnsubscribeMethod::None
}

/// Parse `List-Unsubscribe` from a [`mail_parser::Message`] in one call.
///
/// Reads both `List-Unsubscribe` and `List-Unsubscribe-Post` from the
/// message and delegates to [`parse_with_post`]. Available only with the
/// `mail-parser` feature enabled.
#[cfg(feature = "mail-parser")]
#[cfg_attr(docsrs, doc(cfg(feature = "mail-parser")))]
pub fn parse_from_message(message: &mail_parser::Message<'_>) -> UnsubscribeMethod {
    let header_value = message
        .header_raw("List-Unsubscribe")
        .unwrap_or("")
        .to_string();
    let post_value = message
        .header_raw("List-Unsubscribe-Post")
        .map(|value| value.to_string());
    parse_with_post(&header_value, post_value.as_deref())
}

fn split_entries(header_value: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in header_value.split(',') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let stripped = trimmed
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .unwrap_or(trimmed);
        let stripped = stripped.trim();
        if !stripped.is_empty() {
            out.push(stripped.to_string());
        }
    }
    out
}

fn is_http(entry: &str) -> bool {
    let lower_prefix = entry.get(..8).map(str::to_ascii_lowercase);
    matches!(
        lower_prefix.as_deref(),
        Some(p) if p.starts_with("https://") || p.starts_with("http://")
    )
}

fn strip_mailto(entry: &str) -> Option<&str> {
    let prefix = entry.get(..7)?;
    if prefix.eq_ignore_ascii_case("mailto:") {
        Some(&entry[7..])
    } else {
        None
    }
}

fn parse_mailto(rest: &str) -> UnsubscribeMethod {
    let (address_part, query) = match rest.split_once('?') {
        Some((address, query)) => (address.to_string(), Some(query)),
        None => (rest.to_string(), None),
    };

    let mut subject = None;
    if let Some(query) = query {
        for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
            if key.eq_ignore_ascii_case("subject") {
                subject = Some(value.into_owned());
                break;
            }
        }
    }

    if address_part.is_empty() {
        UnsubscribeMethod::None
    } else {
        UnsubscribeMethod::Mailto {
            address: address_part,
            subject,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn empty_header_returns_none() {
        assert_eq!(parse(""), UnsubscribeMethod::None);
        assert_eq!(parse("   "), UnsubscribeMethod::None);
    }

    #[test]
    fn single_mailto_returns_mailto() {
        match parse("<mailto:u@example.com>") {
            UnsubscribeMethod::Mailto { address, subject } => {
                assert_eq!(address, "u@example.com");
                assert!(subject.is_none());
            }
            other => panic!("expected Mailto, got {other:?}"),
        }
    }

    #[test]
    fn mailto_with_subject_extracts_subject() {
        match parse("<mailto:u@example.com?subject=Unsubscribe>") {
            UnsubscribeMethod::Mailto { address, subject } => {
                assert_eq!(address, "u@example.com");
                assert_eq!(subject.as_deref(), Some("Unsubscribe"));
            }
            other => panic!("expected Mailto, got {other:?}"),
        }
    }

    #[test]
    fn mailto_with_subject_and_body_drops_body() {
        match parse("<mailto:u@example.com?subject=Unsubscribe&body=please>") {
            UnsubscribeMethod::Mailto { address, subject } => {
                assert_eq!(address, "u@example.com");
                assert_eq!(subject.as_deref(), Some("Unsubscribe"));
            }
            other => panic!("expected Mailto, got {other:?}"),
        }
    }

    #[test]
    fn single_https_returns_http_link() {
        match parse("<https://example.com/unsub>") {
            UnsubscribeMethod::HttpLink { url } => {
                assert_eq!(url.as_str(), "https://example.com/unsub");
            }
            other => panic!("expected HttpLink, got {other:?}"),
        }
    }

    #[test]
    fn mailto_preferred_over_http_when_no_one_click() {
        let header = "<mailto:u@example.com>, <https://example.com/unsub>";
        match parse(header) {
            UnsubscribeMethod::Mailto { address, .. } => {
                assert_eq!(address, "u@example.com");
            }
            other => panic!("expected Mailto, got {other:?}"),
        }
    }

    #[test]
    fn one_click_picks_http_url() {
        let header = "<mailto:u@example.com>, <https://example.com/unsub?u=abc>";
        let post = Some("List-Unsubscribe=One-Click");
        match parse_with_post(header, post) {
            UnsubscribeMethod::OneClick { url } => {
                assert_eq!(url.as_str(), "https://example.com/unsub?u=abc");
            }
            other => panic!("expected OneClick, got {other:?}"),
        }
    }

    #[test]
    fn one_click_is_case_insensitive() {
        let header = "<https://example.com/unsub>";
        let post = Some("LIST-UNSUBSCRIBE=ONE-CLICK");
        assert!(matches!(
            parse_with_post(header, post),
            UnsubscribeMethod::OneClick { .. }
        ));
    }

    #[test]
    fn one_click_without_http_falls_back() {
        let header = "<mailto:u@example.com>";
        let post = Some("List-Unsubscribe=One-Click");
        match parse_with_post(header, post) {
            UnsubscribeMethod::Mailto { address, .. } => {
                assert_eq!(address, "u@example.com");
            }
            other => panic!("expected Mailto fallback, got {other:?}"),
        }
    }

    #[test]
    fn multiple_https_returns_first() {
        let header = "<https://example.com/desktop/unsub>, <https://example.com/mobile/unsub>";
        match parse(header) {
            UnsubscribeMethod::HttpLink { url } => {
                assert_eq!(url.as_str(), "https://example.com/desktop/unsub");
            }
            other => panic!("expected HttpLink, got {other:?}"),
        }
    }

    #[test]
    fn malformed_url_returns_none_when_only_candidate() {
        assert_eq!(parse("<https://>"), UnsubscribeMethod::None);
    }

    #[test]
    fn whitespace_quirks_tolerated() {
        // Producers in the wild sometimes pad inside the angle brackets.
        // We strip the < > envelope then trim, so both entries are
        // recoverable. Mailto wins per preference order.
        let header = "  < mailto:u@example.com > ,  < https://example.com/unsub > ";
        match parse(header) {
            UnsubscribeMethod::Mailto { address, subject } => {
                assert_eq!(address, "u@example.com");
                assert!(subject.is_none());
            }
            other => panic!("expected Mailto, got {other:?}"),
        }
    }

    #[test]
    fn http_scheme_case_insensitive() {
        let header = "<HTTPS://example.com/unsub>";
        match parse(header) {
            UnsubscribeMethod::HttpLink { url } => {
                assert_eq!(url.scheme(), "https");
            }
            other => panic!("expected HttpLink, got {other:?}"),
        }
    }
}
