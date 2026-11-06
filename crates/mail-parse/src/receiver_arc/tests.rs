use super::test_fixtures::*;
use super::*;
use chrono::Duration;
async fn check(raw: &str, dns: &FakeDns) -> Result<ArrivalEvidence, &'static str> {
    let auth = MessageAuthenticator::new_system_conf().expect("system resolver construction");
    verify_with_cache(
        &auth,
        dns,
        raw.as_bytes(),
        "notifications@stripe.com",
        ReceiverPolicy {
            sealer: "google.com",
            authserv_id: "mx.google.com",
        },
        Utc::now(),
    )
    .await
}
#[tokio::test]
async fn receiver_crypto_binds_origin_body_interpretation_and_replay_identity() {
    let (raw, dns) = fixture("google.com", "stripe.com", &SIGNED, false).await;
    let original = check(&raw, &dns).await.expect("genuine receiver assertion");
    assert!(matches!(original.sender, SenderCheck::Verified { .. }));
    assert!(original.message_fingerprint.is_some());
    assert!(!original.is_live);
    for trace in [
        "Delivered-To: other@example.com\r\n",
        "Received: forged new delivery\r\n",
    ] {
        let copy = check(&format!("{trace}{raw}"), &dns)
            .await
            .expect("intact copy");
        assert_eq!(copy.message_fingerprint, original.message_fingerprint);
        assert_eq!(copy.received_at, original.received_at);
    }
    for changed in [
        raw.replace("Your payment failed. Please", "Your payment paid. Please"),
        raw.replace(
            "Subject: Your payment of $20.00 failed",
            "Subject: forged task",
        ),
        raw.replace("header.from=stripe.com", "header.from=evil.example"),
        format!("From: Evil <attacker@evil.example>\r\n{raw}"),
        format!("Subject: forged\r\n{raw}"),
        format!(
            "{}{}",
            &raw[..raw.find("DKIM-Signature:").expect("sender signature")],
            raw
        ),
        raw.replace("Content-Type: text/plain", "Content-Type: text/html"),
    ] {
        assert!(check(&changed, &dns).await.is_err());
    }
    println!("crypto: genuine=1 intact_copy_residual=2 tampered_or_ambiguous_rejected=7");
}
#[tokio::test]
async fn receiver_policy_rejects_passing_untrusted_sealers_and_bare_results() {
    let (raw, dns) = fixture("evil.example", "stripe.com", &SIGNED, false).await;
    assert!(check(&raw, &dns).await.is_err());
    let (raw, dns) = fixture("google.com", "evil.example", &SIGNED, false).await;
    let forged = format!(
        "Authentication-Results: mx.google.com; dmarc=pass header.from=stripe.com\r\n{raw}"
    );
    assert!(check(&forged, &dns).await.is_err());
    assert!(check(RAW, &dns).await.is_err());
    let (raw, dns) = fixture("google.com", "stripe.com", &SIGNED, true).await;
    assert!(check(&raw, &dns).await.is_err());
}
#[tokio::test]
async fn future_receiver_clock_rejected_and_old_clock_cannot_be_freshened() {
    let (raw, dns) = fixture("google.com", "stripe.com", &SIGNED, false).await;
    let auth = MessageAuthenticator::new_system_conf().expect("system resolver construction");
    let at = check(&raw, &dns)
        .await
        .expect("valid")
        .received_at
        .expect("signed time");
    let policy = ReceiverPolicy {
        sealer: "google.com",
        authserv_id: "mx.google.com",
    };
    assert!(verify_with_cache(
        &auth,
        &dns,
        raw.as_bytes(),
        "notifications@stripe.com",
        policy,
        at - Duration::seconds(1)
    )
    .await
    .is_err());
    let old = verify_with_cache(
        &auth,
        &dns,
        raw.as_bytes(),
        "notifications@stripe.com",
        policy,
        at + Duration::days(90),
    )
    .await
    .expect("old evidence retained for history");
    assert_eq!(old.received_at, Some(at));
}
