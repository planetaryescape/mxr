//! One-time codes from recent mail: found by rules, newest first, current
//! while in their window, never a promo code.

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::{Duration, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{MessageBody, MessageDirection, MessageMetadata};
use mxr_protocol::OneTimeCodeData;

async fn mail(
    fx: &Fixture,
    from: &str,
    subject: &str,
    age: Duration,
    body: Option<&str>,
) -> MessageId {
    let mut envelope = fx.message(&ThreadId::new(), from, ME, age, None).await;
    envelope.subject = subject.to_string();
    envelope.date = Utc::now() - age;
    fx.store_envelope(&envelope, MessageDirection::Inbound)
        .await;
    if let Some(text) = body {
        add_body(fx, &envelope.id, text).await;
    }
    envelope.id
}

async fn add_body(fx: &Fixture, id: &MessageId, text: &str) {
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: id.clone(),
            text_plain: Some(text.to_string()),
            text_html: None,
            attachments: vec![],
            fetched_at: Utc::now(),
            metadata: MessageMetadata::default(),
        })
        .await
        .unwrap();
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

/// A sender's Date header is not believed into the future: a mail dated
/// 2099 would otherwise be the latest code until 2099.
#[tokio::test]
async fn mail_dated_in_the_future_is_not_the_latest_code() {
    let fx = Fixture::new().await;
    mail(
        &fx,
        "otp@attacker.example",
        "Your verification code is 999999",
        Duration::days(-365),
        None,
    )
    .await;
    mail(
        &fx,
        "verify@x.example",
        "222222 is your X verification code",
        Duration::minutes(1),
        None,
    )
    .await;
    let codes = codes(&fx).await;
    assert_eq!(
        codes.iter().map(|c| c.code.as_str()).collect::<Vec<_>>(),
        ["222222"]
    );
    assert_eq!(codes[0].sender_host, "x.example");
}

/// A sign-in alert stays two days in Updates; its code lasts what it says.
#[tokio::test]
async fn a_sign_in_alerts_code_expires_on_its_own_clock() {
    let fx = Fixture::new().await;
    mail(
        &fx,
        "no-reply@accounts.example",
        "New sign-in to your account",
        Duration::minutes(10),
        Some("Someone signed in from Chrome.\n\nYour security code is 551204.\nIt expires in 5 minutes."),
    )
    .await;
    let codes = codes(&fx).await;
    assert_eq!(codes.len(), 1);
    assert_eq!(codes[0].code, "551204");
    assert!(!codes[0].current, "five minutes, not the alert's two days");
}

/// A code only the body gives shows once the body arrives, even after a
/// read that found no body and cached what it could see.
#[tokio::test]
async fn a_code_shows_once_its_body_arrives() {
    let fx = Fixture::new().await;
    let id = mail(
        &fx,
        "security@bank.example",
        "MFA Login Authentication",
        Duration::minutes(1),
        None,
    )
    .await;
    // Synced with headers only, as IMAP can: no body yet.
    sqlx::query("DELETE FROM bodies WHERE message_id = ?")
        .bind(id.as_str())
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    assert!(codes(&fx).await.is_empty(), "no body, no code yet");
    add_body(&fx, &id, "Your one-time passcode is 882308.").await;
    let codes = codes(&fx).await;
    assert_eq!(
        codes.iter().map(|c| c.code.as_str()).collect::<Vec<_>>(),
        ["882308"]
    );
}

/// Newer expired codes never push one that still works past the limit.
#[tokio::test]
async fn a_current_code_comes_before_newer_expired_ones() {
    let fx = Fixture::new().await;
    mail(
        &fx,
        "no-reply@long.example",
        "Your verification code is 700001",
        Duration::minutes(30),
        Some("Your verification code is 700001. It is valid for 1 hour."),
    )
    .await;
    for (i, minutes) in [12, 13, 14, 15, 16, 17].iter().enumerate() {
        mail(
            &fx,
            "no-reply@short.example",
            &format!("Your verification code is 80000{i}"),
            Duration::minutes(*minutes),
            None,
        )
        .await;
    }
    let codes = match request(
        &fx,
        Request::ListOneTimeCodes {
            account_id: None,
            limit: Some(5),
        },
    )
    .await
    {
        ResponseData::OneTimeCodes { codes } => codes,
        other => panic!("expected OneTimeCodes, got {other:?}"),
    };
    assert_eq!(codes.len(), 5);
    assert_eq!(codes[0].code, "700001");
    assert!(codes[0].current);
    assert!(codes[1..].iter().all(|code| !code.current));
}

#[tokio::test]
async fn reading_codes_is_read_only() {
    assert!(request_is_read_only(&Request::ListOneTimeCodes {
        account_id: None,
        limit: None,
    }));
}
