//! Private cursor schema for the IMAP adapter.
//!
//! mxr's MSP-aligned `SyncCursor` is opaque bytes (see
//! `crates/core/src/types.rs`). The IMAP adapter wraps its per-mailbox
//! `(uid_validity, uid_next, highest_modseq)` triples plus the negotiated
//! CONDSTORE/QRESYNC/MOVE/UIDPLUS capability set in this versioned JSON
//! envelope.
//!
//! Wire shape:
//!
//! ```text
//! {"v":"1","mailboxes":[{...}],"capabilities":{...}}
//! ```
//!
//! Pre-Phase-B installs persisted cursors as the old `SyncCursor::Imap`
//! tagged-enum shape. [`ImapCursor::decode`] accepts both for one release
//! so existing users don't see a forced full resync after upgrade. The
//! old shape also kept scalar `uid_validity` / `uid_next` fields outside
//! `mailboxes` (a backward-compat hack from before per-folder cursors);
//! the legacy decoder reconstructs a singleton INBOX entry when
//! `mailboxes` is empty, preserving the existing migration behaviour
//! that previously lived in the `sync_messages` dispatch.

use mxr_core::types::{ImapCapabilityState, ImapMailboxCursor};
use mxr_core::{MxrError, SyncCursor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "v")]
pub(crate) enum ImapCursor {
    #[serde(rename = "1")]
    V1(ImapCursorV1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ImapCursorV1 {
    pub(crate) mailboxes: Vec<ImapMailboxCursor>,
    #[serde(default)]
    pub(crate) capabilities: Option<ImapCapabilityState>,
}

impl ImapCursor {
    pub(crate) fn new(
        mailboxes: Vec<ImapMailboxCursor>,
        capabilities: Option<ImapCapabilityState>,
    ) -> Self {
        Self::V1(ImapCursorV1 {
            mailboxes,
            capabilities,
        })
    }

    pub(crate) fn encode(&self) -> SyncCursor {
        let bytes = serde_json::to_vec(self).expect("ImapCursor serialises infallibly");
        SyncCursor::from_bytes(bytes)
    }

    /// Decode an opaque cursor handed to us by the daemon. Returns:
    /// - `Ok(None)` for empty bytes or the legacy `"Initial"` string.
    /// - `Ok(Some(cursor))` for a v1 envelope or a recognised legacy
    ///   `{"Imap": {...}}` shape.
    /// - `Err(MxrError::SyncCursorExpired)` for any unrecognised payload
    ///   — the daemon clears state and runs a full sync via the
    ///   Phase A.2 recovery path.
    pub(crate) fn decode(raw: &SyncCursor) -> Result<Option<Self>, MxrError> {
        if raw.is_empty() {
            return Ok(None);
        }

        if let Ok(cursor) = serde_json::from_slice::<Self>(raw.as_bytes()) {
            return Ok(Some(cursor));
        }

        match decode_legacy(raw.as_bytes()) {
            Some(LegacyOutcome::Initial) => {
                tracing::info!("migrated legacy IMAP cursor (Initial) to empty bytes");
                Ok(None)
            }
            Some(LegacyOutcome::Cursor(legacy)) => {
                tracing::info!(
                    mailboxes = legacy.mailboxes.len(),
                    "migrated legacy IMAP cursor to v1 envelope"
                );
                Ok(Some(legacy.into_v1()))
            }
            None => Err(MxrError::SyncCursorExpired {
                reason: format!(
                    "IMAP cursor format unrecognised ({} bytes)",
                    raw.as_bytes().len()
                ),
            }),
        }
    }

    pub(crate) fn into_mailboxes(self) -> Vec<ImapMailboxCursor> {
        let Self::V1(v) = self;
        v.mailboxes
    }

    pub(crate) fn describe(&self) -> String {
        let Self::V1(v) = self;
        let fallback = v
            .mailboxes
            .iter()
            .find(|m| m.mailbox.eq_ignore_ascii_case("INBOX"))
            .or_else(|| v.mailboxes.first());
        let (uid_validity, uid_next) = fallback
            .map(|m| (m.uid_validity, m.uid_next))
            .unwrap_or((0, 0));
        format!(
            "imap uid_validity={uid_validity} uid_next={uid_next} mailboxes={}",
            v.mailboxes.len()
        )
    }
}

// --- legacy shim ------------------------------------------------------------

enum LegacyOutcome {
    Initial,
    Cursor(LegacyImap),
}

#[derive(Deserialize)]
enum LegacyImapWrapper {
    Imap(LegacyImap),
}

#[derive(Deserialize)]
struct LegacyImap {
    uid_validity: u32,
    uid_next: u32,
    #[serde(default)]
    mailboxes: Vec<ImapMailboxCursor>,
    #[serde(default)]
    capabilities: Option<ImapCapabilityState>,
}

impl LegacyImap {
    fn into_v1(mut self) -> ImapCursor {
        // The old shape kept scalar uid_validity/uid_next alongside
        // (possibly empty) mailboxes for backward compat. If the
        // mailboxes vec is empty, reconstruct a singleton INBOX entry
        // from the scalar fields — same shim that previously lived in
        // sync_messages dispatch.
        if self.mailboxes.is_empty() {
            self.mailboxes.push(ImapMailboxCursor {
                mailbox: "INBOX".to_string(),
                uid_validity: self.uid_validity,
                uid_next: self.uid_next,
                highest_modseq: None,
            });
        }
        ImapCursor::new(self.mailboxes, self.capabilities)
    }
}

fn decode_legacy(bytes: &[u8]) -> Option<LegacyOutcome> {
    if let Ok(s) = serde_json::from_slice::<String>(bytes) {
        if s == "Initial" {
            return Some(LegacyOutcome::Initial);
        }
        return None;
    }
    serde_json::from_slice::<LegacyImapWrapper>(bytes)
        .ok()
        .map(|LegacyImapWrapper::Imap(legacy)| LegacyOutcome::Cursor(legacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_mailbox() -> ImapMailboxCursor {
        ImapMailboxCursor {
            mailbox: "INBOX".to_string(),
            uid_validity: 1,
            uid_next: 100,
            highest_modseq: Some(42),
        }
    }

    #[test]
    fn v1_round_trip() {
        let cursor = ImapCursor::new(vec![sample_mailbox()], None);
        let encoded = cursor.encode();
        let decoded = ImapCursor::decode(&encoded).unwrap().unwrap();
        let mailboxes = decoded.into_mailboxes();
        assert_eq!(mailboxes.len(), 1);
        assert_eq!(mailboxes[0].uid_next, 100);
    }

    #[test]
    fn empty_bytes_decode_to_none() {
        let result = ImapCursor::decode(&SyncCursor::empty()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn legacy_imap_tagged_with_mailboxes_decodes() {
        let legacy = SyncCursor::from_bytes(
            br#"{"Imap":{"uid_validity":1,"uid_next":100,"mailboxes":[{"mailbox":"INBOX","uid_validity":1,"uid_next":100,"highest_modseq":null}]}}"#.to_vec(),
        );
        let decoded = ImapCursor::decode(&legacy).unwrap().unwrap();
        let mailboxes = decoded.into_mailboxes();
        assert_eq!(mailboxes.len(), 1);
        assert_eq!(mailboxes[0].uid_next, 100);
    }

    #[test]
    fn legacy_imap_tagged_without_mailboxes_synthesises_inbox() {
        // The very old shape had only scalar fields and no mailboxes vec.
        let legacy = SyncCursor::from_bytes(
            br#"{"Imap":{"uid_validity":7,"uid_next":999}}"#.to_vec(),
        );
        let decoded = ImapCursor::decode(&legacy).unwrap().unwrap();
        let mailboxes = decoded.into_mailboxes();
        assert_eq!(mailboxes.len(), 1);
        assert_eq!(mailboxes[0].mailbox, "INBOX");
        assert_eq!(mailboxes[0].uid_validity, 7);
        assert_eq!(mailboxes[0].uid_next, 999);
    }

    #[test]
    fn legacy_initial_string_decodes_as_none() {
        let legacy = SyncCursor::from_bytes(br#""Initial""#.to_vec());
        assert!(ImapCursor::decode(&legacy).unwrap().is_none());
    }

    #[test]
    fn unrecognised_payload_surfaces_expired() {
        let garbage = SyncCursor::from_bytes(b"not json at all".to_vec());
        let err = ImapCursor::decode(&garbage).unwrap_err();
        assert!(matches!(err, MxrError::SyncCursorExpired { .. }));
    }

    #[test]
    fn gmail_legacy_shape_surfaces_expired() {
        let legacy =
            SyncCursor::from_bytes(br#"{"Gmail":{"history_id":12345}}"#.to_vec());
        let err = ImapCursor::decode(&legacy).unwrap_err();
        assert!(matches!(err, MxrError::SyncCursorExpired { .. }));
    }

    #[test]
    fn describe_summarises_inbox() {
        let cursor = ImapCursor::new(vec![sample_mailbox()], None);
        let desc = cursor.describe();
        assert!(desc.contains("imap"));
        assert!(desc.contains("uid_validity=1"));
        assert!(desc.contains("uid_next=100"));
        assert!(desc.contains("mailboxes=1"));
    }
}
