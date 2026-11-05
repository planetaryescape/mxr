//! The `[safety]` config reaches the send-path checks. A configured value has
//! to change the verdict a real draft gets, and an absent section has to give
//! exactly the safety crate's defaults.

use super::*;
use mxr_core::types::{Address, Draft, DraftContent, DraftIntent, DraftSafetyIssueCode};
use mxr_core::DraftSafetySeverity;

fn draft_to(state: &AppState, email: &str, body: &str) -> Draft {
    Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id: state.default_account_id(),
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![Address {
            name: None,
            email: email.into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Thursday".into(),
        content: DraftContent::markdown(body),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

async fn safety_report(state: &Arc<AppState>, draft: &Draft) -> mxr_core::types::DraftSafetyReport {
    let ResponseData::DraftSafetyReportResponse { report } =
        crate::handler::mutations::check_draft_safety_request(
            state,
            draft,
            &mxr_protocol::DraftSafetyContextData {
                allow_llm: false,
                ..Default::default()
            },
        )
        .await
        .unwrap()
    else {
        panic!("expected a safety report");
    };
    report
}

/// A stored baseline of how the recipient writes, with `formality` as their
/// formality score. Enough samples and words that the voice match counts.
async fn seed_recipient_baseline(state: &AppState, email: &str, formality: f64) {
    let baseline = mxr_relationship::stylometry::StylometryMetrics {
        formality_score: formality,
        avg_sentence_len: 18.0,
        word_count: 42,
        ..Default::default()
    };
    state
        .store
        .upsert_contact_style(&mxr_store::ContactStyleRecord {
            account_id: state.default_account_id(),
            email: email.to_string(),
            formality_score: formality,
            formality_score_theirs: formality,
            avg_sentence_len: 18.0,
            avg_sentence_len_theirs: 18.0,
            msg_count_used: 12,
            msg_count_used_theirs: 8,
            metrics_json: "{}".to_string(),
            metrics_json_theirs: serde_json::to_string(&baseline).unwrap(),
            computed_at: chrono::Utc::now(),
            source_hash: "safety-config-test".to_string(),
            drift_detected: false,
            drift_reason: None,
            drift_detected_at: None,
        })
        .await
        .unwrap();
}

fn has_tone_mismatch(report: &mxr_core::types::DraftSafetyReport) -> bool {
    report
        .issues
        .iter()
        .any(|issue| issue.code == DraftSafetyIssueCode::ToneMismatch)
}

#[tokio::test]
async fn a_configured_tone_threshold_changes_the_send_path_decision() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let body = "hey, thursday works for me, thanks!";
    let formality = mxr_relationship::compute_metrics(body).formality_score;
    assert!(
        formality < 0.7,
        "fixture formality {formality} leaves no room"
    );
    // The draft sits 0.3 from the recipient's baseline: under the default
    // 0.4, over a configured 0.25.
    seed_recipient_baseline(&state, "sam@example.com", formality + 0.3).await;
    let draft = draft_to(&state, "sam@example.com", body);

    let report = safety_report(&state, &draft).await;
    assert!(
        !has_tone_mismatch(&report),
        "default must not warn: {report:?}"
    );

    let mut config = state.config_snapshot();
    config.safety.tone.formality_delta_threshold = 0.25;
    state.set_config_for_test(config).await;

    let report = safety_report(&state, &draft).await;
    assert!(
        has_tone_mismatch(&report),
        "configured 0.25 must warn: {report:?}"
    );
    assert!(report.allowed, "a tone mismatch warns, it never blocks");
}

#[tokio::test]
async fn a_configured_sensitive_domain_blocks_the_send_path() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let draft = draft_to(&state, "ceo@competitor.com", "Thursday works.");

    let report = safety_report(&state, &draft).await;
    assert!(
        !report
            .issues
            .iter()
            .any(|issue| issue.code == DraftSafetyIssueCode::WrongRecipient),
        "no sensitive domains configured: {report:?}"
    );

    let mut config = state.config_snapshot();
    config.safety.recipients.sensitive_domains = vec!["competitor.com".into()];
    state.set_config_for_test(config).await;

    let report = safety_report(&state, &draft).await;
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == DraftSafetyIssueCode::WrongRecipient
                && issue.severity == DraftSafetySeverity::Blocker),
        "configured sensitive domain must block: {report:?}"
    );
    assert!(!report.allowed);
}

#[test]
fn an_absent_safety_section_gives_the_safety_crate_defaults() {
    let config = mxr_config::load_config_from_str("[general]\n").unwrap();
    let configured = super::super::mutations::safety_check_config(&config.safety);
    assert_eq!(
        serde_json::to_value(&configured).unwrap(),
        serde_json::to_value(mxr_safety::SafetyConfig::default()).unwrap(),
    );
}

#[test]
fn an_out_of_range_or_nan_threshold_is_kept_inside_the_documented_range() {
    let threshold = |value: f64| {
        let mut config = mxr_config::SafetyConfig::default();
        config.tone.formality_delta_threshold = value;
        super::super::mutations::safety_check_config(&config)
            .tone
            .formality_delta_threshold
    };
    assert_eq!(threshold(3.0), 1.0);
    assert_eq!(threshold(-1.0), 0.0);
    assert_eq!(threshold(f64::NAN), 0.4);
}
