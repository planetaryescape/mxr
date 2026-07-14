#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

use async_trait::async_trait;
use futures::{future::BoxFuture, TryStreamExt};
use std::sync::Arc;

use crate::config::ImapConfig;
use crate::error::ImapProviderError;
use crate::types::{
    FetchedMessage, FolderInfo, ImapAddress, ImapCapabilities, ImapEnvelope, MailboxInfo,
    NamespaceInfo, QresyncInfo,
};

pub type Result<T> = std::result::Result<T, ImapProviderError>;

/// Abstraction over an IMAP session for testability.
#[async_trait]
pub trait ImapSession: Send {
    async fn capabilities(&mut self) -> Result<ImapCapabilities>;
    async fn enable(&mut self, capabilities: &[&str]) -> Result<()>;
    async fn namespace(&mut self) -> Result<Option<NamespaceInfo>>;
    async fn select(&mut self, mailbox: &str) -> Result<MailboxInfo>;
    async fn select_qresync(
        &mut self,
        mailbox: &str,
        uid_validity: u32,
        highest_modseq: u64,
        known_uids: &str,
    ) -> Result<QresyncInfo>;
    async fn uid_fetch(&mut self, uid_set: &str, query: &str) -> Result<Vec<FetchedMessage>>;
    /// `UID SEARCH <query>`. Used (with `ALL`) for the QRESYNC/CONDSTORE-less
    /// delta path so we can detect server-side deletions by diffing UIDs.
    async fn uid_search(&mut self, query: &str) -> Result<Vec<u32>>;
    async fn uid_store(&mut self, uid_set: &str, flags: &str) -> Result<()>;
    async fn uid_copy(&mut self, uid_set: &str, mailbox: &str) -> Result<()>;
    async fn uid_move(&mut self, uid_set: &str, mailbox: &str) -> Result<()>;
    /// `APPEND <mailbox> (<flags>) {len}` — file a rendered RFC822 message into
    /// `mailbox`. Returns the APPENDUID UID when the server surfaces it under
    /// UIDPLUS, else `Ok(None)`.
    async fn uid_append(
        &mut self,
        mailbox: &str,
        flags: &[&str],
        body: &[u8],
    ) -> Result<Option<u32>>;
    async fn uid_expunge(&mut self, uid_set: &str) -> Result<()>;
    async fn expunge(&mut self) -> Result<()>;
    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>>;
    async fn create_mailbox(&mut self, mailbox: &str) -> Result<()>;
    async fn rename_mailbox(&mut self, old_mailbox: &str, new_mailbox: &str) -> Result<()>;
    async fn delete_mailbox(&mut self, mailbox: &str) -> Result<()>;
    async fn logout(&mut self) -> Result<()>;
}

/// Factory that creates fresh IMAP sessions (connection-per-call pattern).
#[async_trait]
pub trait ImapSessionFactory: Send + Sync {
    async fn create_session(&self) -> Result<Box<dyn ImapSession>>;

    /// Phase 3.1: open a dedicated IDLE watcher session for INBOX.
    /// Returns `Ok(None)` when the server doesn't advertise IDLE, when
    /// IDLE is feature-gated off, or when the factory is a mock that
    /// hasn't enabled IDLE for the test. Default: no IDLE.
    ///
    /// The watcher owns its own connection per RFC 2177 — IDLE blocks
    /// the session for the duration, so the daemon's regular sync
    /// session must stay separate.
    async fn create_idle_watcher(&self) -> Result<Option<Box<dyn mxr_core::IdleWatcher>>> {
        Ok(None)
    }
}

/// Type alias for the TLS stream used by async-imap (futures-based async IO).
type ImapTlsStream = async_native_tls::TlsStream<async_std::net::TcpStream>;

/// Upper bound on establishing an authenticated IMAP session (TCP connect +
/// TLS handshake + greeting + auth). None of those reads has an inherent
/// timeout, so a dead or half-open network would otherwise hang session
/// setup indefinitely — wedging whichever sync currently holds the
/// per-account provider lock.
const SESSION_SETUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Deadline for a single IMAP round trip — and, on streaming commands, an
/// *inactivity* bound on the next response item (each arriving item resets
/// it), so large-but-progressing transfers are never cut off while a dead
/// connection mid-sync fails fast instead of wedging the sync that holds
/// the per-account provider lock.
const COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Deadline for commands that can legitimately run long on big mailboxes
/// or slow servers without exposing per-item progress we could reset on:
/// `SELECT (QRESYNC)` streams one line per changed/vanished message since
/// the stored MODSEQ, and `STATUS` may force the server to recompute
/// UNSEEN over a large mailbox. Still bounded so a dead connection cannot
/// wedge the sync indefinitely.
const SLOW_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Deadline for APPEND, which uploads an entire message in one command.
/// Scaled by size so a large-but-progressing upload on a slow uplink is
/// not cut off (base 300s + 1s per 64 KiB ≈ tolerates ~0.5 Mbps), while
/// remaining a hard bound for dead connections.
fn append_timeout(body_len: usize) -> std::time::Duration {
    std::time::Duration::from_secs(300 + (body_len / 65_536) as u64)
}

fn command_timeout_error(what: &str, limit: std::time::Duration) -> ImapProviderError {
    ImapProviderError::Timeout(format!("IMAP {what} timed out after {limit:?}"))
}

/// Bound a whole command future. A timeout leaves the connection in an
/// unknown protocol state; callers must not keep using the session after
/// an error (all call sites open a session per operation and drop it on
/// failure, which closes the socket).
async fn bounded<T>(
    what: &str,
    limit: std::time::Duration,
    fut: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(limit, fut)
        .await
        .map_err(|_| command_timeout_error(what, limit))?
}

/// Collect a response stream with `limit` applied per item rather than in
/// total: progress resets the clock, stalls fail fast.
async fn collect_bounded<S, T, E>(
    what: &str,
    limit: std::time::Duration,
    stream: S,
) -> Result<Vec<T>>
where
    S: futures::Stream<Item = std::result::Result<T, E>>,
    E: std::fmt::Display,
{
    let mut stream = std::pin::pin!(stream);
    let mut items = Vec::new();
    loop {
        match tokio::time::timeout(limit, stream.try_next()).await {
            Err(_) => return Err(command_timeout_error(what, limit)),
            Ok(Ok(Some(item))) => items.push(item),
            Ok(Ok(None)) => return Ok(items),
            Ok(Err(e)) => return Err(ImapProviderError::protocol_detail(e.to_string())),
        }
    }
}

struct AnonymousAuthenticator;

impl async_imap::Authenticator for AnonymousAuthenticator {
    type Response = &'static [u8];

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        b""
    }
}

// -- XOAUTH2 authenticator ---------------------------------------------------

/// SASL XOAUTH2 authenticator for OAuth2-based IMAP login (RFC 7628 / Google protocol).
/// Used by Outlook/Exchange and Gmail IMAP when authenticating with OAuth2 tokens.
pub struct XOAuth2Authenticator {
    user: String,
    access_token: String,
}

impl async_imap::Authenticator for XOAuth2Authenticator {
    type Response = Vec<u8>;

    fn process(&mut self, _challenge: &[u8]) -> Vec<u8> {
        // async-imap base64-encodes this before sending, so we return raw bytes.
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.user, self.access_token
        )
        .into_bytes()
    }
}

type TokenFn = Arc<dyn Fn() -> BoxFuture<'static, anyhow::Result<String>> + Send + Sync>;

/// IMAP session factory that authenticates using XOAUTH2.
/// Accepts any async token-fetching callback, decoupled from the OAuth2 provider.
pub struct XOAuth2ImapSessionFactory {
    host: String,
    port: u16,
    username: String,
    token_fn: TokenFn,
}

impl XOAuth2ImapSessionFactory {
    pub fn new(host: String, port: u16, username: String, token_fn: TokenFn) -> Self {
        Self {
            host,
            port,
            username,
            token_fn,
        }
    }
}

#[async_trait]
impl ImapSessionFactory for XOAuth2ImapSessionFactory {
    async fn create_session(&self) -> Result<Box<dyn ImapSession>> {
        bounded(
            "XOAUTH2 session setup",
            SESSION_SETUP_TIMEOUT,
            self.create_session_inner(),
        )
        .await
    }
}

impl XOAuth2ImapSessionFactory {
    async fn create_session_inner(&self) -> Result<Box<dyn ImapSession>> {
        let access_token = (self.token_fn)()
            .await
            .map_err(|e| ImapProviderError::Auth(format!("token fetch failed: {e}")))?;

        let tcp = async_std::net::TcpStream::connect((&*self.host, self.port))
            .await
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;

        let tls = async_native_tls::TlsConnector::new();
        let tls_stream = tls
            .connect(&self.host, tcp)
            .await
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;

        let mut client = async_imap::Client::new(tls_stream);
        let greeting = client
            .read_response()
            .await
            .ok_or_else(|| ImapProviderError::Connection("no greeting from server".into()))?
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;

        // If the server sent PREAUTH, the session is already authenticated.
        let session = match greeting.parsed() {
            async_imap::imap_proto::Response::Data {
                status: async_imap::imap_proto::Status::PreAuth,
                ..
            } => client.into_session(),
            _ => {
                let authenticator = XOAuth2Authenticator {
                    user: self.username.clone(),
                    access_token,
                };
                client
                    .authenticate("XOAUTH2", authenticator)
                    .await
                    .map_err(|(e, _)| ImapProviderError::Auth(e.to_string()))?
            }
        };

        Ok(Box::new(RealImapSession { session }))
    }
}

// -- Password-based session factory ------------------------------------------

/// Production session factory that connects via TLS to an IMAP server.
pub struct RealImapSessionFactory {
    config: ImapConfig,
}

impl RealImapSessionFactory {
    pub fn new(config: ImapConfig) -> Self {
        Self { config }
    }
}

impl RealImapSessionFactory {
    /// Phase 3.1: open a fresh, authenticated `async_imap::Session` for
    /// either the regular sync path or a dedicated IDLE watcher.
    /// Centralised so the IDLE path doesn't drift from the sync path
    /// auth-wise (TLS, PREAUTH, ANONYMOUS, fallback LOGIN).
    async fn open_authenticated_session(&self) -> Result<async_imap::Session<ImapTlsStream>> {
        bounded(
            "session setup",
            SESSION_SETUP_TIMEOUT,
            self.open_authenticated_session_inner(),
        )
        .await
    }

    async fn open_authenticated_session_inner(&self) -> Result<async_imap::Session<ImapTlsStream>> {
        let tcp = async_std::net::TcpStream::connect((&*self.config.host, self.config.port))
            .await
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;
        let tls = async_native_tls::TlsConnector::new();
        let tls_stream = tls
            .connect(&self.config.host, tcp)
            .await
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;
        let mut client = async_imap::Client::new(tls_stream);
        let greeting = client
            .read_response()
            .await
            .ok_or_else(|| {
                ImapProviderError::Connection(
                    "Server closed the IMAP connection before sending a greeting.".into(),
                )
            })?
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;

        let session = match greeting.parsed() {
            async_imap::imap_proto::Response::Data {
                status: async_imap::imap_proto::Status::PreAuth,
                ..
            } => client.into_session(),
            async_imap::imap_proto::Response::Data {
                status: async_imap::imap_proto::Status::Ok,
                ..
            } if self.config.auth_required => {
                let password = self.config.resolve_password()?;
                client
                    .login(&self.config.username, &password)
                    .await
                    .map_err(|e| ImapProviderError::Auth(e.0.to_string()))?
            }
            async_imap::imap_proto::Response::Data {
                status: async_imap::imap_proto::Status::Ok,
                ..
            } => match client
                .authenticate("ANONYMOUS", AnonymousAuthenticator)
                .await
            {
                Ok(session) => session,
                Err((_, client)) => {
                    let fallback_username = if self.config.username.trim().is_empty() {
                        "anonymous".to_string()
                    } else {
                        self.config.username.clone()
                    };
                    let fallback_password = if !self.config.username.trim().is_empty()
                        && !self.config.password_ref.trim().is_empty()
                    {
                        self.config
                            .resolve_password()
                            .unwrap_or_else(|_| "anonymous".to_string())
                    } else {
                        "anonymous".to_string()
                    };
                    client
                        .login(&fallback_username, &fallback_password)
                        .await
                        .map_err(|e| ImapProviderError::Auth(e.0.to_string()))?
                }
            },
            other => {
                return Err(ImapProviderError::Connection(format!(
                    "Unexpected IMAP greeting from server: {other:?}"
                )));
            }
        };
        Ok(session)
    }
    async fn create_session_inner(&self) -> Result<Box<dyn ImapSession>> {
        let tcp = async_std::net::TcpStream::connect((&*self.config.host, self.config.port))
            .await
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;

        let tls = async_native_tls::TlsConnector::new();
        let tls_stream = tls
            .connect(&self.config.host, tcp)
            .await
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;

        let mut client = async_imap::Client::new(tls_stream);
        let greeting = client
            .read_response()
            .await
            .ok_or_else(|| {
                ImapProviderError::Connection(
                    "Server closed the IMAP connection before sending a greeting.".into(),
                )
            })?
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;

        let session = match greeting.parsed() {
            async_imap::imap_proto::Response::Data {
                status: async_imap::imap_proto::Status::PreAuth,
                ..
            } => client.into_session(),
            async_imap::imap_proto::Response::Data {
                status: async_imap::imap_proto::Status::Ok,
                ..
            } if self.config.auth_required => {
                let password = self.config.resolve_password()?;
                client
                    .login(&self.config.username, &password)
                    .await
                    .map_err(|e| ImapProviderError::Auth(e.0.to_string()))?
            }
            async_imap::imap_proto::Response::Data {
                status: async_imap::imap_proto::Status::Ok,
                ..
            } => match client
                .authenticate("ANONYMOUS", AnonymousAuthenticator)
                .await
            {
                Ok(session) => session,
                Err((anonymous_error, client)) => {
                    let fallback_username = if self.config.username.trim().is_empty() {
                        "anonymous".to_string()
                    } else {
                        self.config.username.clone()
                    };
                    let fallback_password = if !self.config.username.trim().is_empty()
                        && !self.config.password_ref.trim().is_empty()
                    {
                        self.config
                            .resolve_password()
                            .unwrap_or_else(|_| "anonymous".to_string())
                    } else {
                        "anonymous".to_string()
                    };

                    match client.login(&fallback_username, &fallback_password).await {
                        Ok(session) => session,
                        Err((login_error, client)) => {
                            let mut session = client.into_session();
                            let names = session
                                .list(Some(""), Some("*"))
                                .await
                                .map_err(|probe_error| {
                                    ImapProviderError::Auth(format!(
                                        "IMAP auth is disabled, but the server neither sent PREAUTH, accepted AUTHENTICATE ANONYMOUS, accepted fallback LOGIN, nor allowed unauthenticated LIST. ANONYMOUS failed with: {anonymous_error}; LOGIN failed with: {login_error}; LIST failed with: {probe_error}"
                                    ))
                                })?;
                            let _: Vec<_> =
                                names.try_collect().await.map_err(|probe_error| {
                                    ImapProviderError::Auth(format!(
                                        "IMAP auth is disabled, but the server neither sent PREAUTH, accepted AUTHENTICATE ANONYMOUS, accepted fallback LOGIN, nor completed unauthenticated LIST. ANONYMOUS failed with: {anonymous_error}; LOGIN failed with: {login_error}; LIST failed with: {probe_error}"
                                    ))
                                })?;
                            session
                        }
                    }
                }
            },
            other => {
                return Err(ImapProviderError::Connection(format!(
                    "Unexpected IMAP greeting from server: {other:?}"
                )));
            }
        };

        Ok(Box::new(RealImapSession { session }))
    }
}

#[async_trait]
impl ImapSessionFactory for RealImapSessionFactory {
    async fn create_session(&self) -> Result<Box<dyn ImapSession>> {
        bounded(
            "session setup",
            SESSION_SETUP_TIMEOUT,
            self.create_session_inner(),
        )
        .await
    }

    async fn create_idle_watcher(&self) -> Result<Option<Box<dyn mxr_core::IdleWatcher>>> {
        let mut session = self.open_authenticated_session().await?;

        // No IDLE capability → fall back to poll-only. These run on the raw
        // async-imap session (not the wrapped trait), so they need their own
        // bounds — a half-open connection here would otherwise wedge the
        // account's idle task forever (the daemon awaits idle_watch without
        // a shutdown select).
        let caps = bounded("CAPABILITY (idle setup)", COMMAND_TIMEOUT, async {
            session
                .capabilities()
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        if !caps.has_str("IDLE") {
            let _ = bounded("LOGOUT (idle setup)", COMMAND_TIMEOUT, async {
                session
                    .logout()
                    .await
                    .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
            })
            .await;
            return Ok(None);
        }

        bounded("SELECT (idle setup)", COMMAND_TIMEOUT, async {
            session
                .select("INBOX")
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;

        Ok(Some(Box::new(RealImapIdleWatcher::new(session))))
    }
}

/// Phase 3.1: real IMAP IDLE watcher. Owns a dedicated authenticated
/// session in IDLE-or-Selected state. `next_event` cycles through the
/// async-imap Handle lifecycle:
///   1. session.idle() → Handle (consumes Session)
///   2. handle.init().await sends IDLE
///   3. handle.wait_with_timeout(25min) waits for events
///   4. on Timeout we re-issue done()→idle()→init() to respect the
///      28-min RFC 2177 cap (servers drop silent IDLE connections).
///   5. on NewData we done()→Session and return Ok(()).
///
/// State is stored as exactly one of `session` or `handle` being
/// `Some` between calls; transitions use `Option::take()` to satisfy
/// async-imap's consume-on-state-change API.
struct RealImapIdleWatcher {
    session: Option<async_imap::Session<ImapTlsStream>>,
    handle: Option<async_imap::extensions::idle::Handle<ImapTlsStream>>,
}

impl RealImapIdleWatcher {
    fn new(session: async_imap::Session<ImapTlsStream>) -> Self {
        Self {
            session: Some(session),
            handle: None,
        }
    }

    /// Cap below the RFC 2177 28-minute server limit so we re-IDLE
    /// well before the server drops us.
    const IDLE_RESET_INTERVAL: std::time::Duration = std::time::Duration::from_secs(25 * 60);

    /// Move from session-state into init'd handle-state. Idempotent
    /// when already in handle-state.
    async fn ensure_idling(&mut self) -> Result<()> {
        if self.handle.is_some() {
            return Ok(());
        }
        let session = self.session.take().ok_or_else(|| {
            ImapProviderError::protocol_detail("idle watcher in invalid state".to_string())
        })?;
        let mut handle = session.idle();
        // Bounded: a half-open connection would otherwise hang the IDLE
        // handshake forever. On timeout the handle is dropped (socket
        // closed) and both state slots stay empty, so the next call errors
        // and the daemon's idle loop reconnects with backoff.
        bounded("IDLE init", COMMAND_TIMEOUT, async {
            handle
                .init()
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        self.handle = Some(handle);
        Ok(())
    }

    /// Send DONE to leave IDLE and recover the Session for the next
    /// cycle. Called both on user-set timeout (re-IDLE) and when we
    /// got a real notification.
    async fn done_idling(&mut self) -> Result<()> {
        let Some(handle) = self.handle.take() else {
            return Ok(());
        };
        // Bounded like `init`: DONE sends a line and reads the tagged
        // response; a dead connection would otherwise wedge the watcher
        // silently — no events, no error, no reconnect.
        let session = bounded("IDLE done", COMMAND_TIMEOUT, async {
            handle
                .done()
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        self.session = Some(session);
        Ok(())
    }
}

#[async_trait]
impl mxr_core::IdleWatcher for RealImapIdleWatcher {
    async fn next_event(&mut self) -> std::result::Result<(), mxr_core::MxrError> {
        loop {
            self.ensure_idling()
                .await
                .map_err(mxr_core::MxrError::from)?;
            let handle = self
                .handle
                .as_mut()
                .ok_or_else(|| mxr_core::MxrError::Provider("idle handle missing".to_string()))?;
            let (fut, _stop) = handle.wait_with_timeout(Self::IDLE_RESET_INTERVAL);
            let response = fut
                .await
                .map_err(|e| mxr_core::MxrError::Provider(e.to_string()))?;
            match response {
                async_imap::extensions::idle::IdleResponse::NewData(_) => {
                    self.done_idling().await.map_err(mxr_core::MxrError::from)?;
                    return Ok(());
                }
                async_imap::extensions::idle::IdleResponse::Timeout => {
                    // RFC 2177 cap reset. Re-issue IDLE and keep
                    // waiting; the daemon doesn't see a wake-up.
                    self.done_idling().await.map_err(mxr_core::MxrError::from)?;
                    continue;
                }
                async_imap::extensions::idle::IdleResponse::ManualInterrupt => {
                    return Err(mxr_core::MxrError::Provider("IDLE interrupted".to_string()));
                }
            }
        }
    }
}

/// Production IMAP session wrapping async_imap::Session.
struct RealImapSession {
    session: async_imap::Session<ImapTlsStream>,
}

fn is_noselect_attribute(attr: &async_imap::types::NameAttribute<'_>) -> bool {
    matches!(attr, async_imap::types::NameAttribute::NoSelect)
}

fn special_use_from_attributes(
    attributes: &[async_imap::types::NameAttribute<'_>],
) -> Option<String> {
    attributes.iter().find_map(|attr| match attr {
        async_imap::types::NameAttribute::Sent => Some("\\Sent".to_string()),
        async_imap::types::NameAttribute::Drafts => Some("\\Drafts".to_string()),
        async_imap::types::NameAttribute::Trash => Some("\\Trash".to_string()),
        async_imap::types::NameAttribute::Junk => Some("\\Junk".to_string()),
        async_imap::types::NameAttribute::All => Some("\\All".to_string()),
        async_imap::types::NameAttribute::Archive => Some("\\Archive".to_string()),
        async_imap::types::NameAttribute::Flagged => Some("\\Flagged".to_string()),
        _ => None,
    })
}

/// Per-folder counts captured from an IMAP `STATUS` response, decoupled
/// from `async_imap::types::Mailbox` so `folder_info_from_status` stays
/// unit-testable without a live server.
struct MailboxStatus {
    exists: u32,
    unseen: Option<u32>,
    uid_validity: Option<u32>,
    uid_next: Option<u32>,
    highest_modseq: Option<u64>,
}

/// Build a `FolderInfo` from a folder's `STATUS` result. A `STATUS` error
/// for one mailbox (shared namespace, missing perms, deleted between LIST
/// and STATUS) must not abort discovery of every other folder, so on error
/// we `warn!` and emit the folder with unknown counts rather than aborting.
fn folder_info_from_status(
    name: String,
    special_use: Option<String>,
    delimiter: Option<String>,
    status: Result<MailboxStatus>,
) -> FolderInfo {
    match status {
        Ok(status) => FolderInfo {
            name,
            special_use,
            delimiter,
            unread_count: status.unseen,
            total_count: Some(status.exists),
            uid_validity: status.uid_validity,
            uid_next: status.uid_next,
            highest_modseq: status.highest_modseq,
            namespace_prefix: None,
        },
        Err(error) => {
            tracing::warn!(
                folder = %name,
                %error,
                "IMAP STATUS failed; emitting folder with unknown counts instead of aborting sync"
            );
            FolderInfo {
                name,
                special_use,
                delimiter,
                unread_count: None,
                total_count: None,
                uid_validity: None,
                uid_next: None,
                highest_modseq: None,
                namespace_prefix: None,
            }
        }
    }
}

fn fetched_message_from_attrs(
    attrs: &[async_imap::imap_proto::types::AttributeValue<'_>],
) -> Option<FetchedMessage> {
    use async_imap::imap_proto::types::{AttributeValue, MessageSection, SectionPath};

    let mut uid = None;
    let mut flags = Vec::new();
    let mut envelope = None;
    let mut body = None;
    let mut header = None;
    let mut size = None;
    let mut internal_date = None;
    let mut gmail_labels = Vec::new();
    let mut gmail_msg_id = None;
    let mut gmail_thread_id = None;

    for attr in attrs {
        match attr {
            AttributeValue::Uid(value) => uid = Some(*value),
            // imap-proto exposes INTERNALDATE as the raw RFC 3501 date string
            // (e.g. "17-Jul-1996 02:44:25 -0700"); parse it into UTC so it can
            // back-stop a missing/unparseable `Date:` header downstream.
            AttributeValue::InternalDate(value) => {
                internal_date = crate::parse::parse_imap_date(value.trim()).ok();
            }
            AttributeValue::Flags(values) => {
                flags = values
                    .iter()
                    .map(|flag| flag.as_ref().to_string())
                    .collect();
            }
            AttributeValue::Envelope(value) => envelope = Some(imap_envelope_from_proto(value)),
            AttributeValue::BodySection {
                section: None,
                data: Some(value),
                ..
            }
            | AttributeValue::Rfc822(Some(value)) => body = Some(value.as_ref().to_vec()),
            AttributeValue::BodySection {
                section: Some(SectionPath::Full(MessageSection::Header)),
                data: Some(value),
                ..
            }
            | AttributeValue::Rfc822Header(Some(value)) => header = Some(value.as_ref().to_vec()),
            AttributeValue::Rfc822Size(value) => size = Some(*value),
            AttributeValue::GmailLabels(values) => {
                gmail_labels = values
                    .iter()
                    .map(|label| label.as_ref().to_string())
                    .collect();
            }
            AttributeValue::GmailMsgId(value) => gmail_msg_id = Some(*value),
            AttributeValue::GmailThrId(value) => gmail_thread_id = Some(*value),
            _ => {}
        }
    }

    Some(FetchedMessage {
        uid: uid?,
        flags,
        envelope,
        body,
        header,
        size,
        internal_date,
        gmail_labels,
        gmail_msg_id,
        gmail_thread_id,
    })
}

fn imap_envelope_from_proto(env: &async_imap::imap_proto::Envelope<'_>) -> ImapEnvelope {
    let convert_addrs = |addrs: Option<&Vec<async_imap::imap_proto::Address>>| -> Vec<ImapAddress> {
        addrs
            .map(|list| {
                list.iter()
                    .map(|addr| {
                        let mailbox = addr
                            .mailbox
                            .as_ref()
                            .map(|s| String::from_utf8_lossy(s).to_string())
                            .unwrap_or_default();
                        let host = addr
                            .host
                            .as_ref()
                            .map(|s| String::from_utf8_lossy(s).to_string())
                            .unwrap_or_default();
                        let name = addr
                            .name
                            .as_ref()
                            .map(|s| String::from_utf8_lossy(s).to_string());
                        ImapAddress {
                            name,
                            email: format!("{mailbox}@{host}"),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    ImapEnvelope {
        date: env
            .date
            .as_ref()
            .map(|d| String::from_utf8_lossy(d).to_string()),
        subject: env
            .subject
            .as_ref()
            .map(|s| String::from_utf8_lossy(s).to_string()),
        from: convert_addrs(env.from.as_ref()),
        to: convert_addrs(env.to.as_ref()),
        cc: convert_addrs(env.cc.as_ref()),
        bcc: convert_addrs(env.bcc.as_ref()),
        message_id: env
            .message_id
            .as_ref()
            .map(|s| String::from_utf8_lossy(s).to_string()),
        in_reply_to: env
            .in_reply_to
            .as_ref()
            .map(|s| String::from_utf8_lossy(s).to_string()),
    }
}

#[async_trait]
impl ImapSession for RealImapSession {
    async fn capabilities(&mut self) -> Result<ImapCapabilities> {
        let capabilities = bounded("CAPABILITY", COMMAND_TIMEOUT, async {
            self.session
                .capabilities()
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        Ok(ImapCapabilities {
            move_ext: capabilities.has_str("MOVE"),
            uidplus: capabilities.has_str("UIDPLUS"),
            idle: capabilities.has_str("IDLE"),
            condstore: capabilities.has_str("CONDSTORE"),
            qresync: capabilities.has_str("QRESYNC"),
            namespace: capabilities.has_str("NAMESPACE"),
            list_status: capabilities.has_str("LIST-STATUS"),
            utf8_accept: capabilities.has_str("UTF8=ACCEPT"),
            imap4rev2: capabilities.has_str("IMAP4rev2"),
            x_gm_ext_1: capabilities.has_str("X-GM-EXT-1"),
        })
    }

    async fn select(&mut self, mailbox: &str) -> Result<MailboxInfo> {
        let mb = bounded("SELECT", COMMAND_TIMEOUT, async {
            self.session
                .select(mailbox)
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;

        Ok(MailboxInfo {
            uid_validity: mb.uid_validity.unwrap_or(0),
            uid_next: mb.uid_next.unwrap_or(0),
            exists: mb.exists,
            highest_modseq: mb.highest_modseq,
        })
    }

    async fn enable(&mut self, capabilities: &[&str]) -> Result<()> {
        if capabilities.is_empty() {
            return Ok(());
        }

        bounded("ENABLE", COMMAND_TIMEOUT, async {
            self.session
                .enable(capabilities)
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        Ok(())
    }

    async fn namespace(&mut self) -> Result<Option<NamespaceInfo>> {
        let namespace = bounded("NAMESPACE", COMMAND_TIMEOUT, async {
            self.session
                .namespace()
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        Ok(Some(NamespaceInfo {
            personal_prefix: namespace.personal.first().map(|entry| entry.prefix.clone()),
            delimiter: namespace
                .personal
                .first()
                .and_then(|entry| entry.delimiter.clone()),
        }))
    }

    async fn select_qresync(
        &mut self,
        mailbox: &str,
        uid_validity: u32,
        highest_modseq: u64,
        known_uids: &str,
    ) -> Result<QresyncInfo> {
        // QRESYNC streams one untagged line per changed/vanished message
        // since the stored MODSEQ inside a single command — after days
        // offline that is legitimately large, so it gets the slow bound.
        let response = bounded("SELECT (QRESYNC)", SLOW_COMMAND_TIMEOUT, async {
            self.session
                .select_qresync(
                    mailbox,
                    format!("{uid_validity} {highest_modseq} {known_uids}"),
                )
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;

        Ok(QresyncInfo {
            mailbox: MailboxInfo {
                uid_validity: response.mailbox.uid_validity.unwrap_or(0),
                uid_next: response.mailbox.uid_next.unwrap_or(0),
                exists: response.mailbox.exists,
                highest_modseq: response.mailbox.highest_modseq,
            },
            vanished: response
                .vanished
                .into_iter()
                .flat_map(std::iter::Iterator::collect::<Vec<_>>)
                .collect(),
            changed: response
                .fetches
                .into_iter()
                .filter_map(|fetch| fetch.uid)
                .collect(),
        })
    }

    async fn uid_fetch(&mut self, uid_set: &str, query: &str) -> Result<Vec<FetchedMessage>> {
        let id = bounded("UID FETCH dispatch", COMMAND_TIMEOUT, async {
            self.session
                .run_command(format!("UID FETCH {uid_set} {query}"))
                .await
                .map_err(|e| ImapProviderError::fetch_detail(e.to_string()))
        })
        .await?;

        let mut messages = Vec::new();
        let mut saw_done = false;
        // Inactivity bound per response: an initial sync fetches whole
        // mailboxes in one `UID FETCH 1:*`, so a total deadline would break
        // large-but-healthy transfers. Each arriving response resets the
        // clock; only a stalled connection times out.
        while let Some(response) =
            tokio::time::timeout(COMMAND_TIMEOUT, self.session.read_response())
                .await
                .map_err(|_| command_timeout_error("UID FETCH read", COMMAND_TIMEOUT))?
        {
            let response = response.map_err(|e| ImapProviderError::fetch_detail(e.to_string()))?;
            match response.parsed() {
                async_imap::imap_proto::Response::Done {
                    tag,
                    status,
                    information,
                    ..
                } if tag == &id => {
                    if matches!(status, async_imap::imap_proto::Status::Ok) {
                        saw_done = true;
                        break;
                    }
                    return Err(ImapProviderError::fetch_detail(format!(
                        "UID FETCH failed: {status:?} {}",
                        information.as_deref().unwrap_or_default()
                    )));
                }
                async_imap::imap_proto::Response::Fetch(_, attrs) => {
                    if let Some(message) = fetched_message_from_attrs(attrs) {
                        messages.push(message);
                    }
                }
                _ => {}
            }
        }

        if !saw_done {
            return Err(ImapProviderError::fetch_detail(
                "UID FETCH ended before tagged OK".to_string(),
            ));
        }

        Ok(messages)
    }

    async fn uid_search(&mut self, query: &str) -> Result<Vec<u32>> {
        let uids = bounded("UID SEARCH", COMMAND_TIMEOUT, async {
            self.session
                .uid_search(query)
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        Ok(uids.into_iter().collect())
    }

    async fn uid_store(&mut self, uid_set: &str, flags: &str) -> Result<()> {
        let stream = bounded("UID STORE dispatch", COMMAND_TIMEOUT, async {
            self.session
                .uid_store(uid_set, flags)
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        // Consume the stream to apply the store
        let _ = collect_bounded("UID STORE", COMMAND_TIMEOUT, stream).await?;
        Ok(())
    }

    async fn uid_copy(&mut self, uid_set: &str, mailbox: &str) -> Result<()> {
        bounded("UID COPY", COMMAND_TIMEOUT, async {
            self.session
                .uid_copy(uid_set, mailbox)
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        Ok(())
    }

    async fn uid_move(&mut self, uid_set: &str, mailbox: &str) -> Result<()> {
        bounded("UID MOVE", COMMAND_TIMEOUT, async {
            self.session
                .uid_mv(uid_set, mailbox)
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        Ok(())
    }

    async fn uid_append(
        &mut self,
        mailbox: &str,
        flags: &[&str],
        body: &[u8],
    ) -> Result<Option<u32>> {
        // The vendored `mxr-async-imap` `append` returns `Result<()>`: it
        // consumes the tagged OK line without surfacing the APPENDUID, so the
        // new UID is not available here even on a UIDPLUS server. File the
        // message and report `None`; the daemon's fallback still records the
        // sent copy locally and the next sync reconciles the real UID by
        // Message-ID. (Confirmed against mxr-async-imap 0.10.6 `Session::append`.)
        let flags = (!flags.is_empty()).then(|| format!("({})", flags.join(" ")));
        bounded("APPEND", append_timeout(body.len()), async {
            self.session
                .append(mailbox, flags.as_deref(), None, body)
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        Ok(None)
    }

    async fn uid_expunge(&mut self, uid_set: &str) -> Result<()> {
        let stream = bounded("UID EXPUNGE dispatch", COMMAND_TIMEOUT, async {
            self.session
                .uid_expunge(uid_set)
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        let _ = collect_bounded("UID EXPUNGE", COMMAND_TIMEOUT, stream).await?;
        Ok(())
    }

    async fn expunge(&mut self) -> Result<()> {
        let stream = bounded("EXPUNGE dispatch", COMMAND_TIMEOUT, async {
            self.session
                .expunge()
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        let _ = collect_bounded("EXPUNGE", COMMAND_TIMEOUT, stream).await?;
        Ok(())
    }

    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>> {
        // Discover folders with a plain LIST so per-folder attributes (notably
        // \Noselect) are populated and the skip below can exclude non-selectable
        // container mailboxes. LIST-STATUS is intentionally avoided: on Gmail it
        // issues STATUS against the non-selectable "[Gmail]" container, and the
        // parsed result drops the \Noselect attribute, letting "[Gmail]" slip
        // through and abort the whole sync on SELECT.
        let names = {
            let stream = bounded("LIST dispatch", COMMAND_TIMEOUT, async {
                self.session
                    .list(Some(""), Some("*"))
                    .await
                    .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
            })
            .await?;

            collect_bounded("LIST", COMMAND_TIMEOUT, stream).await?
        };

        let mut folders = Vec::with_capacity(names.len());
        for name in &names {
            // Skip non-selectable container folders (e.g. Gmail's "[Gmail]" parent,
            // marked \Noselect). Issuing SELECT against them aborts the whole sync.
            if name.attributes().iter().any(is_noselect_attribute) {
                continue;
            }

            // A STATUS failure for one mailbox must not abort the whole LIST
            // (and therefore the whole sync). Map any error into the helper,
            // which warns and emits the folder with unknown counts. A
            // *timeout* is different: the connection is dead or desynced, so
            // continuing to issue commands on this session would misattribute
            // later responses — abort the whole call instead.
            let status = tokio::time::timeout(
                // Slow bound: STATUS may force the server to recompute
                // UNSEEN over a large mailbox and legitimately take a
                // while; a total sync outage from an aggressive deadline
                // is worse than a slow listing.
                SLOW_COMMAND_TIMEOUT,
                self.session
                    .status(name.name(), "(MESSAGES UNSEEN UIDNEXT UIDVALIDITY)"),
            )
            .await
            .map_err(|_| command_timeout_error("STATUS", SLOW_COMMAND_TIMEOUT))?
            .map(|mailbox| MailboxStatus {
                exists: mailbox.exists,
                unseen: mailbox.unseen,
                uid_validity: mailbox.uid_validity,
                uid_next: mailbox.uid_next,
                highest_modseq: mailbox.highest_modseq,
            })
            .map_err(|e| {
                ImapProviderError::protocol_detail(format!("STATUS {} failed: {e}", name.name()))
            });

            let special_use = special_use_from_attributes(name.attributes());

            let special_use = if name.name().eq_ignore_ascii_case("inbox") && special_use.is_none()
            {
                Some("\\Inbox".to_string())
            } else {
                special_use
            };

            // Namespace discovery is optional, and some servers answer it in a
            // format the upstream parser rejects. Avoid issuing NAMESPACE here
            // so folder discovery remains usable for account setup and sync.
            folders.push(folder_info_from_status(
                name.name().to_string(),
                special_use,
                name.delimiter().map(ToString::to_string),
                status,
            ));
        }

        Ok(folders)
    }

    async fn create_mailbox(&mut self, mailbox: &str) -> Result<()> {
        bounded("CREATE", COMMAND_TIMEOUT, async {
            self.session
                .create(mailbox)
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        Ok(())
    }

    async fn rename_mailbox(&mut self, old_mailbox: &str, new_mailbox: &str) -> Result<()> {
        bounded("RENAME", COMMAND_TIMEOUT, async {
            self.session
                .rename(old_mailbox, new_mailbox)
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        Ok(())
    }

    async fn delete_mailbox(&mut self, mailbox: &str) -> Result<()> {
        bounded("DELETE", COMMAND_TIMEOUT, async {
            self.session
                .delete(mailbox)
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        Ok(())
    }

    async fn logout(&mut self) -> Result<()> {
        bounded("LOGOUT", COMMAND_TIMEOUT, async {
            self.session
                .logout()
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))
        })
        .await?;
        Ok(())
    }
}

// -- Mock session for tests ---------------------------------------------------

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, Default)]
    pub struct CommandLog {
        pub commands: Vec<String>,
    }

    pub(crate) struct MockSessionState {
        pub(crate) fetch_queues_by_mailbox: HashMap<String, VecDeque<Vec<FetchedMessage>>>,
        /// Per-mailbox UID set returned by `UID SEARCH ALL`. None means an empty
        /// set; tests opt in by inserting via `MockImapSessionFactory`.
        pub(crate) uid_search_results: HashMap<String, Vec<u32>>,
        /// UID returned by `uid_append` (simulating a UIDPLUS server's
        /// APPENDUID). `None` mimics a server that does not report the UID.
        pub(crate) append_uid: Option<u32>,
    }

    pub struct MockImapSession {
        pub mailbox_info: MailboxInfo,
        pub capabilities: ImapCapabilities,
        pub namespace: Option<NamespaceInfo>,
        pub qresync_response: Option<QresyncInfo>,
        /// Simulate `SELECT (QRESYNC)` hitting its command deadline.
        pub qresync_times_out: bool,
        pub folders: Vec<FolderInfo>,
        pub log: Arc<Mutex<CommandLog>>,
        state: Arc<Mutex<MockSessionState>>,
        selected_mailbox: Option<String>,
    }

    impl MockImapSession {
        pub(crate) fn new(
            mailbox_info: MailboxInfo,
            capabilities: ImapCapabilities,
            namespace: Option<NamespaceInfo>,
            qresync_response: Option<QresyncInfo>,
            folders: Vec<FolderInfo>,
            log: Arc<Mutex<CommandLog>>,
            state: Arc<Mutex<MockSessionState>>,
        ) -> Self {
            Self {
                mailbox_info,
                capabilities,
                namespace,
                qresync_response,
                qresync_times_out: false,
                folders,
                log,
                state,
                selected_mailbox: None,
            }
        }
    }

    #[async_trait]
    impl ImapSession for MockImapSession {
        async fn capabilities(&mut self) -> Result<ImapCapabilities> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push("CAPABILITY".to_string());
            Ok(self.capabilities.clone())
        }

        async fn enable(&mut self, capabilities: &[&str]) -> Result<()> {
            if !capabilities.is_empty() {
                self.log
                    .lock()
                    .unwrap()
                    .commands
                    .push(format!("ENABLE {}", capabilities.join(" ")));
            }
            Ok(())
        }

        async fn namespace(&mut self) -> Result<Option<NamespaceInfo>> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push("NAMESPACE".to_string());
            Ok(self.namespace.clone())
        }

        async fn select(&mut self, mailbox: &str) -> Result<MailboxInfo> {
            self.selected_mailbox = Some(mailbox.to_string());
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("SELECT {mailbox}"));
            Ok(self.mailbox_info.clone())
        }

        async fn select_qresync(
            &mut self,
            mailbox: &str,
            _uid_validity: u32,
            _highest_modseq: u64,
            _known_uids: &str,
        ) -> Result<QresyncInfo> {
            self.selected_mailbox = Some(mailbox.to_string());
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("SELECT {mailbox} QRESYNC"));
            if self.qresync_times_out {
                return Err(command_timeout_error(
                    "SELECT (QRESYNC)",
                    SLOW_COMMAND_TIMEOUT,
                ));
            }
            Ok(self
                .qresync_response
                .clone()
                .unwrap_or_else(|| QresyncInfo {
                    mailbox: self.mailbox_info.clone(),
                    vanished: vec![],
                    changed: vec![],
                }))
        }

        async fn uid_fetch(&mut self, uid_set: &str, query: &str) -> Result<Vec<FetchedMessage>> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("UID FETCH {uid_set} {query}"));
            let mailbox = self
                .selected_mailbox
                .clone()
                .unwrap_or_else(|| "INBOX".to_string());
            let mut state = self.state.lock().unwrap();
            Ok(state
                .fetch_queues_by_mailbox
                .get_mut(&mailbox)
                .and_then(VecDeque::pop_front)
                .unwrap_or_default())
        }

        async fn uid_search(&mut self, query: &str) -> Result<Vec<u32>> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("UID SEARCH {query}"));
            let mailbox = self
                .selected_mailbox
                .clone()
                .unwrap_or_else(|| "INBOX".to_string());
            Ok(self
                .state
                .lock()
                .unwrap()
                .uid_search_results
                .get(&mailbox)
                .cloned()
                .unwrap_or_default())
        }

        async fn uid_store(&mut self, uid_set: &str, flags: &str) -> Result<()> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("UID STORE {uid_set} {flags}"));
            Ok(())
        }

        async fn uid_copy(&mut self, uid_set: &str, mailbox: &str) -> Result<()> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("UID COPY {uid_set} {mailbox}"));
            Ok(())
        }

        async fn uid_move(&mut self, uid_set: &str, mailbox: &str) -> Result<()> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("UID MOVE {uid_set} {mailbox}"));
            Ok(())
        }

        async fn uid_append(
            &mut self,
            mailbox: &str,
            flags: &[&str],
            body: &[u8],
        ) -> Result<Option<u32>> {
            self.log.lock().unwrap().commands.push(format!(
                "APPEND {mailbox} ({}) {{{}}}",
                flags.join(" "),
                body.len()
            ));
            Ok(self.state.lock().unwrap().append_uid)
        }

        async fn uid_expunge(&mut self, uid_set: &str) -> Result<()> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("UID EXPUNGE {uid_set}"));
            Ok(())
        }

        async fn expunge(&mut self) -> Result<()> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push("EXPUNGE".to_string());
            Ok(())
        }

        async fn list_folders(&mut self) -> Result<Vec<FolderInfo>> {
            self.log.lock().unwrap().commands.push("LIST".to_string());
            Ok(self.folders.clone())
        }

        async fn create_mailbox(&mut self, mailbox: &str) -> Result<()> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("CREATE {mailbox}"));
            Ok(())
        }

        async fn rename_mailbox(&mut self, old_mailbox: &str, new_mailbox: &str) -> Result<()> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("RENAME {old_mailbox} {new_mailbox}"));
            Ok(())
        }

        async fn delete_mailbox(&mut self, mailbox: &str) -> Result<()> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("DELETE {mailbox}"));
            Ok(())
        }

        async fn logout(&mut self) -> Result<()> {
            self.log.lock().unwrap().commands.push("LOGOUT".to_string());
            Ok(())
        }
    }

    pub struct MockImapSessionFactory {
        pub mailbox_info: MailboxInfo,
        pub capabilities: ImapCapabilities,
        pub namespace: Option<NamespaceInfo>,
        pub qresync_response: Option<QresyncInfo>,
        /// Simulate `SELECT (QRESYNC)` hitting its command deadline.
        pub qresync_times_out: bool,
        pub folders: Vec<FolderInfo>,
        pub log: Arc<Mutex<CommandLog>>,
        state: Arc<Mutex<MockSessionState>>,
        /// Phase 3.1: when set, `create_idle_watcher` returns a mock
        /// watcher whose `next_event` awaits this Notify. Tests use
        /// `notify_one()` to simulate server-pushed EXISTS/EXPUNGE.
        idle_trigger: Option<std::sync::Arc<tokio::sync::Notify>>,
    }

    impl MockImapSessionFactory {
        pub fn new(
            mailbox_info: MailboxInfo,
            fetch_responses: Vec<Vec<FetchedMessage>>,
            folders: Vec<FolderInfo>,
        ) -> Self {
            let fetch_queues_by_mailbox = build_fetch_queues(&fetch_responses, &folders);
            Self {
                mailbox_info,
                capabilities: ImapCapabilities::default(),
                namespace: None,
                qresync_response: None,
                qresync_times_out: false,
                folders,
                log: Arc::new(Mutex::new(CommandLog::default())),
                state: Arc::new(Mutex::new(MockSessionState {
                    fetch_queues_by_mailbox,
                    uid_search_results: HashMap::new(),
                    append_uid: None,
                })),
                idle_trigger: None,
            }
        }

        /// Phase 3.1: enable IDLE on this mock factory and return the
        /// trigger handle test code uses to simulate events.
        pub fn enable_idle(&mut self) -> std::sync::Arc<tokio::sync::Notify> {
            let trigger = std::sync::Arc::new(tokio::sync::Notify::new());
            self.idle_trigger = Some(trigger.clone());
            trigger
        }

        pub fn with_capabilities(mut self, capabilities: ImapCapabilities) -> Self {
            self.capabilities = capabilities;
            self
        }

        pub fn with_namespace(mut self, namespace: NamespaceInfo) -> Self {
            self.namespace = Some(namespace);
            self
        }

        pub fn with_qresync(mut self, response: QresyncInfo) -> Self {
            self.qresync_response = Some(response);
            self
        }

        /// Make `SELECT (QRESYNC)` fail with a command timeout, for tests
        /// asserting the caller does NOT reuse the session afterwards.
        pub fn with_qresync_timeout(mut self) -> Self {
            self.qresync_times_out = true;
            self
        }

        /// Inject the UID set returned by `UID SEARCH ALL` for a mailbox.
        /// Used by tests that exercise the QRESYNC/CONDSTORE-less delete-detection path.
        pub fn with_uid_search(self, mailbox: &str, uids: Vec<u32>) -> Self {
            self.state
                .lock()
                .unwrap()
                .uid_search_results
                .insert(mailbox.to_string(), uids);
            self
        }

        /// Simulate a UIDPLUS server that reports `uid` as the APPENDUID for
        /// the next `uid_append`. Used by the Sent-APPEND tests.
        pub fn with_append_uid(self, uid: u32) -> Self {
            self.state.lock().unwrap().append_uid = Some(uid);
            self
        }

        pub fn with_mailbox_fetches(
            self,
            mailbox: &str,
            responses: Vec<Vec<FetchedMessage>>,
        ) -> Self {
            self.state
                .lock()
                .unwrap()
                .fetch_queues_by_mailbox
                .insert(mailbox.to_string(), VecDeque::from(responses));
            self
        }

        pub fn commands(&self) -> Vec<String> {
            self.log.lock().unwrap().commands.clone()
        }
    }

    #[async_trait]
    impl ImapSessionFactory for MockImapSessionFactory {
        async fn create_session(&self) -> Result<Box<dyn ImapSession>> {
            let mut session = MockImapSession::new(
                self.mailbox_info.clone(),
                self.capabilities.clone(),
                self.namespace.clone(),
                self.qresync_response.clone(),
                self.folders.clone(),
                self.log.clone(),
                self.state.clone(),
            );
            session.qresync_times_out = self.qresync_times_out;
            Ok(Box::new(session))
        }

        async fn create_idle_watcher(&self) -> Result<Option<Box<dyn mxr_core::IdleWatcher>>> {
            let Some(trigger) = self.idle_trigger.clone() else {
                return Ok(None);
            };
            self.log
                .lock()
                .unwrap()
                .commands
                .push("IDLE_WATCH SELECT INBOX".to_string());
            Ok(Some(Box::new(MockIdleWatcher { trigger })))
        }
    }

    struct MockIdleWatcher {
        trigger: std::sync::Arc<tokio::sync::Notify>,
    }

    #[async_trait]
    impl mxr_core::IdleWatcher for MockIdleWatcher {
        async fn next_event(&mut self) -> std::result::Result<(), mxr_core::MxrError> {
            self.trigger.notified().await;
            Ok(())
        }
    }

    pub(crate) fn build_fetch_queues(
        fetch_responses: &[Vec<FetchedMessage>],
        folders: &[FolderInfo],
    ) -> HashMap<String, VecDeque<Vec<FetchedMessage>>> {
        let sync_mailboxes = folders
            .iter()
            .filter(|folder| folder.special_use.as_deref() != Some("\\All"))
            .map(|folder| folder.name.clone())
            .collect::<Vec<_>>();

        let mailbox_names = if sync_mailboxes.is_empty() {
            vec!["INBOX".to_string()]
        } else {
            sync_mailboxes
        };

        let mut queues = HashMap::new();
        if mailbox_names.len() == 1 {
            queues.insert(
                mailbox_names[0].clone(),
                fetch_responses.iter().cloned().collect(),
            );
            return queues;
        }

        if fetch_responses.len() == mailbox_names.len() {
            for (mailbox, response) in mailbox_names
                .into_iter()
                .zip(fetch_responses.iter().cloned())
            {
                queues.insert(mailbox, VecDeque::from(vec![response]));
            }
            return queues;
        }

        let mut fallback = VecDeque::new();
        for response in fetch_responses.iter().cloned() {
            fallback.push_back(response);
        }
        queues.insert("INBOX".to_string(), fallback);
        queues
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::mock::*;
    use super::*;
    use crate::types::*;
    use async_imap::types::NameAttribute;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn mock_state(
        fetch_responses: Vec<Vec<FetchedMessage>>,
        folders: Vec<FolderInfo>,
    ) -> Arc<Mutex<MockSessionState>> {
        Arc::new(Mutex::new(MockSessionState {
            fetch_queues_by_mailbox: build_fetch_queues(&fetch_responses, &folders),
            uid_search_results: HashMap::new(),
            append_uid: None,
        }))
    }

    /// Timeouts must be distinguishable from other connection errors:
    /// the QRESYNC→SELECT fallback reuses the session on NO/BAD but must
    /// not after a timeout (response still mid-flight).
    #[test]
    fn command_timeouts_are_distinguishable() {
        assert!(command_timeout_error("SELECT", COMMAND_TIMEOUT).is_timeout());
        assert!(!ImapProviderError::Connection("refused".into()).is_timeout());
        assert!(!ImapProviderError::protocol_detail("NO go away").is_timeout());
    }

    /// APPEND's deadline scales with upload size so large-but-progressing
    /// uploads on slow uplinks are not cut off.
    #[test]
    fn append_timeout_scales_with_body_size() {
        assert_eq!(append_timeout(0).as_secs(), 300);
        // A 25 MiB message earns ~400 extra seconds (~0.5 Mbps floor).
        assert_eq!(append_timeout(25 * 1024 * 1024).as_secs(), 700);
    }

    /// A command future that never resolves (dead connection) must fail
    /// with a timeout error instead of hanging the sync.
    #[tokio::test]
    async fn bounded_times_out_hung_command() {
        let result: Result<()> = bounded(
            "SELECT",
            std::time::Duration::from_millis(10),
            std::future::pending(),
        )
        .await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("timed out"), "unexpected error: {err}");
    }

    /// The stream bound is per item, not total: a transfer that keeps
    /// making progress may exceed the limit overall and still succeed.
    #[tokio::test]
    async fn collect_bounded_allows_slow_but_progressing_stream() {
        let limit = std::time::Duration::from_millis(50);
        let stream = futures::stream::unfold(0u32, |n| async move {
            if n >= 5 {
                return None;
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            Some((Ok::<u32, ImapProviderError>(n), n + 1))
        });
        // 5 items × 30ms = 150ms total, well past the 50ms per-item limit.
        let items = collect_bounded("FETCH", limit, stream).await.unwrap();
        assert_eq!(items, vec![0, 1, 2, 3, 4]);
    }

    /// A stream that stops producing items (stalled connection) fails
    /// once the inactivity limit elapses.
    #[tokio::test]
    async fn collect_bounded_times_out_stalled_stream() {
        use futures::StreamExt;
        let limit = std::time::Duration::from_millis(10);
        let first = futures::stream::iter(vec![Ok::<u32, ImapProviderError>(1)]);
        let stalled = first.chain(futures::stream::pending());
        let err = collect_bounded("FETCH", limit, stalled)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("timed out"), "unexpected error: {err}");
    }

    #[test]
    fn noselect_attribute_is_detected_by_variant() {
        assert!(is_noselect_attribute(&NameAttribute::NoSelect));
        assert!(!is_noselect_attribute(&NameAttribute::Sent));
    }

    #[test]
    fn folder_info_from_status_ok_populates_counts() {
        let info = folder_info_from_status(
            "Archive".to_string(),
            Some("\\Archive".to_string()),
            Some("/".to_string()),
            Ok(MailboxStatus {
                exists: 12,
                unseen: Some(3),
                uid_validity: Some(1),
                uid_next: Some(20),
                highest_modseq: Some(99),
            }),
        );
        assert_eq!(info.name, "Archive");
        assert_eq!(info.total_count, Some(12));
        assert_eq!(info.unread_count, Some(3));
        assert_eq!(info.uid_next, Some(20));
    }

    #[test]
    fn folder_info_from_status_error_emits_folder_without_counts() {
        // Regression: one folder's STATUS failure must not abort discovery of
        // the others. The un-STATUS-able folder is still emitted, just with
        // unknown counts, so `list_folders` returns the remaining folders.
        let info = folder_info_from_status(
            "Shared/Team".to_string(),
            None,
            Some("/".to_string()),
            Err(ImapProviderError::protocol_detail(
                "STATUS Shared/Team failed: permission denied".to_string(),
            )),
        );
        assert_eq!(info.name, "Shared/Team");
        assert_eq!(info.total_count, None);
        assert_eq!(info.unread_count, None);
        assert_eq!(info.uid_next, None);
        assert_eq!(info.uid_validity, None);
    }

    #[test]
    fn special_use_attributes_are_mapped_by_variant() {
        assert_eq!(
            special_use_from_attributes(&[NameAttribute::Sent]),
            Some("\\Sent".to_string())
        );
        assert_eq!(
            special_use_from_attributes(&[NameAttribute::Extension("\\Foo".into())]),
            None
        );
    }

    #[tokio::test]
    async fn mock_session_returns_canned_select_data() {
        let log = Arc::new(Mutex::new(CommandLog::default()));
        let mut session = MockImapSession::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 100,
                exists: 50,
                highest_modseq: Some(10),
            },
            ImapCapabilities::default(),
            None,
            None,
            vec![],
            log.clone(),
            mock_state(vec![], vec![]),
        );

        let info = session.select("INBOX").await.unwrap();
        assert_eq!(info.uid_validity, 1);
        assert_eq!(info.uid_next, 100);
        assert_eq!(info.exists, 50);
        assert_eq!(log.lock().unwrap().commands, vec!["SELECT INBOX"]);
    }

    #[tokio::test]
    async fn mock_session_returns_canned_fetch_data() {
        let log = Arc::new(Mutex::new(CommandLog::default()));
        let messages = vec![FetchedMessage {
            uid: 1,
            flags: vec!["\\Seen".to_string()],
            envelope: None,
            body: None,
            header: None,
            size: Some(512),
            internal_date: None,
            gmail_labels: vec![],
            gmail_msg_id: None,
            gmail_thread_id: None,
        }];

        let mut session = MockImapSession::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 2,
                exists: 1,
                highest_modseq: None,
            },
            ImapCapabilities::default(),
            None,
            None,
            vec![],
            log.clone(),
            mock_state(vec![messages], vec![]),
        );

        let result = session.uid_fetch("1:*", "(FLAGS ENVELOPE)").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].uid, 1);
    }

    #[tokio::test]
    async fn mock_factory_creates_sessions() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 10,
                exists: 5,
                highest_modseq: None,
            },
            vec![],
            vec![FolderInfo {
                name: "INBOX".to_string(),
                special_use: Some("\\Inbox".to_string()),
                delimiter: Some("/".to_string()),
                unread_count: None,
                total_count: None,
                uid_validity: None,
                uid_next: None,
                highest_modseq: None,
                namespace_prefix: None,
            }],
        );

        let mut session = factory.create_session().await.unwrap();
        let folders = session.list_folders().await.unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "INBOX");
    }
}
