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
    /// Written only while a generic (non-Gmail) newest-first initial backfill
    /// is in progress. An older binary only knows `"1"`, fails to decode this,
    /// and takes the `SyncCursorExpired` full-resync path. Decoding it as a
    /// v1 cursor instead would delta-sync upward from the newest page and
    /// never fetch the older mail.
    #[serde(rename = "2")]
    V2(ImapCursorV2),
}

/// Per-folder newest-first backfill state, in the order folders are fetched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ImapCursorV2 {
    #[serde(default)]
    pub(crate) capabilities: Option<ImapCapabilityState>,
    /// Folders fully fetched; each is already a normal delta cursor.
    pub(crate) completed: Vec<ImapMailboxCursor>,
    /// Folders still to fetch. The first entry is the one in progress.
    pub(crate) pending: Vec<ImapFolderBackfill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ImapFolderBackfill {
    pub(crate) mailbox: String,
    /// SELECT-time state captured when this folder's backfill began. Its
    /// `uid_next` is the frozen horizon: mail above it is left to the first
    /// delta sync. `None` until the folder's first page.
    pub(crate) started: Option<ImapMailboxCursor>,
    /// Highest UID not yet fetched; the walk goes downward. 0 means done.
    pub(crate) next_high: u32,
    /// Lowest UID that failed to parse so far. At completion the folder's
    /// `uid_next` is floored to it so the first delta re-fetches it.
    #[serde(default)]
    pub(crate) min_failed_uid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ImapCursorV1 {
    pub(crate) mailboxes: Vec<ImapMailboxCursor>,
    #[serde(default)]
    pub(crate) capabilities: Option<ImapCapabilityState>,
    #[serde(default)]
    pub(crate) backfill: Option<ImapBackfillCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ImapBackfillCursor {
    pub(crate) mailbox: ImapMailboxCursor,
    pub(crate) next_uid: u32,
    pub(crate) end_uid: u32,
    #[serde(default)]
    pub(crate) gmail_all_mail: bool,
}

impl ImapCursor {
    pub(crate) fn new(
        mailboxes: Vec<ImapMailboxCursor>,
        capabilities: Option<ImapCapabilityState>,
    ) -> Self {
        Self::V1(ImapCursorV1 {
            mailboxes,
            capabilities,
            backfill: None,
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

    pub(crate) fn new_backfill(
        mailbox: ImapMailboxCursor,
        next_uid: u32,
        end_uid: u32,
        gmail_all_mail: bool,
        capabilities: Option<ImapCapabilityState>,
    ) -> Self {
        Self::V1(ImapCursorV1 {
            mailboxes: vec![mailbox.clone()],
            capabilities,
            backfill: Some(ImapBackfillCursor {
                mailbox,
                next_uid,
                end_uid,
                gmail_all_mail,
            }),
        })
    }

    pub(crate) fn new_generic_backfill(
        completed: Vec<ImapMailboxCursor>,
        pending: Vec<ImapFolderBackfill>,
        capabilities: Option<ImapCapabilityState>,
    ) -> Self {
        Self::V2(ImapCursorV2 {
            capabilities,
            completed,
            pending,
        })
    }

    pub(crate) fn into_mailboxes(self) -> Vec<ImapMailboxCursor> {
        match self {
            Self::V1(v) => v.mailboxes,
            Self::V2(v) => v.completed,
        }
    }

    pub(crate) fn backfill(&self) -> Option<&ImapBackfillCursor> {
        match self {
            Self::V1(v) => v.backfill.as_ref(),
            Self::V2(_) => None,
        }
    }

    pub(crate) fn into_generic_backfill(self) -> Option<ImapCursorV2> {
        match self {
            Self::V1(_) => None,
            Self::V2(v) => Some(v),
        }
    }

    pub(crate) fn describe(&self) -> String {
        let v = match self {
            Self::V1(v) => v,
            Self::V2(v) => {
                return format!(
                    "imap generic_backfill completed={} pending={}",
                    v.completed.len(),
                    v.pending.len()
                );
            }
        };
        let fallback = v
            .mailboxes
            .iter()
            .find(|m| m.mailbox.eq_ignore_ascii_case("INBOX"))
            .or_else(|| v.mailboxes.first());
        let (uid_validity, uid_next) = fallback.map_or((0, 0), |m| (m.uid_validity, m.uid_next));
        let suffix = v.backfill.as_ref().map_or_else(String::new, |backfill| {
            format!(
                " backfill_mailbox={} next_uid={} end_uid={}",
                backfill.mailbox.mailbox, backfill.next_uid, backfill.end_uid
            )
        });
        format!(
            "imap uid_validity={uid_validity} uid_next={uid_next} mailboxes={}{}",
            v.mailboxes.len(),
            suffix
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
        let legacy =
            SyncCursor::from_bytes(br#"{"Imap":{"uid_validity":7,"uid_next":999}}"#.to_vec());
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
        let legacy = SyncCursor::from_bytes(br#"{"Gmail":{"history_id":12345}}"#.to_vec());
        let err = ImapCursor::decode(&legacy).unwrap_err();
        assert!(matches!(err, MxrError::SyncCursorExpired { .. }));
    }

    /// The enum as the previous release shipped it: only `"1"`.
    #[derive(Deserialize)]
    #[serde(tag = "v")]
    enum PreviousReleaseCursor {
        #[serde(rename = "1")]
        V1(ImapCursorV1),
    }

    fn previous_release_decode(bytes: &[u8]) -> Option<usize> {
        match serde_json::from_slice::<PreviousReleaseCursor>(bytes) {
            Ok(PreviousReleaseCursor::V1(v)) => Some(v.mailboxes.len()),
            Err(_) => None,
        }
    }

    fn in_progress_backfill() -> ImapCursor {
        ImapCursor::new_generic_backfill(
            vec![sample_mailbox()],
            vec![ImapFolderBackfill {
                mailbox: "Archive".into(),
                started: Some(sample_mailbox()),
                next_high: 50,
                min_failed_uid: Some(7),
            }],
            None,
        )
    }

    #[test]
    fn in_progress_backfill_is_undecodable_by_the_previous_release() {
        let encoded = in_progress_backfill().encode();
        assert_eq!(previous_release_decode(encoded.as_bytes()), None);
        // The previous release then tries its legacy shim, which also rejects
        // it, so it reports SyncCursorExpired and runs a full resync.
        assert!(decode_legacy(encoded.as_bytes()).is_none());
    }

    #[test]
    fn in_progress_backfill_round_trips() {
        let decoded = ImapCursor::decode(&in_progress_backfill().encode())
            .unwrap()
            .unwrap()
            .into_generic_backfill()
            .unwrap();
        assert_eq!(decoded.completed.len(), 1);
        assert_eq!(decoded.pending[0].next_high, 50);
        assert_eq!(decoded.pending[0].min_failed_uid, Some(7));
    }

    #[test]
    fn v1_fixtures_still_decode_as_before() {
        let completed = SyncCursor::from_bytes(
            br#"{"v":"1","mailboxes":[{"mailbox":"INBOX","uid_validity":1,"uid_next":100,"highest_modseq":null}]}"#.to_vec(),
        );
        assert_eq!(previous_release_decode(completed.as_bytes()), Some(1));
        let decoded = ImapCursor::decode(&completed).unwrap().unwrap();
        assert!(decoded.backfill().is_none());
        assert!(decoded.clone().into_generic_backfill().is_none());
        assert_eq!(decoded.into_mailboxes()[0].uid_next, 100);

        let gmail_backfill = SyncCursor::from_bytes(
            br#"{"v":"1","mailboxes":[{"mailbox":"All Mail","uid_validity":1,"uid_next":900,"highest_modseq":null}],"backfill":{"mailbox":{"mailbox":"All Mail","uid_validity":1,"uid_next":900,"highest_modseq":null},"next_uid":501,"end_uid":899,"gmail_all_mail":true}}"#.to_vec(),
        );
        let decoded = ImapCursor::decode(&gmail_backfill).unwrap().unwrap();
        let backfill = decoded.backfill().expect("gmail backfill kept");
        assert!(backfill.gmail_all_mail);
        assert_eq!(backfill.next_uid, 501);
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
