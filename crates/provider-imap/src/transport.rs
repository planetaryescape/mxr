#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

//! How an IMAP connection is secured: implicit TLS, STARTTLS, or (loopback
//! only) plaintext, plus optional pinning of the server certificate.
//!
//! The decision lives here, in one place, so the sync session, the IDLE
//! watcher and `mxr accounts test` cannot drift apart on what they accept.

use std::fmt;
use std::net::{IpAddr, Ipv6Addr};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use async_std::net::TcpStream;
use constant_time_eq::constant_time_eq_32;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use sha2::{Digest, Sha256};

use crate::config::ImapConfig;
use crate::error::ImapProviderError;

type Result<T> = std::result::Result<T, ImapProviderError>;

/// The one port where `use_tls = true` means STARTTLS rather than implicit TLS.
pub const STARTTLS_PORT: u16 = 143;

/// Longest server line the STARTTLS negotiation will buffer. The negotiation
/// reads byte by byte, so nothing past the tagged reply is ever consumed
/// before the TLS handshake takes over the socket.
const MAX_NEGOTIATION_LINE: usize = 8 * 1024;

/// How long to wait for a plaintext greeting when working out whether a failed
/// TLS handshake was really a plaintext server.
const PLAINTEXT_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    /// TLS from the first byte (port 993).
    ImplicitTls,
    /// Plain connect, `STARTTLS`, then TLS (port 143).
    StartTls,
    /// No encryption. Only ever chosen for loopback hosts.
    Plaintext,
}

impl TransportMode {
    /// Pick the mode for an account, or refuse a combination that would send
    /// the password in the clear.
    pub fn resolve(host: &str, port: u16, use_tls: bool) -> Result<Self> {
        if use_tls {
            return Ok(if port == STARTTLS_PORT {
                Self::StartTls
            } else {
                Self::ImplicitTls
            });
        }
        if is_loopback_host(host) {
            return Ok(Self::Plaintext);
        }
        Err(ImapProviderError::Connection(format!(
            "refusing to connect to {host}:{port} without TLS: use_tls = false would send \
             your password and mail unencrypted. Plaintext IMAP is only allowed for loopback \
             hosts (127.0.0.0/8, ::1, localhost). Set use_tls = true and use port 993 (implicit \
             TLS), or port 143 for STARTTLS."
        )))
    }
}

/// True for `localhost` and for literal loopback addresses. Names that merely
/// contain `127.` (`127.0.0.1.example.com`) and `0.0.0.0` are not loopback.
pub fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    // `[::1]` is how a literal IPv6 host is written in a URL-like host field.
    let literal = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = literal.parse::<IpAddr>() {
        return is_loopback_ip(ip);
    }
    host.trim_end_matches('.').eq_ignore_ascii_case("localhost")
}

fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback() || is_mapped_loopback(v6),
    }
}

fn is_mapped_loopback(v6: Ipv6Addr) -> bool {
    v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
}

// -- Certificate pinning --------------------------------------------------------

/// SHA-256 of a server's leaf certificate (DER).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CertFingerprint([u8; 32]);

impl CertFingerprint {
    pub fn of_der(der: &[u8]) -> Self {
        Self(Sha256::digest(der).into())
    }

    /// Parse 64 hex digits. Colons and whitespace between bytes are accepted so
    /// an `openssl x509 -fingerprint` line can be pasted as is.
    pub fn parse(input: &str) -> Result<Self> {
        let digits: Vec<u8> = input
            .bytes()
            .filter(|b| *b != b':' && !b.is_ascii_whitespace())
            .collect();
        let invalid = || {
            ImapProviderError::Connection(
                "tls_cert_sha256 must be the SHA-256 of the server certificate as 64 hex digits \
                 (run `mxr accounts test <account>` to print it)"
                    .into(),
            )
        };
        if digits.len() != 64 {
            return Err(invalid());
        }
        let mut bytes = [0u8; 32];
        let (pairs, _) = digits.as_chunks::<2>();
        for (slot, [hi, lo]) in bytes.iter_mut().zip(pairs) {
            let hi = hex_value(*hi).ok_or_else(invalid)?;
            let lo = hex_value(*lo).ok_or_else(invalid)?;
            *slot = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }

    /// Constant time, so a probing server learns nothing from timing.
    pub fn matches(&self, other: &Self) -> bool {
        constant_time_eq_32(&self.0, &other.0)
    }
}

fn hex_value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

impl fmt::Display for CertFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for CertFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CertFingerprint({self})")
    }
}

/// What the TLS handshake must prove about the server.
#[derive(Debug, Clone, Copy)]
enum Verify {
    /// Platform trust store and hostname, the normal case.
    System,
    /// Accept exactly this leaf certificate, even self-signed, and nothing
    /// else. Chain and hostname checks are replaced by the pin.
    Pinned(CertFingerprint),
    /// Trust nothing; only read the certificate so the user can copy its
    /// fingerprint. No credentials are ever sent on a connection opened this way.
    InspectOnly,
}

// -- Stream ---------------------------------------------------------------------

/// Either a TLS stream or, for loopback bridges, the bare socket.
pub(crate) enum ImapStream {
    Plain(TcpStream),
    Tls(async_native_tls::TlsStream<TcpStream>),
}

impl fmt::Debug for ImapStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plain(_) => f.write_str("ImapStream::Plain"),
            Self::Tls(_) => f.write_str("ImapStream::Tls"),
        }
    }
}

impl AsyncRead for ImapStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            Self::Tls(stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ImapStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            Self::Tls(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_flush(cx),
            Self::Tls(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_close(cx),
            Self::Tls(stream) => Pin::new(stream).poll_close(cx),
        }
    }
}

/// The server's opening status line, reduced to what auth needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Greeting {
    Ready,
    PreAuth,
}

// -- Connecting -----------------------------------------------------------------

/// Open a connection that is secured per the account config and positioned
/// right after the server greeting, ready for LOGIN / AUTHENTICATE.
pub(crate) async fn connect(
    config: &ImapConfig,
) -> Result<(async_imap::Client<ImapStream>, Greeting)> {
    let mode = TransportMode::resolve(&config.host, config.port, config.use_tls)?;
    let verify = match config.pinned_fingerprint()? {
        Some(_) if mode == TransportMode::Plaintext => {
            // A pin on an unencrypted connection would look like protection
            // and protect nothing.
            return Err(ImapProviderError::Connection(
                "tls_cert_sha256 is set but use_tls = false: a certificate pin only applies to \
                 a TLS connection. Remove the pin, or set use_tls = true."
                    .into(),
            ));
        }
        Some(pin) => Verify::Pinned(pin),
        None => Verify::System,
    };
    connect_mode(&config.host, config.port, mode, verify).await
}

async fn connect_mode(
    host: &str,
    port: u16,
    mode: TransportMode,
    verify: Verify,
) -> Result<(async_imap::Client<ImapStream>, Greeting)> {
    let tcp = tcp_connect(host, port).await?;
    match mode {
        TransportMode::Plaintext => {
            // `localhost` is a name, so check where it really led.
            let peer = tcp
                .peer_addr()
                .map_err(|e| ImapProviderError::Connection(e.to_string()))?;
            if !is_loopback_ip(peer.ip()) {
                return Err(ImapProviderError::Connection(format!(
                    "refusing plaintext IMAP: {host} resolved to {}, which is not a loopback \
                     address. Use TLS (port 993) for remote servers.",
                    peer.ip()
                )));
            }
            let mut client = async_imap::Client::new(ImapStream::Plain(tcp));
            let greeting = read_greeting(&mut client).await?;
            Ok((client, greeting))
        }
        TransportMode::ImplicitTls => {
            let tls = tls_handshake(host, port, tcp, verify).await?;
            let mut client = async_imap::Client::new(ImapStream::Tls(tls));
            let greeting = read_greeting(&mut client).await?;
            Ok((client, greeting))
        }
        TransportMode::StartTls => {
            let tcp = negotiate_starttls(tcp, host, port).await?;
            let tls = tls_handshake(host, port, tcp, verify).await?;
            // The server sends no second greeting after the upgrade.
            Ok((
                async_imap::Client::new(ImapStream::Tls(tls)),
                Greeting::Ready,
            ))
        }
    }
}

async fn tcp_connect(host: &str, port: u16) -> Result<TcpStream> {
    TcpStream::connect((host, port))
        .await
        .map_err(|e| ImapProviderError::Connection(e.to_string()))
}

async fn read_greeting(client: &mut async_imap::Client<ImapStream>) -> Result<Greeting> {
    let greeting = client
        .read_response()
        .await
        .ok_or_else(|| {
            ImapProviderError::Connection(
                "Server closed the IMAP connection before sending a greeting.".into(),
            )
        })?
        .map_err(|e| ImapProviderError::Connection(e.to_string()))?;
    match greeting.parsed() {
        async_imap::imap_proto::Response::Data {
            status: async_imap::imap_proto::Status::PreAuth,
            ..
        } => Ok(Greeting::PreAuth),
        async_imap::imap_proto::Response::Data {
            status: async_imap::imap_proto::Status::Ok,
            ..
        } => Ok(Greeting::Ready),
        other => Err(ImapProviderError::Connection(format!(
            "Unexpected IMAP greeting from server: {other:?}"
        ))),
    }
}

async fn tls_handshake(
    host: &str,
    port: u16,
    tcp: TcpStream,
    verify: Verify,
) -> Result<async_native_tls::TlsStream<TcpStream>> {
    let mut connector = async_native_tls::TlsConnector::new();
    if !matches!(verify, Verify::System) {
        // Verification is replaced by the pin comparison below (or, for
        // InspectOnly, skipped because nothing secret is sent).
        connector = connector
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true);
    }
    let stream = match connector.connect(host, tcp).await {
        Ok(stream) => stream,
        Err(error) => return Err(describe_tls_failure(host, port, &error.to_string()).await),
    };
    if let Verify::Pinned(expected) = verify {
        let actual = peer_fingerprint(&stream)?;
        if !actual.matches(&expected) {
            return Err(ImapProviderError::Connection(format!(
                "the certificate presented by {host}:{port} (SHA-256 {actual}) does not match \
                 tls_cert_sha256 ({expected}). If you expected the certificate to change, run \
                 `mxr accounts test <account>` and update tls_cert_sha256; otherwise treat this \
                 server as untrusted."
            )));
        }
    }
    Ok(stream)
}

fn peer_fingerprint(stream: &async_native_tls::TlsStream<TcpStream>) -> Result<CertFingerprint> {
    let certificate = stream
        .peer_certificate()
        .map_err(|e| ImapProviderError::Connection(format!("reading server certificate: {e}")))?
        .ok_or_else(|| {
            ImapProviderError::Connection("the server presented no certificate".into())
        })?;
    let der = certificate
        .to_der()
        .map_err(|e| ImapProviderError::Connection(format!("encoding server certificate: {e}")))?;
    Ok(CertFingerprint::of_der(&der))
}

/// Turn a failed handshake into an error that says what went wrong. A TLS
/// client talking to a plaintext server only sees a protocol error whose text
/// differs per TLS library ("record overflow", "wrong version number", ...), so
/// instead of matching those, ask the server: a plaintext IMAP server speaks
/// first, a TLS server waits for us.
async fn describe_tls_failure(host: &str, port: u16, tls_error: &str) -> ImapProviderError {
    if answers_in_plaintext(host, port).await {
        return ImapProviderError::Connection(format!(
            "the server answered in plaintext on a TLS connection ({host}:{port}); if this is a \
             local bridge set use_tls = false, otherwise use port 993"
        ));
    }
    ImapProviderError::Connection(format!(
        "TLS handshake with {host}:{port} failed: {tls_error}"
    ))
}

async fn answers_in_plaintext(host: &str, port: u16) -> bool {
    let Ok(mut tcp) = TcpStream::connect((host, port)).await else {
        return false;
    };
    let mut head = [0u8; 4];
    let read = async_std::future::timeout(PLAINTEXT_PROBE_TIMEOUT, tcp.read_exact(&mut head)).await;
    matches!(read, Ok(Ok(()))) && &head == b"* OK"
}

// -- STARTTLS -------------------------------------------------------------------

/// Run the plaintext half of STARTTLS and hand back the socket, ready for the
/// TLS handshake. Refuses (never falls back to plaintext) when the greeting is
/// not a plain OK, the server does not advertise STARTTLS, or it rejects it.
async fn negotiate_starttls(mut tcp: TcpStream, host: &str, port: u16) -> Result<TcpStream> {
    let greeting = read_line(&mut tcp).await?;
    if !status_is(&greeting, "OK") {
        return Err(ImapProviderError::Connection(format!(
            "cannot upgrade {host}:{port} to TLS: the server greeting was not a plain OK, so \
             STARTTLS is not available. Refusing to continue unencrypted."
        )));
    }
    let capability = command(&mut tcp, "mxr1", "CAPABILITY").await?;
    if !advertises_starttls(&capability) {
        return Err(ImapProviderError::Connection(format!(
            "{host}:{port} does not advertise STARTTLS, so the connection cannot be encrypted. \
             Refusing to send credentials in plaintext; use a server or port that offers TLS \
             (port 993)."
        )));
    }
    command(&mut tcp, "mxr2", "STARTTLS").await?;
    Ok(tcp)
}

/// Send one tagged command and return the untagged lines that preceded its
/// reply. Errors if the tagged reply is not OK.
async fn command(tcp: &mut TcpStream, tag: &str, text: &str) -> Result<Vec<String>> {
    tcp.write_all(format!("{tag} {text}\r\n").as_bytes())
        .await
        .map_err(|e| ImapProviderError::Connection(e.to_string()))?;
    tcp.flush()
        .await
        .map_err(|e| ImapProviderError::Connection(e.to_string()))?;
    let mut untagged = Vec::new();
    loop {
        let line = read_line(tcp).await?;
        if line.starts_with(&format!("{tag} ")) {
            if status_is(&line, "OK") {
                return Ok(untagged);
            }
            return Err(ImapProviderError::Connection(format!(
                "server rejected {text}: {}",
                line.trim_end()
            )));
        }
        untagged.push(line);
    }
}

async fn read_line(tcp: &mut TcpStream) -> Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let read = tcp
            .read(&mut byte)
            .await
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;
        if read == 0 {
            return Err(ImapProviderError::Connection(
                "Server closed the IMAP connection during STARTTLS negotiation.".into(),
            ));
        }
        line.push(byte[0]);
        if byte[0] == b'\n' {
            return Ok(String::from_utf8_lossy(&line).into_owned());
        }
        if line.len() > MAX_NEGOTIATION_LINE {
            return Err(ImapProviderError::Connection(
                "Server sent an oversized line during STARTTLS negotiation.".into(),
            ));
        }
    }
}

/// `* OK ...` style status check on the second token.
fn status_is(line: &str, status: &str) -> bool {
    line.split_whitespace()
        .nth(1)
        .is_some_and(|token| token.eq_ignore_ascii_case(status))
}

fn advertises_starttls(untagged: &[String]) -> bool {
    untagged.iter().any(|line| {
        let mut tokens = line.split_whitespace();
        tokens.next() == Some("*")
            && tokens
                .next()
                .is_some_and(|t| t.eq_ignore_ascii_case("CAPABILITY"))
            && tokens.any(|t| t.eq_ignore_ascii_case("STARTTLS"))
    })
}

// -- Fingerprint probe ----------------------------------------------------------

/// Connect the way the account would and report the SHA-256 of the server's
/// leaf certificate, trusted or not, so a user can copy it into
/// `tls_cert_sha256`. Returns `None` for a plaintext (loopback) account. Only
/// the TLS handshake (and, for STARTTLS, CAPABILITY and STARTTLS) is sent: no
/// credentials.
pub async fn probe_server_certificate_sha256(config: &ImapConfig) -> Result<Option<String>> {
    let mode = TransportMode::resolve(&config.host, config.port, config.use_tls)?;
    let tcp = tcp_connect(&config.host, config.port).await?;
    let tcp = match mode {
        TransportMode::Plaintext => return Ok(None),
        TransportMode::ImplicitTls => tcp,
        TransportMode::StartTls => negotiate_starttls(tcp, &config.host, config.port).await?,
    };
    let tls = tls_handshake(&config.host, config.port, tcp, Verify::InspectOnly).await?;
    Ok(Some(peer_fingerprint(&tls)?.to_string()))
}

#[cfg(test)]
mod tests {
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
        let cert_pem = include_bytes!("test_fixtures/test-server.cert.pem");
        let key_pem = include_bytes!("test_fixtures/test-server.key.pem");
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
}
