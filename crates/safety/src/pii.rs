//! PII / secrets detector.
//!
//! Local-only. Scans subject + body (NOT quoted context). Returns redacted
//! previews — never raw values. Per docs:
//! - Blocker: PEM private keys, obvious API secrets (`sk-…`, `ghp_…`,
//!   `xoxb-…`, AWS keys/secrets, generic `client_secret=` / `api_key=`).
//! - Warning: SSN-shaped, Luhn-valid card numbers.

#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "PII unit tests serialize fixture output directly for failure clarity"
    )
)]

use mxr_core::types::{Draft, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetySeverity};
use mxr_reader::{clean, ReaderConfig};
use once_cell::sync::Lazy;
use regex::Regex;

static SSN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(\d{3})-(\d{2})-(\d{4})\b").expect("SSN regex literal compiles"));
static CC_DIGITS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(\d[ -]?){11,18}\d\b").expect("credit-card regex literal compiles")
});
static SK_PREFIX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bsk-[A-Za-z0-9_-]{16,}\b").expect("OpenAI key regex literal compiles")
});
static GH_PREFIX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bghp_[A-Za-z0-9]{20,}\b").expect("GitHub token regex literal compiles")
});
static SLACK_PREFIX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b").expect("Slack token regex literal compiles")
});
static AWS_KEY_ID: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\bAKIA[0-9A-Z]{16}\b").expect("AWS key id regex literal compiles"));
// Catches all four canonical/loose forms:
//   AWS_ACCESS_KEY_ID=...      (canonical env var)
//   AWS_SECRET_ACCESS_KEY=...  (canonical env var)
//   aws_access_key=...         (loose)
//   aws_secret_key=...         (loose)
static AWS_KEY_LABEL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\baws[_-]?(access[_-]?key([_-]?id)?|secret[_-]?(access[_-]?)?key)\s*[:=]\s*\S+",
    )
    .expect("AWS key label regex literal compiles")
});
static GENERIC_API_KEY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\bapi[_-]?key\s*[:=]\s*['"]?[A-Za-z0-9_\-]{12,}['"]?"#)
        .expect("generic API key regex literal compiles")
});
static CLIENT_SECRET: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\bclient[_-]?secret\s*[:=]\s*['"]?[A-Za-z0-9_\-]{12,}['"]?"#)
        .expect("client secret regex literal compiles")
});
static PEM_PRIVATE_KEY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")
        .expect("PEM private key regex literal compiles")
});

pub fn check(draft: &Draft) -> Vec<DraftSafetyIssue> {
    let cleaned_body = reader_clean(&draft.body_markdown);
    let mut hay = String::with_capacity(draft.subject.len() + cleaned_body.len() + 1);
    hay.push_str(&draft.subject);
    hay.push('\n');
    hay.push_str(&cleaned_body);

    let mut issues = Vec::new();

    if PEM_PRIVATE_KEY.is_match(&hay) {
        issues.push(blocker(
            DraftSafetyIssueCode::PiiSecret,
            "PEM private key detected in draft",
            "redacted: -----BEGIN ... PRIVATE KEY-----",
        ));
    }

    for (re, label) in [
        (&*SK_PREFIX, "secret-style key (sk-…)"),
        (&*GH_PREFIX, "GitHub personal access token (ghp_…)"),
        (&*SLACK_PREFIX, "Slack token (xox…)"),
        (&*AWS_KEY_ID, "AWS access key id (AKIA…)"),
    ] {
        if let Some(m) = re.find(&hay) {
            issues.push(blocker(
                DraftSafetyIssueCode::PiiSecret,
                format!("{label} detected"),
                redact_token(m.as_str()),
            ));
        }
    }

    if let Some(m) = AWS_KEY_LABEL.find(&hay) {
        issues.push(blocker(
            DraftSafetyIssueCode::PiiSecret,
            "AWS credential label with assignment",
            redact_assignment(m.as_str()),
        ));
    }
    if let Some(m) = GENERIC_API_KEY.find(&hay) {
        issues.push(blocker(
            DraftSafetyIssueCode::PiiSecret,
            "api_key=… assignment",
            redact_assignment(m.as_str()),
        ));
    }
    if let Some(m) = CLIENT_SECRET.find(&hay) {
        issues.push(blocker(
            DraftSafetyIssueCode::PiiSecret,
            "client_secret=… assignment",
            redact_assignment(m.as_str()),
        ));
    }

    if let Some(c) = SSN.captures(&hay) {
        let last4 = &c[3];
        issues.push(warn(
            DraftSafetyIssueCode::PiiSecret,
            "SSN-shaped value detected",
            format!("***-**-{last4}"),
        ));
    }

    for m in CC_DIGITS.find_iter(&hay) {
        let raw = m.as_str();
        let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
        if (12..=19).contains(&digits.len()) && luhn_valid(&digits) {
            let last4 = &digits[digits.len() - 4..];
            issues.push(warn(
                DraftSafetyIssueCode::PiiSecret,
                "credit-card-shaped value detected (Luhn-valid)",
                format!("**** **** **** {last4}"),
            ));
            break;
        }
    }

    issues
}

fn blocker(
    code: DraftSafetyIssueCode,
    message: impl Into<String>,
    redacted: impl Into<String>,
) -> DraftSafetyIssue {
    DraftSafetyIssue::new(code, DraftSafetySeverity::Blocker, message).with_detail(redacted)
}

fn warn(
    code: DraftSafetyIssueCode,
    message: impl Into<String>,
    redacted: impl Into<String>,
) -> DraftSafetyIssue {
    DraftSafetyIssue::new(code, DraftSafetySeverity::Warning, message).with_detail(redacted)
}

fn redact_token(raw: &str) -> String {
    if raw.len() <= 8 {
        return "***".to_string();
    }
    let head: String = raw.chars().take(4).collect();
    let tail: String = raw
        .chars()
        .skip(raw.chars().count().saturating_sub(4))
        .collect();
    format!("{head}…{tail}")
}

fn redact_assignment(raw: &str) -> String {
    if let Some(eq_idx) = raw.find('=').or_else(|| raw.find(':')) {
        let label = raw[..eq_idx].trim_end();
        format!("{label}=***")
    } else {
        "***".to_string()
    }
}

pub(crate) fn luhn_valid(digits: &str) -> bool {
    let mut sum = 0u32;
    let mut alt = false;
    for ch in digits.chars().rev() {
        let Some(d) = ch.to_digit(10) else {
            return false;
        };
        let mut n = d;
        if alt {
            n *= 2;
            if n > 9 {
                n -= 9;
            }
        }
        sum += n;
        alt = !alt;
    }
    sum.is_multiple_of(10)
}

fn reader_clean(body: &str) -> String {
    let cfg = ReaderConfig {
        html_command: None,
        strip_signatures: true,
        collapse_quotes: true,
        strip_boilerplate: false,
        strip_tracking: false,
    };
    clean(Some(body), None, &cfg).content
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::types::{Draft, DraftIntent};
    use mxr_core::{AccountId, DraftId};

    fn d(body: &str) -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            reply_headers: None,
            intent: DraftIntent::New,
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "subject".into(),
            body_markdown: body.into(),
            attachments: Vec::new(),
            inline_calendar_reply: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn luhn_known_good_passes() {
        // 4242 4242 4242 4242 is the canonical Stripe test card; Luhn-valid.
        assert!(luhn_valid("4242424242424242"));
    }

    #[test]
    fn luhn_known_bad_fails() {
        assert!(!luhn_valid("1234567890123456"));
    }

    #[test]
    fn pem_private_key_blocks() {
        let body = concat!(
            "Hi,\n",
            "-----BEGIN RSA ",
            "PRIVATE KEY-----\nMIIEpAIBAA...\n"
        );
        let issues = check(&d(body));
        assert!(
            issues
                .iter()
                .any(|i| i.severity == DraftSafetySeverity::Blocker
                    && i.code == DraftSafetyIssueCode::PiiSecret),
            "expected PEM blocker, got {issues:?}"
        );
        // Critical: redacted preview must NOT contain raw key bytes.
        for issue in &issues {
            if let Some(detail) = &issue.detail {
                assert!(
                    !detail.contains("MIIEpAIBAA"),
                    "redacted detail leaked raw key body: {detail}"
                );
            }
        }
    }

    #[test]
    fn luhn_valid_cc_warns() {
        let issues = check(&d("Card: 4242 4242 4242 4242 thanks"));
        assert!(
            issues
                .iter()
                .any(|i| i.severity == DraftSafetySeverity::Warning
                    && i.code == DraftSafetyIssueCode::PiiSecret),
            "expected CC warning, got {issues:?}"
        );
        // Redacted preview present; raw digits absent.
        for issue in &issues {
            if let Some(detail) = &issue.detail {
                assert!(detail.contains("4242"), "should keep last 4");
                assert!(
                    !detail.contains("4242 4242 4242 4242"),
                    "should not echo full pan"
                );
            }
        }
    }

    #[test]
    fn random_digit_groups_pass() {
        // 16 digits but Luhn-invalid.
        let issues = check(&d("ref number 1234567890123456 fyi"));
        assert!(
            issues.is_empty(),
            "Luhn-invalid digit group should not warn ({issues:?})"
        );
    }

    #[test]
    fn ssn_warns() {
        let issues = check(&d("dob 123-45-6789"));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, DraftSafetySeverity::Warning);
        assert_eq!(issues[0].detail.as_deref(), Some("***-**-6789"));
        let dump = serde_json::to_string(&issues).unwrap();
        assert!(
            !dump.contains("123-45-6789"),
            "raw SSN leaked in serialized issues: {dump}"
        );
    }

    #[test]
    fn no_raw_secret_leaks_for_any_kind() {
        let cases: &[(&str, &str)] = &[
            ("token sk-abcdefghijklmnopqrstuv", "abcdefghijklmnopqrstuv"),
            (
                "aws_access_key_id=AKIAIOSFODNN7EXAMPLE",
                "AKIAIOSFODNN7EXAMPLE",
            ),
            (
                "config api_key=secret_value_long_enough_to_match",
                "secret_value_long_enough_to_match",
            ),
            (
                "client_secret=mysecretvaluepassword123",
                "mysecretvaluepassword123",
            ),
            ("Card: 4242 4242 4242 4242 thanks", "4242 4242 4242 4242"),
            ("dob 987-65-4321", "987-65-4321"),
            (
                concat!(
                    "Hi,\n",
                    "-----BEGIN RSA ",
                    "PRIVATE KEY-----\nMIIEpAIBAA0xyz\n"
                ),
                "MIIEpAIBAA0xyz",
            ),
        ];
        for (body, raw) in cases {
            let issues = check(&d(body));
            let json = serde_json::to_string(&issues).unwrap();
            assert!(!issues.is_empty(), "expected an issue for body: {body}");
            assert!(
                !json.contains(raw),
                "raw secret leaked into JSON for body=`{body}`: dump={json}"
            );
        }
    }

    #[test]
    fn cc_length_boundaries() {
        // 11 digits — below Luhn-min, never warns.
        assert!(check(&d("ref 12345678901 fyi")).is_empty());
        // 12 digits, Luhn-valid — warns.
        assert!(luhn_valid("100000000008"));
        let twelve = check(&d("ref 100000000008 fyi"));
        assert!(
            twelve
                .iter()
                .any(|i| matches!(i.severity, DraftSafetySeverity::Warning)),
            "12-digit Luhn-valid PAN should warn ({twelve:?})"
        );
    }

    #[test]
    fn sk_prefix_blocks_with_redacted_preview() {
        let issues = check(&d("token sk-abcdefghijklmnopqrstuv"));
        assert!(issues
            .iter()
            .any(|i| i.severity == DraftSafetySeverity::Blocker));
        for issue in &issues {
            if let Some(detail) = &issue.detail {
                assert!(
                    !detail.contains("abcdefghijklmnop"),
                    "redaction leaked token: {detail}"
                );
            }
        }
    }

    #[test]
    fn aws_label_blocks() {
        let issues = check(&d("aws_access_key_id=AKIAIOSFODNN7EXAMPLE"));
        assert!(issues
            .iter()
            .any(|i| i.severity == DraftSafetySeverity::Blocker));
    }

    #[test]
    fn generic_api_key_blocks() {
        let issues = check(&d("config api_key=secret_value_long_enough_to_match"));
        assert!(issues
            .iter()
            .any(|i| i.severity == DraftSafetySeverity::Blocker));
    }

    #[test]
    fn quoted_context_does_not_trigger_secret_in_quote() {
        let body = "Got it.\n\nOn Mon, Alice wrote:\n> sk-abcdefghijklmnopqrstuv\n";
        let issues = check(&d(body));
        // After reader cleanup, quoted block is collapsed; secret in the
        // quote should not raise.
        assert!(
            issues.is_empty(),
            "secret in quoted context should be ignored ({issues:?})"
        );
    }

    #[test]
    fn json_serialization_omits_raw_secret() {
        let issues = check(&d("token sk-supersecretkeyvalueXYZ"));
        let json = serde_json::to_string(&issues).unwrap();
        assert!(
            !json.contains("supersecretkeyvalueXYZ"),
            "json leaked: {json}"
        );
    }
}
