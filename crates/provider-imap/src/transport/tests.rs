#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

use super::*;
use async_std::net::TcpListener;

#[test]
fn loopback_hosts_are_recognised() {
    for host in [
        "127.0.0.1",
        "127.0.0.2",
        "127.255.255.254",
        "::1",
        "[::1]",
        "::ffff:127.0.0.1",
        "localhost",
        "LocalHost",
        "localhost.",
    ] {
        assert!(is_loopback_host(host), "{host} should be loopback");
    }
}

#[test]
fn non_loopback_hosts_are_refused() {
    for host in [
        "127.0.0.1.example.com",
        "0.0.0.0",
        "::",
        "128.0.0.1",
        "10.0.0.5",
        "192.168.1.10",
        "imap.example.com",
        "notlocalhost",
        "localhost.example.com",
        "",
    ] {
        assert!(!is_loopback_host(host), "{host} must not be loopback");
    }
}

#[test]
fn plaintext_is_refused_for_remote_hosts_with_an_actionable_error() {
    let error = TransportMode::resolve("imap.example.com", 143, false).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("unencrypted"), "{message}");
    assert!(message.contains("993"), "{message}");
    assert!(TransportMode::resolve("127.0.0.1.example.com", 143, false).is_err());
    assert!(TransportMode::resolve("0.0.0.0", 143, false).is_err());
}

#[test]
fn mode_follows_use_tls_and_port() {
    assert_eq!(
        TransportMode::resolve("127.0.0.1", 3143, false).unwrap(),
        TransportMode::Plaintext
    );
    assert_eq!(
        TransportMode::resolve("imap.example.com", 993, true).unwrap(),
        TransportMode::ImplicitTls
    );
    assert_eq!(
        TransportMode::resolve("imap.example.com", 143, true).unwrap(),
        TransportMode::StartTls
    );
    // use_tls = true on a loopback bridge still means TLS.
    assert_eq!(
        TransportMode::resolve("127.0.0.1", 1143, true).unwrap(),
        TransportMode::ImplicitTls
    );
}

#[test]
fn fingerprint_parses_hex_with_separators_and_rejects_the_rest() {
    let plain = "ab".repeat(32);
    let colons = vec!["AB"; 32].join(":");
    assert_eq!(
        CertFingerprint::parse(&plain).unwrap(),
        CertFingerprint::parse(&colons).unwrap()
    );
    assert_eq!(CertFingerprint::parse(&plain).unwrap().to_string(), plain);
    assert!(CertFingerprint::parse(&"ab".repeat(31)).is_err());
    assert!(CertFingerprint::parse(&"zz".repeat(32)).is_err());
    assert!(CertFingerprint::parse("").is_err());
}

#[test]
fn starttls_is_detected_in_capability_lines() {
    let lines = |text: &str| vec![text.to_string()];
    assert!(advertises_starttls(&lines(
        "* CAPABILITY IMAP4rev1 starttls LOGINDISABLED\r\n"
    )));
    assert!(!advertises_starttls(&lines(
        "* CAPABILITY IMAP4rev1 AUTH=PLAIN\r\n"
    )));
    // A mailbox or text that merely mentions the word is not a capability.
    assert!(!advertises_starttls(&lines("* OK STARTTLS later\r\n")));
}

// -- Local server tests ------------------------------------------------------

struct TestCert {
    acceptor: async_native_tls::TlsAcceptor,
    fingerprint: CertFingerprint,
}

/// Self-signed RSA fixture (CN=mxr-test-imap, valid for 100 years). Test
/// only: it protects nothing, and exists because macOS cannot import the
/// EC keys a generator would produce.
fn test_cert() -> TestCert {
    let cert_pem = include_bytes!("../test_fixtures/test-server.cert.pem");
    let key_pem = include_bytes!("../test_fixtures/test-server.key.pem");
    let identity = native_tls::Identity::from_pkcs8(cert_pem, key_pem).unwrap();
    let der = native_tls::Certificate::from_pem(cert_pem)
        .unwrap()
        .to_der()
        .unwrap();
    TestCert {
        acceptor: native_tls::TlsAcceptor::new(identity).unwrap().into(),
        fingerprint: CertFingerprint::of_der(&der),
    }
}

/// A well-formed fingerprint that is not the fixture's.
fn other_fingerprint(real: CertFingerprint) -> CertFingerprint {
    let mut bytes = real.0;
    bytes[0] ^= 0xff;
    CertFingerprint(bytes)
}

async fn bind_local() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// Serve one implicit-TLS connection: handshake, greet, then answer one read.
fn serve_implicit_tls(listener: TcpListener, cert: &TestCert) {
    let acceptor = cert.acceptor.clone();
    async_std::task::spawn(async move {
        if let Ok((tcp, _)) = listener.accept().await {
            if let Ok(mut tls) = acceptor.accept(tcp).await {
                let _ = tls.write_all(b"* OK ready\r\n").await;
                let _ = tls.flush().await;
                let mut sink = [0u8; 64];
                let _ = tls.read(&mut sink).await;
            }
        }
    });
}

/// Serve one plaintext connection that greets, then optionally speaks
/// STARTTLS and upgrades.
fn serve_plain(listener: TcpListener, starttls: Option<TestCert>) {
    async_std::task::spawn(async move {
        let Ok((mut tcp, _)) = listener.accept().await else {
            return;
        };
        let _ = tcp.write_all(b"* OK ready\r\n").await;
        let Some(cert) = starttls else {
            // Plain bridge or a server without STARTTLS: answer CAPABILITY
            // without it, then hold the socket until the client leaves.
            let mut buf = [0u8; 256];
            if let Ok(n) = tcp.read(&mut buf).await {
                if n > 0 {
                    let _ = tcp
                        .write_all(b"* CAPABILITY IMAP4rev1\r\nmxr1 OK done\r\n")
                        .await;
                }
            }
            let _ = tcp.read(&mut buf).await;
            return;
        };
        let mut buf = [0u8; 256];
        let _ = tcp.read(&mut buf).await;
        let _ = tcp
            .write_all(b"* CAPABILITY IMAP4rev1 STARTTLS\r\nmxr1 OK done\r\n")
            .await;
        let _ = tcp.read(&mut buf).await;
        let _ = tcp.write_all(b"mxr2 OK begin TLS\r\n").await;
        if let Ok(mut tls) = cert.acceptor.accept(tcp).await {
            let mut sink = [0u8; 64];
            let _ = tls.read(&mut sink).await;
        }
    });
}

#[tokio::test]
async fn plaintext_loopback_bridge_connects() {
    let (listener, port) = bind_local().await;
    serve_plain(listener, None);
    let (_client, greeting) =
        connect_mode("127.0.0.1", port, TransportMode::Plaintext, Verify::System)
            .await
            .unwrap();
    assert_eq!(greeting, Greeting::Ready);
}

#[tokio::test]
async fn pinned_certificate_is_accepted_even_when_self_signed() {
    let cert = test_cert();
    let (listener, port) = bind_local().await;
    let pin = cert.fingerprint;
    serve_implicit_tls(listener, &cert);
    let (_client, greeting) = connect_mode(
        "127.0.0.1",
        port,
        TransportMode::ImplicitTls,
        Verify::Pinned(pin),
    )
    .await
    .unwrap();
    assert_eq!(greeting, Greeting::Ready);
}

#[tokio::test]
async fn a_different_pin_is_rejected_and_reports_both_fingerprints() {
    let cert = test_cert();

    let (listener, port) = bind_local().await;
    serve_implicit_tls(listener, &cert);
    let error = connect_mode(
        "127.0.0.1",
        port,
        TransportMode::ImplicitTls,
        Verify::Pinned(other_fingerprint(cert.fingerprint)),
    )
    .await
    .map(|_| ())
    .unwrap_err()
    .to_string();
    assert!(error.contains("does not match tls_cert_sha256"), "{error}");
    assert!(error.contains(&cert.fingerprint.to_string()), "{error}");
}

#[tokio::test]
async fn self_signed_certificate_without_a_pin_fails_verification() {
    let cert = test_cert();
    let (listener, port) = bind_local().await;
    serve_implicit_tls(listener, &cert);
    let result = connect_mode(
        "127.0.0.1",
        port,
        TransportMode::ImplicitTls,
        Verify::System,
    )
    .await;
    assert!(result.is_err(), "unpinned self-signed cert must be refused");
}

#[tokio::test]
async fn tls_against_a_plaintext_server_names_the_mismatch() {
    let (listener, port) = bind_local().await;
    // Two connections arrive: the failed handshake, then the plaintext probe.
    async_std::task::spawn(async move {
        for _ in 0..2 {
            if let Ok((mut tcp, _)) = listener.accept().await {
                let _ = tcp.write_all(b"* OK ready\r\n").await;
                async_std::task::spawn(async move {
                    let mut buf = [0u8; 64];
                    let _ = tcp.read(&mut buf).await;
                });
            }
        }
    });
    let error = connect_mode(
        "127.0.0.1",
        port,
        TransportMode::ImplicitTls,
        Verify::System,
    )
    .await
    .map(|_| ())
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("answered in plaintext on a TLS connection"),
        "{error}"
    );
    assert!(error.contains("use_tls = false"), "{error}");
    assert!(error.contains("993"), "{error}");
}

#[tokio::test]
async fn starttls_not_advertised_is_refused() {
    let (listener, port) = bind_local().await;
    serve_plain(listener, None);
    let error = connect_mode("127.0.0.1", port, TransportMode::StartTls, Verify::System)
        .await
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not advertise STARTTLS"), "{error}");
    assert!(error.contains("plaintext"), "{error}");
}

#[tokio::test]
async fn starttls_upgrades_and_enforces_the_pin() {
    let cert = test_cert();
    let (listener, port) = bind_local().await;
    let pin = cert.fingerprint;
    serve_plain(listener, Some(cert));
    let (_client, greeting) = connect_mode(
        "127.0.0.1",
        port,
        TransportMode::StartTls,
        Verify::Pinned(pin),
    )
    .await
    .unwrap();
    assert_eq!(greeting, Greeting::Ready);

    let cert = test_cert();
    let wrong = other_fingerprint(cert.fingerprint);
    let (listener, port) = bind_local().await;
    serve_plain(listener, Some(cert));
    let result = connect_mode(
        "127.0.0.1",
        port,
        TransportMode::StartTls,
        Verify::Pinned(wrong),
    )
    .await;
    assert!(result.is_err(), "a different pin must fail after STARTTLS");
}

#[tokio::test]
async fn probe_reports_the_certificate_fingerprint_without_trusting_it() {
    let cert = test_cert();
    let expected = cert.fingerprint.to_string();
    let (listener, port) = bind_local().await;
    serve_implicit_tls(listener, &cert);
    let config = ImapConfig::new(
        "127.0.0.1".into(),
        port,
        "user".into(),
        "mxr/test".into(),
        true,
        true,
    );
    let fingerprint = probe_server_certificate_sha256(&config).await.unwrap();
    assert_eq!(fingerprint, Some(expected));
}

#[tokio::test]
async fn inspection_and_pin_refusal_never_send_authentication() {
    use crate::session::{ImapSessionFactory, RealImapSessionFactory};
    use mxr_core::provider::MailSyncProvider;

    for inspect in [true, false] {
        let cert = test_cert();
        let expected = cert.fingerprint.to_string();
        let wrong = other_fingerprint(cert.fingerprint).to_string();
        let (listener, port) = bind_local().await;
        let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
        async_std::task::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept test connection");
            let mut tls = cert.acceptor.accept(tcp).await.expect("complete test TLS");
            let _ = tls.write_all(b"* OK ready\r\n").await;
            let _ = tls.flush().await;
            let mut bytes = [0; 2048];
            let count = tls.read(&mut bytes).await.unwrap_or(0);
            let _ = seen_tx.send(bytes[..count].to_vec());
        });
        let config = ImapConfig::new(
            "127.0.0.1".into(),
            port,
            "probe-user".into(),
            "mxr/test".into(),
            true,
            true,
        )
        .with_tls_cert_sha256(Some(wrong))
        .expect("valid mismatching pin")
        .with_password("probe-password".into());

        if inspect {
            let provider = crate::ImapProvider::new(
                mxr_core::AccountId::from_provider_id("imap", "transport-test"),
                config,
            );
            assert_eq!(
                provider
                    .server_certificate_sha256()
                    .await
                    .expect("inspect certificate"),
                Some(expected)
            );
        } else {
            let error = RealImapSessionFactory::new(config)
                .create_session()
                .await
                .err()
                .expect("refuse mismatching pin");
            assert!(
                error.to_string().contains("does not match tls_cert_sha256"),
                "{error}"
            );
        }
        let sent = tokio::time::timeout(Duration::from_secs(5), seen_rx)
            .await
            .expect("server observes connection close")
            .expect("server reports captured bytes");
        assert!(
            sent.is_empty(),
            "no IMAP command may follow inspection or pin refusal"
        );
    }
}

#[tokio::test]
async fn probe_gives_up_on_a_server_that_accepts_and_stays_silent() {
    let (listener, port) = bind_local().await;
    let (seen_tx, seen_rx) = tokio::sync::oneshot::channel::<Vec<u8>>();
    async_std::task::spawn(async move {
        if let Ok((mut tcp, _)) = listener.accept().await {
            let mut buf = [0u8; 4096];
            let n = tcp.read(&mut buf).await.unwrap_or(0);
            let _ = seen_tx.send(buf[..n].to_vec());
            // Hold the socket open without ever replying.
            let _ = tcp.read(&mut buf).await;
        }
    });
    let config = ImapConfig::new(
        "127.0.0.1".into(),
        port,
        "user".into(),
        "mxr/test".into(),
        true,
        true,
    )
    .with_password("hunter2-secret".into());

    let started = std::time::Instant::now();
    let error = probe_within(&config, Duration::from_millis(300))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("timed out"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
    let sent = tokio::time::timeout(Duration::from_secs(5), seen_rx)
        .await
        .expect("the synthetic server observes the credential-free handshake")
        .unwrap();
    let sent = String::from_utf8_lossy(&sent);
    assert!(
        !sent.contains("hunter2-secret") && !sent.contains("LOGIN"),
        "{sent}"
    );
}
