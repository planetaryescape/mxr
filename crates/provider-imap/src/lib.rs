pub mod config;
pub mod error;
pub mod folders;
pub mod parse;
pub mod session;
pub mod types;

use async_trait::async_trait;
use config::ImapConfig;
use mxr_core::id::{AccountId, MessageId};
use mxr_core::provider::MailSyncProvider;
use mxr_core::types::*;
use session::{ImapSessionFactory, RealImapSessionFactory};
use tracing::{debug, warn};

pub struct ImapProvider {
    account_id: AccountId,
    config: ImapConfig,
    trash_folder: String,
    session_factory: Box<dyn ImapSessionFactory>,
}

impl ImapProvider {
    pub fn new(account_id: AccountId, config: ImapConfig) -> Self {
        let session_factory = Box::new(RealImapSessionFactory::new(config.clone()));
        Self {
            account_id,
            config,
            trash_folder: "Trash".to_string(),
            session_factory,
        }
    }

    /// Constructor for tests — inject a mock session factory.
    #[cfg(test)]
    pub fn with_session_factory(
        account_id: AccountId,
        config: ImapConfig,
        session_factory: Box<dyn ImapSessionFactory>,
    ) -> Self {
        Self {
            account_id,
            config,
            trash_folder: "Trash".to_string(),
            session_factory,
        }
    }

    pub fn with_trash_folder(mut self, folder: String) -> Self {
        self.trash_folder = folder;
        self
    }

    /// Initial sync: fetch all messages from INBOX via UID FETCH.
    async fn initial_sync(&self) -> mxr_core::provider::Result<SyncBatch> {
        debug!("Starting IMAP initial sync for account {}", self.account_id);

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let mailbox_info = session
            .select("INBOX")
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        if mailbox_info.exists == 0 {
            let _ = session.logout().await;
            return Ok(SyncBatch {
                upserted: vec![],
                deleted_provider_ids: vec![],
                label_changes: vec![],
                next_cursor: SyncCursor::Imap {
                    uid_validity: mailbox_info.uid_validity,
                    uid_next: mailbox_info.uid_next,
                },
            });
        }

        let fetched = session
            .uid_fetch(
                "1:*",
                "(FLAGS ENVELOPE BODY.PEEK[HEADER] RFC822.SIZE)",
            )
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let mut envelopes = Vec::with_capacity(fetched.len());
        for msg in &fetched {
            match parse::imap_fetch_to_envelope(msg, "INBOX", &self.account_id) {
                Ok(env) => envelopes.push(env),
                Err(e) => warn!(uid = msg.uid, error = %e, "Failed to parse IMAP message"),
            }
        }

        let _ = session.logout().await;

        debug!(
            "IMAP initial sync complete: {} messages",
            envelopes.len()
        );

        Ok(SyncBatch {
            upserted: envelopes,
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor: SyncCursor::Imap {
                uid_validity: mailbox_info.uid_validity,
                uid_next: mailbox_info.uid_next,
            },
        })
    }

    /// Delta sync: fetch only new messages since last uid_next.
    async fn delta_sync(
        &self,
        old_uid_validity: u32,
        old_uid_next: u32,
    ) -> mxr_core::provider::Result<SyncBatch> {
        debug!(
            old_uid_validity,
            old_uid_next, "Starting IMAP delta sync for account {}", self.account_id
        );

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let mailbox_info = session
            .select("INBOX")
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        // UIDVALIDITY changed — must do full resync
        if mailbox_info.uid_validity != old_uid_validity {
            warn!(
                old = old_uid_validity,
                new = mailbox_info.uid_validity,
                "UIDVALIDITY changed, falling back to initial sync"
            );
            let _ = session.logout().await;
            return self.initial_sync().await;
        }

        // No new messages
        if mailbox_info.uid_next <= old_uid_next {
            let _ = session.logout().await;
            return Ok(SyncBatch {
                upserted: vec![],
                deleted_provider_ids: vec![],
                label_changes: vec![],
                next_cursor: SyncCursor::Imap {
                    uid_validity: mailbox_info.uid_validity,
                    uid_next: mailbox_info.uid_next,
                },
            });
        }

        let uid_range = format!("{}:*", old_uid_next);
        let fetched = session
            .uid_fetch(
                &uid_range,
                "(FLAGS ENVELOPE BODY.PEEK[HEADER] RFC822.SIZE)",
            )
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let mut envelopes = Vec::with_capacity(fetched.len());
        for msg in &fetched {
            // IMAP may return the last message with UID < uid_next if using "uid_next:*"
            // and there are no new messages. Filter those out.
            if msg.uid < old_uid_next {
                continue;
            }
            match parse::imap_fetch_to_envelope(msg, "INBOX", &self.account_id) {
                Ok(env) => envelopes.push(env),
                Err(e) => warn!(uid = msg.uid, error = %e, "Failed to parse IMAP message"),
            }
        }

        let _ = session.logout().await;

        debug!(
            "IMAP delta sync complete: {} new messages",
            envelopes.len()
        );

        Ok(SyncBatch {
            upserted: envelopes,
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor: SyncCursor::Imap {
                uid_validity: mailbox_info.uid_validity,
                uid_next: mailbox_info.uid_next,
            },
        })
    }
}

#[async_trait]
impl MailSyncProvider for ImapProvider {
    fn name(&self) -> &str {
        "imap"
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn capabilities(&self) -> SyncCapabilities {
        SyncCapabilities {
            labels: false,
            server_search: true,
            delta_sync: false,
            push: false,
            batch_operations: false,
        }
    }

    async fn authenticate(&mut self) -> mxr_core::provider::Result<()> {
        let _password = self
            .config
            .resolve_password()
            .map_err(mxr_core::error::MxrError::from)?;
        Ok(())
    }

    async fn refresh_auth(&mut self) -> mxr_core::provider::Result<()> {
        Ok(())
    }

    async fn sync_labels(&self) -> mxr_core::provider::Result<Vec<Label>> {
        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let folder_list = session
            .list_folders()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let _ = session.logout().await;

        Ok(folder_list
            .iter()
            .map(|f| {
                folders::map_folder_to_label(
                    &f.name,
                    f.special_use.as_deref(),
                    &self.account_id,
                )
            })
            .collect())
    }

    async fn sync_messages(&self, cursor: &SyncCursor) -> mxr_core::provider::Result<SyncBatch> {
        match cursor {
            SyncCursor::Initial => self.initial_sync().await,
            SyncCursor::Imap {
                uid_validity,
                uid_next,
            } => self.delta_sync(*uid_validity, *uid_next).await,
            other => Err(mxr_core::error::MxrError::Provider(format!(
                "IMAP provider received incompatible cursor: {other:?}"
            ))),
        }
    }

    async fn fetch_body(
        &self,
        provider_message_id: &str,
    ) -> mxr_core::provider::Result<MessageBody> {
        let (mailbox, uid) = folders::parse_provider_id(provider_message_id)
            .map_err(mxr_core::error::MxrError::from)?;

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        session
            .select(&mailbox)
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let fetched = session
            .uid_fetch(&uid.to_string(), "BODY[]")
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let _ = session.logout().await;

        let msg = fetched.first().ok_or_else(|| {
            mxr_core::error::MxrError::Provider(format!(
                "Message not found: {provider_message_id}"
            ))
        })?;

        let raw = msg.body.as_ref().ok_or_else(|| {
            mxr_core::error::MxrError::Provider(format!(
                "Empty body for message: {provider_message_id}"
            ))
        })?;

        let message_id = MessageId::from_provider_id("imap", provider_message_id);
        Ok(parse::parse_message_body(raw, &message_id))
    }

    async fn fetch_attachment(
        &self,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> mxr_core::provider::Result<Vec<u8>> {
        let (mailbox, uid) = folders::parse_provider_id(provider_message_id)
            .map_err(mxr_core::error::MxrError::from)?;

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        session
            .select(&mailbox)
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let fetched = session
            .uid_fetch(&uid.to_string(), "BODY[]")
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let _ = session.logout().await;

        let msg = fetched.first().ok_or_else(|| {
            mxr_core::error::MxrError::Provider(format!(
                "Message not found: {provider_message_id}"
            ))
        })?;

        let raw = msg.body.as_ref().ok_or_else(|| {
            mxr_core::error::MxrError::Provider("Empty body".into())
        })?;

        let parsed = mail_parser::MessageParser::default().parse(raw);
        let parsed = parsed.ok_or_else(|| {
            mxr_core::error::MxrError::Provider("Failed to parse message".into())
        })?;

        let part_idx: usize = provider_attachment_id.parse().map_err(|_| {
            mxr_core::error::MxrError::Provider(format!(
                "Invalid attachment ID: {provider_attachment_id}"
            ))
        })?;

        let part = parsed.parts.get(part_idx).ok_or_else(|| {
            mxr_core::error::MxrError::Provider(format!(
                "Attachment part {part_idx} not found"
            ))
        })?;

        Ok(part.contents().to_vec())
    }

    async fn modify_labels(
        &self,
        provider_message_id: &str,
        add: &[String],
        remove: &[String],
    ) -> mxr_core::provider::Result<()> {
        let (mailbox, uid) = folders::parse_provider_id(provider_message_id)
            .map_err(mxr_core::error::MxrError::from)?;

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        session
            .select(&mailbox)
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let uid_str = uid.to_string();

        // Map label names to IMAP flag operations
        let add_flags: Vec<&str> = add
            .iter()
            .filter_map(|l| label_to_flag(l))
            .collect();
        let remove_flags: Vec<&str> = remove
            .iter()
            .filter_map(|l| label_to_flag(l))
            .collect();

        if !add_flags.is_empty() {
            let flag_str = format!("+FLAGS ({})", add_flags.join(" "));
            session
                .uid_store(&uid_str, &flag_str)
                .await
                .map_err(mxr_core::error::MxrError::from)?;
        }

        if !remove_flags.is_empty() {
            let flag_str = format!("-FLAGS ({})", remove_flags.join(" "));
            session
                .uid_store(&uid_str, &flag_str)
                .await
                .map_err(mxr_core::error::MxrError::from)?;
        }

        // Handle folder moves (labels that are actually folder names)
        let add_folders: Vec<&str> = add
            .iter()
            .filter(|l| label_to_flag(l).is_none())
            .map(|s| s.as_str())
            .collect();

        for folder in add_folders {
            session
                .uid_copy(&uid_str, folder)
                .await
                .map_err(mxr_core::error::MxrError::from)?;
        }

        let _ = session.logout().await;
        Ok(())
    }

    async fn trash(&self, provider_message_id: &str) -> mxr_core::provider::Result<()> {
        let (mailbox, uid) = folders::parse_provider_id(provider_message_id)
            .map_err(mxr_core::error::MxrError::from)?;

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        session
            .select(&mailbox)
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let uid_str = uid.to_string();

        // Copy to trash folder
        session
            .uid_copy(&uid_str, &self.trash_folder)
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        // Mark deleted in source
        session
            .uid_store(&uid_str, "+FLAGS (\\Deleted)")
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        // Expunge
        session
            .expunge()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let _ = session.logout().await;

        debug!(
            provider_id = provider_message_id,
            trash_folder = %self.trash_folder,
            "IMAP trash complete"
        );

        Ok(())
    }

    async fn set_read(
        &self,
        provider_message_id: &str,
        read: bool,
    ) -> mxr_core::provider::Result<()> {
        let (mailbox, uid) = folders::parse_provider_id(provider_message_id)
            .map_err(mxr_core::error::MxrError::from)?;

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        session
            .select(&mailbox)
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let flag_op = if read {
            "+FLAGS (\\Seen)"
        } else {
            "-FLAGS (\\Seen)"
        };

        session
            .uid_store(&uid.to_string(), flag_op)
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let _ = session.logout().await;
        Ok(())
    }

    async fn set_starred(
        &self,
        provider_message_id: &str,
        starred: bool,
    ) -> mxr_core::provider::Result<()> {
        let (mailbox, uid) = folders::parse_provider_id(provider_message_id)
            .map_err(mxr_core::error::MxrError::from)?;

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        session
            .select(&mailbox)
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let flag_op = if starred {
            "+FLAGS (\\Flagged)"
        } else {
            "-FLAGS (\\Flagged)"
        };

        session
            .uid_store(&uid.to_string(), flag_op)
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let _ = session.logout().await;
        Ok(())
    }
}

/// Map known label names to IMAP flags. Returns None for folder-based labels.
fn label_to_flag(label: &str) -> Option<&'static str> {
    match label.to_uppercase().as_str() {
        "READ" | "SEEN" => Some("\\Seen"),
        "STARRED" | "FLAGGED" => Some("\\Flagged"),
        "DRAFT" | "DRAFTS" => Some("\\Draft"),
        "ANSWERED" => Some("\\Answered"),
        "DELETED" | "TRASH" => Some("\\Deleted"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::mock::MockImapSessionFactory;
    use crate::types::*;

    fn test_config() -> ImapConfig {
        ImapConfig {
            host: "imap.test.com".to_string(),
            port: 993,
            username: "test@test.com".to_string(),
            password_ref: "test/imap".to_string(),
            use_tls: true,
        }
    }

    fn make_fetched_message(uid: u32, subject: &str, from_email: &str) -> FetchedMessage {
        FetchedMessage {
            uid,
            flags: vec!["\\Seen".to_string()],
            envelope: Some(ImapEnvelope {
                date: Some("Mon, 1 Jan 2024 12:00:00 +0000".to_string()),
                subject: Some(subject.to_string()),
                from: vec![ImapAddress {
                    name: None,
                    email: from_email.to_string(),
                }],
                to: vec![ImapAddress {
                    name: None,
                    email: "me@test.com".to_string(),
                }],
                cc: vec![],
                bcc: vec![],
                message_id: Some(format!("<msg{uid}@test.com>")),
                in_reply_to: None,
            }),
            body: None,
            header: None,
            size: Some(1024),
        }
    }

    // -- sync_labels ----------------------------------------------------------

    #[tokio::test]
    async fn sync_labels_returns_mapped_folders() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 1,
                exists: 0,
            },
            vec![],
            vec![
                FolderInfo {
                    name: "INBOX".to_string(),
                    special_use: Some("\\Inbox".to_string()),
                },
                FolderInfo {
                    name: "Sent Messages".to_string(),
                    special_use: Some("\\Sent".to_string()),
                },
                FolderInfo {
                    name: "Drafts".to_string(),
                    special_use: Some("\\Drafts".to_string()),
                },
                FolderInfo {
                    name: "Projects/Work".to_string(),
                    special_use: None,
                },
            ],
        );

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        let labels = provider.sync_labels().await.unwrap();
        assert_eq!(labels.len(), 4);
        assert_eq!(labels[0].name, "INBOX");
        assert_eq!(labels[0].kind, LabelKind::System);
        assert_eq!(labels[1].name, "SENT");
        assert_eq!(labels[1].kind, LabelKind::System);
        assert_eq!(labels[3].name, "Projects/Work");
        assert_eq!(labels[3].kind, LabelKind::Folder);
    }

    // -- sync_messages: initial -----------------------------------------------

    #[tokio::test]
    async fn initial_sync_fetches_inbox_messages() {
        let messages = vec![
            make_fetched_message(1, "Hello", "alice@example.com"),
            make_fetched_message(2, "Meeting", "bob@example.com"),
            make_fetched_message(3, "Report", "carol@example.com"),
        ];

        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 4,
                exists: 3,
            },
            vec![messages],
            vec![],
        );

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        let batch = provider.sync_messages(&SyncCursor::Initial).await.unwrap();

        assert_eq!(batch.upserted.len(), 3);
        assert_eq!(batch.upserted[0].subject, "Hello");
        assert_eq!(batch.upserted[1].subject, "Meeting");
        assert_eq!(batch.upserted[2].subject, "Report");
        assert!(batch.deleted_provider_ids.is_empty());

        match batch.next_cursor {
            SyncCursor::Imap {
                uid_validity,
                uid_next,
            } => {
                assert_eq!(uid_validity, 1);
                assert_eq!(uid_next, 4);
            }
            other => panic!("Expected Imap cursor, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn initial_sync_empty_mailbox() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 1,
                exists: 0,
            },
            vec![],
            vec![],
        );

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        let batch = provider.sync_messages(&SyncCursor::Initial).await.unwrap();
        assert!(batch.upserted.is_empty());
    }

    // -- sync_messages: delta -------------------------------------------------

    #[tokio::test]
    async fn delta_sync_fetches_new_messages() {
        let new_msg = make_fetched_message(4, "New message", "dave@example.com");

        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 5,
                exists: 4,
            },
            vec![vec![new_msg]],
            vec![],
        );

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        let cursor = SyncCursor::Imap {
            uid_validity: 1,
            uid_next: 4,
        };
        let batch = provider.sync_messages(&cursor).await.unwrap();

        assert_eq!(batch.upserted.len(), 1);
        assert_eq!(batch.upserted[0].subject, "New message");

        match batch.next_cursor {
            SyncCursor::Imap { uid_next, .. } => assert_eq!(uid_next, 5),
            other => panic!("Expected Imap cursor, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn delta_sync_no_new_messages() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 4,
                exists: 3,
            },
            vec![],
            vec![],
        );

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        let cursor = SyncCursor::Imap {
            uid_validity: 1,
            uid_next: 4,
        };
        let batch = provider.sync_messages(&cursor).await.unwrap();

        assert!(batch.upserted.is_empty());
    }

    #[tokio::test]
    async fn delta_sync_uid_validity_changed_falls_back_to_initial() {
        // UID validity changed: server returns uid_validity=2, cursor has 1
        // The delta_sync detects the mismatch and falls back to initial_sync.
        // initial_sync creates a new session, so we need the factory to handle two sessions.
        // First session (delta): SELECT returns uid_validity=2 (mismatch)
        // Second session (initial fallback): SELECT returns uid_validity=2, exists=1

        let msg = make_fetched_message(1, "After reset", "alice@example.com");

        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 2, // Changed from 1
                uid_next: 2,
                exists: 1,
            },
            vec![vec![msg]], // Used by the initial_sync fallback
            vec![],
        );

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        let cursor = SyncCursor::Imap {
            uid_validity: 1, // Old value
            uid_next: 100,
        };
        let batch = provider.sync_messages(&cursor).await.unwrap();

        // Should have fallen back to initial sync and got messages
        assert_eq!(batch.upserted.len(), 1);
        assert_eq!(batch.upserted[0].subject, "After reset");

        match batch.next_cursor {
            SyncCursor::Imap { uid_validity, .. } => assert_eq!(uid_validity, 2),
            other => panic!("Expected Imap cursor, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn delta_sync_filters_old_uids() {
        // IMAP returns uid 3 when we ask for "4:*" and there are no new messages
        // (server returns the highest existing UID). We should filter it out.
        let old_msg = FetchedMessage {
            uid: 3, // Below our uid_next of 4
            flags: vec![],
            envelope: Some(ImapEnvelope {
                date: Some("Mon, 1 Jan 2024 12:00:00 +0000".to_string()),
                subject: Some("Old message".to_string()),
                from: vec![ImapAddress {
                    name: None,
                    email: "alice@example.com".to_string(),
                }],
                to: vec![],
                cc: vec![],
                bcc: vec![],
                message_id: None,
                in_reply_to: None,
            }),
            body: None,
            header: None,
            size: None,
        };

        // uid_next > old_uid_next so delta path is taken, but the fetch returns only old UIDs
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 5, // > old_uid_next of 4
                exists: 3,
            },
            vec![vec![old_msg]],
            vec![],
        );

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        let cursor = SyncCursor::Imap {
            uid_validity: 1,
            uid_next: 4,
        };
        let batch = provider.sync_messages(&cursor).await.unwrap();

        // The old message should be filtered out
        assert!(batch.upserted.is_empty());
    }

    // -- fetch_body -----------------------------------------------------------

    #[tokio::test]
    async fn fetch_body_returns_parsed_body() {
        let raw_email = b"From: alice@example.com\r\nTo: bob@example.com\r\nSubject: Test\r\nContent-Type: text/plain\r\n\r\nHello world";

        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 2,
                exists: 1,
            },
            vec![vec![FetchedMessage {
                uid: 42,
                flags: vec![],
                envelope: None,
                body: Some(raw_email.to_vec()),
                header: None,
                size: None,
            }]],
            vec![],
        );

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        let body = provider.fetch_body("INBOX:42").await.unwrap();
        assert!(body.text_plain.is_some());
        assert!(body.text_plain.unwrap().contains("Hello world"));
    }

    // -- mutations ------------------------------------------------------------

    #[tokio::test]
    async fn set_read_sends_correct_flags() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 2,
                exists: 1,
            },
            vec![],
            vec![],
        );
        let log = factory.log.clone();

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        provider.set_read("INBOX:42", true).await.unwrap();

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"SELECT INBOX".to_string()));
        assert!(commands.contains(&"UID STORE 42 +FLAGS (\\Seen)".to_string()));
    }

    #[tokio::test]
    async fn set_read_false_removes_seen() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 2,
                exists: 1,
            },
            vec![],
            vec![],
        );
        let log = factory.log.clone();

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        provider.set_read("INBOX:42", false).await.unwrap();

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"UID STORE 42 -FLAGS (\\Seen)".to_string()));
    }

    #[tokio::test]
    async fn set_starred_sends_correct_flags() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 2,
                exists: 1,
            },
            vec![],
            vec![],
        );
        let log = factory.log.clone();

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        provider.set_starred("INBOX:42", true).await.unwrap();

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"UID STORE 42 +FLAGS (\\Flagged)".to_string()));
    }

    #[tokio::test]
    async fn trash_copies_and_deletes() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 2,
                exists: 1,
            },
            vec![],
            vec![],
        );
        let log = factory.log.clone();

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        provider.trash("INBOX:42").await.unwrap();

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"SELECT INBOX".to_string()));
        assert!(commands.contains(&"UID COPY 42 Trash".to_string()));
        assert!(commands.contains(&"UID STORE 42 +FLAGS (\\Deleted)".to_string()));
        assert!(commands.contains(&"EXPUNGE".to_string()));
    }

    #[tokio::test]
    async fn modify_labels_maps_flags_and_folders() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 2,
                exists: 1,
            },
            vec![],
            vec![],
        );
        let log = factory.log.clone();

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        provider
            .modify_labels(
                "INBOX:42",
                &["STARRED".to_string(), "Archive".to_string()],
                &["READ".to_string()],
            )
            .await
            .unwrap();

        let commands = log.lock().unwrap().commands.clone();
        // STARRED maps to \Flagged flag
        assert!(commands.contains(&"UID STORE 42 +FLAGS (\\Flagged)".to_string()));
        // READ maps to \Seen flag removal
        assert!(commands.contains(&"UID STORE 42 -FLAGS (\\Seen)".to_string()));
        // Archive is a folder, should be COPY'd
        assert!(commands.contains(&"UID COPY 42 Archive".to_string()));
    }

    // -- fetch_attachment -----------------------------------------------------

    #[tokio::test]
    async fn fetch_attachment_extracts_bytes() {
        let raw = concat!(
            "From: alice@example.com\r\n",
            "To: bob@example.com\r\n",
            "Subject: Test\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: multipart/mixed; boundary=\"boundary2\"\r\n",
            "\r\n",
            "--boundary2\r\n",
            "Content-Type: text/plain\r\n",
            "\r\n",
            "See attached.\r\n",
            "--boundary2\r\n",
            "Content-Type: application/pdf; name=\"report.pdf\"\r\n",
            "Content-Disposition: attachment; filename=\"report.pdf\"\r\n",
            "Content-Transfer-Encoding: base64\r\n",
            "\r\n",
            "SGVsbG8gV29ybGQ=\r\n",
            "--boundary2--\r\n",
        );

        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 2,
                exists: 1,
            },
            vec![vec![FetchedMessage {
                uid: 10,
                flags: vec![],
                envelope: None,
                body: Some(raw.as_bytes().to_vec()),
                header: None,
                size: None,
            }]],
            vec![],
        );

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        // Part index 2 should be the attachment (0=root multipart, 1=text, 2=attachment)
        let bytes = provider.fetch_attachment("INBOX:10", "2").await.unwrap();
        assert!(!bytes.is_empty());
    }

    // -- incompatible cursor --------------------------------------------------

    #[tokio::test]
    async fn sync_messages_rejects_gmail_cursor() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 1,
                exists: 0,
            },
            vec![],
            vec![],
        );

        let provider = ImapProvider::with_session_factory(
            AccountId::new(),
            test_config(),
            Box::new(factory),
        );

        let result = provider
            .sync_messages(&SyncCursor::Gmail { history_id: 1 })
            .await;
        assert!(result.is_err());
    }

    // -- integration: full sync flow ------------------------------------------

    #[tokio::test]
    async fn full_sync_flow_initial_then_delta_then_fetch_and_mutate() {
        let account_id = AccountId::new();
        let config = test_config();

        // Phase 1: Initial sync — 3 messages
        let initial_messages = vec![
            make_fetched_message(1, "First", "alice@example.com"),
            make_fetched_message(2, "Second", "bob@example.com"),
            make_fetched_message(3, "Third", "carol@example.com"),
        ];

        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 4,
                exists: 3,
            },
            vec![initial_messages],
            vec![],
        );

        let provider =
            ImapProvider::with_session_factory(account_id.clone(), config.clone(), Box::new(factory));

        let batch1 = provider.sync_messages(&SyncCursor::Initial).await.unwrap();
        assert_eq!(batch1.upserted.len(), 3);

        let cursor1 = batch1.next_cursor;

        // Phase 2: Delta sync — 1 new message
        let new_msg = make_fetched_message(4, "Fourth", "dave@example.com");
        let factory2 = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 5,
                exists: 4,
            },
            vec![vec![new_msg]],
            vec![],
        );

        let provider2 =
            ImapProvider::with_session_factory(account_id.clone(), config.clone(), Box::new(factory2));

        let batch2 = provider2.sync_messages(&cursor1).await.unwrap();
        assert_eq!(batch2.upserted.len(), 1);
        assert_eq!(batch2.upserted[0].subject, "Fourth");

        // Phase 3: Fetch body
        let raw_email = b"From: dave@example.com\r\nSubject: Fourth\r\nContent-Type: text/plain\r\n\r\nHello from Dave";
        let factory3 = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 5,
                exists: 4,
            },
            vec![vec![FetchedMessage {
                uid: 4,
                flags: vec![],
                envelope: None,
                body: Some(raw_email.to_vec()),
                header: None,
                size: None,
            }]],
            vec![],
        );

        let provider3 =
            ImapProvider::with_session_factory(account_id.clone(), config.clone(), Box::new(factory3));

        let body = provider3.fetch_body("INBOX:4").await.unwrap();
        assert!(body.text_plain.unwrap().contains("Hello from Dave"));

        // Phase 4: Mutate — star the message
        let factory4 = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 5,
                exists: 4,
            },
            vec![],
            vec![],
        );
        let log4 = factory4.log.clone();

        let provider4 =
            ImapProvider::with_session_factory(account_id, config, Box::new(factory4));

        provider4.set_starred("INBOX:4", true).await.unwrap();
        let cmds = log4.lock().unwrap().commands.clone();
        assert!(cmds.contains(&"UID STORE 4 +FLAGS (\\Flagged)".to_string()));
    }
}
