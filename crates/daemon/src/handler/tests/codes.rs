//! One-time codes from recent mail: found by rules, newest first, current
//! while in their window, never a promo code.

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::{Duration, Utc};
use mxr_core::id::ThreadId;
use mxr_core::types::{MessageBody, MessageDirection, MessageMetadata};
use mxr_protocol::OneTimeCodeData;

async fn mail(fx: &Fixture, from: &str, subject: &str, age: Duration, body: Option<&str>) {
    let mut envelope = fx.message(&ThreadId::new(), from, ME, age, None).await;
    envelope.subject = subject.to_string();
    envelope.date = Utc::now() - age;
    fx.store_envelope(&envelope, MessageDirection::Inbound)
        .await;
    if let Some(text) = body {
        fx.state
            .store
            .insert_body(&MessageBody {
                message_id: envelope.id.clone(),
                text_plain: Some(text.to_string()),
                text_html: None,
                attachments: vec![],
                fetched_at: envelope.date,
                metadata: MessageMetadata::default(),
            })
            .await
            .unwrap();
    }
}

async fn codes(fx: &Fixture) -> Vec<OneTimeCodeData> {
    match request(
        fx,
        Request::ListOneTimeCodes {
            account_id: None,
            limit: None,
        },
    )
    .await
    {
        ResponseData::OneTimeCodes { codes } => codes,
        other => panic!("expected OneTimeCodes, got {other:?}"),
    }
}

#[tokio::test]
async fn the_newest_code_comes_first_and_old_ones_are_expired() {
    let fx = Fixture::new().await;
    mail(
        &fx,
        "no-reply@acme.example",
        "Your verification code is 111111",
        Duration::minutes(40),
        None,
    )
    .await;
    mail(
        &fx,
        "verify@x.example",
        "222222 is your X verification code",
        Duration::minutes(2),
        None,
    )
    .await;
    let codes = codes(&fx).await;
    let listed: Vec<_> = codes.iter().map(|c| (c.code.as_str(), c.current)).collect();
    assert_eq!(listed, [("222222", true), ("111111", false)]);
    assert_eq!(codes[0].sender_email, "verify@x.example");
}

#[tokio::test]
async fn a_code_only_the_body_gives_is_found_and_a_promo_is_not() {
    let fx = Fixture::new().await;
    mail(
        &fx,
        "security@bank.example",
        "MFA Login Authentication",
        Duration::minutes(1),
        Some("Your one-time passcode is 882308. It is valid for 5 minutes."),
    )
    .await;
    mail(
        &fx,
        "deals@shop.example",
        "Your promo code is SAVE20NOW",
        Duration::minutes(1),
        None,
    )
    .await;
    let codes = codes(&fx).await;
    assert_eq!(codes.len(), 1, "{codes:?}");
    assert_eq!(codes[0].code, "882308");
    // The lifetime the message states.
    assert_eq!(
        codes[0].expires_at - codes[0].received_at,
        Duration::minutes(5)
    );
}

#[tokio::test]
async fn reading_codes_is_read_only() {
    assert!(request_is_read_only(&Request::ListOneTimeCodes {
        account_id: None,
        limit: None,
    }));
}
