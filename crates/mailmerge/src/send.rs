//! Reviewed CLI submission and recovery for one campaign record.
use crate::{
    manifest::{FailureReason, Manifest, RecordStatus, SendReview},
    mxr::{Mxr, OutcomeState},
};
use std::path::Path;

pub fn dispatch_record(
    client: &Mxr,
    manifest: &mut Manifest,
    state_dir: &Path,
    hash: &str,
    at: Option<&str>,
    retry_failed: bool,
) -> anyhow::Result<RecordStatus> {
    let Some(mut entry) = manifest.entry(hash).cloned() else {
        anyhow::bail!("Campaign record disappeared")
    };
    let draft = entry
        .draft_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Record has no draft"))?;
    let owner = match &entry.review {
        Some(review) => review.expected_workspace.as_ref(),
        None => entry.expected_workspace.as_ref(),
    };
    let Some(owner) = owner.cloned() else {
        anyhow::bail!("This retained campaign has no original workspace owner. Preserve its manifest and explicitly reopen/review the original drafts before continuing.");
    };
    let prior = client.outcome(&manifest.account, &format!("draft:{draft}"), &owner);
    let rejected = match prior {
        Ok(Some(OutcomeState::Accepted)) => {
            entry.status = RecordStatus::Sent;
            entry.error = None;
            manifest.upsert(entry);
            manifest.save(state_dir)?;
            return Ok(RecordStatus::Sent);
        }
        Ok(Some(OutcomeState::Unknown)) | Err(_) => {
            entry.status = if entry
                .review
                .as_ref()
                .is_some_and(|r| r.scheduled_at.is_some())
            {
                RecordStatus::ScheduleUncertain
            } else {
                RecordStatus::Uncertain
            };
            entry.error = Some(FailureReason::Outcome);
            let status = entry.status;
            manifest.upsert(entry);
            manifest.save(state_dir)?;
            return Ok(status);
        }
        Ok(Some(OutcomeState::NotAccepted)) if !retry_failed => {
            entry.status = RecordStatus::Failed;
            entry.error = Some(FailureReason::Send);
            manifest.upsert(entry);
            manifest.save(state_dir)?;
            return Ok(RecordStatus::Failed);
        }
        Ok(Some(OutcomeState::NotAccepted)) => true,
        Ok(None) => false,
    };
    if let Some(review) = &entry.review {
        if review.scheduled_at.is_some() && !rejected {
            entry.status = RecordStatus::ScheduleUncertain;
            entry.error = Some(FailureReason::ScheduleOutcome);
            manifest.upsert(entry);
            manifest.save(state_dir)?;
            return Ok(RecordStatus::ScheduleUncertain);
        }
        if !rejected {
            if at.is_some() && review.scheduled_at.as_deref() != at {
                anyhow::bail!("Requested delivery mode or time differs from the retained review; query its outcome before changing the command")
            }
            match client.outcome(
                &review.account_id,
                &review.operation_id,
                review
                    .expected_workspace
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Retained review lacks original workspace"))?,
            ) {
                Ok(Some(OutcomeState::Accepted)) => {
                    entry.status = RecordStatus::Sent;
                    entry.error = None;
                    manifest.upsert(entry);
                    manifest.save(state_dir)?;
                    return Ok(RecordStatus::Sent);
                }
                Ok(Some(OutcomeState::Unknown)) | Err(_) => {
                    entry.status = RecordStatus::Uncertain;
                    entry.error = Some(FailureReason::Outcome);
                    manifest.upsert(entry);
                    manifest.save(state_dir)?;
                    return Ok(RecordStatus::Uncertain);
                }
                Ok(Some(OutcomeState::NotAccepted)) => {
                    anyhow::bail!("Send outcome changed during review; query the retained operation before continuing")
                }
                Ok(None) => {}
            }
        }
    }
    if entry.review.is_none() || rejected {
        let mut review = match client.preview(&manifest.account, &draft, &owner) {
            Ok(review) => review,
            Err(_) => {
                entry.status = RecordStatus::Failed;
                entry.error = Some(FailureReason::Preview);
                manifest.upsert(entry);
                manifest.save(state_dir)?;
                return Ok(RecordStatus::Failed);
            }
        };
        review.scheduled_at = at
            .map(str::to_string)
            .or_else(|| entry.review.as_ref().and_then(|r| r.scheduled_at.clone()));
        if rejected {
            review.attempt = entry
                .review
                .as_ref()
                .map_or(1, |r| r.attempt.saturating_add(1));
            review
                .operation_id
                .push_str(&format!(":mailmerge-retry:{}", review.attempt));
        }
        entry.review = Some(review);
    }
    let review: SendReview = entry
        .review
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Missing send review"))?;
    entry.status = if review.scheduled_at.is_some() {
        RecordStatus::ScheduleUncertain
    } else {
        RecordStatus::Uncertain
    };
    entry.error = Some(FailureReason::Outcome);
    manifest.upsert(entry.clone());
    manifest.save(state_dir)?;
    let submitted = client.send_draft(&draft, &review);
    if review.scheduled_at.is_some() {
        entry.status = if submitted.is_ok() {
            RecordStatus::Scheduled
        } else {
            RecordStatus::ScheduleUncertain
        };
        entry.error = if submitted.is_ok() {
            None
        } else {
            Some(FailureReason::ScheduleOutcome)
        };
    } else {
        match client.outcome(
            &review.account_id,
            &review.operation_id,
            review
                .expected_workspace
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Retained review lacks original workspace"))?,
        ) {
            Ok(Some(OutcomeState::Accepted)) => {
                entry.status = RecordStatus::Sent;
                entry.error = None;
            }
            Ok(Some(OutcomeState::NotAccepted)) => {
                entry.status = RecordStatus::Failed;
                entry.error = Some(FailureReason::Send);
            }
            Ok(Some(OutcomeState::Unknown) | None) | Err(_) => {
                entry.status = RecordStatus::Uncertain;
                entry.error = Some(FailureReason::Outcome);
            }
        }
    }
    let status = entry.status;
    manifest.upsert(entry);
    manifest.save(state_dir)?;
    Ok(status)
}

#[cfg(all(test, unix))]
#[expect(clippy::unwrap_used, reason = "test fixtures fail loudly")]
mod tests {
    use super::*;
    use crate::{manifest::RecordEntry, stub::Stub};
    fn fixture(review: bool) -> (tempfile::TempDir, Stub, Manifest) {
        let directory = tempfile::tempdir().unwrap();
        let stub = Stub::new();
        let mut manifest = Manifest::new("campaign".into(), "legacy-selector".into());
        manifest.upsert(RecordEntry {
            expected_workspace: Some(crate::manifest::WorkspaceOwner {
                workspace_id: "workspace-a".into(),
                recovery_generation: "generation-a".into(),
            }),
            record_hash: "hash".into(),
            to: "recipient@example.com".into(),
            draft_id: Some("draft-1".into()),
            status: RecordStatus::Drafted,
            error: None,
            review: review.then_some(SendReview {
                expected_workspace: Some(crate::manifest::WorkspaceOwner {
                    workspace_id: "workspace-a".into(),
                    recovery_generation: "generation-a".into(),
                }),
                account_id: "notto".into(),
                expected_revision: 1,
                operation_id: "draft-1:1".into(),
                attempt: 0,
                scheduled_at: None,
            }),
        });
        (directory, stub, manifest)
    }
    #[test]
    fn unknown_preserves_review_and_cannot_enter_retry_failed() {
        let (dir, stub, mut manifest) = fixture(true);
        std::fs::write(stub.dir().join("outcomes/draft-1"), "unknown").unwrap();
        for _ in 0..2 {
            assert_eq!(
                dispatch_record(
                    &Mxr::new(Some(stub.binary())),
                    &mut manifest,
                    dir.path(),
                    "hash",
                    None,
                    true
                )
                .unwrap(),
                RecordStatus::Uncertain
            );
        }
        assert!(stub.calls_to("send").is_empty());
        assert_eq!(
            manifest.records[0].review.as_ref().unwrap().operation_id,
            "draft-1:1"
        );
    }
    #[test]
    fn lost_schedule_ack_never_becomes_immediate_or_rescheduled() {
        let (dir, stub, mut manifest) = fixture(true);
        manifest.records[0].review.as_mut().unwrap().scheduled_at =
            Some("2026-10-12T12:00:00Z".into());
        for at in [None, Some("2026-10-13T12:00:00Z")] {
            assert_eq!(
                dispatch_record(
                    &Mxr::new(Some(stub.binary())),
                    &mut manifest,
                    dir.path(),
                    "hash",
                    at,
                    true
                )
                .unwrap(),
                RecordStatus::ScheduleUncertain
            );
        }
        assert!(stub.calls_to("send").is_empty());
    }
    #[test]
    fn checkpoint_write_failure_prevents_submitting_the_reviewed_command() {
        let (dir, stub, mut manifest) = fixture(false);
        let blocked = dir.path().join("not-a-directory");
        std::fs::write(&blocked, "owned fixture").unwrap();
        assert!(dispatch_record(
            &Mxr::new(Some(stub.binary())),
            &mut manifest,
            &blocked,
            "hash",
            None,
            false
        )
        .is_err());
        assert!(stub.calls_to("send").is_empty());
    }
}
