//! Newest-first paged initial sync for generic IMAP servers.
//!
//! Gmail-over-IMAP has its own ascending All Mail pager in `lib.rs`; this
//! module serves every other server. A first sync used to download every
//! folder before returning one batch, so a new account showed nothing until
//! the last folder landed. Now each batch holds at most
//! [`GENERIC_BACKFILL_PAGE_MESSAGES`] messages, newest UIDs first, INBOX
//! first, and the sync engine commits each batch as it arrives.
//!
//! Folder order is INBOX, then the rest in LIST order. UIDs are only
//! comparable inside one folder and fetching dates across folders would cost
//! a second pass, so there is no global newest-first; INBOX is what a user
//! opens first. A page fills from the active folder and spills into the next
//! one when the folder runs out, so a small INBOX does not waste a round trip.
//!
//! Between pages the cursor is a `"v":"2"` envelope (see `cursor.rs`) holding,
//! per folder, the frozen horizon (`started.uid_next`) and the highest UID not
//! yet fetched. A finished folder becomes a normal delta cursor with
//! `uid_next` = frozen horizon, so mail that arrived during the backfill is
//! fetched by the first delta sync.

use std::collections::HashSet;

use mxr_core::provider::Result;
use mxr_core::types::{ImapMailboxCursor, SyncBatch, SyncedMessage};
use tracing::{debug, warn};

use crate::cursor::{ImapCursor, ImapCursorV2, ImapFolderBackfill};
use crate::session::ImapSession;
use crate::types::{FolderInfo, ImapCapabilities};
use crate::{CollectSyncedMessages, ImapProvider, IMAP_FETCH_UID_BATCH_SIZE};

/// Matches the Gmail All Mail page (500 UIDs) so both IMAP paths report
/// progress at the same grain.
pub(crate) const GENERIC_BACKFILL_PAGE_MESSAGES: usize = 500;

/// First `UID SEARCH` window below `next_high`, in UIDs. Mailboxes are often
/// sparse (archived-out INBOX, emptied Trash), so existing UIDs are found by
/// search and only those are fetched; the window grows by
/// [`DISCOVERY_WINDOW_GROWTH`] while it finds too few, which bounds a folder
/// with UIDNEXT 200,000 and 50 messages to a handful of searches.
const DISCOVERY_WINDOW_START: u32 = 2_000;
const DISCOVERY_WINDOW_GROWTH: u32 = 4;

impl ImapProvider {
    pub(crate) async fn continue_generic_backfill(&self, state: ImapCursorV2) -> Result<SyncBatch> {
        let mut session = self
            .session_factory
            .create_session()
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        let capabilities = session
            .capabilities()
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        Self::enable_session(&mut *session, &capabilities).await?;
        let folders = session
            .list_folders()
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        let sync_folders = Self::syncable_folders(&folders);
        self.generic_backfill_page(session, capabilities, sync_folders, Some(state))
            .await
    }

    pub(crate) async fn generic_backfill_page(
        &self,
        mut session: Box<dyn ImapSession>,
        capabilities: ImapCapabilities,
        sync_folders: Vec<FolderInfo>,
        state: Option<ImapCursorV2>,
    ) -> Result<SyncBatch> {
        let (mut completed, mut pending) = reconcile(state, &sync_folders);
        let mut synced: Vec<SyncedMessage> = Vec::new();

        while synced.len() < GENERIC_BACKFILL_PAGE_MESSAGES && !pending.is_empty() {
            let mut folder = pending.remove(0);
            session = self
                .backfill_folder(session, &capabilities, &mut folder, &mut synced)
                .await?;
            match folder.finished_cursor() {
                Some(mailbox) => completed.push(mailbox),
                None => {
                    pending.insert(0, folder);
                    break;
                }
            }
        }
        let _ = session.logout().await;

        let has_more = !pending.is_empty();
        let capability_state = Some(Self::capability_state(&capabilities));
        let next_cursor = if has_more {
            ImapCursor::new_generic_backfill(completed, pending, capability_state).encode()
        } else {
            Self::build_imap_cursor(completed, &capabilities)
        };
        debug!(
            fetched = synced.len(),
            has_more, "IMAP generic backfill page complete"
        );

        Ok(SyncBatch {
            upserted: synced,
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor,
            has_more,
            threads_changed: vec![],
            // UIDs are sparse and EXISTS counts the whole mailbox, so neither
            // gives an honest "left to fetch".
            remaining_estimate: None,
        })
    }

    /// Fetch windows of the folder from `next_high` downward until the page is
    /// full or the folder is exhausted.
    async fn backfill_folder(
        &self,
        mut session: Box<dyn ImapSession>,
        capabilities: &ImapCapabilities,
        folder: &mut ImapFolderBackfill,
        synced: &mut Vec<SyncedMessage>,
    ) -> Result<Box<dyn ImapSession>> {
        let info = session
            .select(&folder.mailbox)
            .await
            .map_err(mxr_core::error::MxrError::from)?;

        let restart = match folder.started.as_ref() {
            None => true,
            Some(started) if started.uid_validity != info.uid_validity => {
                warn!(
                    mailbox = %folder.mailbox,
                    old = started.uid_validity,
                    new = info.uid_validity,
                    "UIDVALIDITY changed during IMAP backfill; restarting this folder"
                );
                true
            }
            Some(_) => false,
        };
        if restart {
            let mut uid_next = info.uid_next;
            if uid_next == 0 && info.exists > 0 {
                // Server omitted UIDNEXT on SELECT; derive the horizon from
                // the highest UID so the walk has a top to start from.
                let uids = session
                    .uid_search("ALL")
                    .await
                    .map_err(mxr_core::error::MxrError::from)?;
                uid_next = uids
                    .into_iter()
                    .max()
                    .map_or(1, |top| top.saturating_add(1));
            }
            folder.started = Some(ImapMailboxCursor {
                mailbox: folder.mailbox.clone(),
                uid_validity: info.uid_validity,
                uid_next,
                highest_modseq: info.highest_modseq,
            });
            folder.next_high = if info.exists == 0 {
                0
            } else {
                uid_next.saturating_sub(1)
            };
            folder.min_failed_uid = None;
        } else if info.exists == 0 {
            // Everything left to fetch was deleted since the last page.
            folder.next_high = 0;
        }

        while folder.next_high >= 1 && synced.len() < GENERIC_BACKFILL_PAGE_MESSAGES {
            let room = GENERIC_BACKFILL_PAGE_MESSAGES - synced.len();
            let (found, next_high) = discover_uids(&mut *session, folder.next_high, room).await?;
            for chunk in found.chunks(IMAP_FETCH_UID_BATCH_SIZE) {
                let uid_set = chunk
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                let mut seen_uids = HashSet::new();
                session = self
                    .collect_synced_messages(
                        session,
                        capabilities,
                        CollectSyncedMessages {
                            mailbox: &folder.mailbox,
                            uid_set: &uid_set,
                            query: Self::imap_fetch_query(),
                            min_uid: 1,
                            seen_uids: &mut seen_uids,
                            account_id: &self.account_id,
                            receiver: self.receiver,
                            synced,
                            min_failed_uid: &mut folder.min_failed_uid,
                        },
                    )
                    .await?;
            }
            folder.next_high = next_high;
        }
        Ok(session)
    }
}

/// UIDs that exist at or below `high`, highest first, at most `room` of them,
/// plus the new `next_high` (just below the lowest UID covered). A window that
/// finds fewer than `room` UIDs is followed by a larger one below it until the
/// folder's bottom is reached.
async fn discover_uids(
    session: &mut dyn ImapSession,
    high: u32,
    room: usize,
) -> Result<(Vec<u32>, u32)> {
    let mut found: Vec<u32> = Vec::new();
    let mut window_high = high;
    let mut window = DISCOVERY_WINDOW_START;
    loop {
        let low = window_high.saturating_sub(window - 1).max(1);
        let mut uids = session
            .uid_search(&format!("UID {low}:{window_high}"))
            .await
            .map_err(mxr_core::error::MxrError::from)?;
        // Servers only return UIDs in range; filtering keeps the walk correct
        // against one that does not.
        uids.retain(|uid| (low..=window_high).contains(uid));
        uids.sort_unstable_by(|a, b| b.cmp(a));
        uids.dedup();
        found.extend(uids);

        if found.len() >= room {
            found.truncate(room);
            // Everything above the lowest kept UID has been seen; the rest of
            // this window below it stays for the next page.
            let lowest = found.last().copied().unwrap_or(low);
            return Ok((found, lowest - 1));
        }
        if low <= 1 {
            return Ok((found, 0));
        }
        window_high = low - 1;
        window = window.saturating_mul(DISCOVERY_WINDOW_GROWTH);
    }
}

impl ImapFolderBackfill {
    fn fresh(mailbox: String) -> Self {
        Self {
            mailbox,
            started: None,
            next_high: 0,
            min_failed_uid: None,
        }
    }

    /// The folder's delta cursor once every UID up to the frozen horizon is
    /// fetched. A parse failure floors `uid_next` so the first delta retries
    /// from the failed UID instead of skipping past it.
    fn finished_cursor(&self) -> Option<ImapMailboxCursor> {
        let started = self.started.as_ref()?;
        if self.next_high != 0 {
            return None;
        }
        let mut mailbox = started.clone();
        ImapProvider::floor_uid_next_to_failed(&mut mailbox, self.min_failed_uid);
        Some(mailbox)
    }
}

/// Fold the stored backfill state (if any) into the current LIST: folders
/// that vanished are dropped so a deleted folder cannot wedge the walk, and
/// folders that appeared are queued at the end.
fn reconcile(
    state: Option<ImapCursorV2>,
    sync_folders: &[FolderInfo],
) -> (Vec<ImapMailboxCursor>, Vec<ImapFolderBackfill>) {
    let Some(state) = state else {
        let pending = inbox_first(sync_folders)
            .into_iter()
            .map(ImapFolderBackfill::fresh)
            .collect();
        return (Vec::new(), pending);
    };
    let live: HashSet<&str> = sync_folders.iter().map(|f| f.name.as_str()).collect();
    let mut completed = state.completed;
    completed.retain(|m| live.contains(m.mailbox.as_str()));
    let mut pending = state.pending;
    pending.retain(|f| live.contains(f.mailbox.as_str()));
    let known: HashSet<String> = completed
        .iter()
        .map(|m| m.mailbox.clone())
        .chain(pending.iter().map(|f| f.mailbox.clone()))
        .collect();
    for name in inbox_first(sync_folders) {
        if !known.contains(&name) {
            pending.push(ImapFolderBackfill::fresh(name));
        }
    }
    (completed, pending)
}

fn inbox_first(sync_folders: &[FolderInfo]) -> Vec<String> {
    let is_inbox = |f: &FolderInfo| {
        f.name.eq_ignore_ascii_case("INBOX") || f.special_use.as_deref() == Some("\\Inbox")
    };
    sync_folders
        .iter()
        .filter(|f| is_inbox(f))
        .chain(sync_folders.iter().filter(|f| !is_inbox(f)))
        .map(|f| f.name.clone())
        .collect()
}
