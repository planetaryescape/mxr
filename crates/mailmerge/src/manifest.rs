//! Campaign manifest — the companion's own local state.
//!
//! This is what makes a rerun safe. Each record is keyed by a hash of its
//! properties; a record that already has a `draft_id` is skipped rather than
//! drafted again, and a record already marked `sent` or `scheduled` is never
//! dispatched twice.
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
    /// `mxr send --at` accepted this draft for later delivery.
    Scheduled,
    /// `mxr send` reported success.
    Sent,
    /// Draft creation, send, or scheduling failed; retryable.
    Failed,
}

/// Why a record failed, in the manifest's own words.
///
/// Deliberately a closed set rather than `mxr`'s error text. Anything a
/// subprocess prints can quote the rendered body back at us, and a property may
/// be an opaque access token; the manifest lives in the working tree, where a
/// leaked token gets committed, backed up, and shared. The full reason goes to
/// the operator's terminal instead, where it is theirs and it is ephemeral.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureReason {
    /// `mxr compose` did not create a draft.
    #[serde(rename = "draft_refused")]
    Draft,
    /// `mxr send` did not send an existing draft.
    #[serde(rename = "send_refused")]
    Send,
    /// `mxr send --at` did not schedule an existing draft.
    #[serde(rename = "schedule_refused")]
    Schedule,
}

impl FailureReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "mxr refused to create the draft",
            Self::Send => "mxr refused to send the draft",
            Self::Schedule => "mxr refused to schedule the draft",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordEntry {
    /// Hash of the record's properties — the idempotency key.
    pub record_hash: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
    pub status: RecordStatus,
    /// Why this record failed. A fixed reason, never text from a subprocess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<FailureReason>,
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

    /// Load an existing manifest, or `None` when this campaign has none yet.
    ///
    /// A manifest that exists but cannot be read is an error, never a `None`.
    /// Treating an unreadable manifest as "no campaign yet" would re-draft
    /// every record and then overwrite the only record of the drafts that
    /// already exist.
    pub fn load_if_present(state_dir: &Path, campaign_id: &str) -> anyhow::Result<Option<Self>> {
        let path = Self::path_in(state_dir, campaign_id);
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw)
                .map(Some)
                .with_context(|| format!("parsing campaign manifest {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => {
                Err(error).with_context(|| format!("reading campaign manifest {}", path.display()))
            }
        }
    }

    pub fn load(state_dir: &Path, campaign_id: &str) -> anyhow::Result<Self> {
        Self::load_if_present(state_dir, campaign_id)?.with_context(|| {
            format!(
                "no campaign manifest at {}",
                Self::path_in(state_dir, campaign_id).display()
            )
        })
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
        std::fs::rename(&temp, &path).with_context(|| format!("finalising {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests assert directly on fixtures")]

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
        // A rerun upserts every record it touches; appending would give one
        // recipient two entries and let the second one be sent again.
        let mut manifest = Manifest::new("c1".into(), "notto".into());
        manifest.upsert(entry("h1", RecordStatus::Drafted, Some("d1")));
        manifest.upsert(entry("h2", RecordStatus::Drafted, Some("d2")));
        manifest.upsert(entry("h1", RecordStatus::Sent, Some("d1")));

        assert_eq!(manifest.records.len(), 2);
        assert_eq!(manifest.entry("h1").unwrap().status, RecordStatus::Sent);
        assert_eq!(manifest.entry("h2").unwrap().status, RecordStatus::Drafted);
        assert_eq!(manifest.count(RecordStatus::Sent), 1);
        assert_eq!(manifest.count(RecordStatus::Drafted), 1);
        assert!(manifest.entry("nope").is_none());
    }

    #[test]
    fn pending_send_excludes_scheduled_sent_and_undrafted() {
        let mut manifest = Manifest::new("c1".into(), "notto".into());
        manifest.upsert(entry("h1", RecordStatus::Drafted, Some("d1")));
        manifest.upsert(entry("h2", RecordStatus::Scheduled, Some("d2")));
        manifest.upsert(entry("h3", RecordStatus::Sent, Some("d3")));
        manifest.upsert(entry("h4", RecordStatus::Failed, None));
        // Drafted but with no draft id: there is nothing for mxr to send.
        manifest.upsert(entry("h5", RecordStatus::Drafted, None));

        let pending: Vec<&str> = manifest
            .pending_send()
            .iter()
            .map(|entry| entry.record_hash.as_str())
            .collect();
        assert_eq!(pending, ["h1"]);
    }

    #[test]
    fn scheduled_status_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = Manifest::new("c1".into(), "notto@example.com".into());
        manifest.upsert(entry("h1", RecordStatus::Scheduled, Some("d1")));
        manifest.save(dir.path()).unwrap();

        let loaded = Manifest::load(dir.path(), "c1").unwrap();
        assert_eq!(loaded.entry("h1").unwrap().status, RecordStatus::Scheduled);
    }

    #[test]
    fn failed_records_are_retrievable_for_retry() {
        let mut manifest = Manifest::new("c1".into(), "notto".into());
        manifest.upsert(entry("h1", RecordStatus::Failed, None));
        manifest.upsert(entry("h2", RecordStatus::Sent, Some("d2")));
        manifest.upsert(entry("h3", RecordStatus::Drafted, Some("d3")));
        manifest.upsert(entry("h4", RecordStatus::Failed, Some("d4")));

        let failed: Vec<&str> = manifest
            .failed()
            .iter()
            .map(|entry| entry.record_hash.as_str())
            .collect();
        assert_eq!(failed, ["h1", "h4"]);
    }

    #[test]
    fn manifest_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = Manifest::new("c1".into(), "notto@example.com".into());
        manifest.upsert(entry("h1", RecordStatus::Drafted, Some("d1")));
        manifest.upsert(RecordEntry {
            error: Some(FailureReason::Draft),
            ..entry("h2", RecordStatus::Failed, None)
        });
        manifest.save(dir.path()).unwrap();

        let loaded = Manifest::load(dir.path(), "c1").unwrap();
        assert_eq!(loaded.campaign_id, "c1");
        assert_eq!(loaded.account, "notto@example.com");
        assert_eq!(loaded.created_at, manifest.created_at);
        assert_eq!(loaded.records.len(), 2);
        assert_eq!(loaded.entry("h1").unwrap().draft_id.as_deref(), Some("d1"));
        assert_eq!(loaded.entry("h1").unwrap().status, RecordStatus::Drafted);
        assert_eq!(loaded.entry("h2").unwrap().draft_id, None);
        assert_eq!(
            loaded.entry("h2").unwrap().error,
            Some(FailureReason::Draft)
        );
    }

    #[test]
    fn saving_twice_replaces_the_manifest_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = Manifest::new("c1".into(), "notto".into());
        manifest.save(dir.path()).unwrap();
        manifest.upsert(entry("h1", RecordStatus::Sent, Some("d1")));
        manifest.save(dir.path()).unwrap();

        assert_eq!(Manifest::load(dir.path(), "c1").unwrap().records.len(), 1);
        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, ["c1.json"], "temp file left behind");
    }

    #[test]
    fn the_manifest_persists_only_the_hash_address_draft_and_status() {
        // A property may be an opaque access token, so campaign state carries a
        // hash instead. Asserted structurally: adding a field that could hold a
        // property value has to fail here.
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = Manifest::new("c1".into(), "notto".into());
        manifest.upsert(entry("h1", RecordStatus::Drafted, Some("d1")));
        manifest.upsert(RecordEntry {
            error: Some(FailureReason::Draft),
            ..entry("h2", RecordStatus::Failed, None)
        });
        manifest.save(dir.path()).unwrap();

        let raw = std::fs::read_to_string(Manifest::path_in(dir.path(), "c1")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();

        let mut top: Vec<&str> = parsed
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        top.sort_unstable();
        assert_eq!(top, ["account", "campaign_id", "created_at", "records"]);

        for record in parsed["records"].as_array().unwrap() {
            let mut fields: Vec<&str> = record
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            fields.sort_unstable();
            assert!(
                fields
                    .iter()
                    .all(|field| ["draft_id", "error", "record_hash", "status", "to"]
                        .contains(field)),
                "a field that could hold a property value: {fields:?}"
            );
        }

        // The failure reason is one of a closed set, so no amount of subprocess
        // output can steer a token into it.
        assert_eq!(parsed["records"][1]["error"], "draft_refused");
    }

    #[test]
    fn a_missing_manifest_reports_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let err = Manifest::load(dir.path(), "nope").unwrap_err();
        assert!(err.to_string().contains("no campaign manifest"), "{err}");
        assert!(err.to_string().contains("nope.json"), "{err}");
        assert!(Manifest::load_if_present(dir.path(), "nope")
            .unwrap()
            .is_none());
    }

    #[test]
    fn an_unreadable_manifest_is_an_error_not_a_fresh_campaign() {
        // The distinction that stops a rerun re-drafting the whole campaign.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(Manifest::path_in(dir.path(), "c1"), "{ truncated").unwrap();

        let err = Manifest::load_if_present(dir.path(), "c1").unwrap_err();
        assert!(
            err.to_string().contains("parsing campaign manifest"),
            "{err}"
        );
        assert!(Manifest::load(dir.path(), "c1").is_err());
    }
}
