//! Risk #4 of the AI-email plan: secrets MUST be redacted at every
//! exit point — JSON serialization, logs, and the `draft_safety_runs`
//! audit table. This test parametrizes over every secret kind the
//! pii detector knows about and asserts that the raw secret never
//! appears in the JSON form of the report or in the persisted audit
//! row, while the redacted preview format does.
//!
//! If a future change un-redacts even one kind, at least two
//! assertions in this file fail (the "raw absent" assertion AND the
//! "preview present" assertion for that kind).

#![expect(
    clippy::unwrap_used,
    reason = "integration tests unwrap stored audit rows and JSON fields for direct invariant failures"
)]

use mxr_core::types::{Address, Draft, DraftIntent, DraftSafetyReport, DraftSafetySeverity};
use mxr_safety::{check_draft_deterministic, SafetyConfig, SafetyContext};
use mxr_store::Store;

/// One row per secret kind. `raw_marker` is a substring of the raw
/// value that proves the raw form is absent from the output.
/// `expected_preview_substring` is text that the redacted preview
/// must contain.
struct SecretCase {
    kind: &'static str,
    body: &'static str,
    raw_marker: &'static str,
    expected_preview_substring: &'static str,
}

fn cases() -> Vec<SecretCase> {
    vec![
        SecretCase {
            kind: "OpenAI sk- prefix",
            body: "key: sk-supersecretliveapikeyabcd1234XYZ",
            raw_marker: "sk-supersecretliveapikeyabcd1234XYZ",
            // detector emits format "sk-...XYZ" -- the leading prefix
            // and trailing 4 are kept; the middle is dots.
            expected_preview_substring: "sk-",
        },
        SecretCase {
            kind: "GitHub PAT (ghp_)",
            body: "token: ghp_realgithubpersonaccesstokenABCDEFGHIJ",
            raw_marker: "ghp_realgithubpersonaccesstokenABCDEFGHIJ",
            expected_preview_substring: "ghp_",
        },
        SecretCase {
            kind: "Slack token (xoxb-)",
            body: concat!(
                "slack: ",
                "xoxb-",
                "1234567890-",
                "abcdefghijklmnop-",
                "qrstuvwxyz"
            ),
            raw_marker: concat!("xoxb-", "1234567890-", "abcdefghijklmnop-", "qrstuvwxyz"),
            // redact_token keeps the first 4 chars; the dash isn't
            // part of the head for `xoxb-...` so the preview begins
            // with `xoxb` (no dash).
            expected_preview_substring: "xoxb",
        },
        SecretCase {
            kind: "AWS access key id",
            body: "creds: AKIAIOSFODNN7EXAMPLE",
            raw_marker: "AKIAIOSFODNN7EXAMPLE",
            expected_preview_substring: "AKIA",
        },
        SecretCase {
            kind: "AWS labeled assignment",
            body: "AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            raw_marker: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            expected_preview_substring: "AWS_SECRET_ACCESS_KEY=",
        },
        SecretCase {
            kind: "generic api_key=",
            body: "api_key=verysecretvaluexyz1234567890",
            raw_marker: "verysecretvaluexyz1234567890",
            expected_preview_substring: "api_key=",
        },
        SecretCase {
            kind: "client_secret=",
            body: "client_secret=oauth-client-secret-value-1234567890",
            raw_marker: "oauth-client-secret-value-1234567890",
            expected_preview_substring: "client_secret=",
        },
        SecretCase {
            kind: "PEM private key",
            body: "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQE...",
            raw_marker: "MIIEowIBAAKCAQE",
            expected_preview_substring: "BEGIN",
        },
        SecretCase {
            kind: "SSN",
            body: "ssn: 123-45-6789",
            raw_marker: "123-45-6789",
            expected_preview_substring: "***-**-6789",
        },
        SecretCase {
            kind: "Luhn-valid CC",
            // 4242 4242 4242 4242 is Stripe's well-known Luhn-valid test card.
            body: "card: 4242 4242 4242 4242",
            raw_marker: "4242 4242 4242 4242",
            expected_preview_substring: "**** **** **** 4242",
        },
    ]
}

fn draft_with(body: &str) -> Draft {
    Draft {
        id: mxr_core::DraftId::new(),
        account_id: mxr_core::AccountId::new(),
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![Address {
            name: None,
            email: "alice@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "test".into(),
        body_markdown: body.into(),
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn build_report_for(body: &str) -> DraftSafetyReport {
    let draft = draft_with(body);
    let ctx = SafetyContext {
        mode_reply_all: false,
        self_addresses: vec!["me@example.com".into()],
        known_contacts: vec![],
        contact_styles: vec![],
        thread_display_names: vec![],
    };
    let config = SafetyConfig::default();
    let outcome = check_draft_deterministic(&draft, &ctx, &config);
    DraftSafetyReport::from_issues(outcome.issues)
}

#[test]
fn json_serialization_never_contains_raw_secrets() {
    let mut failures = Vec::new();
    for case in cases() {
        let report = build_report_for(case.body);
        // Sanity: detector saw the secret. If this fails, the input
        // sample isn't tripping the detector at all and the rest of
        // the assertions are vacuous.
        let pii_issues: Vec<_> = report
            .issues
            .iter()
            .filter(|i| matches!(i.code, mxr_core::DraftSafetyIssueCode::PiiSecret))
            .collect();
        if pii_issues.is_empty() {
            failures.push(format!(
                "[{}] PII detector did not fire on body {:?}",
                case.kind, case.body
            ));
            continue;
        }

        let json = serde_json::to_string(&report).expect("report serializes");

        if json.contains(case.raw_marker) {
            failures.push(format!(
                "[{}] RAW SECRET LEAKED into JSON. raw_marker={:?} json={}",
                case.kind, case.raw_marker, json
            ));
        }
        if !json.contains(case.expected_preview_substring) {
            failures.push(format!(
                "[{}] redacted preview {:?} missing from JSON. json={}",
                case.kind, case.expected_preview_substring, json
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "leaks:\n  - {}",
        failures.join("\n  - ")
    );
}

#[tokio::test]
async fn audit_table_never_persists_raw_secrets() {
    let store = Store::in_memory().await.unwrap();
    let account = mxr_core::Account {
        id: mxr_core::AccountId::new(),
        name: "T".into(),
        email: "me@example.com".into(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    store.insert_account(&account).await.unwrap();

    let mut failures = Vec::new();
    for case in cases() {
        let report = build_report_for(case.body);
        if report.issues.is_empty() {
            failures.push(format!(
                "[{}] PII detector did not fire (sanity)",
                case.kind
            ));
            continue;
        }
        let _id = store
            .record_safety_run(&account.id, None, &report)
            .await
            .expect("audit row persists");
        // Read back the issues_json column and grep for the raw form.
        let row: (String,) = sqlx::query_as(
            "SELECT issues_json FROM draft_safety_runs ORDER BY checked_at DESC LIMIT 1",
        )
        .fetch_one(store.reader())
        .await
        .expect("audit row read-back");
        if row.0.contains(case.raw_marker) {
            failures.push(format!(
                "[{}] RAW SECRET LEAKED into draft_safety_runs.issues_json. raw_marker={:?} row={}",
                case.kind, case.raw_marker, row.0
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "audit leaks:\n  - {}",
        failures.join("\n  - ")
    );
}

#[test]
fn pii_issues_use_blocker_or_warning_severity_only() {
    // Defence-in-depth: an Info-severity PII issue would slip past
    // the daemon's `enforce_draft_safety_with_override` gate (Info
    // doesn't trigger blocker), so the detector must never emit one.
    for case in cases() {
        let report = build_report_for(case.body);
        for issue in &report.issues {
            if matches!(issue.code, mxr_core::DraftSafetyIssueCode::PiiSecret) {
                assert!(
                    matches!(
                        issue.severity,
                        DraftSafetySeverity::Blocker | DraftSafetySeverity::Warning
                    ),
                    "[{}] PiiSecret issue must be Blocker or Warning; got {:?}",
                    case.kind,
                    issue.severity
                );
            }
        }
    }
}
