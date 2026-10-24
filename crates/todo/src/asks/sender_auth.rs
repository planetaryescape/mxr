//! Receiver-reported alignment under an explicitly configured RFC8601 trust boundary.
use msg_auth_status::{alloc_yes::AuthenticationResults, dkim::DkimResultCode, spf::SpfResultCode};
use std::borrow::Cow;

pub(super) fn aligned<'a>(
    headers: &'a [String],
    trusted: &[String],
    sender: &str,
) -> Option<(&'a str, String, &'static str, String)> {
    let sender = crate::action_link::email_domain(sender)?;
    for raw in headers {
        let value = msg_auth_status::mail_parser::HeaderValue::Text(Cow::Borrowed(raw));
        let parsed = AuthenticationResults::from(&value);
        let host = parsed.host.as_ref()?;
        if !trusted.iter().any(|id| id.eq_ignore_ascii_case(host.host)) {
            continue;
        }
        // The first configured receiver's report is authoritative, including a
        // failure. A later forged pass cannot replace it.
        if !parsed.errors.is_empty() || parsed.none_done || host.version.is_some_and(|v| v != 1) {
            return None;
        }
        for result in &parsed.dkim_result {
            if result.code != DkimResultCode::Pass {
                continue;
            }
            let domain = result.header_d.or_else(|| {
                result
                    .header_i
                    .and_then(|i| i.rsplit_once('@').map(|(_, domain)| domain))
            });
            if let Some(domain) = domain.filter(|domain| {
                crate::action_link::registrable_domain(domain).as_deref() == Some(&sender)
            }) {
                return Some((raw, host.host.to_string(), "dkim", domain.to_string()));
            }
        }
        for result in &parsed.spf_result {
            if result.code != SpfResultCode::Pass {
                continue;
            }
            if let Some(identity) = result.smtp_mailfrom {
                let domain = identity
                    .rsplit_once('@')
                    .map_or(identity, |(_, domain)| domain);
                if crate::action_link::registrable_domain(domain).as_deref() == Some(&sender) {
                    return Some((raw, host.host.to_string(), "spf", domain.to_string()));
                }
            }
        }
        return None;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_boundary_alignment_and_first_report_cannot_be_forged_by_later_headers() {
        let trusted = vec!["mx.example.net".into()];
        let pass = "mx.example.net; dkim=pass header.d=example.org".to_string();
        assert!(aligned(std::slice::from_ref(&pass), &[], "bill@example.org").is_none());
        assert!(aligned(std::slice::from_ref(&pass), &trusted, "bill@example.org").is_some());
        assert!(aligned(
            &["attacker.invalid; dkim=pass header.d=example.org".into()],
            &trusted,
            "bill@example.org"
        )
        .is_none());
        assert!(aligned(
            &[
                "mx.example.net; dkim=fail header.d=example.org".into(),
                pass.clone()
            ],
            &trusted,
            "bill@example.org"
        )
        .is_none());
        assert!(aligned(
            &["mx.example.net; dkim=pass header.d=other.org".into()],
            &trusted,
            "bill@example.org"
        )
        .is_none());
        assert!(aligned(
            &["mx.example.net; dkim=pass header.d=example.org (unterminated".into()],
            &trusted,
            "bill@example.org"
        )
        .is_none());
        assert!(aligned(
            &["mx.example.net; spf=pass smtp.mailfrom=sender@sub.example.org".into()],
            &trusted,
            "bill@example.org"
        )
        .is_some());
    }
}
