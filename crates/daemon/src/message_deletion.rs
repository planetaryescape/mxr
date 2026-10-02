//! Clears what lives outside SQLite once messages leave the store.
//!
//! The store delete takes every row derived from a message with it. Two
//! things it cannot reach: the semantic engine's in-memory index and the
//! files the daemon wrote for the message. Every path that deletes messages
//! (sync, account purge) calls [`forget_deleted_messages`] after its delete
//! commits.

use crate::state::AppState;
use mxr_core::id::MessageId;
use std::collections::HashSet;
use std::path::Path;

/// Drops deleted messages from the semantic index and removes their
/// attachment files. Failures are logged, not returned: the rows are already
/// gone, and a leftover is retried by nothing, so the log is the record.
pub(crate) async fn forget_deleted_messages(state: &AppState, message_ids: &[MessageId]) {
    if message_ids.is_empty() {
        return;
    }
    if let Err(error) = state.semantic.forget_messages(message_ids).await {
        tracing::warn!(
            messages = message_ids.len(),
            %error,
            "semantic index could not forget deleted messages"
        );
    }
    remove_attachment_files(&state.attachment_dir(), message_ids).await;
}

/// Removes the per-message directories the daemon writes under the
/// attachment dir: downloaded attachments (`<id>/`) and inline HTML images
/// (`_html_assets/<id>/`).
///
/// Lists each root once and removes the entries named for a deleted id, so
/// an account purge of 100k messages costs one listing, not 200k probes.
/// Only directories named exactly as a deleted message id are touched.
async fn remove_attachment_files(attachment_dir: &Path, message_ids: &[MessageId]) {
    let deleted: HashSet<String> = message_ids.iter().map(MessageId::as_str).collect();
    let roots = [
        attachment_dir.to_path_buf(),
        attachment_dir.join(crate::handler::HTML_ASSETS_DIR),
    ];
    let result = tokio::task::spawn_blocking(move || {
        for root in roots {
            let entries = match std::fs::read_dir(&root) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    tracing::warn!(path = %root.display(), %error, "could not list attachment files");
                    continue;
                }
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                if !name.to_str().is_some_and(|name| deleted.contains(name)) {
                    continue;
                }
                let path = entry.path();
                if let Err(error) = std::fs::remove_dir_all(&path) {
                    tracing::warn!(
                        path = %path.display(),
                        %error,
                        "could not remove files of a deleted message"
                    );
                }
            }
        }
    })
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, "attachment cleanup task failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        remove_attachment_files(root.path(), &[deleted.clone(), MessageId::new()]).await;

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
