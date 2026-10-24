use super::*;

#[tokio::test]
async fn a_speakers_response_offer_is_not_an_owner_request() {
    let fx = fixture().await;
    let offer = source(&fx, "I can respond tomorrow.").await;
    let request = source(&fx, "Can you respond tomorrow?").await;
    let (offer, turn, errors) = observe(&fx, &offer, false).await;
    let (_, request_turn, request_errors) = observe(&fx, &request, true).await;
    assert_eq!(request_errors, 0);
    assert!(request_turn);
    assert_eq!(errors, 0);
    assert!(!turn);
    assert!(offer
        .units
        .iter()
        .any(|u| offer.quote(u) == Some("I can respond tomorrow.")
            && u.tags
                .as_ref()
                .is_some_and(|t| t.act == mxr_core::asks::Act::Offer)));
}

#[tokio::test]
async fn optional_feedback_invitations_and_direct_response_requests_keep_their_own_scope() {
    let fx = fixture().await;
    let mut failures = Vec::new();
    for (text, expected) in [
        ("How did your appointment go? We would appreciate your feedback.", false),
        ("How was the workshop? I would welcome your feedback.", false),
        ("How did your appointment go? Please respond to me with your feedback.", true),
        ("How was the workshop?", true),
        ("How did your appointment go? We would appreciate your feedback.\n\nCould you respond with the reference number?", true),
    ] {
        let mail = source(&fx, text).await;
        let (answer, turn, errors) = observe(&fx, &mail, expected).await;
        if errors != 0 || turn != expected { failures.push(text); }
        if !expected && errors == 0 {
            assert!(answer.units.iter().any(|u| answer.quote(u) == Some(text)
                && u.tags.as_ref().is_some_and(|t| t.act == mxr_core::asks::Act::Offer)));
        }
    }
    assert!(failures.is_empty(), "wrong debt for {failures:?}");
}
