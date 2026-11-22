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
fn plaintext_is_refused_for_remote_hosts_with_an_actionable_error() {
    let error = TransportMode::resolve("imap.example.com", 143, false).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("unencrypted"), "{message}");
    assert!(message.contains("993"), "{message}");
    assert!(TransportMode::resolve("127.0.0.1.example.com", 143, false).is_err());
    assert!(TransportMode::resolve("0.0.0.0", 143, false).is_err());
}

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

#[tokio::test]
async fn rejects_starttls_response_with_excessive_untagged_lines() {
    let (listener, port) = bind_local().await;
    let server = async_std::task::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = read_line(&mut socket).await.unwrap();
        assert_eq!(request, "mxr1 CAPABILITY\r\n");
        let line = format!("* OK {}\r\n", "x".repeat(1024));
        let response = format!("{}mxr1 OK done\r\n", line.repeat(100));
        let _ = socket.write_all(response.as_bytes()).await;
    });
    let mut socket = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let result = async_std::future::timeout(
        Duration::from_secs(5),
        command(&mut socket, "mxr1", "CAPABILITY"),
    )
    .await
    .expect("a finite oversized response must not hang");
    drop(socket);
    server.await;
    let error = result
        .map(|_| ())
        .expect_err("individual short lines must still share a response budget");
    assert!(error.to_string().contains("oversized response"), "{error}");
}

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
