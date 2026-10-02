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
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

/// Deleted messages cleared per round, bounding one round's work.
const DRAIN_BATCH: u32 = 1_000;

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
        let failed = forget_deleted_messages(state, &message_ids).await;
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
/// attachment files. Returns the messages whose files could not be removed.
/// A semantic failure is logged and not retried: the index is rebuilt from
/// the store, which no longer holds the messages, on the next rebuild or
/// restart.
async fn forget_deleted_messages(
    state: &AppState,
    message_ids: &[MessageId],
) -> HashSet<MessageId> {
    if let Err(error) = state.semantic.forget_messages(message_ids).await {
        tracing::warn!(
            messages = message_ids.len(),
            %error,
            "semantic index could not forget deleted messages"
        );
    }
    remove_attachment_files(&state.attachment_dir(), message_ids).await
}

/// Removes the per-message directories the daemon writes under the
/// attachment dir: downloaded attachments (`<id>/`) and inline HTML images
/// (`_html_assets/<id>/`). Returns the messages whose files may still be
/// there; a directory that does not exist counts as removed.
///
/// Lists each root once and removes the entries named for a deleted id, so
/// an account purge of 100k messages costs one listing, not 200k probes.
/// Only directories named exactly as a deleted message id are touched.
async fn remove_attachment_files(
    attachment_dir: &Path,
    message_ids: &[MessageId],
) -> HashSet<MessageId> {
    let by_name: std::collections::HashMap<String, MessageId> = message_ids
        .iter()
        .map(|id| (id.as_str(), id.clone()))
        .collect();
    let all = message_ids.iter().cloned().collect::<HashSet<_>>();
    let roots = [
        attachment_dir.to_path_buf(),
        attachment_dir.join(crate::handler::HTML_ASSETS_DIR),
    ];
    let result = tokio::task::spawn_blocking(move || {
        let mut failed = HashSet::new();
        for root in roots {
            let entries = match std::fs::read_dir(&root) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    // Nothing under this root could be checked.
                    tracing::warn!(path = %root.display(), %error, "could not list attachment files");
                    return by_name.into_values().collect();
                }
            };
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        tracing::warn!(path = %root.display(), %error, "could not list attachment files");
                        return by_name.into_values().collect();
                    }
                };
                let Some(message_id) = entry.file_name().to_str().and_then(|name| by_name.get(name))
                else {
                    continue;
                };
                let path = entry.path();
                match std::fs::remove_dir_all(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        tracing::warn!(
                            path = %path.display(),
                            %error,
                            "could not remove files of a deleted message"
                        );
                        failed.insert(message_id.clone());
                    }
                }
            }
        }
        failed
    })
    .await;
    match result {
        Ok(failed) => failed,
        Err(error) => {
            tracing::warn!(%error, "attachment cleanup task failed");
            all
        }
    }
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
        let failed =
            remove_attachment_files(root.path(), &[deleted.clone(), MessageId::new()]).await;
        assert!(failed.is_empty());

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
