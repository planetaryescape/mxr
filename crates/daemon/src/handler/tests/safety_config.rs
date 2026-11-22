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
