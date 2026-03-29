pub mod config;
pub mod error;
pub mod folders;
pub mod parse;
pub mod session;
pub mod types;

use async_trait::async_trait;
use config::ImapConfig;
use mxr_core::id::AccountId;
use mxr_core::provider::MailSyncProvider;
use mxr_core::types::*;
use session::{ImapSessionFactory, RealImapSessionFactory};
use std::collections::{HashMap, HashSet};
use tracing::{debug, warn};

use crate::types::{FolderInfo, ImapCapabilities};

pub struct ImapProvider {
    account_id: AccountId,
    trash_folder: String,
    session_factory: Box<dyn ImapSessionFactory>,
}

impl ImapProvider {
    pub fn new(account_id: AccountId, config: ImapConfig) -> Self {
        let session_factory = Box::new(RealImapSessionFactory::new(config.clone()));
        Self {
            account_id,
            trash_folder: "Trash".to_string(),
            session_factory,
        }
    }

    /// Constructor for tests — inject a mock session factory.
    #[cfg(test)]
    pub fn with_session_factory(
        account_id: AccountId,
        _config: ImapConfig,
        session_factory: Box<dyn ImapSessionFactory>,
    ) -> Self {
        Self {
            account_id,
            trash_folder: "Trash".to_string(),
            session_factory,
        }
    }

    pub fn with_trash_folder(mut self, folder: String) -> Self {
        self.trash_folder = folder;
        self
    }

    fn build_imap_cursor(
        mailboxes: Vec<ImapMailboxCursor>,
        capabilities: &ImapCapabilities,
    ) -> SyncCursor {
        let fallback = mailboxes
            .iter()
            .find(|mailbox| mailbox.mailbox.eq_ignore_ascii_case("INBOX"))
            .or_else(|| mailboxes.first());

        SyncCursor::Imap {
            uid_validity: fallback.map(|mailbox| mailbox.uid_validity).unwrap_or(0),
            uid_next: fallback.map(|mailbox| mailbox.uid_next).unwrap_or(0),
            mailboxes,
            capabilities: Some(ImapCapabilityState {
                move_ext: capabilities.move_ext,
                uidplus: capabilities.uidplus,
                idle: capabilities.idle,
                condstore: capabilities.condstore,
                qresync: capabilities.qresync,
                namespace: capabilities.namespace,
                list_status: capabilities.list_status,
                utf8_accept: capabilities.utf8_accept,
                imap4rev2: capabilities.imap4rev2,
            }),
        }
    }

    fn syncable_folders(folders: &[FolderInfo]) -> Vec<FolderInfo> {
        let folders: Vec<FolderInfo> = folders
            .iter()
            .filter(|folder| folder.special_use.as_deref() != Some("\\All"))
            .cloned()
            .collect();

        if folders.is_empty() {
            vec![FolderInfo {
                name: "INBOX".to_string(),
                special_use: Some("\\Inbox".to_string()),
                ..Default::default()
            }]
        } else {
            folders
        }
    }

    fn resolve_folder_for_label(label: &str, folders: &[FolderInfo]) -> Option<String> {
        let label_upper = label.to_ascii_uppercase();
        let special_use = match label_upper.as_str() {
            "INBOX" => Some("\\Inbox"),
            "SENT" => Some("\\Sent"),
            "DRAFT" | "DRAFTS" => Some("\\Drafts"),
            "TRASH" => Some("\\Trash"),
            "SPAM" => Some("\\Junk"),
            "ARCHIVE" => Some("\\Archive"),
            "ALL" => Some("\\All"),
            _ => None,
        };

        if let Some(special_use) = special_use {
            if let Some(folder) = folders.iter().find(|folder| {
                folder
                    .special_use
                    .as_deref()
                    .is_some_and(|value: &str| value.eq_ignore_ascii_case(special_use))
                    || (special_use == "\\Inbox" && folder.name.eq_ignore_ascii_case("INBOX"))
            }) {
                return Some(folder.name.clone());
            }
        }

        folders
            .iter()
            .find(|folder| folder.name.eq_ignore_ascii_case(label))
            .map(|folder| folder.name.clone())
            .or_else(|| {
                if label_upper == "ALL" || label_upper == "TRASH" {
                    None
                } else {
                    Some(label.to_string())
                }
            })
    }

    fn enableable_capabilities(capabilities: &ImapCapabilities) -> Vec<&'static str> {
        let mut enabled = Vec::new();
        if capabilities.qresync {
            enabled.push("QRESYNC");
        }
        if capabilities.condstore && !capabilities.qresync {
            enabled.push("CONDSTORE");
        }
        if capabilities.utf8_accept {
            enabled.push("UTF8=ACCEPT");
        }
        enabled
    }

    async fn enable_session(
        session: &mut dyn session::ImapSession,
        capabilities: &ImapCapabilities,
    ) -> mxr_core::provider::Result<()> {
        let enabled = Self::enableable_capabilities(capabilities);
        session
            .enable(&enabled)
            .await
            .map_err(mxr_core::error::MxrError::from)
    }

    fn known_uid_set(mailbox: &ImapMailboxCursor) -> Option<String> {
        (mailbox.uid_next > 1).then(|| format!("1:{}", mailbox.uid_next - 1))
    }

    fn fetch_query_for_changed_since(modseq: u64) -> String {
        format!("(FLAGS BODY.PEEK[] RFC822.SIZE) (CHANGEDSINCE {modseq})")
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "sync fetch state is explicit at this call boundary"
    )]
    async fn collect_synced_messages(
        session: &mut dyn session::ImapSession,
        mailbox: &str,
        uid_set: &str,
        query: &str,
        min_uid: u32,
        seen_uids: &mut HashSet<u32>,
        account_id: &AccountId,
        synced: &mut Vec<SyncedMessage>,
    ) -> mxr_core::provider::Result<()> {
        let fetched = session
            .uid_fetch(uid_set, query)
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        for msg in &fetched {
            if msg.uid < min_uid || !seen_uids.insert(msg.uid) {
                continue;
            }
            match parse::imap_fetch_to_synced_message(msg, mailbox, account_id) {
                Ok(sm) => synced.push(sm),
                Err(e) => warn!(
                    mailbox = %mailbox,
                    uid = msg.uid,
                    error = %e,
                    "Failed to parse IMAP message"
                ),
            }
        }

        Ok(())
    }

    async fn delete_selected_message(
        session: &mut dyn session::ImapSession,
        uid: &str,
        capabilities: &ImapCapabilities,
    ) -> mxr_core::provider::Result<()> {
        session
            .uid_store(uid, "+FLAGS (\\Deleted)")
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        if capabilities.uidplus {
            session
                .uid_expunge(uid)
                .await
                .map_err(mxr_core::error::MxrError::from)?;
        } else {
            session
                .expunge()
                .await
                .map_err(mxr_core::error::MxrError::from)?;
        }

        Ok(())
    }

    async fn move_selected_message(
        session: &mut dyn session::ImapSession,
        uid: &str,
        target_folder: &str,
        capabilities: &ImapCapabilities,
    ) -> mxr_core::provider::Result<()> {
        if capabilities.move_ext {
            session
                .uid_move(uid, target_folder)
                .await
                .map_err(mxr_core::error::MxrError::from)?;
        } else {
            session
                .uid_copy(uid, target_folder)
                .await
                .map_err(mxr_core::error::MxrError::from)?;
            Self::delete_selected_message(session, uid, capabilities).await?;
        }

        Ok(())
    }

    async fn assert_mutable_folder(&self, folder_name: &str) -> mxr_core::provider::Result<()> {
        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let folders = session
            .list_folders()
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        let _ = session.logout().await;

        let is_special_use = folders
            .iter()
            .find(|folder| folder.name == folder_name)
            .is_some_and(|folder| {
                folder.special_use.is_some() || folder.name.eq_ignore_ascii_case("inbox")
            });

        if is_special_use {
            return Err(mxr_core::error::MxrError::Provider(
                "Cannot modify IMAP system folders".to_string(),
            ));
        }

        Ok(())
    }

    /// Initial sync: fetch all messages from syncable mailboxes via UID FETCH.
    async fn initial_sync(&self) -> mxr_core::provider::Result<SyncBatch> {
        debug!("Starting IMAP initial sync for account {}", self.account_id);

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let capabilities = session
            .capabilities()
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        debug!(?capabilities, "IMAP server capabilities");
        Self::enable_session(&mut *session, &capabilities).await?;

        let folders = session
            .list_folders()
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        let sync_folders = Self::syncable_folders(&folders);

        let mut synced = Vec::new();
        let mut mailboxes = Vec::with_capacity(sync_folders.len());
        for folder in &sync_folders {
            let mailbox_info = session
                .select(&folder.name)
                .await
                .map_err(mxr_core::error::MxrError::from)?;
            mailboxes.push(ImapMailboxCursor {
                mailbox: folder.name.clone(),
                uid_validity: mailbox_info.uid_validity,
                uid_next: mailbox_info.uid_next,
                highest_modseq: mailbox_info.highest_modseq,
            });

            if mailbox_info.exists == 0 {
                continue;
            }

            let fetched = session
                .uid_fetch("1:*", "(FLAGS BODY.PEEK[] RFC822.SIZE)")
                .await
                .map_err(mxr_core::error::MxrError::from)?;

            for msg in &fetched {
                match parse::imap_fetch_to_synced_message(msg, &folder.name, &self.account_id) {
                    Ok(sm) => synced.push(sm),
                    Err(e) => warn!(
                        mailbox = %folder.name,
                        uid = msg.uid,
                        error = %e,
                        "Failed to parse IMAP message"
                    ),
                }
            }
        }

        let _ = session.logout().await;

        debug!("IMAP initial sync complete: {} messages", synced.len());

        Ok(SyncBatch {
            upserted: synced,
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor: Self::build_imap_cursor(mailboxes, &capabilities),
        })
    }

    /// Delta sync: fetch new messages per mailbox since the last known UIDNEXT.
    async fn delta_sync(
        &self,
        old_mailboxes: &[ImapMailboxCursor],
    ) -> mxr_core::provider::Result<SyncBatch> {
        debug!(
            mailbox_count = old_mailboxes.len(),
            "Starting IMAP delta sync for account {}", self.account_id
        );

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let capabilities = session
            .capabilities()
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        debug!(?capabilities, "IMAP server capabilities");
        Self::enable_session(&mut *session, &capabilities).await?;

        let folders = session
            .list_folders()
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        let sync_folders = Self::syncable_folders(&folders);
        let old_by_mailbox: HashMap<&str, &ImapMailboxCursor> = old_mailboxes
            .iter()
            .map(|mailbox| (mailbox.mailbox.as_str(), mailbox))
            .collect();

        let mut synced = Vec::new();
        let mut mailboxes = Vec::with_capacity(sync_folders.len());
        let mut deleted_provider_ids = Vec::new();

        for folder in &sync_folders {
            let old_mailbox = old_by_mailbox.get(folder.name.as_str()).copied();
            let mut qresync_used = false;
            let mut seen_uids = HashSet::new();
            let mailbox_info =
                if capabilities.qresync {
                    match old_mailbox
                        .and_then(|mailbox| mailbox.highest_modseq.map(|modseq| (mailbox, modseq)))
                        .and_then(|(mailbox, modseq)| {
                            Self::known_uid_set(mailbox)
                                .map(|known_uids| (mailbox, modseq, known_uids))
                        }) {
                        Some((old_mailbox, highest_modseq, known_uids)) => {
                            match session
                                .select_qresync(
                                    &folder.name,
                                    old_mailbox.uid_validity,
                                    highest_modseq,
                                    &known_uids,
                                )
                                .await
                            {
                                Ok(response) => {
                                    qresync_used = true;
                                    deleted_provider_ids.extend(response.vanished.iter().map(
                                        |uid| folders::format_provider_id(&folder.name, *uid),
                                    ));
                                    if !response.changed.is_empty() {
                                        let uid_set = response
                                            .changed
                                            .iter()
                                            .map(u32::to_string)
                                            .collect::<Vec<_>>()
                                            .join(",");
                                        Self::collect_synced_messages(
                                            &mut *session,
                                            &folder.name,
                                            &uid_set,
                                            "(FLAGS BODY.PEEK[] RFC822.SIZE)",
                                            1,
                                            &mut seen_uids,
                                            &self.account_id,
                                            &mut synced,
                                        )
                                        .await?;
                                    }
                                    response.mailbox
                                }
                                Err(error) => {
                                    warn!(
                                        mailbox = %folder.name,
                                        error = %error,
                                        "QRESYNC failed, falling back to SELECT"
                                    );
                                    session
                                        .select(&folder.name)
                                        .await
                                        .map_err(mxr_core::error::MxrError::from)?
                                }
                            }
                        }
                        None => session
                            .select(&folder.name)
                            .await
                            .map_err(mxr_core::error::MxrError::from)?,
                    }
                } else {
                    session
                        .select(&folder.name)
                        .await
                        .map_err(mxr_core::error::MxrError::from)?
                };
            mailboxes.push(ImapMailboxCursor {
                mailbox: folder.name.clone(),
                uid_validity: mailbox_info.uid_validity,
                uid_next: mailbox_info.uid_next,
                highest_modseq: mailbox_info.highest_modseq,
            });

            if !qresync_used && capabilities.condstore && mailbox_info.highest_modseq.is_some() {
                let Some(old_mailbox) = old_mailbox else {
                    continue;
                };
                if mailbox_info.uid_validity == old_mailbox.uid_validity
                    && old_mailbox
                        .highest_modseq
                        .zip(mailbox_info.highest_modseq)
                        .is_some_and(|(old_modseq, new_modseq)| new_modseq > old_modseq)
                {
                    Self::collect_synced_messages(
                        &mut *session,
                        &folder.name,
                        "1:*",
                        &Self::fetch_query_for_changed_since(
                            old_mailbox.highest_modseq.expect("checked is_some"),
                        ),
                        1,
                        &mut seen_uids,
                        &self.account_id,
                        &mut synced,
                    )
                    .await?;
                }
            }

            let query = match old_mailbox {
                Some(old_mailbox) if mailbox_info.uid_validity != old_mailbox.uid_validity => {
                    warn!(
                        mailbox = %folder.name,
                        old = old_mailbox.uid_validity,
                        new = mailbox_info.uid_validity,
                        "UIDVALIDITY changed, resyncing mailbox from scratch"
                    );
                    if mailbox_info.exists == 0 {
                        None
                    } else {
                        Some("1:*".to_string())
                    }
                }
                Some(old_mailbox) if mailbox_info.uid_next > old_mailbox.uid_next => {
                    Some(format!("{}:*", old_mailbox.uid_next))
                }
                Some(_) => None,
                None if mailbox_info.exists == 0 => None,
                None => Some("1:*".to_string()),
            };

            let Some(query) = query else {
                continue;
            };

            let min_uid = match old_mailbox {
                Some(old_mailbox) if mailbox_info.uid_validity == old_mailbox.uid_validity => {
                    old_mailbox.uid_next
                }
                _ => 1,
            };

            Self::collect_synced_messages(
                &mut *session,
                &folder.name,
                &query,
                "(FLAGS BODY.PEEK[] RFC822.SIZE)",
                min_uid,
                &mut seen_uids,
                &self.account_id,
                &mut synced,
            )
            .await?;
        }

        let _ = session.logout().await;

        debug!("IMAP delta sync complete: {} new messages", synced.len());

        Ok(SyncBatch {
            upserted: synced,
            deleted_provider_ids,
            label_changes: vec![],
            next_cursor: Self::build_imap_cursor(mailboxes, &capabilities),
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
            delta_sync: true,
            push: false,
            batch_operations: false,
            native_thread_ids: false,
        }
    }

    async fn authenticate(&mut self) -> mxr_core::provider::Result<()> {
        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        let _ = session.logout().await;
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
                let mut label = folders::map_folder_to_label(
                    &f.name,
                    f.special_use.as_deref(),
                    &self.account_id,
                );
                label.unread_count = f.unread_count.unwrap_or(0);
                label.total_count = f.total_count.unwrap_or(0);
                label
            })
            .collect())
    }

    async fn sync_messages(&self, cursor: &SyncCursor) -> mxr_core::provider::Result<SyncBatch> {
        match cursor {
            SyncCursor::Initial => self.initial_sync().await,
            SyncCursor::Imap {
                uid_validity,
                uid_next,
                mailboxes,
                ..
            } => {
                let legacy_mailboxes = if mailboxes.is_empty() {
                    vec![ImapMailboxCursor {
                        mailbox: "INBOX".to_string(),
                        uid_validity: *uid_validity,
                        uid_next: *uid_next,
                        highest_modseq: None,
                    }]
                } else {
                    mailboxes.clone()
                };
                self.delta_sync(&legacy_mailboxes).await
            }
            other => Err(mxr_core::error::MxrError::Provider(format!(
                "IMAP provider received incompatible cursor: {other:?}"
            ))),
        }
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
            mxr_core::error::MxrError::Provider(format!("Message not found: {provider_message_id}"))
        })?;

        let raw = msg
            .body
            .as_ref()
            .ok_or_else(|| mxr_core::error::MxrError::Provider("Empty body".into()))?;

        let parsed = mail_parser::MessageParser::default().parse(raw);
        let parsed = parsed
            .ok_or_else(|| mxr_core::error::MxrError::Provider("Failed to parse message".into()))?;

        let part_idx: usize = provider_attachment_id.parse().map_err(|_| {
            mxr_core::error::MxrError::Provider(format!(
                "Invalid attachment ID: {provider_attachment_id}"
            ))
        })?;

        let part = parsed.parts.get(part_idx).ok_or_else(|| {
            mxr_core::error::MxrError::Provider(format!("Attachment part {part_idx} not found"))
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
        let capabilities = session
            .capabilities()
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        let folders = session
            .list_folders()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let uid_str = uid.to_string();

        // Map label names to IMAP flag operations
        let add_flags: Vec<&str> = add.iter().filter_map(|l| label_to_flag(l)).collect();
        let remove_flags: Vec<&str> = remove.iter().filter_map(|l| label_to_flag(l)).collect();

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
        let add_folders: Vec<String> = add
            .iter()
            .filter(|l| label_to_flag(l).is_none())
            .filter_map(|label| Self::resolve_folder_for_label(label, &folders))
            .filter(|folder| !folder.eq_ignore_ascii_case(&mailbox))
            .collect();

        let remove_current_mailbox = remove.iter().any(|label| {
            label.eq_ignore_ascii_case(&mailbox)
                || (mailbox.eq_ignore_ascii_case("INBOX") && label.eq_ignore_ascii_case("INBOX"))
        });

        if remove_current_mailbox && add_folders.is_empty() && mailbox.eq_ignore_ascii_case("INBOX")
        {
            let archive_folder =
                Self::resolve_folder_for_label("ARCHIVE", &folders).ok_or_else(|| {
                    mxr_core::error::MxrError::Provider(
                        "Archive folder not found on IMAP server".to_string(),
                    )
                })?;
            Self::move_selected_message(&mut *session, &uid_str, &archive_folder, &capabilities)
                .await?;
        } else if remove_current_mailbox && add_folders.len() == 1 {
            Self::move_selected_message(&mut *session, &uid_str, &add_folders[0], &capabilities)
                .await?;
        } else {
            for folder in &add_folders {
                session
                    .uid_copy(&uid_str, folder)
                    .await
                    .map_err(mxr_core::error::MxrError::from)?;
            }

            if remove_current_mailbox {
                Self::delete_selected_message(&mut *session, &uid_str, &capabilities).await?;
            }
        }

        let _ = session.logout().await;
        Ok(())
    }

    async fn create_label(
        &self,
        name: &str,
        _color: Option<&str>,
    ) -> mxr_core::provider::Result<Label> {
        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        session
            .create_mailbox(name)
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        let _ = session.logout().await;

        Ok(folders::map_folder_to_label(name, None, &self.account_id))
    }

    async fn rename_label(
        &self,
        provider_label_id: &str,
        new_name: &str,
    ) -> mxr_core::provider::Result<Label> {
        self.assert_mutable_folder(provider_label_id).await?;

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        session
            .rename_mailbox(provider_label_id, new_name)
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        let _ = session.logout().await;

        Ok(folders::map_folder_to_label(
            new_name,
            None,
            &self.account_id,
        ))
    }

    async fn delete_label(&self, provider_label_id: &str) -> mxr_core::provider::Result<()> {
        self.assert_mutable_folder(provider_label_id).await?;

        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        session
            .delete_mailbox(provider_label_id)
            .await
            .map_err(mxr_core::error::MxrError::from)?;
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
        let capabilities = session
            .capabilities()
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        let folders = session
            .list_folders()
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let uid_str = uid.to_string();
        let trash_folder = Self::resolve_folder_for_label("TRASH", &folders)
            .unwrap_or_else(|| self.trash_folder.clone());

        if mailbox.eq_ignore_ascii_case(&trash_folder) {
            let _ = session.logout().await;
            return Ok(());
        }

        Self::move_selected_message(&mut *session, &uid_str, &trash_folder, &capabilities).await?;

        let _ = session.logout().await;

        debug!(
            provider_id = provider_message_id,
            trash_folder = %trash_folder,
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
            auth_required: true,
            use_tls: true,
        }
    }

    fn make_fetched_message(uid: u32, subject: &str, from_email: &str) -> FetchedMessage {
        // Build raw RFC822 message for full body parsing
        let raw = format!(
            "From: {from_email}\r\nTo: me@test.com\r\nSubject: {subject}\r\nDate: Mon, 1 Jan 2024 12:00:00 +0000\r\nMessage-ID: <msg{uid}@test.com>\r\nContent-Type: text/plain\r\n\r\nBody of {subject}"
        );
        FetchedMessage {
            uid,
            flags: vec!["\\Seen".to_string()],
            envelope: None,
            body: Some(raw.into_bytes()),
            header: None,
            size: Some(1024),
        }
    }

    fn mailbox_info(uid_validity: u32, uid_next: u32, exists: u32) -> MailboxInfo {
        MailboxInfo {
            uid_validity,
            uid_next,
            exists,
            highest_modseq: None,
        }
    }

    fn folder_info(name: &str, special_use: Option<&str>) -> FolderInfo {
        FolderInfo {
            name: name.to_string(),
            special_use: special_use.map(str::to_string),
            ..Default::default()
        }
    }

    fn imap_cursor(uid_validity: u32, uid_next: u32) -> SyncCursor {
        SyncCursor::Imap {
            uid_validity,
            uid_next,
            mailboxes: vec![ImapMailboxCursor {
                mailbox: "INBOX".into(),
                uid_validity,
                uid_next,
                highest_modseq: None,
            }],
            capabilities: None,
        }
    }

    // -- sync_labels ----------------------------------------------------------

    #[tokio::test]
    async fn sync_labels_returns_mapped_folders() {
        let factory = MockImapSessionFactory::new(
            mailbox_info(1, 1, 0),
            vec![],
            vec![
                folder_info("INBOX", Some("\\Inbox")),
                folder_info("Sent Messages", Some("\\Sent")),
                folder_info("Drafts", Some("\\Drafts")),
                folder_info("Projects/Work", None),
            ],
        );

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let labels = provider.sync_labels().await.unwrap();
        assert_eq!(labels.len(), 4);
        assert_eq!(labels[0].name, "INBOX");
        assert_eq!(labels[0].kind, LabelKind::System);
        assert_eq!(labels[1].name, "SENT");
        assert_eq!(labels[1].kind, LabelKind::System);
        assert_eq!(labels[3].name, "Projects/Work");
        assert_eq!(labels[3].kind, LabelKind::Folder);
    }

    #[tokio::test]
    async fn imap_provider_passes_sync_conformance() {
        let factory = MockImapSessionFactory::new(
            mailbox_info(1, 3, 2),
            vec![
                vec![
                    make_fetched_message(1, "Inbox fixture", "alice@example.com"),
                    make_fetched_message(2, "Attachment fixture", "bob@example.com"),
                ],
                vec![],
            ],
            vec![
                folder_info("INBOX", Some("\\Inbox")),
                folder_info("Archive", Some("\\Archive")),
            ],
        );
        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));
        mxr_provider_fake::conformance::run_sync_conformance(&provider).await;
    }

    #[tokio::test]
    async fn sync_labels_surfaces_folder_counts() {
        let mut inbox = folder_info("INBOX", Some("\\Inbox"));
        inbox.unread_count = Some(7);
        inbox.total_count = Some(12);

        let factory = MockImapSessionFactory::new(mailbox_info(1, 1, 0), vec![], vec![inbox]);

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let labels = provider.sync_labels().await.unwrap();
        assert_eq!(labels[0].unread_count, 7);
        assert_eq!(labels[0].total_count, 12);
    }

    // -- sync_messages: initial -----------------------------------------------

    #[tokio::test]
    async fn initial_sync_fetches_inbox_messages() {
        let messages = vec![
            make_fetched_message(1, "Hello", "alice@example.com"),
            make_fetched_message(2, "Meeting", "bob@example.com"),
            make_fetched_message(3, "Report", "carol@example.com"),
        ];

        let factory = MockImapSessionFactory::new(mailbox_info(1, 4, 3), vec![messages], vec![]);

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let batch = provider.sync_messages(&SyncCursor::Initial).await.unwrap();

        assert_eq!(batch.upserted.len(), 3);
        assert_eq!(batch.upserted[0].envelope.subject, "Hello");
        assert_eq!(batch.upserted[1].envelope.subject, "Meeting");
        assert_eq!(batch.upserted[2].envelope.subject, "Report");
        assert!(batch.deleted_provider_ids.is_empty());

        match batch.next_cursor {
            SyncCursor::Imap {
                uid_validity,
                uid_next,
                ..
            } => {
                assert_eq!(uid_validity, 1);
                assert_eq!(uid_next, 4);
            }
            other => panic!("Expected Imap cursor, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn initial_sync_empty_mailbox() {
        let factory = MockImapSessionFactory::new(mailbox_info(1, 1, 0), vec![], vec![]);

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let batch = provider.sync_messages(&SyncCursor::Initial).await.unwrap();
        assert!(batch.upserted.is_empty());
    }

    #[tokio::test]
    async fn initial_sync_fetches_multiple_mailboxes_and_skips_all_mail() {
        let factory = MockImapSessionFactory::new(
            mailbox_info(1, 2, 1),
            vec![
                vec![make_fetched_message(
                    1,
                    "Inbox message",
                    "alice@example.com",
                )],
                vec![make_fetched_message(
                    1,
                    "Archive message",
                    "bob@example.com",
                )],
            ],
            vec![
                folder_info("INBOX", Some("\\Inbox")),
                folder_info("Archive", Some("\\Archive")),
                folder_info("All Mail", Some("\\All")),
            ],
        );

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let batch = provider.sync_messages(&SyncCursor::Initial).await.unwrap();
        assert_eq!(batch.upserted.len(), 2);
        assert_eq!(batch.upserted[0].envelope.provider_id, "INBOX:1");
        assert_eq!(batch.upserted[1].envelope.provider_id, "Archive:1");
        match batch.next_cursor {
            SyncCursor::Imap { mailboxes, .. } => {
                assert_eq!(mailboxes.len(), 2);
                assert!(mailboxes.iter().any(|mailbox| mailbox.mailbox == "INBOX"));
                assert!(mailboxes.iter().any(|mailbox| mailbox.mailbox == "Archive"));
            }
            other => panic!("Expected Imap cursor, got: {other:?}"),
        }
    }

    // -- sync_messages: delta -------------------------------------------------

    #[tokio::test]
    async fn delta_sync_fetches_new_messages() {
        let new_msg = make_fetched_message(4, "New message", "dave@example.com");

        let factory =
            MockImapSessionFactory::new(mailbox_info(1, 5, 4), vec![vec![new_msg]], vec![]);

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let cursor = imap_cursor(1, 4);
        let batch = provider.sync_messages(&cursor).await.unwrap();

        assert_eq!(batch.upserted.len(), 1);
        assert_eq!(batch.upserted[0].envelope.subject, "New message");

        match batch.next_cursor {
            SyncCursor::Imap { uid_next, .. } => assert_eq!(uid_next, 5),
            other => panic!("Expected Imap cursor, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn delta_sync_no_new_messages() {
        let factory = MockImapSessionFactory::new(mailbox_info(1, 4, 3), vec![], vec![]);

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let cursor = imap_cursor(1, 4);
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
                highest_modseq: None,
            },
            vec![vec![msg]], // Used by the initial_sync fallback
            vec![],
        );

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let cursor = imap_cursor(1, 100); // Old value
        let batch = provider.sync_messages(&cursor).await.unwrap();

        // Should have fallen back to initial sync and got messages
        assert_eq!(batch.upserted.len(), 1);
        assert_eq!(batch.upserted[0].envelope.subject, "After reset");

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
            envelope: None,
            body: Some(b"From: alice@example.com\r\nSubject: Old message\r\nDate: Mon, 1 Jan 2024 12:00:00 +0000\r\nContent-Type: text/plain\r\n\r\nOld body".to_vec()),
            header: None,
            size: None,
        };

        // uid_next > old_uid_next so delta path is taken, but the fetch returns only old UIDs
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 5, // > old_uid_next of 4
                exists: 3,
                highest_modseq: None,
            },
            vec![vec![old_msg]],
            vec![],
        );

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let cursor = imap_cursor(1, 4);
        let batch = provider.sync_messages(&cursor).await.unwrap();

        // The old message should be filtered out
        assert!(batch.upserted.is_empty());
    }

    #[tokio::test]
    async fn delta_sync_enables_qresync_and_tracks_vanished_messages() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 5,
                exists: 4,
                highest_modseq: Some(20),
            },
            vec![
                vec![make_fetched_message(
                    3,
                    "Changed message",
                    "alice@example.com",
                )],
                vec![make_fetched_message(4, "Brand new", "bob@example.com")],
            ],
            vec![],
        )
        .with_capabilities(ImapCapabilities {
            condstore: true,
            qresync: true,
            utf8_accept: true,
            ..Default::default()
        })
        .with_qresync(QresyncInfo {
            mailbox: MailboxInfo {
                uid_validity: 1,
                uid_next: 5,
                exists: 4,
                highest_modseq: Some(20),
            },
            vanished: vec![2],
            changed: vec![3],
        });
        let log = factory.log.clone();

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let cursor = SyncCursor::Imap {
            uid_validity: 1,
            uid_next: 4,
            mailboxes: vec![ImapMailboxCursor {
                mailbox: "INBOX".into(),
                uid_validity: 1,
                uid_next: 4,
                highest_modseq: Some(10),
            }],
            capabilities: None,
        };

        let batch = provider.sync_messages(&cursor).await.unwrap();

        assert_eq!(batch.deleted_provider_ids, vec!["INBOX:2"]);
        assert_eq!(batch.upserted.len(), 2);
        assert!(batch
            .upserted
            .iter()
            .any(|message| message.envelope.subject == "Changed message"));
        assert!(batch
            .upserted
            .iter()
            .any(|message| message.envelope.subject == "Brand new"));

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"ENABLE QRESYNC UTF8=ACCEPT".to_string()));
        assert!(commands.contains(&"SELECT INBOX QRESYNC".to_string()));
    }

    #[tokio::test]
    async fn delta_sync_uses_condstore_changedsince_when_qresync_is_unavailable() {
        let factory = MockImapSessionFactory::new(
            MailboxInfo {
                uid_validity: 1,
                uid_next: 5,
                exists: 4,
                highest_modseq: Some(20),
            },
            vec![
                vec![
                    make_fetched_message(3, "Changed flags", "alice@example.com"),
                    make_fetched_message(4, "New via condstore", "bob@example.com"),
                ],
                vec![make_fetched_message(
                    4,
                    "New via condstore",
                    "bob@example.com",
                )],
            ],
            vec![],
        )
        .with_capabilities(ImapCapabilities {
            condstore: true,
            ..Default::default()
        });
        let log = factory.log.clone();

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let cursor = SyncCursor::Imap {
            uid_validity: 1,
            uid_next: 4,
            mailboxes: vec![ImapMailboxCursor {
                mailbox: "INBOX".into(),
                uid_validity: 1,
                uid_next: 4,
                highest_modseq: Some(10),
            }],
            capabilities: None,
        };

        let batch = provider.sync_messages(&cursor).await.unwrap();

        assert_eq!(batch.upserted.len(), 2);
        assert!(batch
            .upserted
            .iter()
            .any(|message| message.envelope.subject == "Changed flags"));
        assert!(batch
            .upserted
            .iter()
            .any(|message| message.envelope.subject == "New via condstore"));

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"ENABLE CONDSTORE".to_string()));
        assert!(commands
            .iter()
            .any(|command| command.contains("CHANGEDSINCE 10")));
    }

    // -- mutations ------------------------------------------------------------

    #[tokio::test]
    async fn set_read_sends_correct_flags() {
        let factory = MockImapSessionFactory::new(mailbox_info(1, 2, 1), vec![], vec![]);
        let log = factory.log.clone();

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        provider.set_read("INBOX:42", true).await.unwrap();

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"SELECT INBOX".to_string()));
        assert!(commands.contains(&"UID STORE 42 +FLAGS (\\Seen)".to_string()));
    }

    #[tokio::test]
    async fn set_read_false_removes_seen() {
        let factory = MockImapSessionFactory::new(mailbox_info(1, 2, 1), vec![], vec![]);
        let log = factory.log.clone();

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        provider.set_read("INBOX:42", false).await.unwrap();

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"UID STORE 42 -FLAGS (\\Seen)".to_string()));
    }

    #[tokio::test]
    async fn set_starred_sends_correct_flags() {
        let factory = MockImapSessionFactory::new(mailbox_info(1, 2, 1), vec![], vec![]);
        let log = factory.log.clone();

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        provider.set_starred("INBOX:42", true).await.unwrap();

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"UID STORE 42 +FLAGS (\\Flagged)".to_string()));
    }

    #[tokio::test]
    async fn trash_copies_and_deletes() {
        let factory = MockImapSessionFactory::new(mailbox_info(1, 2, 1), vec![], vec![]);
        let log = factory.log.clone();

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        provider.trash("INBOX:42").await.unwrap();

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"SELECT INBOX".to_string()));
        assert!(commands.contains(&"UID COPY 42 Trash".to_string()));
        assert!(commands.contains(&"UID STORE 42 +FLAGS (\\Deleted)".to_string()));
        assert!(commands.contains(&"EXPUNGE".to_string()));
    }

    #[tokio::test]
    async fn trash_uses_move_when_server_supports_it() {
        let factory = MockImapSessionFactory::new(
            mailbox_info(1, 2, 1),
            vec![],
            vec![folder_info("Trash", Some("\\Trash"))],
        )
        .with_capabilities(ImapCapabilities {
            move_ext: true,
            ..Default::default()
        });
        let log = factory.log.clone();

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        provider.trash("INBOX:42").await.unwrap();

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"UID MOVE 42 Trash".to_string()));
        assert!(!commands.contains(&"UID COPY 42 Trash".to_string()));
        assert!(!commands.contains(&"EXPUNGE".to_string()));
    }

    #[tokio::test]
    async fn trash_uses_uid_expunge_when_uidplus_is_available() {
        let factory = MockImapSessionFactory::new(
            mailbox_info(1, 2, 1),
            vec![],
            vec![folder_info("Trash", Some("\\Trash"))],
        )
        .with_capabilities(ImapCapabilities {
            uidplus: true,
            ..Default::default()
        });
        let log = factory.log.clone();

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        provider.trash("INBOX:42").await.unwrap();

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"UID COPY 42 Trash".to_string()));
        assert!(commands.contains(&"UID EXPUNGE 42".to_string()));
        assert!(!commands.contains(&"EXPUNGE".to_string()));
    }

    #[tokio::test]
    async fn modify_labels_maps_flags_and_folders() {
        let factory = MockImapSessionFactory::new(mailbox_info(1, 2, 1), vec![], vec![]);
        let log = factory.log.clone();

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

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

    #[tokio::test]
    async fn modify_labels_archive_moves_out_of_inbox() {
        let factory = MockImapSessionFactory::new(
            mailbox_info(1, 2, 1),
            vec![],
            vec![folder_info("Archive", Some("\\Archive"))],
        )
        .with_capabilities(ImapCapabilities {
            move_ext: true,
            ..Default::default()
        });
        let log = factory.log.clone();

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        provider
            .modify_labels("INBOX:42", &[], &["INBOX".to_string()])
            .await
            .unwrap();

        let commands = log.lock().unwrap().commands.clone();
        assert!(commands.contains(&"UID MOVE 42 Archive".to_string()));
        assert!(!commands.contains(&"UID STORE 42 +FLAGS (\\Deleted)".to_string()));
    }

    #[tokio::test]
    async fn rename_label_rejects_special_use_folder() {
        let factory = MockImapSessionFactory::new(
            mailbox_info(1, 2, 1),
            vec![],
            vec![folder_info("INBOX", Some("\\Inbox"))],
        );

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        let err = provider.rename_label("INBOX", "Archive").await.unwrap_err();
        assert!(err.to_string().contains("system folders"));
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
            mailbox_info(1, 2, 1),
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

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

        // Part index 2 should be the attachment (0=root multipart, 1=text, 2=attachment)
        let bytes = provider.fetch_attachment("INBOX:10", "2").await.unwrap();
        assert!(!bytes.is_empty());
    }

    // -- incompatible cursor --------------------------------------------------

    #[tokio::test]
    async fn sync_messages_rejects_gmail_cursor() {
        let factory = MockImapSessionFactory::new(mailbox_info(1, 1, 0), vec![], vec![]);

        let provider =
            ImapProvider::with_session_factory(AccountId::new(), test_config(), Box::new(factory));

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

        let factory =
            MockImapSessionFactory::new(mailbox_info(1, 4, 3), vec![initial_messages], vec![]);

        let provider = ImapProvider::with_session_factory(
            account_id.clone(),
            config.clone(),
            Box::new(factory),
        );

        let batch1 = provider.sync_messages(&SyncCursor::Initial).await.unwrap();
        assert_eq!(batch1.upserted.len(), 3);

        let cursor1 = batch1.next_cursor;

        // Phase 2: Delta sync — 1 new message
        let new_msg = make_fetched_message(4, "Fourth", "dave@example.com");
        let factory2 =
            MockImapSessionFactory::new(mailbox_info(1, 5, 4), vec![vec![new_msg]], vec![]);

        let provider2 = ImapProvider::with_session_factory(
            account_id.clone(),
            config.clone(),
            Box::new(factory2),
        );

        let batch2 = provider2.sync_messages(&cursor1).await.unwrap();
        assert_eq!(batch2.upserted.len(), 1);
        assert_eq!(batch2.upserted[0].envelope.subject, "Fourth");
        // Body is eagerly fetched during sync
        assert!(batch2.upserted[0]
            .body
            .text_plain
            .as_deref()
            .unwrap_or("")
            .contains("Body of Fourth"));

        // Phase 3: Mutate — star the message
        let factory3 = MockImapSessionFactory::new(mailbox_info(1, 5, 4), vec![], vec![]);
        let log3 = factory3.log.clone();

        let provider3 = ImapProvider::with_session_factory(account_id, config, Box::new(factory3));

        provider3.set_starred("INBOX:4", true).await.unwrap();
        let cmds = log3.lock().unwrap().commands.clone();
        assert!(cmds.contains(&"UID STORE 4 +FLAGS (\\Flagged)".to_string()));
    }
}
