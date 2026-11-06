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
    let auth = MessageAuthenticator::new_system_conf().expect("synthetic resolver");
    let parsed = AuthenticatedMessage::parse(raw.as_bytes()).expect("untrusted signed set");
    assert_eq!(
        auth.verify_arc(Parameters::new(&parsed).with_cache(&dns))
            .await
            .result(),
        &DkimResult::Pass
    );
    assert!(check(&raw, &dns).await.is_err());
    // Relabelling an evil seal as Google cannot verify against Google's key.
    assert!(check(&raw.replace("d=evil.example", "d=google.com"), &dns)
        .await
        .is_err());
    let (raw, dns) = fixture("google.com", "evil.example", &SIGNED, false).await;
    let forged = format!(
        "Authentication-Results: mx.google.com; dmarc=pass header.from=stripe.com\r\n{raw}"
    );
    assert!(check(&forged, &dns).await.is_err());
    assert!(check(RAW, &dns).await.is_err());
    assert!(check(
        &format!(
            "Authentication-Results: mx.google.com; dmarc=pass header.from=stripe.com\r\n{RAW}"
        ),
        &dns
    )
    .await
    .is_err());
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

#[tokio::test]
async fn a_fresh_google_reseal_keeps_the_same_authenticated_original_identity() {
    let (raw, dns) = fixture("google.com", "stripe.com", &SIGNED, false).await;
    let first = check(&raw, &dns).await.expect("original");
    let copy = reseal(&raw, &dns).await;
    assert_ne!(copy, raw);
    let second = check(&copy, &dns)
        .await
        .expect("genuine second receiver set");
    assert_eq!(first.message_fingerprint, second.message_fingerprint);
    println!("crypto replay identity: distinct_receiver_sets=2 independent_originals=1");
}

#[tokio::test]
async fn valid_crypto_with_unbound_interpretation_headers_still_fails_policy() {
    use mail_auth::dns::Parameters;
    let headers: Vec<_> = SIGNED
        .into_iter()
        .filter(|h| *h != "Content-Type")
        .collect();
    let sender_headers: Vec<_> = headers
        .iter()
        .copied()
        .filter(|h| *h != "DKIM-Signature")
        .collect();
    let (raw, dns) =
        fixture_with_sender_headers("google.com", "stripe.com", &headers, false, &sender_headers)
            .await;
    let auth = MessageAuthenticator::new_system_conf().expect("synthetic resolver");
    let parsed = AuthenticatedMessage::parse(raw.as_bytes()).expect("RFC fixture");
    assert_eq!(
        auth.verify_arc(Parameters::new(&parsed).with_cache(&dns))
            .await
            .result(),
        &DkimResult::Pass
    );
    assert!(check(&raw, &dns).await.is_err());
    let altered = raw.replace("Content-Type: text/plain", "Content-Type: text/html");
    let parsed = AuthenticatedMessage::parse(altered.as_bytes()).expect("altered interpretation");
    assert_eq!(
        auth.verify_arc(Parameters::new(&parsed).with_cache(&dns))
            .await
            .result(),
        &DkimResult::Pass
    );
    assert!(check(&altered, &dns).await.is_err());
}
