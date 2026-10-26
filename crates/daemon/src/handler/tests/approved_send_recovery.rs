use super::send_outcomes::{
    call, fixture_with_account_and_sender, fixture_with_sender, outcome, send,
};
use super::*;
use mxr_core::{
    Draft, SendFailure, SendIdentity, SendOutcomeState, SendReceipt, SendSubmissionPhase,
};

#[derive(Default)]
struct RecoveringSender {
    calls: AtomicUsize,
    submissions: AtomicUsize,
    ambiguous: bool,
    always_wait: bool,
    retry_hint: Option<u64>,
    evidence: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl mxr_core::MailSendProvider for RecoveringSender {
    fn name(&self) -> &str {
        "gmail"
    }
    fn legacy_cooldown_retry_after_secs(&self, diagnostic: &str) -> Option<u64> {
        diagnostic
            .starts_with(
                "provider acceptance is uncertain: Rate limited by Gmail asked mxr to wait until ",
            )
            .then_some(900)
    }
    async fn send(
        &self,
        _draft: &Draft,
        _from: &mxr_core::Address,
        _message_id: &str,
    ) -> Result<SendReceipt, mxr_core::MxrError> {
        unreachable!()
    }
    async fn send_classified(
        &self,
        _draft: &Draft,
        _from: &mxr_core::Address,
        message_id: &str,
    ) -> Result<SendReceipt, SendFailure> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if (call == 0 || self.always_wait) && !self.ambiguous {
            let retry_after_secs = self.retry_hint.unwrap_or(90);
            return Err(SendFailure::NotSubmitted {
                phase: SendSubmissionPhase::Cooldown,
                retry_after_secs: Some(retry_after_secs),
                error: mxr_core::MxrError::RateLimited {
                    retry_after_secs,
                    detail: None,
                },
            });
        }
        self.submissions.fetch_add(1, Ordering::SeqCst);
        if self.ambiguous {
            return Err(SendFailure::Unknown(mxr_core::MxrError::Provider(
                "controlled lost response after submission".into(),
            )));
        }
        Ok(SendReceipt {
            provider_message_id: Some("accepted-synthetic".into()),
            rfc2822_message_id: message_id.into(),
            sent_at: chrono::Utc::now(),
        })
    }
    async fn reconcile_send(
        &self,
        message_id: &str,
        _from: &mxr_core::Address,
    ) -> Result<Option<SendReceipt>, mxr_core::MxrError> {
        Ok(self.evidence.load(Ordering::SeqCst).then(|| SendReceipt {
            provider_message_id: Some("evidence-synthetic".into()),
            rfc2822_message_id: message_id.into(),
            sent_at: chrono::Utc::now(),
        }))
    }
}

#[tokio::test]
async fn approved_send_recovery_unrepresentable_deadline_preserves_non_submission_and_phase() {
    let sender = Arc::new(RecoveringSender {
        retry_hint: Some(u64::MAX),
        ..Default::default()
    });
    let (state, _, draft) = fixture_with_sender(Some(sender.clone())).await;
    let identity = SendIdentity::for_draft(&draft);
    assert!(matches!(
        call(&state, send(&draft, identity.clone())).await,
        Response::Error { .. }
    ));
    let retained = outcome(&state, &draft, &identity).await;
    assert_eq!(retained.state, SendOutcomeState::NotAccepted);
    assert!(retained.rfc2822_message_id.is_some());
    assert_eq!(
        retained.recovery.unwrap().phase,
        SendSubmissionPhase::Cooldown
    );
    super::super::send_recovery::process_due(
        &state,
        chrono::Utc::now() + chrono::Duration::days(30),
    )
    .await
    .unwrap();
    assert_eq!(sender.calls.load(Ordering::SeqCst), 1);
    assert_eq!(sender.submissions.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn approved_send_recovery_remote_uncertainty_omits_local_provider_diagnostics() {
    let sender = Arc::new(RecoveringSender {
        ambiguous: true,
        ..Default::default()
    });
    let (state, _, draft) = fixture_with_sender(Some(sender)).await;
    let identity = SendIdentity::for_draft(&draft);
    let response =
        super::super::mutations::send_stored_draft_remote(&state, &draft.id, Some(&identity), None)
            .await
            .unwrap_err()
            .into_response();
    let Response::Error {
        message,
        code,
        retryable,
        details,
        ..
    } = response
    else {
        panic!("expected a protected uncertain outcome")
    };
    assert!(!message.contains("controlled lost response"));
    assert!(message.contains("do not resend"));
    assert_eq!(code, "send_outcome_unknown");
    assert!(!retryable);
    assert_eq!(details.unwrap()["outcome"]["state"], "unknown");
}

#[tokio::test]
async fn approved_send_recovery_stops_after_bounded_attempts_and_never_sends_an_unapproved_draft() {
    let sender = Arc::new(RecoveringSender {
        always_wait: true,
        ..Default::default()
    });
    let (state, _, draft) = fixture_with_sender(Some(sender.clone())).await;
    // A normal saved draft is not a queue entry, even far in the future.
    super::super::send_recovery::process_due(
        &state,
        chrono::Utc::now() + chrono::Duration::days(30),
    )
    .await
    .unwrap();
    assert_eq!(sender.calls.load(Ordering::SeqCst), 0);
    let identity = SendIdentity::for_draft(&draft);
    call(&state, send(&draft, identity.clone())).await;
    let mut last_retry = chrono::Utc::now();
    for attempt in 1..=12 {
        let current = state
            .store
            .get_send_outcome(&identity.operation_id)
            .await
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(current.recovery.as_ref().unwrap().attempts, attempt);
        if attempt == 12 {
            assert_eq!(current.state, SendOutcomeState::NotAccepted);
            assert_eq!(
                current.recovery.unwrap().reason.as_deref(),
                Some("retry_limit_reached")
            );
        } else {
            assert_eq!(current.state, SendOutcomeState::Waiting);
            let retry = current.recovery.unwrap().retry_at.unwrap();
            assert!(retry > last_retry);
            super::super::send_recovery::process_due(&state, retry - chrono::Duration::seconds(1))
                .await
                .unwrap();
            assert_eq!(sender.calls.load(Ordering::SeqCst), attempt as usize);
            super::super::send_recovery::process_due(&state, retry)
                .await
                .unwrap();
            last_retry = retry;
        }
    }
    assert_eq!(sender.submissions.load(Ordering::SeqCst), 0);
    super::super::send_recovery::process_due(&state, last_retry + chrono::Duration::days(1))
        .await
        .unwrap();
    assert_eq!(sender.calls.load(Ordering::SeqCst), 12);
}

#[tokio::test]
async fn approved_send_recovery_receipt_failure_resolves_from_evidence_without_a_second_send() {
    let sender = Arc::new(RecoveringSender {
        calls: AtomicUsize::new(1),
        ..Default::default()
    });
    let (state, _, draft) = fixture_with_sender(Some(sender.clone())).await;
    let identity = SendIdentity::for_draft(&draft);
    sqlx::query("CREATE TRIGGER fail_accept BEFORE UPDATE ON draft_send_outcomes WHEN NEW.state='accepted' BEGIN SELECT RAISE(ABORT,'receipt failure'); END").execute(state.store.writer()).await.unwrap();
    assert!(matches!(
        call(&state, send(&draft, identity.clone())).await,
        Response::Error { .. }
    ));
    let unknown = state
        .store
        .get_send_outcome(&identity.operation_id)
        .await
        .unwrap()
        .unwrap()
        .0;
    assert_eq!(unknown.state, SendOutcomeState::Unknown);
    assert!(unknown.rfc2822_message_id.is_some());
    sqlx::query("DROP TRIGGER fail_accept")
        .execute(state.store.writer())
        .await
        .unwrap();
    sender.evidence.store(true, Ordering::SeqCst);
    super::super::send_recovery::process_due(
        &state,
        chrono::Utc::now() + chrono::Duration::minutes(5),
    )
    .await
    .unwrap();
    let accepted = outcome(&state, &draft, &identity).await;
    assert_eq!(accepted.state, SendOutcomeState::Accepted);
    assert!(accepted.local_message_id.is_some());
    assert_eq!(accepted.rfc2822_message_id, unknown.rfc2822_message_id);
    assert_eq!(sender.submissions.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn approved_send_recovery_scheduled_wait_is_not_reported_as_sent() {
    let sender = Arc::new(RecoveringSender::default());
    let (state, _, draft) = fixture_with_sender(Some(sender.clone())).await;
    let now = chrono::Utc::now();
    state
        .store
        .schedule_send(&draft.id, now - chrono::Duration::minutes(1))
        .await
        .unwrap();
    assert_eq!(
        crate::loops::process_due_scheduled_sends(&state, now)
            .await
            .unwrap(),
        1
    );
    let waiting = state
        .store
        .get_draft_send_outcome(&draft.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(waiting.state, SendOutcomeState::Waiting);
    let attempt: String =
        sqlx::query_scalar("SELECT outcome FROM scheduled_send_attempts WHERE draft_id=?")
            .bind(draft.id.as_str())
            .fetch_one(state.store.reader())
            .await
            .unwrap();
    assert_eq!(attempt, "waiting");
    assert_eq!(sender.submissions.load(Ordering::SeqCst), 0);
    super::super::send_recovery::process_due(&state, waiting.recovery.unwrap().retry_at.unwrap())
        .await
        .unwrap();
    let attempt: String =
        sqlx::query_scalar("SELECT outcome FROM scheduled_send_attempts WHERE draft_id=?")
            .bind(draft.id.as_str())
            .fetch_one(state.store.reader())
            .await
            .unwrap();
    assert_eq!(attempt, "sent");
    assert_eq!(sender.submissions.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn approved_send_recovery_local_ingestion_repairs_acceptance_without_resending() {
    let sender = Arc::new(RecoveringSender {
        calls: AtomicUsize::new(1),
        ..Default::default()
    });
    let (state, _, draft) = fixture_with_sender(Some(sender.clone())).await;
    let identity = SendIdentity::for_draft(&draft);
    sqlx::query("CREATE TRIGGER fail_ingest BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT,'ingest failure'); END").execute(state.store.writer()).await.unwrap();
    call(&state, send(&draft, identity.clone())).await;
    let accepted = outcome(&state, &draft, &identity).await;
    assert_eq!(accepted.state, SendOutcomeState::Accepted);
    assert!(accepted.local_message_id.is_none());
    sqlx::query("DROP TRIGGER fail_ingest")
        .execute(state.store.writer())
        .await
        .unwrap();
    super::super::send_recovery::process_due(
        &state,
        chrono::Utc::now() + chrono::Duration::minutes(5),
    )
    .await
    .unwrap();
    let repaired = outcome(&state, &draft, &identity).await;
    assert_eq!(repaired.state, SendOutcomeState::Accepted);
    assert!(repaired.local_message_id.is_some());
    assert_eq!(repaired.rfc2822_message_id, accepted.rfc2822_message_id);
    assert_eq!(sender.submissions.load(Ordering::SeqCst), 1);
}

async fn legacy_fixture() -> (
    Arc<AppState>,
    Draft,
    SendIdentity,
    Arc<RecoveringSender>,
    tempfile::TempDir,
) {
    use sha2::{Digest, Sha256};
    let temp = tempfile::tempdir().unwrap();
    let sender = Arc::new(RecoveringSender {
        calls: AtomicUsize::new(1),
        ..Default::default()
    });
    let (mut state, _, mut draft) = fixture_with_sender(Some(sender.clone())).await;
    let account = crate::test_fixtures::test_account_with_id(draft.account_id.clone());
    draft.from = Some(mxr_core::Address {
        name: None,
        email: account.email.clone(),
    });
    let part = mxr_core::DraftAttachment {
        id: mxr_core::DraftAttachmentId::new(),
        draft_id: draft.id.clone(),
        account_id: draft.account_id.clone(),
        filename: "approved.pdf".into(),
        mime_type: "application/pdf".into(),
        content_id: None,
        content_location: None,
        length_bytes: 8,
        sha256: Sha256::digest(b"approved")
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    };
    draft.managed_attachments = Some(vec![part.clone()]);
    let store = mxr_store::Store::new(&temp.path().join("legacy.db"))
        .await
        .unwrap();
    store.insert_account(&account).await.unwrap();
    store.insert_draft(&draft).await.unwrap();
    Arc::get_mut(&mut state).unwrap().store = Arc::new(store);
    super::super::draft_attachments::import_forward(&state, &part, b"approved")
        .await
        .unwrap();
    let identity = SendIdentity::for_draft(&draft);
    state
        .store
        .set_draft_message_id_header(&draft.id, "<legacy-synthetic@example.test>")
        .await
        .unwrap();
    assert!(state
        .store
        .claim_draft_send(
            &identity,
            &draft.id,
            &draft.account_id,
            1,
            &mutations::send_command_hash(&draft).unwrap()
        )
        .await
        .unwrap());
    sqlx::query("UPDATE draft_send_outcomes SET rfc2822_message_id=NULL WHERE operation_id=?")
        .bind(&identity.operation_id)
        .execute(state.store.writer())
        .await
        .unwrap();
    std::fs::create_dir_all(temp.path().join("logs")).unwrap();
    (state, draft, identity, sender, temp)
}

fn legacy_log(identity: &SendIdentity) -> serde_json::Value {
    serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(), "level":"WARN", "target":"mxr::handler::mutations",
        "fields": { "operation_id":identity.operation_id,
            "message":"provider acceptance is uncertain: Rate limited by Gmail asked mxr to wait until 12:15:00 UTC; Gmail labels.list (rateLimitExceeded: User-rate limit exceeded; 4 requests in the last hour) — retry after 900s" }
    })
}

#[tokio::test]
async fn approved_send_recovery_legacy_gate_proof_previews_and_resumes_only_the_original_operation()
{
    let (state, draft, identity, sender, temp) = legacy_fixture().await;
    std::fs::write(
        temp.path().join("logs/mxr.log"),
        format!("{}\n", legacy_log(&identity)),
    )
    .unwrap();
    let request = |dry_run, proof| Request::RecoverSendOutcome {
        account_id: draft.account_id.clone(),
        operation_id: identity.operation_id.clone(),
        dry_run,
        expected_evidence_sha256: proof,
    };
    let preview = call(&state, request(true, None)).await;
    let Response::Ok {
        data:
            ResponseData::SendRecoveryPreview {
                recoverable: true,
                evidence_sha256: Some(proof),
                ..
            },
    } = preview
    else {
        panic!("expected matched evidence, got {preview:?}");
    };
    assert!(state
        .store
        .get_send_approval(&identity.operation_id)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        state
            .store
            .get_send_outcome(&identity.operation_id)
            .await
            .unwrap()
            .unwrap()
            .0
            .state,
        SendOutcomeState::Unknown
    );
    assert_eq!(sender.submissions.load(Ordering::SeqCst), 0);
    assert!(matches!(
        call(&state, request(false, Some("wrong-proof".into()))).await,
        Response::Error { .. }
    ));
    assert!(matches!(
        call(&state, request(false, Some(proof.clone()))).await,
        Response::Ok {
            data: ResponseData::SendOutcome { .. }
        }
    ));
    let waiting = outcome(&state, &draft, &identity).await;
    assert_eq!(waiting.state, SendOutcomeState::Waiting);
    assert!(
        waiting.recovery.as_ref().unwrap().retry_at.unwrap()
            >= chrono::Utc::now() + chrono::Duration::seconds(890),
        "legacy recovery must retain the original provider deadline even after a daemon restart"
    );
    assert_eq!(
        waiting.rfc2822_message_id.as_deref(),
        Some("<legacy-synthetic@example.test>")
    );
    assert_eq!(
        state
            .store
            .get_send_approval(&identity.operation_id)
            .await
            .unwrap()
            .unwrap()
            .legacy_evidence_sha256,
        Some(proof)
    );
    super::super::send_recovery::process_due(&state, waiting.recovery.unwrap().retry_at.unwrap())
        .await
        .unwrap();
    let accepted = outcome(&state, &draft, &identity).await;
    assert_eq!(accepted.state, SendOutcomeState::Accepted);
    assert_eq!(accepted.identity, identity);
    assert_eq!(sender.submissions.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn approved_send_recovery_legacy_absence_or_ambiguous_evidence_cannot_authorize_resending() {
    for change in [
        "absent",
        "ambiguous",
        "wrong_operation",
        "wrong_time",
        "draft_changed",
    ] {
        let (state, draft, identity, sender, temp) = legacy_fixture().await;
        let mut log = legacy_log(&identity);
        match change {
            "ambiguous" => {
                log["fields"]["message"] =
                    "provider acceptance is uncertain: timeout after HTTP dispatch".into();
            }
            "wrong_operation" => log["fields"]["operation_id"] = "other-operation".into(),
            "wrong_time" => {
                log["timestamp"] = (chrono::Utc::now() - chrono::Duration::hours(1))
                    .to_rfc3339()
                    .into();
            }
            "draft_changed" => {
                sqlx::query("UPDATE drafts SET revision=revision+1,subject='Changed' WHERE id=?")
                    .bind(draft.id.as_str())
                    .execute(state.store.writer())
                    .await
                    .unwrap();
            }
            _ => (),
        }
        if change != "absent" {
            std::fs::write(temp.path().join("logs/mxr.log"), format!("{log}\n")).unwrap();
        }
        let response = call(
            &state,
            Request::RecoverSendOutcome {
                account_id: draft.account_id.clone(),
                operation_id: identity.operation_id.clone(),
                dry_run: true,
                expected_evidence_sha256: None,
            },
        )
        .await;
        assert!(
            matches!(
                response,
                Response::Ok {
                    data: ResponseData::SendRecoveryPreview {
                        recoverable: false,
                        ..
                    }
                }
            ),
            "{change}: {response:?}"
        );
        assert_eq!(
            state
                .store
                .get_send_outcome(&identity.operation_id)
                .await
                .unwrap()
                .unwrap()
                .0
                .state,
            SendOutcomeState::Unknown
        );
        assert!(state
            .store
            .get_send_approval(&identity.operation_id)
            .await
            .unwrap()
            .is_none());
        assert_eq!(sender.submissions.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn approved_send_recovery_restarts_and_replays_the_same_intent_only_once() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("isolated.db");
    let sender = Arc::new(RecoveringSender::default());
    let (mut state, _, draft) = fixture_with_sender(Some(sender.clone())).await;
    let account = crate::test_fixtures::test_account_with_id(draft.account_id.clone());
    let store = mxr_store::Store::new(&path).await.unwrap();
    store.insert_account(&account).await.unwrap();
    store.insert_draft(&draft).await.unwrap();
    Arc::get_mut(&mut state).unwrap().store = Arc::new(store);
    let identity = SendIdentity::for_draft(&draft);
    assert!(matches!(
        call(&state, send(&draft, identity.clone())).await,
        Response::Ok {
            data: ResponseData::SendOutcome { .. }
        }
    ));
    let waiting = outcome(&state, &draft, &identity).await;
    assert_eq!(waiting.state, SendOutcomeState::Waiting);
    let retry_at = waiting.recovery.as_ref().unwrap().retry_at.unwrap();
    assert_eq!(sender.submissions.load(Ordering::SeqCst), 0);
    drop(state);

    let (mut restarted, _, _) =
        fixture_with_account_and_sender(draft.account_id.clone(), Some(sender.clone())).await;
    Arc::get_mut(&mut restarted).unwrap().store =
        Arc::new(mxr_store::Store::new(&path).await.unwrap());
    let replay = call(&restarted, send(&draft, identity.clone())).await;
    assert!(matches!(
        replay,
        Response::Ok {
            data: ResponseData::SendOutcome { .. }
        }
    ));
    assert_eq!(sender.calls.load(Ordering::SeqCst), 1);
    let (first, second) = tokio::join!(
        super::super::send_recovery::process_due(&restarted, retry_at),
        super::super::send_recovery::process_due(&restarted, retry_at)
    );
    assert_eq!(first.unwrap() + second.unwrap(), 1);
    assert_eq!(sender.submissions.load(Ordering::SeqCst), 1);
    let accepted = outcome(&restarted, &draft, &identity).await;
    assert_eq!(accepted.state, SendOutcomeState::Accepted);
    assert_eq!(accepted.rfc2822_message_id, waiting.rfc2822_message_id);
    let attempts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM approved_send_attempts WHERE operation_id=?")
            .bind(&identity.operation_id)
            .fetch_one(restarted.store.reader())
            .await
            .unwrap();
    assert_eq!(attempts, 2);
}

#[tokio::test]
async fn approved_send_recovery_draft_edits_owner_changes_and_changed_frozen_files_cancel_approval()
{
    for change in [
        "draft",
        "recipient",
        "sender",
        "owner",
        "attachment",
        "missing",
    ] {
        let sender = Arc::new(RecoveringSender::default());
        let (state, _, mut draft) = fixture_with_sender(Some(sender.clone())).await;
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("immutable.pdf");
        std::fs::write(&file, b"approved bytes").unwrap();
        draft.attachments.push(file);
        state.store.update_draft(&draft).await.unwrap();
        let draft = state.store.get_draft(&draft.id).await.unwrap().unwrap();
        let identity = SendIdentity::for_draft(&draft);
        call(&state, send(&draft, identity.clone())).await;
        let waiting = outcome(&state, &draft, &identity).await;
        assert_eq!(waiting.state, SendOutcomeState::Waiting);
        match change {
            "draft" => {
                let mut edited = draft.clone();
                edited.subject = "Unapproved new subject".into();
                assert!(state.store.update_draft(&edited).await.unwrap());
            }
            "recipient" => {
                let mut edited = draft.clone();
                edited.to[0].email = "unapproved@example.test".into();
                assert!(state.store.update_draft(&edited).await.unwrap());
            }
            "sender" => {
                sqlx::query("UPDATE accounts SET email='changed-sender@example.test' WHERE id=?")
                    .bind(draft.account_id.as_str())
                    .execute(state.store.writer())
                    .await
                    .unwrap();
            }
            "owner" => {
                sqlx::query("UPDATE archive_workspace SET recovery_generation=?")
                    .bind(mxr_core::DraftId::new().as_str())
                    .execute(state.store.writer())
                    .await
                    .unwrap();
            }
            _ => {
                let approval = state
                    .store
                    .get_send_approval(&identity.operation_id)
                    .await
                    .unwrap()
                    .unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(
                        &approval.files[0].path,
                        std::fs::Permissions::from_mode(0o600),
                    )
                    .unwrap();
                }
                if change == "missing" {
                    std::fs::remove_file(&approval.files[0].path).unwrap();
                } else {
                    std::fs::write(&approval.files[0].path, b"tampered").unwrap();
                }
            }
        }
        if matches!(change, "draft" | "recipient") {
            assert_eq!(
                state
                    .store
                    .get_send_outcome(&identity.operation_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .0
                    .state,
                SendOutcomeState::NotAccepted,
                "editing must revoke approval immediately"
            );
        }
        super::super::send_recovery::process_due(
            &state,
            waiting.recovery.unwrap().retry_at.unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(sender.submissions.load(Ordering::SeqCst), 0, "{change}");
        let cancelled = state
            .store
            .get_send_outcome(&identity.operation_id)
            .await
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(cancelled.state, SendOutcomeState::NotAccepted, "{change}");
        assert_eq!(
            cancelled.recovery.unwrap().reason.as_deref(),
            Some("approval_invalidated")
        );
    }
}

#[tokio::test]
async fn approved_send_recovery_ambiguous_submission_needs_positive_evidence_and_never_resends() {
    let sender = Arc::new(RecoveringSender {
        ambiguous: true,
        ..Default::default()
    });
    let (state, _, draft) = fixture_with_sender(Some(sender.clone())).await;
    let identity = SendIdentity::for_draft(&draft);
    assert!(matches!(
        call(&state, send(&draft, identity.clone())).await,
        Response::Error { .. }
    ));
    let unknown = state
        .store
        .get_send_outcome(&identity.operation_id)
        .await
        .unwrap()
        .unwrap()
        .0;
    assert_eq!(unknown.state, SendOutcomeState::Unknown);
    let future = chrono::Utc::now() + chrono::Duration::hours(1);
    super::super::send_recovery::process_due(&state, future)
        .await
        .unwrap();
    assert_eq!(
        state
            .store
            .get_send_outcome(&identity.operation_id)
            .await
            .unwrap()
            .unwrap()
            .0
            .state,
        SendOutcomeState::Unknown
    );
    assert_eq!(sender.submissions.load(Ordering::SeqCst), 1);
    sender.evidence.store(true, Ordering::SeqCst);
    super::super::send_recovery::process_due(&state, future + chrono::Duration::hours(1))
        .await
        .unwrap();
    let accepted = outcome(&state, &draft, &identity).await;
    assert_eq!(accepted.state, SendOutcomeState::Accepted);
    assert_eq!(accepted.rfc2822_message_id, unknown.rfc2822_message_id);
    assert_eq!(sender.calls.load(Ordering::SeqCst), 1);
}
