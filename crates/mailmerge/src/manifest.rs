//! Campaign manifest — the companion's own local state.
//!
//! This is what makes a rerun safe. Each record is keyed by a hash of its
//! properties; a record that already has a `draft_id` is skipped rather than
//! drafted again, and a record already marked `sent` is never sent twice.
//!
//! What the manifest deliberately does NOT contain: property values. A
//! `product_definition_url` may carry an opaque access token, so only the hash
//! and the recipient address are persisted.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordStatus {
    /// A local mxr draft exists for this record. Nothing has been sent.
    Drafted,
    /// `mxr send` reported success.
    Sent,
    /// Draft creation or send failed; retryable.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordEntry {
    /// Hash of the record's properties — the idempotency key.
    pub record_hash: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
    pub status: RecordStatus,
    /// Failure reason, kept short and never containing property values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub campaign_id: String,
    pub account: String,
    pub created_at: String,
    pub records: Vec<RecordEntry>,
}

impl Manifest {
    pub fn new(campaign_id: String, account: String) -> Self {
        Self {
            campaign_id,
            account,
            created_at: chrono::Utc::now().to_rfc3339(),
            records: Vec::new(),
        }
    }

    pub fn entry(&self, record_hash: &str) -> Option<&RecordEntry> {
        self.records
            .iter()
            .find(|entry| entry.record_hash == record_hash)
    }

    /// Insert or update one record's state.
    pub fn upsert(&mut self, entry: RecordEntry) {
        match self
            .records
            .iter_mut()
            .find(|existing| existing.record_hash == entry.record_hash)
        {
            Some(existing) => *existing = entry,
            None => self.records.push(entry),
        }
    }

    pub fn count(&self, status: RecordStatus) -> usize {
        self.records
            .iter()
            .filter(|entry| entry.status == status)
            .count()
    }

    /// Records with a draft that has not been sent.
    pub fn pending_send(&self) -> Vec<&RecordEntry> {
        self.records
            .iter()
            .filter(|entry| entry.status == RecordStatus::Drafted && entry.draft_id.is_some())
            .collect()
    }

    pub fn failed(&self) -> Vec<&RecordEntry> {
        self.records
            .iter()
            .filter(|entry| entry.status == RecordStatus::Failed)
            .collect()
    }

    pub fn path_in(state_dir: &Path, campaign_id: &str) -> PathBuf {
        state_dir.join(format!("{campaign_id}.json"))
    }

    pub fn load(state_dir: &Path, campaign_id: &str) -> anyhow::Result<Self> {
        let path = Self::path_in(state_dir, campaign_id);
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("no campaign manifest at {}", path.display()))?;
        serde_json::from_str(&raw)
            .with_context(|| format!("parsing campaign manifest {}", path.display()))
    }

    /// Write the manifest atomically.
    ///
    /// Written through a temp file and renamed so an interrupted run cannot
    /// leave a truncated manifest — which would lose the record of drafts that
    /// already exist and make a rerun duplicate them.
    pub fn save(&self, state_dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(state_dir)
            .with_context(|| format!("creating state dir {}", state_dir.display()))?;
        let path = Self::path_in(state_dir, &self.campaign_id);
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", temp.display()))?;
        std::fs::rename(&temp, &path)
            .with_context(|| format!("finalising {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(hash: &str, status: RecordStatus, draft: Option<&str>) -> RecordEntry {
        RecordEntry {
            record_hash: hash.to_string(),
            to: format!("{hash}@example.com"),
            draft_id: draft.map(str::to_string),
            status,
            error: None,
        }
    }

    #[test]
    fn upsert_replaces_rather_than_appends() {
        let mut manifest = Manifest::new("c1".into(), "notto".into());
        manifest.upsert(entry("h1", RecordStatus::Drafted, Some("d1")));
        manifest.upsert(entry("h1", RecordStatus::Sent, Some("d1")));
        assert_eq!(manifest.records.len(), 1);
        assert_eq!(manifest.records[0].status, RecordStatus::Sent);
    }

    #[test]
    fn pending_send_excludes_sent_and_undrafted() {
        let mut manifest = Manifest::new("c1".into(), "notto".into());
        manifest.upsert(entry("h1", RecordStatus::Drafted, Some("d1")));
        manifest.upsert(entry("h2", RecordStatus::Sent, Some("d2")));
        manifest.upsert(entry("h3", RecordStatus::Failed, None));
        let pending = manifest.pending_send();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].record_hash, "h1");
    }

    #[test]
    fn failed_records_are_retrievable_for_retry() {
        let mut manifest = Manifest::new("c1".into(), "notto".into());
        manifest.upsert(entry("h1", RecordStatus::Failed, None));
        manifest.upsert(entry("h2", RecordStatus::Sent, Some("d2")));
        assert_eq!(manifest.failed().len(), 1);
    }

    #[test]
    fn manifest_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = Manifest::new("c1".into(), "notto".into());
        manifest.upsert(entry("h1", RecordStatus::Drafted, Some("d1")));
        manifest.save(dir.path()).unwrap();

        let loaded = Manifest::load(dir.path(), "c1").unwrap();
        assert_eq!(loaded.campaign_id, "c1");
        assert_eq!(loaded.records.len(), 1);
        assert_eq!(loaded.records[0].draft_id.as_deref(), Some("d1"));
    }

    #[test]
    fn the_manifest_never_persists_property_values() {
        // Tokens live in the data file, not in campaign state.
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = Manifest::new("c1".into(), "notto".into());
        manifest.upsert(entry("h1", RecordStatus::Drafted, Some("d1")));
        manifest.save(dir.path()).unwrap();

        let raw = std::fs::read_to_string(Manifest::path_in(dir.path(), "c1")).unwrap();
        assert!(!raw.contains("product_definition_url"));
        assert!(!raw.contains("opaque-token"));
        // Only hash, address, draft id, status.
        assert!(raw.contains("record_hash"));
    }

    #[test]
    fn a_missing_manifest_reports_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let err = Manifest::load(dir.path(), "nope").unwrap_err();
        assert!(err.to_string().contains("no campaign manifest"), "{err}");
    }
}
