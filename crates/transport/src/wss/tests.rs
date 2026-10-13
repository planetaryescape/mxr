#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "private TLS fixtures assert protocol outcomes"
)]
use super::*;
use rustls::pki_types::PrivatePkcs8KeyDer;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::protocol::frame::{
    coding::{Data, OpCode},
    Frame,
};

async fn fixture() -> (TcpListener, TlsAcceptor, WssConnector) {
    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let der = certificate.cert.der().clone();
    let key = PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der());
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let server = rustls::ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![der.clone()], key.into())
        .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(der).unwrap();
    let client = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut connector = WssConnector::new(
        format!(
            "https://localhost:{}",
            listener.local_addr().unwrap().port()
        ),
        "own-client",
        &"a".repeat(64),
    )
    .unwrap();
    connector.test_tls = Some(tokio_tungstenite::Connector::Rustls(Arc::new(client)));
    (listener, TlsAcceptor::from(Arc::new(server)), connector)
}

#[test]
fn origin_normalization_rejects_credentials_downgrades_and_resource_urls() {
    for value in [
        "http://localhost",
        "https://user@example.com",
        "https://user:password@example.com",
        "https://example.com/path",
        "https://example.com?token=x",
        "https://example.com#token",
        "wss://example.com",
        "https://example.com:bad",
    ] {
        assert!(https_origin(value).is_err(), "{value}");
    }
    assert_eq!(
        https_origin("https://EXAMPLE.com:443/").unwrap(),
        "https://example.com"
    );
}

#[tokio::test]
async fn partial_and_coalesced_binary_messages_preserve_bytes_and_drop_closes_socket() {
    let (listener, tls, connector) = fixture().await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let tls = tls.accept(tcp).await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tls).await.unwrap();
        let mut received = Vec::new();
        while received.len() < 100_000 {
            let Message::Binary(data) = ws.next().await.unwrap().unwrap() else {
                panic!("binary frame expected");
            };
            received.extend_from_slice(&data);
        }
        assert_eq!(received, vec![7; 100_000]);
        for data in [b"ab".as_slice(), b"cdef".as_slice()] {
            ws.send(Message::Binary(data.to_vec().into()))
                .await
                .unwrap();
        }
        // Dropping the byte-stream must drop its owned bridge task and socket.
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
                .await
                .is_ok()
        );
    });
    let mut io = connector.connect().await.unwrap();
    io.write_all(&vec![7; 100_000]).await.unwrap();
    let mut received = [0; 6];
    io.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, b"abcdef");
    drop(io);
    server.await.unwrap();
}

#[tokio::test]
async fn fragmented_binary_messages_are_reassembled_as_one_byte_stream() {
    let (listener, tls, connector) = fixture().await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let tls = tls.accept(tcp).await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tls).await.unwrap();
        ws.send(Message::Frame(Frame::message(
            b"frag".to_vec(),
            OpCode::Data(Data::Binary),
            false,
        )))
        .await
        .unwrap();
        ws.send(Message::Frame(Frame::message(
            b"mented".to_vec(),
            OpCode::Data(Data::Continue),
            true,
        )))
        .await
        .unwrap();
    });
    let mut io = connector.connect().await.unwrap();
    let mut received = [0; 10];
    io.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, b"fragmented");
    server.await.unwrap();
}

#[tokio::test]
async fn large_binary_response_drains_through_small_reads() {
    let (listener, tls, connector) = fixture().await;
    let payload = vec![42; 1024 * 1024];
    let server_payload = payload.clone();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let tls = tls.accept(tcp).await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tls).await.unwrap();
        ws.send(Message::Binary(server_payload.into()))
            .await
            .unwrap();
    });
    let mut io = connector.connect().await.unwrap();
    let mut received = Vec::with_capacity(payload.len());
    let mut chunk = [0; 512];
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while received.len() < payload.len() {
            let remaining = payload.len() - received.len();
            let size = chunk.len().min(remaining);
            io.read_exact(&mut chunk[..size]).await.unwrap();
            received.extend_from_slice(&chunk[..size]);
        }
    })
    .await
    .unwrap();
    assert_eq!(received, payload);
    server.await.unwrap();
}

#[tokio::test]
async fn cancelling_a_pending_websocket_handshake_closes_its_tls_stream() {
    let (listener, tls, connector) = fixture().await;
    let (handshake_sent, handshake_received) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut tls = tls.accept(tcp).await.unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            tls.read_exact(&mut byte).await.unwrap();
            request.push(byte[0]);
        }
        handshake_sent.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), tls.read(&mut byte))
            .await
            .unwrap()
    });
    let connecting = tokio::spawn(async move { connector.connect().await });
    handshake_received.await.unwrap();
    connecting.abort();
    let result = server.await.unwrap();
    assert!(result.is_err() || result.unwrap() == 0);
}

#[tokio::test]
async fn text_messages_close_the_binary_byte_stream() {
    let (listener, tls, connector) = fixture().await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let tls = tls.accept(tcp).await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tls).await.unwrap();
        ws.send(Message::Text("not an IPC frame".into()))
            .await
            .unwrap();
    });
    let mut io = connector.connect().await.unwrap();
    let mut byte = [0];
    assert_eq!(io.read(&mut byte).await.unwrap(), 0);
    server.await.unwrap();
}

#[tokio::test]
async fn redirect_never_sends_credentials_to_second_origin_and_errors_are_redacted() {
    let (listener, tls, connector) = fixture().await;
    let second = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = second.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut tls = tls.accept(tcp).await.unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            tls.read_exact(&mut byte).await.unwrap();
            request.push(byte[0]);
        }
        assert!(String::from_utf8(request)
            .unwrap()
            .contains("Bearer own-client:"));
        let response=format!("HTTP/1.1 302 Found\r\nLocation: https://localhost:{port}/api/v1/remote/ipc\r\nX-Private-Token: {}\r\nContent-Length: 0\r\n\r\n","a".repeat(64));
        tls.write_all(response.as_bytes()).await.unwrap();
    });
    let error = connector.connect().await.err().unwrap().to_string();
    assert!(!error.contains(&"a".repeat(64)));
    assert!(!error.contains("X-Private-Token"));
    server.await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), second.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn untrusted_certificate_is_rejected_without_websocket_credentials() {
    let (listener, tls, mut connector) = fixture().await;
    connector.test_tls = None;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        assert!(tls.accept(tcp).await.is_err());
    });
    assert!(connector.connect().await.is_err());
    server.await.unwrap();
}

#[tokio::test]
async fn oversized_messages_close_the_byte_stream_without_unbounded_buffering() {
    let (listener, tls, connector) = fixture().await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let tls = tls.accept(tcp).await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tls).await.unwrap();
        let _ = ws.send(Message::Binary(vec![0; LIMIT + 1].into())).await;
    });
    let mut io = connector.connect().await.unwrap();
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(3), io.read(&mut byte))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    server.await.unwrap();
}
