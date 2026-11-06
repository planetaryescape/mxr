#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap fixture setup for direct failures"
)]

//! `use_tls = false` reaches a plaintext loopback bridge, and is refused for
//! anything else. Drives the production session factory over real sockets.

use async_std::io::{ReadExt, WriteExt};
use async_std::net::TcpListener;
use mxr_provider_imap::config::ImapConfig;
use mxr_provider_imap::session::{ImapSessionFactory, RealImapSessionFactory};

/// A bridge that greets, accepts any LOGIN and then idles.
async fn spawn_plain_bridge() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    async_std::task::spawn(async move {
        let Ok((mut tcp, _)) = listener.accept().await else {
            return;
        };
        let _ = tcp.write_all(b"* OK bridge ready\r\n").await;
        let mut buf = [0u8; 512];
        while let Ok(n) = tcp.read(&mut buf).await {
            if n == 0 {
                break;
            }
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let tag = request.split_whitespace().next().unwrap_or("a").to_string();
            let _ = tcp.write_all(format!("{tag} OK done\r\n").as_bytes()).await;
        }
    });
    port
}

#[tokio::test]
async fn plaintext_session_opens_against_a_loopback_bridge() {
    let port = spawn_plain_bridge().await;
    let config = ImapConfig::new(
        "127.0.0.1".into(),
        port,
        "user".into(),
        "mxr/test".into(),
        true,
        false,
    )
    .with_password("secret".into());

    let session = RealImapSessionFactory::new(config).create_session().await;

    assert!(session.is_ok(), "{:?}", session.err());
}

#[tokio::test]
async fn a_pin_on_a_plaintext_account_is_an_error_not_a_silent_no_op() {
    let port = spawn_plain_bridge().await;
    let config = ImapConfig::new(
        "127.0.0.1".into(),
        port,
        "user".into(),
        "mxr/test".into(),
        true,
        false,
    )
    .with_tls_cert_sha256(Some("ab".repeat(32)))
    .unwrap()
    .with_password("secret".into());

    let error = RealImapSessionFactory::new(config)
        .create_session()
        .await
        .map(|_| ())
        .unwrap_err()
        .to_string();

    assert!(error.contains("use_tls = false"), "{error}");
}

#[tokio::test]
async fn plaintext_is_refused_for_a_remote_host_before_any_connection() {
    let config = ImapConfig::new(
        "imap.example.com".into(),
        143,
        "user".into(),
        "mxr/test".into(),
        true,
        false,
    )
    .with_password("secret".into());

    let error = RealImapSessionFactory::new(config)
        .create_session()
        .await
        .map(|_| ())
        .unwrap_err()
        .to_string();

    assert!(error.contains("refusing to connect"), "{error}");
    assert!(error.contains("unencrypted"), "{error}");
}
