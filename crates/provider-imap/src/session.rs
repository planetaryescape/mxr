use async_trait::async_trait;

use crate::config::ImapConfig;
use crate::error::ImapProviderError;
use crate::types::{FetchedMessage, FolderInfo, ImapAddress, ImapEnvelope, MailboxInfo};

pub type Result<T> = std::result::Result<T, ImapProviderError>;

/// Abstraction over an IMAP session for testability.
#[async_trait]
pub trait ImapSession: Send {
    async fn select(&mut self, mailbox: &str) -> Result<MailboxInfo>;
    async fn uid_fetch(&mut self, uid_set: &str, query: &str) -> Result<Vec<FetchedMessage>>;
    async fn uid_store(&mut self, uid_set: &str, flags: &str) -> Result<()>;
    async fn uid_copy(&mut self, uid_set: &str, mailbox: &str) -> Result<()>;
    async fn expunge(&mut self) -> Result<()>;
    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>>;
    async fn logout(&mut self) -> Result<()>;
}

/// Factory that creates fresh IMAP sessions (connection-per-call pattern).
#[async_trait]
pub trait ImapSessionFactory: Send + Sync {
    async fn create_session(&self) -> Result<Box<dyn ImapSession>>;
}

/// Type alias for the TLS stream used by async-imap (futures-based async IO).
type ImapTlsStream = async_native_tls::TlsStream<async_std::net::TcpStream>;

/// Production session factory that connects via TLS to an IMAP server.
pub struct RealImapSessionFactory {
    config: ImapConfig,
}

impl RealImapSessionFactory {
    pub fn new(config: ImapConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ImapSessionFactory for RealImapSessionFactory {
    async fn create_session(&self) -> Result<Box<dyn ImapSession>> {
        let password = self.config.resolve_password()?;

        let tcp = async_std::net::TcpStream::connect((&*self.config.host, self.config.port))
            .await
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;

        let tls = async_native_tls::TlsConnector::new();
        let tls_stream = tls
            .connect(&self.config.host, tcp)
            .await
            .map_err(|e| ImapProviderError::Connection(e.to_string()))?;

        let client = async_imap::Client::new(tls_stream);

        let session = client
            .login(&self.config.username, &password)
            .await
            .map_err(|e| ImapProviderError::Auth(e.0.to_string()))?;

        Ok(Box::new(RealImapSession { session }))
    }
}

/// Production IMAP session wrapping async_imap::Session.
struct RealImapSession {
    session: async_imap::Session<ImapTlsStream>,
}

#[async_trait]
impl ImapSession for RealImapSession {
    async fn select(&mut self, mailbox: &str) -> Result<MailboxInfo> {
        let mb = self
            .session
            .select(mailbox)
            .await
            .map_err(|e| ImapProviderError::Protocol(e.to_string()))?;

        Ok(MailboxInfo {
            uid_validity: mb.uid_validity.unwrap_or(0),
            uid_next: mb.uid_next.unwrap_or(0),
            exists: mb.exists,
        })
    }

    async fn uid_fetch(&mut self, uid_set: &str, query: &str) -> Result<Vec<FetchedMessage>> {
        use futures::TryStreamExt;

        let stream = self
            .session
            .uid_fetch(uid_set, query)
            .await
            .map_err(|e| ImapProviderError::Fetch(e.to_string()))?;

        let fetches: Vec<_> = stream
            .try_collect()
            .await
            .map_err(|e| ImapProviderError::Fetch(e.to_string()))?;

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

    async fn uid_store(&mut self, uid_set: &str, flags: &str) -> Result<()> {
        use futures::TryStreamExt;
        let stream = self
            .session
            .uid_store(uid_set, flags)
            .await
            .map_err(|e| ImapProviderError::Protocol(e.to_string()))?;
        // Consume the stream to apply the store
        let _: Vec<_> = stream
            .try_collect()
            .await
            .map_err(|e| ImapProviderError::Protocol(e.to_string()))?;
        Ok(())
    }

    async fn uid_copy(&mut self, uid_set: &str, mailbox: &str) -> Result<()> {
        self.session
            .uid_copy(uid_set, mailbox)
            .await
            .map_err(|e| ImapProviderError::Protocol(e.to_string()))?;
        Ok(())
    }

    async fn expunge(&mut self) -> Result<()> {
        use futures::TryStreamExt;
        let stream = self
            .session
            .expunge()
            .await
            .map_err(|e| ImapProviderError::Protocol(e.to_string()))?;
        let _: Vec<_> = stream
            .try_collect()
            .await
            .map_err(|e| ImapProviderError::Protocol(e.to_string()))?;
        Ok(())
    }

    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>> {
        use futures::TryStreamExt;
        let stream = self
            .session
            .list(Some(""), Some("*"))
            .await
            .map_err(|e| ImapProviderError::Protocol(e.to_string()))?;

        let names: Vec<_> = stream
            .try_collect()
            .await
            .map_err(|e| ImapProviderError::Protocol(e.to_string()))?;

        let mut folders = Vec::with_capacity(names.len());
        for name in &names {
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
            });
        }

        Ok(folders)
    }

    async fn logout(&mut self) -> Result<()> {
        self.session
            .logout()
            .await
            .map_err(|e| ImapProviderError::Protocol(e.to_string()))?;
        Ok(())
    }
}

// -- Mock session for tests ---------------------------------------------------

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, Default)]
    pub struct CommandLog {
        pub commands: Vec<String>,
    }

    pub struct MockImapSession {
        pub mailbox_info: MailboxInfo,
        pub fetch_responses: Vec<Vec<FetchedMessage>>,
        pub folders: Vec<FolderInfo>,
        pub log: Arc<Mutex<CommandLog>>,
        fetch_call_count: usize,
    }

    impl MockImapSession {
        pub fn new(
            mailbox_info: MailboxInfo,
            fetch_responses: Vec<Vec<FetchedMessage>>,
            folders: Vec<FolderInfo>,
            log: Arc<Mutex<CommandLog>>,
        ) -> Self {
            Self {
                mailbox_info,
                fetch_responses,
                folders,
                log,
                fetch_call_count: 0,
            }
        }
    }

    #[async_trait]
    impl ImapSession for MockImapSession {
        async fn select(&mut self, mailbox: &str) -> Result<MailboxInfo> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("SELECT {mailbox}"));
            Ok(self.mailbox_info.clone())
        }

        async fn uid_fetch(&mut self, uid_set: &str, query: &str) -> Result<Vec<FetchedMessage>> {
            self.log
                .lock()
                .unwrap()
                .commands
                .push(format!("UID FETCH {uid_set} {query}"));
            let idx = self.fetch_call_count;
            self.fetch_call_count += 1;
            Ok(self.fetch_responses.get(idx).cloned().unwrap_or_default())
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

        async fn logout(&mut self) -> Result<()> {
            self.log.lock().unwrap().commands.push("LOGOUT".to_string());
            Ok(())
        }
    }

    pub struct MockImapSessionFactory {
        pub mailbox_info: MailboxInfo,
        pub fetch_responses: Vec<Vec<FetchedMessage>>,
        pub folders: Vec<FolderInfo>,
        pub log: Arc<Mutex<CommandLog>>,
    }

    impl MockImapSessionFactory {
        pub fn new(
            mailbox_info: MailboxInfo,
            fetch_responses: Vec<Vec<FetchedMessage>>,
            folders: Vec<FolderInfo>,
        ) -> Self {
            Self {
                mailbox_info,
                fetch_responses,
                folders,
                log: Arc::new(Mutex::new(CommandLog::default())),
            }
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
                self.fetch_responses.clone(),
                self.folders.clone(),
                self.log.clone(),
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::*;
    use super::*;
    use crate::types::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn mock_session_returns_canned_select_data() {
        let log = Arc::new(Mutex::new(CommandLog::default()));
        let mut session = MockImapSession::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 100,
                exists: 50,
            },
            vec![],
            vec![],
            log.clone(),
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
            },
            vec![messages],
            vec![],
            log,
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
            },
            vec![],
            vec![FolderInfo {
                name: "INBOX".to_string(),
                special_use: Some("\\Inbox".to_string()),
            }],
        );

        let mut session = factory.create_session().await.unwrap();
        let folders = session.list_folders().await.unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "INBOX");
    }
}
