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
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.create_session_inner(),
        )
        .await
        .map_err(|_| ImapProviderError::Connection("XOAUTH2 session setup timed out".into()))?
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
}

#[async_trait]
impl ImapSessionFactory for RealImapSessionFactory {
    async fn create_session(&self) -> Result<Box<dyn ImapSession>> {
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

    async fn create_idle_watcher(&self) -> Result<Option<Box<dyn mxr_core::IdleWatcher>>> {
        let mut session = self.open_authenticated_session().await?;

        // No IDLE capability → fall back to poll-only.
        let caps = session
            .capabilities()
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        if !caps.has_str("IDLE") {
            let _ = session.logout().await;
            return Ok(None);
        }

        session
            .select("INBOX")
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;

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
        handle
            .init()
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
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
        let session = handle
            .done()
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
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

#[async_trait]
impl ImapSession for RealImapSession {
    async fn capabilities(&mut self) -> Result<ImapCapabilities> {
        let capabilities = self
            .session
            .capabilities()
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
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
        })
    }

    async fn select(&mut self, mailbox: &str) -> Result<MailboxInfo> {
        let mb = self
            .session
            .select(mailbox)
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;

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

        self.session
            .enable(capabilities)
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        Ok(())
    }

    async fn namespace(&mut self) -> Result<Option<NamespaceInfo>> {
        let namespace = self
            .session
            .namespace()
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
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
        let response = self
            .session
            .select_qresync(
                mailbox,
                format!("{uid_validity} {highest_modseq} {known_uids}"),
            )
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;

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
        use futures::TryStreamExt;

        let stream = self
            .session
            .uid_fetch(uid_set, query)
            .await
            .map_err(|e| ImapProviderError::fetch_detail(e.to_string()))?;

        let fetches: Vec<_> = stream
            .try_collect()
            .await
            .map_err(|e| ImapProviderError::fetch_detail(e.to_string()))?;

        let mut messages = Vec::with_capacity(fetches.len());
        for fetch in &fetches {
            let uid = match fetch.uid {
                Some(u) => u,
                None => continue,
            };

            let flags: Vec<String> = fetch
                .flags()
                .map(|f| match f {
                    async_imap::types::Flag::Seen => "\\Seen".to_string(),
                    async_imap::types::Flag::Answered => "\\Answered".to_string(),
                    async_imap::types::Flag::Flagged => "\\Flagged".to_string(),
                    async_imap::types::Flag::Deleted => "\\Deleted".to_string(),
                    async_imap::types::Flag::Draft => "\\Draft".to_string(),
                    async_imap::types::Flag::Recent => "\\Recent".to_string(),
                    async_imap::types::Flag::MayCreate => "\\MayCreate".to_string(),
                    async_imap::types::Flag::Custom(ref s) => s.to_string(),
                })
                .collect();

            let envelope = fetch.envelope().map(|env| {
                let convert_addrs =
                    |addrs: Option<&Vec<async_imap::imap_proto::Address>>| -> Vec<ImapAddress> {
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
            });

            let body = fetch.body().map(|b: &[u8]| b.to_vec());
            let header = fetch.header().map(|h: &[u8]| h.to_vec());
            let size = fetch.size;

            messages.push(FetchedMessage {
                uid,
                flags,
                envelope,
                body,
                header,
                size,
            });
        }

        Ok(messages)
    }

    async fn uid_search(&mut self, query: &str) -> Result<Vec<u32>> {
        let uids = self
            .session
            .uid_search(query)
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        Ok(uids.into_iter().collect())
    }

    async fn uid_store(&mut self, uid_set: &str, flags: &str) -> Result<()> {
        use futures::TryStreamExt;
        let stream = self
            .session
            .uid_store(uid_set, flags)
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        // Consume the stream to apply the store
        let _: Vec<_> = stream
            .try_collect()
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        Ok(())
    }

    async fn uid_copy(&mut self, uid_set: &str, mailbox: &str) -> Result<()> {
        self.session
            .uid_copy(uid_set, mailbox)
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        Ok(())
    }

    async fn uid_move(&mut self, uid_set: &str, mailbox: &str) -> Result<()> {
        self.session
            .uid_mv(uid_set, mailbox)
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        Ok(())
    }

    async fn uid_expunge(&mut self, uid_set: &str) -> Result<()> {
        use futures::TryStreamExt;
        let stream = self
            .session
            .uid_expunge(uid_set)
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        let _: Vec<_> = stream
            .try_collect()
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        Ok(())
    }

    async fn expunge(&mut self) -> Result<()> {
        use futures::TryStreamExt;
        let stream = self
            .session
            .expunge()
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        let _: Vec<_> = stream
            .try_collect()
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        Ok(())
    }

    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>> {
        // Discover folders with a plain LIST so per-folder attributes (notably
        // \Noselect) are populated and the skip below can exclude non-selectable
        // container mailboxes. LIST-STATUS is intentionally avoided: on Gmail it
        // issues STATUS against the non-selectable "[Gmail]" container, and the
        // parsed result drops the \Noselect attribute, letting "[Gmail]" slip
        // through and abort the whole sync on SELECT. Per-folder message counts
        // are populated later during folder sync.
        let names = {
            use futures::TryStreamExt;
            let stream = self
                .session
                .list(Some(""), Some("*"))
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;

            stream
                .try_collect::<Vec<_>>()
                .await
                .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?
        };

        let mut folders = Vec::with_capacity(names.len());
        for name in &names {
            // Skip non-selectable container folders (e.g. Gmail's "[Gmail]" parent,
            // marked \Noselect). Issuing SELECT against them aborts the whole sync.
            if name
                .attributes()
                .iter()
                .any(|attr| format!("{attr:?}") == "NoSelect")
            {
                continue;
            }

            let special_use = name.attributes().iter().find_map(|attr| {
                let s = format!("{attr:?}");
                match s.as_str() {
                    "Sent" => Some("\\Sent".to_string()),
                    "Drafts" => Some("\\Drafts".to_string()),
                    "Trash" => Some("\\Trash".to_string()),
                    "Junk" => Some("\\Junk".to_string()),
                    "All" => Some("\\All".to_string()),
                    "Archive" => Some("\\Archive".to_string()),
                    "Flagged" => Some("\\Flagged".to_string()),
                    _ => None,
                }
            });

            let special_use = if name.name().eq_ignore_ascii_case("inbox") && special_use.is_none()
            {
                Some("\\Inbox".to_string())
            } else {
                special_use
            };

            folders.push(FolderInfo {
                name: name.name().to_string(),
                special_use,
                delimiter: name.delimiter().map(ToString::to_string),
                // Counts/cursors are filled in during per-folder sync; a plain
                // LIST carries no STATUS data.
                unread_count: None,
                total_count: None,
                uid_validity: None,
                uid_next: None,
                highest_modseq: None,
                // Namespace discovery is optional, and some servers answer it in a
                // format the upstream parser rejects. Avoid issuing NAMESPACE here
                // so folder discovery remains usable for account setup and sync.
                namespace_prefix: None,
            });
        }

        Ok(folders)
    }

    async fn create_mailbox(&mut self, mailbox: &str) -> Result<()> {
        self.session
            .create(mailbox)
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        Ok(())
    }

    async fn rename_mailbox(&mut self, old_mailbox: &str, new_mailbox: &str) -> Result<()> {
        self.session
            .rename(old_mailbox, new_mailbox)
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        Ok(())
    }

    async fn delete_mailbox(&mut self, mailbox: &str) -> Result<()> {
        self.session
            .delete(mailbox)
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
        Ok(())
    }

    async fn logout(&mut self) -> Result<()> {
        self.session
            .logout()
            .await
            .map_err(|e| ImapProviderError::protocol_detail(e.to_string()))?;
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
    }

    pub struct MockImapSession {
        pub mailbox_info: MailboxInfo,
        pub capabilities: ImapCapabilities,
        pub namespace: Option<NamespaceInfo>,
        pub qresync_response: Option<QresyncInfo>,
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
                folders,
                log: Arc::new(Mutex::new(CommandLog::default())),
                state: Arc::new(Mutex::new(MockSessionState {
                    fetch_queues_by_mailbox,
                    uid_search_results: HashMap::new(),
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

        pub fn commands(&self) -> Vec<String> {
            self.log.lock().unwrap().commands.clone()
        }
    }

    #[async_trait]
    impl ImapSessionFactory for MockImapSessionFactory {
        async fn create_session(&self) -> Result<Box<dyn ImapSession>> {
            Ok(Box::new(MockImapSession::new(
                self.mailbox_info.clone(),
                self.capabilities.clone(),
                self.namespace.clone(),
                self.qresync_response.clone(),
                self.folders.clone(),
                self.log.clone(),
                self.state.clone(),
            )))
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
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn mock_state(
        fetch_responses: Vec<Vec<FetchedMessage>>,
        folders: Vec<FolderInfo>,
    ) -> Arc<Mutex<MockSessionState>> {
        Arc::new(Mutex::new(MockSessionState {
            fetch_queues_by_mailbox: build_fetch_queues(&fetch_responses, &folders),
            uid_search_results: HashMap::new(),
        }))
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
