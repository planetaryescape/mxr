//! Private cursor schema for the Gmail adapter.
//!
//! mxr's MSP-aligned `SyncCursor` is opaque bytes (see
//! `crates/core/src/types.rs`). Each adapter owns its own internal
//! representation and (de)serialises through this opaque envelope.
//!
//! The wire shape is a versioned JSON envelope:
//!
//! ```text
//! {"v":"1","history_id":12345}                              // delta-sync ready
//! {"v":"1","history_id":12345,"page_token":"CICqz4f1m..."}  // mid-backfill
//! ```
//!
//! Pre-Phase-B installs persisted cursors as the old tagged enum
//! (`{"Gmail":{"history_id":N}}` etc.). [`GmailCursor::decode`] accepts
//! both shapes for one release so existing users don't see a forced
//! full resync after upgrade; the legacy path can be removed once this
//! release has rolled out.

use mxr_core::{MxrError, SyncCursor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "v")]
pub(crate) enum GmailCursor {
    #[serde(rename = "1")]
    V1(GmailCursorV1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GmailCursorV1 {
    pub(crate) history_id: u64,
    /// Present iff a multi-page initial backfill is mid-flight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) page_token: Option<String>,
}

impl GmailCursor {
    pub(crate) fn delta(history_id: u64) -> Self {
        Self::V1(GmailCursorV1 {
            history_id,
            page_token: None,
        })
    }

    pub(crate) fn backfill(history_id: u64, page_token: String) -> Self {
        Self::V1(GmailCursorV1 {
            history_id,
            page_token: Some(page_token),
        })
    }

    /// Encode this cursor for persistence. Infallible — `serde_json`
    /// cannot fail on owned plain-data types.
    pub(crate) fn encode(&self) -> SyncCursor {
        let bytes = serde_json::to_vec(self).expect("GmailCursor serialises infallibly");
        SyncCursor::from_bytes(bytes)
    }

    /// Decode an opaque cursor handed to us by the daemon. Returns:
    /// - `Ok(None)` for an empty cursor (run initial sync) or a legacy
    ///   `"Initial"` sentinel.
    /// - `Ok(Some(cursor))` for a v1 envelope or a recognised legacy
    ///   tagged-enum shape.
    /// - `Err(MxrError::SyncCursorExpired)` for any unrecognised payload
    ///   — the daemon clears state and falls back to a full sync via
    ///   the Phase A.2 recovery path.
    pub(crate) fn decode(raw: &SyncCursor) -> Result<Option<Self>, MxrError> {
        if raw.is_empty() {
            return Ok(None);
        }

        if let Ok(cursor) = serde_json::from_slice::<Self>(raw.as_bytes()) {
            return Ok(Some(cursor));
        }

        match decode_legacy(raw.as_bytes()) {
            Some(LegacyOutcome::Initial) => {
                tracing::info!("migrated legacy Gmail cursor (Initial) to empty bytes");
                Ok(None)
            }
            Some(LegacyOutcome::Cursor(cursor)) => {
                tracing::info!(
                    history_id = cursor.history_id_for_log(),
                    "migrated legacy Gmail cursor to v1 envelope"
                );
                Ok(Some(cursor.into_v1()))
            }
            None => Err(MxrError::SyncCursorExpired {
                reason: format!(
                    "Gmail cursor format unrecognised ({} bytes)",
                    raw.as_bytes().len()
                ),
            }),
        }
    }

    pub(crate) fn is_backfill(&self) -> bool {
        let Self::V1(v) = self;
        v.page_token.is_some()
    }

    pub(crate) fn describe(&self) -> String {
        let Self::V1(v) = self;
        match &v.page_token {
            Some(token) => format!(
                "gmail_backfill history_id={} page_token={}",
                v.history_id,
                truncate_token(token)
            ),
            None => format!("gmail history_id={}", v.history_id),
        }
    }
}

fn truncate_token(token: &str) -> String {
    let head: String = token.chars().take(24).collect();
    if token.chars().count() > 24 {
        format!("{head}...")
    } else {
        head
    }
}

// --- legacy shim ------------------------------------------------------------

enum LegacyOutcome {
    Initial,
    Cursor(LegacyGmailVariant),
}

#[derive(Deserialize)]
enum LegacyGmailVariant {
    Gmail { history_id: u64 },
    GmailBackfill { history_id: u64, page_token: String },
}

impl LegacyGmailVariant {
    fn history_id_for_log(&self) -> u64 {
        match self {
            Self::Gmail { history_id } | Self::GmailBackfill { history_id, .. } => *history_id,
        }
    }

    fn into_v1(self) -> GmailCursor {
        match self {
            Self::Gmail { history_id } => GmailCursor::delta(history_id),
            Self::GmailBackfill {
                history_id,
                page_token,
            } => GmailCursor::backfill(history_id, page_token),
        }
    }
}

fn decode_legacy(bytes: &[u8]) -> Option<LegacyOutcome> {
    // Old `SyncCursor::Initial` serialised as the bare JSON string "Initial".
    if let Ok(s) = serde_json::from_slice::<String>(bytes) {
        if s == "Initial" {
            return Some(LegacyOutcome::Initial);
        }
        return None;
    }
    // Old tagged enum: {"Gmail":{...}} or {"GmailBackfill":{...}}. Imap
    // variants are intentionally unrecognised here — they'd indicate a
    // store corruption (Gmail provider receiving an IMAP-shape cursor)
    // and the SyncCursorExpired path is the right response.
    serde_json::from_slice::<LegacyGmailVariant>(bytes)
        .ok()
        .map(LegacyOutcome::Cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_delta_round_trip() {
        let cursor = GmailCursor::delta(12345);
        let encoded = cursor.encode();
        let decoded = GmailCursor::decode(&encoded).unwrap().unwrap();
        let GmailCursor::V1(v) = decoded;
        assert_eq!(v.history_id, 12345);
        assert!(v.page_token.is_none());
    }

    #[test]
    fn v1_backfill_round_trip() {
        let cursor = GmailCursor::backfill(99, "page-token-abc".into());
        assert!(cursor.is_backfill());
        let encoded = cursor.encode();
        let decoded = GmailCursor::decode(&encoded).unwrap().unwrap();
        assert!(decoded.is_backfill());
        let GmailCursor::V1(v) = decoded;
        assert_eq!(v.history_id, 99);
        assert_eq!(v.page_token.as_deref(), Some("page-token-abc"));
    }

    #[test]
    fn empty_bytes_decode_to_none() {
        let result = GmailCursor::decode(&SyncCursor::empty()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn legacy_gmail_tagged_decodes() {
        let legacy = SyncCursor::from_bytes(br#"{"Gmail":{"history_id":54321}}"#.to_vec());
        let decoded = GmailCursor::decode(&legacy).unwrap().unwrap();
        let GmailCursor::V1(v) = decoded;
        assert_eq!(v.history_id, 54321);
        assert!(v.page_token.is_none());
    }

    #[test]
    fn legacy_gmail_backfill_tagged_decodes() {
        let legacy = SyncCursor::from_bytes(
            br#"{"GmailBackfill":{"history_id":7,"page_token":"abc"}}"#.to_vec(),
        );
        let decoded = GmailCursor::decode(&legacy).unwrap().unwrap();
        assert!(decoded.is_backfill());
        let GmailCursor::V1(v) = decoded;
        assert_eq!(v.history_id, 7);
        assert_eq!(v.page_token.as_deref(), Some("abc"));
    }

    #[test]
    fn legacy_initial_string_decodes_as_none() {
        let legacy = SyncCursor::from_bytes(br#""Initial""#.to_vec());
        let result = GmailCursor::decode(&legacy).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn unrecognised_payload_surfaces_expired() {
        let garbage = SyncCursor::from_bytes(b"definitely not json".to_vec());
        let err = GmailCursor::decode(&garbage).unwrap_err();
        assert!(matches!(err, MxrError::SyncCursorExpired { .. }));
    }

    #[test]
    fn imap_legacy_shape_surfaces_expired() {
        // If somehow an IMAP-tagged cursor reaches the Gmail adapter,
        // the daemon's SyncCursorExpired recovery is the right answer.
        let legacy = SyncCursor::from_bytes(
            br#"{"Imap":{"uid_validity":1,"uid_next":2,"mailboxes":[]}}"#.to_vec(),
        );
        let err = GmailCursor::decode(&legacy).unwrap_err();
        assert!(matches!(err, MxrError::SyncCursorExpired { .. }));
    }

    #[test]
    fn describe_truncates_long_page_tokens() {
        let cursor = GmailCursor::backfill(1, "a".repeat(100));
        let desc = cursor.describe();
        assert!(desc.contains("gmail_backfill"));
        assert!(desc.contains("history_id=1"));
        assert!(desc.contains("..."));
    }
}
