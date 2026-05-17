//! Recipient history / domain checks.
//!
//! - typo distance vs. a stronger known contact
//! - configured sensitive-domain blocker
//! - first-time external recipient (when configured)
//!
//! Account's own addresses are filtered out by the caller via
//! `SafetyContext.self_addresses`. Self-addresses are never flagged.

use mxr_core::types::{Draft, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetySeverity};

use crate::{config::SafetyRecipientConfig, KnownContact, SafetyContext};

pub fn check(
    draft: &Draft,
    ctx: &SafetyContext,
    cfg: &SafetyRecipientConfig,
) -> Vec<DraftSafetyIssue> {
    let mut issues = Vec::new();
    let recipients = draft.to.iter().chain(&draft.cc).chain(&draft.bcc);

    for addr in recipients {
        let email = addr.email.trim().to_ascii_lowercase();
        if email.is_empty() || is_self(&email, ctx) {
            continue;
        }
        let (_local, domain) = match email.rsplit_once('@') {
            Some(x) => x,
            None => continue,
        };

        // 1. configured sensitive-domain blocker
        if cfg
            .sensitive_domains
            .iter()
            .any(|d| domain.eq_ignore_ascii_case(d.trim()))
        {
            issues.push(DraftSafetyIssue::new(
                DraftSafetyIssueCode::WrongRecipient,
                DraftSafetySeverity::Blocker,
                format!("recipient {email} is on the configured sensitive-domain list"),
            ));
            continue;
        }

        // 2. typo distance vs. a stronger contact
        if let Some(better) = best_typo_candidate(&email, ctx) {
            issues.push(DraftSafetyIssue::new(
                DraftSafetyIssueCode::WrongRecipient,
                DraftSafetySeverity::Warning,
                format!(
                    "recipient {email} is one edit away from a more frequent contact {}",
                    better.email
                ),
            ));
            continue;
        }

        // 3. first-time external (only when configured)
        if cfg.warn_on_first_time_external {
            let known = ctx
                .known_contacts
                .iter()
                .any(|c| c.email.eq_ignore_ascii_case(&email));
            let is_internal = cfg
                .internal_domains
                .iter()
                .any(|d| domain.eq_ignore_ascii_case(d.trim()));
            if !known && !is_internal {
                issues.push(DraftSafetyIssue::new(
                    DraftSafetyIssueCode::WrongRecipient,
                    DraftSafetySeverity::Warning,
                    format!("first-time external recipient: {email}"),
                ));
            }
        }
    }

    issues
}

fn is_self(email: &str, ctx: &SafetyContext) -> bool {
    ctx.self_addresses
        .iter()
        .any(|s| s.eq_ignore_ascii_case(email))
}

fn best_typo_candidate<'a>(email: &str, ctx: &'a SafetyContext) -> Option<&'a KnownContact> {
    // Only suggest if the typed recipient has zero-or-weak history AND a
    // strong contact is exactly one edit away. This is the documented
    // boundary: don't warn when both addresses have strong history.
    let typed_strength = ctx
        .known_contacts
        .iter()
        .find(|c| c.email.eq_ignore_ascii_case(email))
        .map(|c| c.total_inbound + c.total_outbound)
        .unwrap_or(0);
    if typed_strength >= 3 {
        return None;
    }
    ctx.known_contacts
        .iter()
        .filter(|c| c.is_strong())
        .filter(|c| !c.email.eq_ignore_ascii_case(email))
        .filter(|c| damerau_levenshtein(&c.email.to_ascii_lowercase(), email) == 1)
        .max_by_key(|c| c.total_inbound + c.total_outbound)
}

/// Damerau-Levenshtein distance, capped at 2 for early exit.
/// Local 30-line impl per docs ("documented; no new dep").
pub(crate) fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let n = a.len();
    let m = b.len();
    if n.abs_diff(m) > 2 {
        return 3;
    }
    let mut prev_prev = vec![0usize; m + 1];
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];
    for i in 1..=n {
        curr[0] = i;
        let mut row_min = curr[0];
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                curr[j] = curr[j].min(prev_prev[j - 2] + 1);
            }
            row_min = row_min.min(curr[j]);
        }
        if row_min > 2 {
            return 3;
        }
        prev_prev.clone_from(&prev);
        prev.clone_from(&curr);
    }
    prev[m]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::types::{Address, Draft, DraftIntent};
    use mxr_core::{AccountId, DraftId};

    fn d(addrs: Vec<Address>) -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            reply_headers: None,
            intent: DraftIntent::New,
            to: addrs,
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "hi".into(),
            body_markdown: "body".into(),
            attachments: Vec::new(),
            inline_calendar_reply: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn addr(email: &str) -> Address {
        Address {
            email: email.into(),
            name: None,
        }
    }

    fn contact(email: &str, in_: u64, out: u64) -> KnownContact {
        KnownContact {
            email: email.into(),
            display_name: None,
            total_inbound: in_,
            total_outbound: out,
        }
    }

    #[test]
    fn dl_basic() {
        assert_eq!(damerau_levenshtein("alice", "alice"), 0);
        assert_eq!(damerau_levenshtein("alice", "alicc"), 1);
        assert_eq!(damerau_levenshtein("alice", "alcie"), 1); // transposition
        assert!(damerau_levenshtein("alice", "bob") > 2);
    }

    #[test]
    fn typo_recipient_warns_when_better_known() {
        let ctx = SafetyContext {
            known_contacts: vec![contact("alice@example.com", 12, 6)],
            ..Default::default()
        };
        let issues = check(
            &d(vec![addr("alcie@example.com")]),
            &ctx,
            &Default::default(),
        );
        assert_eq!(issues.len(), 1, "expected typo warning, got {issues:?}");
        // message must name BOTH the typed address and the candidate.
        assert!(
            issues[0].message.contains("alcie@example.com"),
            "warning omits the typed recipient: {}",
            issues[0].message
        );
        assert!(
            issues[0].message.contains("alice@example.com"),
            "warning omits the candidate: {}",
            issues[0].message
        );
        assert_eq!(issues[0].severity, DraftSafetySeverity::Warning);
    }

    #[test]
    fn typo_threshold_at_strength_boundary() {
        // is_strong() is sum >= 3. A typed recipient at sum == 3 must
        // suppress the warning; sum == 2 must allow it.
        let ctx_typed_strong = SafetyContext {
            known_contacts: vec![
                contact("alice@example.com", 50, 50),
                contact("alcie@example.com", 2, 1), // sum=3 → strong, suppresses
            ],
            ..Default::default()
        };
        assert!(
            check(
                &d(vec![addr("alcie@example.com")]),
                &ctx_typed_strong,
                &Default::default()
            )
            .is_empty(),
            "sum==3 must count as strong"
        );

        let ctx_typed_weak = SafetyContext {
            known_contacts: vec![
                contact("alice@example.com", 50, 50),
                contact("alcie@example.com", 2, 0), // sum=2 → weak, warns
            ],
            ..Default::default()
        };
        assert_eq!(
            check(
                &d(vec![addr("alcie@example.com")]),
                &ctx_typed_weak,
                &Default::default()
            )
            .len(),
            1,
            "sum==2 must still trigger the typo warn"
        );
    }

    #[test]
    fn no_typo_warning_when_typed_recipient_is_also_strong() {
        let ctx = SafetyContext {
            known_contacts: vec![
                contact("alice@example.com", 12, 6),
                contact("alcie@example.com", 8, 4),
            ],
            ..Default::default()
        };
        let issues = check(
            &d(vec![addr("alcie@example.com")]),
            &ctx,
            &Default::default(),
        );
        assert!(
            issues.is_empty(),
            "both addresses have prior history; should not warn"
        );
    }

    #[test]
    fn sensitive_domain_blocks() {
        let cfg = SafetyRecipientConfig {
            sensitive_domains: vec!["competitor.com".into()],
            ..Default::default()
        };
        let issues = check(
            &d(vec![addr("ceo@competitor.com")]),
            &Default::default(),
            &cfg,
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, DraftSafetySeverity::Blocker);
    }

    #[test]
    fn sensitive_domain_match_is_case_insensitive() {
        let cfg = SafetyRecipientConfig {
            sensitive_domains: vec!["Competitor.COM".into()],
            ..Default::default()
        };
        let issues = check(
            &d(vec![addr("CEO@COMPETITOR.com")]),
            &Default::default(),
            &cfg,
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, DraftSafetySeverity::Blocker);
    }

    #[test]
    fn first_time_external_warns_when_enabled() {
        let cfg = SafetyRecipientConfig {
            internal_domains: vec!["company.com".into()],
            warn_on_first_time_external: true,
            ..Default::default()
        };
        let ctx = SafetyContext {
            known_contacts: vec![contact("alice@company.com", 5, 5)],
            ..Default::default()
        };
        let issues = check(&d(vec![addr("stranger@external.com")]), &ctx, &cfg);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, DraftSafetySeverity::Warning);
    }

    #[test]
    fn first_time_external_silent_when_disabled() {
        let cfg = SafetyRecipientConfig {
            internal_domains: vec!["company.com".into()],
            warn_on_first_time_external: false,
            ..Default::default()
        };
        let issues = check(
            &d(vec![addr("stranger@external.com")]),
            &Default::default(),
            &cfg,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn first_time_external_does_not_warn_for_internal_domain() {
        let cfg = SafetyRecipientConfig {
            internal_domains: vec!["company.com".into()],
            warn_on_first_time_external: true,
            ..Default::default()
        };
        let issues = check(
            &d(vec![addr("newhire@company.com")]),
            &Default::default(),
            &cfg,
        );
        assert!(issues.is_empty(), "internal domain should not warn");
    }

    #[test]
    fn self_address_is_ignored() {
        let ctx = SafetyContext {
            self_addresses: vec!["me@x.com".into()],
            known_contacts: vec![contact("alice@x.com", 10, 10)],
            ..Default::default()
        };
        let cfg = SafetyRecipientConfig {
            sensitive_domains: vec!["x.com".into()],
            warn_on_first_time_external: true,
            ..Default::default()
        };
        // self goes to "me@x.com" which is on sensitive list — but is filtered.
        let issues = check(&d(vec![addr("me@x.com")]), &ctx, &cfg);
        assert!(issues.is_empty(), "self address should produce no issues");
    }
}
