//! Clears what lives outside SQLite once messages leave the store.
//!
//! The store delete takes every row derived from a message with it. Two
//! things it cannot reach: the semantic engine's in-memory index and the
//! files the daemon wrote for the message. The delete records each message
//! in `pending_message_forgets` in its own transaction; [`drain_pending_forgets`]
//! clears those. It runs after every delete, at startup and on a timer, so a
//! failed sync pass, a crash, or a file that could not be removed leaves the
//! work owed, not lost.

use crate::state::AppState;
use mxr_core::id::MessageId;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Deleted messages cleared per round, bounding one round's work.
const DRAIN_BATCH: u32 = 1_000;

/// Longest the cleanup waits on the semantic worker while sync is held off.
const SEMANTIC_FORGET_TIMEOUT: Duration = Duration::from_secs(10);

/// How often owed cleanups are retried with nothing else prompting it.
pub(crate) const DRAIN_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// Clears every deleted message whose cleanup is due. A message whose files
/// could not be removed stays recorded and is retried after a backoff (the
/// store caps the attempts).
pub(crate) async fn drain_pending_forgets(state: &AppState) {
    loop {
        let message_ids = match state.store.list_pending_message_forgets(DRAIN_BATCH).await {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(%error, "could not read deleted messages owed cleanup");
                return;
            }
        };
        if message_ids.is_empty() {
            return;
        }
        let Some(failed) = forget_deleted_messages(state, &message_ids, |_| async {}).await else {
            return;
        };
        let cleared = message_ids
            .iter()
            .filter(|id| !failed.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        if let Err(error) = state.store.clear_pending_message_forgets(&cleared).await {
            // Left recorded; the next drain repeats the (idempotent) work.
            tracing::warn!(%error, "could not mark deleted messages cleared");
            return;
        }
        if !failed.is_empty() {
            let failed = failed.into_iter().collect::<Vec<_>>();
            match state.store.record_failed_message_forgets(&failed).await {
                Ok(0) => {}
                Ok(gave_up) => tracing::error!(
                    gave_up,
                    attempts = mxr_store::MAX_FORGET_ATTEMPTS,
                    "giving up on removing files of deleted messages; they stay on disk"
                ),
                Err(error) => {
                    tracing::warn!(%error, "could not record failed cleanups");
                    return;
                }
            }
        }
        if message_ids.len() < DRAIN_BATCH as usize {
            return;
        }
    }
}

/// Runs [`drain_pending_forgets`] every [`DRAIN_INTERVAL`] until shutdown.
pub(crate) async fn drain_pending_forgets_periodically(state: std::sync::Arc<AppState>) {
    let mut shutdown = state.shutdown_receiver();
    loop {
        drain_pending_forgets(&state).await;
        tokio::select! {
            () = tokio::time::sleep(DRAIN_INTERVAL) => {}
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow_and_update() {
                    return;
                }
            }
        }
    }
}

/// Drops deleted messages from the semantic index and removes their
/// attachment files. Returns the messages whose files could not be removed,
/// or `None` when the attachment dir could not be listed and nothing was
/// done.
///
/// An id that names a stored message again (an IMAP UID reused for a new
/// message) is skipped and counts as done: its files and index entries are
/// the new message's.
///
/// Sync takes the same store lock to store messages, so the lock is held
/// only for short steps: the attachment dirs are listed without it, then
/// each message's folders are removed under its own hold (check the id is
/// still absent, remove, release), and the semantic forget runs once under
/// a final hold that re-checks the ids and is bounded by a timeout. A
/// semantic failure is logged and not retried: the index is rebuilt from
/// the store, which no longer holds the messages, on the next rebuild or
/// restart. `after_each` runs after every message's hold is released.
async fn forget_deleted_messages<F, Fut>(
    state: &AppState,
    message_ids: &[MessageId],
    mut after_each: F,
) -> Option<HashSet<MessageId>>
where
    F: FnMut(&MessageId) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut dirs = find_message_dirs(&state.attachment_dir(), message_ids).await?;
    let mut failed = HashSet::new();
    for message_id in message_ids {
        {
            let _cleanup = state.store.lock_message_cleanup().await;
            match state
                .store
                .existing_message_ids(std::slice::from_ref(message_id))
                .await
            {
                Ok(live) if live.is_empty() => {
                    let paths = dirs.remove(message_id).unwrap_or_default();
                    if !remove_dirs(paths).await {
                        failed.insert(message_id.clone());
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(%error, "could not check a deleted message before its cleanup");
                    failed.insert(message_id.clone());
                }
            }
        }
        after_each(message_id).await;
    }

    let _cleanup = state.store.lock_message_cleanup().await;
    let live = match state.store.existing_message_ids(message_ids).await {
        Ok(live) => live,
        Err(error) => {
            tracing::warn!(%error, "could not check deleted messages before forgetting them");
            return Some(failed);
        }
    };
    let gone = message_ids
        .iter()
        .filter(|id| !live.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    if gone.is_empty() {
        return Some(failed);
    }
    // Bounded: sync waits on this lock, and the semantic worker can be busy
    // with an ingest batch. A forget that times out is not retried; the
    // vectors go at the next index rebuild, which reads only stored rows.
    match tokio::time::timeout(
        SEMANTIC_FORGET_TIMEOUT,
        state.semantic.forget_messages(&gone),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!(
            messages = gone.len(),
            %error,
            "semantic index could not forget deleted messages"
        ),
        Err(_) => tracing::warn!(
            messages = gone.len(),
            "semantic index did not answer the forget in time; its next rebuild drops them"
        ),
    }
    Some(failed)
}

/// The per-message directories the daemon writes under the attachment dir,
/// for each of these messages: downloaded attachments (`<id>/`) and inline
/// HTML images (`_html_assets/<id>/`). `None` when a root that exists could
/// not be listed, so nothing can be removed safely this round.
///
/// Lists each root once, so an account purge of 100k messages costs one
/// listing, not 200k probes. Only directories named exactly as one of the
/// message ids are returned.
async fn find_message_dirs(
    attachment_dir: &Path,
    message_ids: &[MessageId],
) -> Option<HashMap<MessageId, Vec<PathBuf>>> {
    let by_name: HashMap<String, MessageId> = message_ids
        .iter()
        .map(|id| (id.as_str(), id.clone()))
        .collect();
    let roots = [
        attachment_dir.to_path_buf(),
        attachment_dir.join(crate::handler::HTML_ASSETS_DIR),
    ];
    let result = tokio::task::spawn_blocking(move || {
        let mut found: HashMap<MessageId, Vec<PathBuf>> = HashMap::new();
        for root in roots {
            let entries = match std::fs::read_dir(&root) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    tracing::warn!(path = %root.display(), %error, "could not list attachment files");
                    return None;
                }
            };
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        tracing::warn!(path = %root.display(), %error, "could not list attachment files");
                        return None;
                    }
                };
                if let Some(message_id) =
                    entry.file_name().to_str().and_then(|name| by_name.get(name))
                {
                    found
                        .entry(message_id.clone())
                        .or_default()
                        .push(entry.path());
                }
            }
        }
        Some(found)
    })
    .await;
    match result {
        Ok(found) => found,
        Err(error) => {
            tracing::warn!(%error, "attachment listing task failed");
            None
        }
    }
}

/// Removes one message's directories. Returns whether they are all gone; a
/// directory that no longer exists counts as removed.
async fn remove_dirs(paths: Vec<PathBuf>) -> bool {
    if paths.is_empty() {
        return true;
    }
    let result = tokio::task::spawn_blocking(move || {
        let mut removed = true;
        for path in paths {
            match std::fs::remove_dir_all(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(
                        path = %path.display(),
                        %error,
                        "could not remove files of a deleted message"
                    );
                    removed = false;
                }
            }
        }
        removed
    })
    .await;
    result.unwrap_or_else(|error| {
        tracing::warn!(%error, "attachment cleanup task failed");
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A delete whose sync pass failed afterwards, or whose daemon died
    /// before the fan-out ran, still has its files cleared by the next drain.
    #[tokio::test]
    async fn a_drain_clears_files_owed_by_an_earlier_delete() {
        let state = AppState::in_memory().await.unwrap();
        let attachments = tempfile::tempdir().unwrap();
        state.set_attachment_dir_for_tests(attachments.path().to_path_buf());
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        let gone = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .provider_id("gone")
            .build();
        state.store.upsert_envelope(&gone).await.unwrap();
        let files = attachments.path().join(gone.id.as_str());
        std::fs::create_dir_all(&files).unwrap();
        // The delete commits; nothing runs the fan-out.
        state
            .store
            .delete_messages_and_derived(&account_id, &["gone".to_string()])
            .await
            .unwrap();
        assert!(files.exists());

        drain_pending_forgets(&state).await;

        assert!(!files.exists());
        assert!(state
            .store
            .list_pending_message_forgets(10)
            .await
            .unwrap()
            .is_empty());
    }

    /// A cleanup owed to a deleted message must not touch a new message that
    /// took the same id (an IMAP UID reused under the same folder): its
    /// files stay and the stale cleanup is dropped.
    #[tokio::test]
    async fn a_cleanup_owed_to_an_id_that_came_back_leaves_the_new_message_alone() {
        let state = AppState::in_memory().await.unwrap();
        let attachments = tempfile::tempdir().unwrap();
        state.set_attachment_dir_for_tests(attachments.path().to_path_buf());
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        let reused = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .provider_id("INBOX:1")
            .build();
        state.store.upsert_envelope(&reused).await.unwrap();
        state
            .store
            .delete_messages_and_derived(&account_id, &["INBOX:1".to_string()])
            .await
            .unwrap();
        // The new message under the same id arrives through a path that does
        // not clear the owed cleanup, and downloads an attachment.
        state.store.upsert_envelope(&reused).await.unwrap();
        let files = attachments.path().join(reused.id.as_str());
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("new.pdf"), b"pdf").unwrap();

        drain_pending_forgets(&state).await;

        assert!(
            files.join("new.pdf").exists(),
            "the new message's file was removed"
        );
        let owed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_message_forgets")
            .fetch_one(state.store.reader())
            .await
            .unwrap();
        assert_eq!(owed, 0, "the stale cleanup stayed owed");
    }

    /// Sync storing a message under an id a cleanup is about to clear must
    /// not land between the cleanup's live check and its removal. With the
    /// sync holding the lock, the drain waits, then sees the revived id and
    /// leaves its files.
    #[tokio::test]
    async fn a_cleanup_waits_for_a_sync_that_revives_its_id() {
        let state = std::sync::Arc::new(AppState::in_memory().await.unwrap());
        let attachments = tempfile::tempdir().unwrap();
        state.set_attachment_dir_for_tests(attachments.path().to_path_buf());
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        let reused = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .provider_id("INBOX:1")
            .build();
        state.store.upsert_envelope(&reused).await.unwrap();
        state
            .store
            .delete_messages_and_derived(&account_id, &["INBOX:1".to_string()])
            .await
            .unwrap();

        // A sync is mid-write: it holds the lock while it stores the new
        // message under the reused id.
        let sync_holds = state.store.lock_message_cleanup().await;
        let drain_state = state.clone();
        let drain = tokio::spawn(async move { drain_pending_forgets(&drain_state).await });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!drain.is_finished(), "the drain did not wait for the sync");
        state.store.upsert_envelope(&reused).await.unwrap();
        let files = attachments.path().join(reused.id.as_str());
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("new.pdf"), b"pdf").unwrap();
        drop(sync_holds);
        drain.await.unwrap();

        assert!(
            files.join("new.pdf").exists(),
            "the new message's file was removed"
        );
    }

    /// A drain over many messages releases the cleanup lock between them,
    /// so sync can store mail mid-drain instead of waiting for the whole
    /// batch. A sync that revives a later message's id in that gap keeps
    /// that message's files: its own hold re-checks the id.
    #[tokio::test]
    async fn a_sync_upsert_can_land_between_the_messages_of_a_drain() {
        let state = AppState::in_memory().await.unwrap();
        let attachments = tempfile::tempdir().unwrap();
        state.set_attachment_dir_for_tests(attachments.path().to_path_buf());
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        let envelopes = ["INBOX:1", "INBOX:2"].map(|provider_id| {
            crate::test_fixtures::TestEnvelopeBuilder::new()
                .account_id(account_id.clone())
                .provider_id(provider_id)
                .build()
        });
        for envelope in &envelopes {
            state.store.upsert_envelope(envelope).await.unwrap();
            let files = attachments.path().join(envelope.id.as_str());
            std::fs::create_dir_all(&files).unwrap();
            std::fs::write(files.join("file.pdf"), b"pdf").unwrap();
        }
        state
            .store
            .delete_messages_and_derived(
                &account_id,
                &["INBOX:1".to_string(), "INBOX:2".to_string()],
            )
            .await
            .unwrap();
        let ids = envelopes.iter().map(|e| e.id.clone()).collect::<Vec<_>>();
        let revived = envelopes[1].clone();
        let mut synced_mid_drain = false;

        let failed = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            forget_deleted_messages(&state, &ids, |done| {
                // After the first message, a sync stores a new message under
                // the second id. Held across the batch, the lock would make
                // this wait for the drain and time the test out.
                let first = *done == ids[0];
                let revived = revived.clone();
                let state = &state;
                synced_mid_drain |= first;
                async move {
                    if first {
                        state
                            .store
                            .apply_sync_upserts(&mut [mxr_store::SyncUpsert {
                                body: crate::test_fixtures::make_empty_body(&revived.id),
                                envelope: revived,
                                direction: mxr_core::types::MessageDirection::Inbound,
                                label_ids: vec![],
                            }])
                            .await
                            .unwrap();
                    }
                }
            }),
        )
        .await
        .expect("a sync upsert could not run between the messages of a drain")
        .unwrap();

        assert!(synced_mid_drain);
        assert!(failed.is_empty());
        assert!(!attachments.path().join(ids[0].as_str()).exists());
        assert!(
            attachments
                .path()
                .join(ids[1].as_str())
                .join("file.pdf")
                .exists(),
            "the revived message's files were removed"
        );
    }

    /// A permission error leaves the files and the owed cleanup in place;
    /// the drain records a retry instead of clearing the row.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_cleanup_that_cannot_remove_files_stays_owed() {
        use std::os::unix::fs::PermissionsExt;
        let state = AppState::in_memory().await.unwrap();
        let attachments = tempfile::tempdir().unwrap();
        state.set_attachment_dir_for_tests(attachments.path().to_path_buf());
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        let gone = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .provider_id("gone")
            .build();
        state.store.upsert_envelope(&gone).await.unwrap();
        let files = attachments.path().join(gone.id.as_str());
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("invoice.pdf"), b"pdf").unwrap();
        state
            .store
            .delete_messages_and_derived(&account_id, &["gone".to_string()])
            .await
            .unwrap();
        // A read-only dir: its entries cannot be unlinked.
        std::fs::set_permissions(&files, std::fs::Permissions::from_mode(0o555)).unwrap();

        drain_pending_forgets(&state).await;

        std::fs::set_permissions(&files, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(files.join("invoice.pdf").exists());
        let attempts: Vec<i64> = sqlx::query_scalar("SELECT attempts FROM pending_message_forgets")
            .fetch_all(state.store.reader())
            .await
            .unwrap();
        assert_eq!(attempts, vec![1], "the owed cleanup was dropped");
    }

    #[tokio::test]
    async fn removes_both_file_dirs_of_deleted_messages_and_nothing_else() {
        let root = tempfile::tempdir().unwrap();
        let deleted = MessageId::new();
        let kept = MessageId::new();
        for id in [&deleted, &kept] {
            let attachments = root.path().join(id.as_str());
            std::fs::create_dir_all(&attachments).unwrap();
            std::fs::write(attachments.join("invoice.pdf"), b"pdf").unwrap();
            let images = root
                .path()
                .join(crate::handler::HTML_ASSETS_DIR)
                .join(id.as_str());
            std::fs::create_dir_all(&images).unwrap();
            std::fs::write(images.join("logo.png"), b"png").unwrap();
        }

        // A message that never had files is not an error.
        let mut found = find_message_dirs(root.path(), &[deleted.clone(), MessageId::new()])
            .await
            .unwrap();
        assert_eq!(found.len(), 1, "only the deleted message has folders");
        assert!(remove_dirs(found.remove(&deleted).unwrap()).await);

        assert!(!root.path().join(deleted.as_str()).exists());
        assert!(!root
            .path()
            .join(crate::handler::HTML_ASSETS_DIR)
            .join(deleted.as_str())
            .exists());
        assert!(root.path().join(kept.as_str()).join("invoice.pdf").exists());
        assert!(root
            .path()
            .join(crate::handler::HTML_ASSETS_DIR)
            .join(kept.as_str())
            .join("logo.png")
            .exists());
    }
}
