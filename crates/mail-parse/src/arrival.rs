//! Parse a receiver verdict only after the caller authenticates its provenance.
//! Matching an authserv-id is syntax validation, not a trust boundary. Live
//! adapters use receiver_arc over original bytes; bare mail headers never qualify.

use chrono::{DateTime, Utc};
use mxr_core::{ArrivalEvidence, SenderCheck, UnverifiedReason};

/// What an adapter knows about its provider's verdict.
#[derive(Debug, Clone, Copy)]
pub enum VerdictPolicy<'a> {
    /// Caller has independently authenticated these results. The string pin
    /// alone is not evidence of receiver authorship.
    Pinned { authserv_id: &'a str },
    /// The provider gives no verdict mxr can trust.
    Unavailable(UnverifiedReason),
}

/// The arrival record for one fetched message.
pub fn arrival_evidence(
    policy: VerdictPolicy<'_>,
    auth_results: &[String],
    from_email: &str,
    received_at: Option<DateTime<Utc>>,
) -> ArrivalEvidence {
    let sender = match policy {
        VerdictPolicy::Unavailable(reason) => SenderCheck::Unverified { reason },
        VerdictPolicy::Pinned { authserv_id } => {
            match verdict(authserv_id, auth_results, from_email) {
                Ok(host) if received_at.is_some() => SenderCheck::Verified { host },
                Ok(_) => SenderCheck::Unverified {
                    reason: UnverifiedReason::NoReceiptTime,
                },
                Err(reason) => SenderCheck::Unverified { reason },
            }
        }
    };
    ArrivalEvidence {
        message_fingerprint: None,
        received_at,
        sender,
        is_live: false,
    }
}

/// The From host, lowercased, without a trailing dot.
pub fn from_host(from_email: &str) -> Option<String> {
    let host = from_email
        .trim()
        .rsplit_once('@')?
        .1
        .trim_end_matches(['.', '>'])
        .to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

fn verdict(
    authserv_id: &str,
    headers: &[String],
    from_email: &str,
) -> Result<String, UnverifiedReason> {
    let host = from_host(from_email).ok_or(UnverifiedReason::NotDmarcPass)?;
    if headers
        .iter()
        .any(|header| strip_comments(header).is_none())
    {
        return Err(UnverifiedReason::AmbiguousVerdict);
    }
    let ours: Vec<&String> = headers
        .iter()
        .filter(|header| header_id(header).is_some_and(|id| id.eq_ignore_ascii_case(authserv_id)))
        .collect();
    let header = match ours.as_slice() {
        [] => return Err(UnverifiedReason::NoVerdictHeader),
        [one] => *one,
        _ => return Err(UnverifiedReason::AmbiguousVerdict),
    };
    let results = parse(header).ok_or(UnverifiedReason::AmbiguousVerdict)?;
    let mut dmarc = results.iter().filter(|r| r.method == "dmarc");
    let (Some(result), None) = (dmarc.next(), dmarc.next()) else {
        return if results.iter().any(|r| r.method == "dmarc") {
            Err(UnverifiedReason::AmbiguousVerdict)
        } else {
            Err(UnverifiedReason::NotDmarcPass)
        };
    };
    let aligned = result
        .property("header.from")
        .is_some_and(|value| value.trim_end_matches('.').eq_ignore_ascii_case(&host));
    if result.result == "pass" && aligned {
        Ok(host)
    } else {
        Err(UnverifiedReason::NotDmarcPass)
    }
}

/// The authserv-id a header names, if it can be read at all.
fn header_id(header: &str) -> Option<String> {
    let cleaned = strip_comments(header)?;
    let first = split_unquoted(&cleaned, |c| c == ';').into_iter().next()?;
    first.split_whitespace().next().map(str::to_string)
}

#[derive(Debug)]
struct MethodResult {
    method: String,
    result: String,
    properties: Vec<(String, String)>,
}

impl MethodResult {
    fn property(&self, name: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// `authserv-id [version]; method=result prop=value; ...`, or None when it
/// is malformed in any way that could change what it means.
fn parse(header: &str) -> Option<Vec<MethodResult>> {
    let cleaned = strip_comments(header)?;
    let mut segments = split_unquoted(&cleaned, |c| c == ';').into_iter();
    let first = segments.next()?;
    let mut head = first.split_whitespace().map(str::to_string);
    head.next()?;
    match (head.next().as_deref(), head.next()) {
        (None | Some("1"), None) => {}
        _ => return None,
    }
    let mut results = Vec::new();
    for segment in segments {
        let segment = segment.trim();
        if segment.is_empty() || segment.eq_ignore_ascii_case("none") {
            continue;
        }
        let mut tokens = split_unquoted(segment, char::is_whitespace)
            .into_iter()
            .filter(|token| !token.is_empty());
        let (method, result) = tokens.next()?.split_once('=').map(|(m, r)| {
            (
                m.split('/').next().unwrap_or(m).to_ascii_lowercase(),
                r.to_ascii_lowercase(),
            )
        })?;
        let mut properties: Vec<(String, String)> = Vec::new();
        for token in tokens {
            let (key, value) = token.split_once('=')?;
            if key.is_empty()
                || value.is_empty()
                || properties
                    .iter()
                    .any(|(existing, _)| existing.eq_ignore_ascii_case(key))
            {
                return None;
            }
            properties.push((key.to_string(), value.trim_matches('"').to_string()));
        }
        results.push(MethodResult {
            method,
            result,
            properties,
        });
    }
    Some(results)
}

/// Removes RFC 5322 comments, nested and escaped; quoted strings are kept.
/// None when a comment or quote is not closed.
fn strip_comments(text: &str) -> Option<String> {
    let (mut out, mut depth, mut quoted, mut escaped) = (String::new(), 0usize, false, false);
    for c in text.chars() {
        if escaped {
            escaped = false;
            if depth == 0 {
                out.push(c);
            }
            continue;
        }
        match c {
            '\\' => {
                escaped = true;
                if depth == 0 {
                    out.push(c);
                }
            }
            '"' if depth == 0 => {
                quoted = !quoted;
                out.push(c);
            }
            '(' if !quoted => depth += 1,
            ')' if !quoted && depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    (depth == 0 && !quoted && !escaped).then_some(out)
}

fn split_unquoted(text: &str, is_separator: impl Fn(char) -> bool) -> Vec<String> {
    let (mut parts, mut current, mut quoted) = (Vec::new(), String::new(), false);
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            _ if is_separator(c) && !quoted => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    parts.push(current);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOGLE: &str = "mx.google.com";
    const PASS: &str = "mx.google.com;\r\n       dkim=pass header.i=@accounts.google.com header.s=20230601 header.b=abc;\r\n       spf=pass (google.com: domain of no-reply@accounts.google.com designates 209.85.0.1 as permitted sender) smtp.mailfrom=no-reply@accounts.google.com;\r\n       dmarc=pass (p=REJECT sp=REJECT dis=NONE) header.from=accounts.google.com";

    fn check(headers: &[&str], from: &str) -> SenderCheck {
        let headers: Vec<String> = headers.iter().map(|h| (*h).to_string()).collect();
        arrival_evidence(
            VerdictPolicy::Pinned {
                authserv_id: GOOGLE,
            },
            &headers,
            from,
            Some(DateTime::UNIX_EPOCH),
        )
        .sender
    }

    fn reason(check: SenderCheck) -> UnverifiedReason {
        match check {
            SenderCheck::Unverified { reason } => reason,
            SenderCheck::Verified { host } => std::panic::resume_unwind(Box::new(host)),
        }
    }

    #[test]
    fn the_providers_own_dmarc_pass_for_the_from_host_verifies() {
        assert_eq!(
            check(&[PASS], "no-reply@accounts.google.com"),
            SenderCheck::Verified {
                host: "accounts.google.com".into()
            }
        );
    }

    #[test]
    fn a_forged_copy_of_the_providers_id_makes_the_verdict_ambiguous_in_either_order() {
        let forged = "mx.google.com; dmarc=pass header.from=evil.example";
        for headers in [[PASS, forged], [forged, PASS]] {
            assert_eq!(
                reason(check(&headers, "a@evil.example")),
                UnverifiedReason::AmbiguousVerdict
            );
            assert_eq!(
                reason(check(&headers, "no-reply@accounts.google.com")),
                UnverifiedReason::AmbiguousVerdict
            );
        }
    }

    #[test]
    fn a_header_with_another_authserv_id_is_never_the_verdict() {
        let forged = "evil.example; dmarc=pass header.from=evil.example";
        assert_eq!(
            reason(check(&[forged], "a@evil.example")),
            UnverifiedReason::NoVerdictHeader
        );
        let lookalike = "mx.google.com.evil.example; dmarc=pass header.from=evil.example";
        assert_eq!(
            reason(check(&[lookalike], "a@evil.example")),
            UnverifiedReason::NoVerdictHeader
        );
        // A forged header does not spoil the provider's genuine pass either.
        assert!(matches!(
            check(&[forged, PASS], "no-reply@accounts.google.com"),
            SenderCheck::Verified { .. }
        ));
    }

    #[test]
    fn the_verdict_must_be_a_dmarc_pass_for_this_from_host() {
        let other_host = "mx.google.com; dmarc=pass header.from=google.com";
        assert_eq!(
            reason(check(&[other_host], "a@accounts.google.com")),
            UnverifiedReason::NotDmarcPass
        );
        for header in [
            "mx.google.com; dmarc=fail header.from=accounts.google.com",
            "mx.google.com; dmarc=none header.from=accounts.google.com",
            "mx.google.com; dkim=pass header.d=accounts.google.com",
            "mx.google.com; none",
            "mx.google.com; dmarc=pass",
        ] {
            assert_eq!(
                reason(check(&[header], "a@accounts.google.com")),
                UnverifiedReason::NotDmarcPass,
                "{header}"
            );
        }
        let two = "mx.google.com; dmarc=pass header.from=accounts.google.com; dmarc=fail header.from=accounts.google.com";
        assert_eq!(
            reason(check(&[two], "a@accounts.google.com")),
            UnverifiedReason::AmbiguousVerdict
        );
    }

    #[test]
    fn a_missing_header_or_a_malformed_one_fails_closed() {
        assert_eq!(
            reason(check(&[], "a@accounts.google.com")),
            UnverifiedReason::NoVerdictHeader
        );
        for header in [
            "mx.google.com; dmarc=pass header.from=accounts.google.com (unterminated",
            "mx.google.com 2; dmarc=pass header.from=accounts.google.com",
            "mx.google.com 1 extra; dmarc=pass header.from=accounts.google.com",
            "mx.google.com; dmarc=pass header.from=\"accounts.google.com",
        ] {
            assert_eq!(
                reason(check(&[header], "a@accounts.google.com")),
                UnverifiedReason::AmbiguousVerdict,
                "{header}"
            );
        }
    }

    #[test]
    fn comments_cannot_hide_or_fake_a_result() {
        let hidden = "mx.google.com; dmarc=fail (dmarc=pass header.from=accounts.google.com) header.from=accounts.google.com";
        assert_eq!(
            reason(check(&[hidden], "a@accounts.google.com")),
            UnverifiedReason::NotDmarcPass
        );
        let cased = "MX.Google.COM 1; DMARC=Pass header.from=Accounts.Google.com.";
        assert!(matches!(
            check(&[cased], "A@accounts.google.com"),
            SenderCheck::Verified { .. }
        ));
    }

    #[test]
    fn an_unpinned_provider_never_verifies_whatever_the_headers_say() {
        let headers = vec![PASS.to_string()];
        for why in [UnverifiedReason::ImapGivesNoVerdict] {
            let evidence = arrival_evidence(
                VerdictPolicy::Unavailable(why),
                &headers,
                "no-reply@accounts.google.com",
                Some(DateTime::UNIX_EPOCH),
            );
            assert_eq!(evidence.sender, SenderCheck::Unverified { reason: why });
            assert!(evidence.received_at.is_some());
        }
    }

    #[test]
    fn a_verdict_without_a_receipt_time_cannot_order_history() {
        let headers = vec![PASS.to_string()];
        let evidence = arrival_evidence(
            VerdictPolicy::Pinned {
                authserv_id: GOOGLE,
            },
            &headers,
            "no-reply@accounts.google.com",
            None,
        );
        assert_eq!(reason(evidence.sender), UnverifiedReason::NoReceiptTime);
    }
    #[test]
    fn duplicate_from_properties_and_a_malformed_peer_verdict_fail_closed() {
        assert_eq!(reason(check(&[
            "mx.google.com; dmarc=pass header.from=accounts.google.com header.from=evil.example"
        ], "no-reply@accounts.google.com")), UnverifiedReason::AmbiguousVerdict);
        assert_eq!(
            reason(check(
                &[PASS, "mx.google.com; (unclosed"],
                "no-reply@accounts.google.com"
            )),
            UnverifiedReason::AmbiguousVerdict
        );
    }
}
