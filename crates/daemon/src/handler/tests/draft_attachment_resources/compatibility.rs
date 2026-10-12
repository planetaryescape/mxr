use super::*;

#[tokio::test]
async fn draft_attachment_resources_pre_upgrade_hash_replay_preserves_unknown_and_accepted() {
    for accepted in [false, true] {
        let (state, fake, draft) = fixture().await;
        let identity = SendIdentity::for_draft(&draft);
        // Released v0.6.82 serializes no managed field. Preserve that exact struct order, not a sorted JSON Value.
        let mut legacy = draft.clone();
        legacy.revision = None;
        legacy.created_at = chrono::DateTime::UNIX_EPOCH;
        legacy.updated_at = chrono::DateTime::UNIX_EPOCH;
        legacy.managed_attachments = None;
        let bytes = serde_json::to_vec(&legacy).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("managed_attachments"));
        let hash: String = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert!(state
            .store
            .claim_draft_send(
                &identity,
                &draft.id,
                &draft.account_id,
                draft.revision.unwrap(),
                &hash
            )
            .await
            .unwrap());
        if accepted {
            state
                .store
                .accept_draft_send(
                    &identity,
                    &mxr_core::SendReceipt {
                        provider_message_id: Some("old-accepted".into()),
                        sent_at: chrono::Utc::now(),
                        rfc2822_message_id: "<old@fake.local>".into(),
                    },
                )
                .await
                .unwrap();
        }
        for resources in [None, Some(vec![])] {
            let mut replay = draft.clone();
            replay.managed_attachments = resources;
            let response = call(
                &state,
                Request::SendDraft {
                    draft: replay,
                    send_identity: Some(identity.clone()),
                    override_safety_token: None,
                },
            )
            .await;
            if accepted {
                assert!(
                    matches!(ok(response), ResponseData::SendOutcome { outcome: Some(outcome) } if outcome.state == mxr_core::SendOutcomeState::Accepted)
                );
            } else {
                error(response, "uncertain");
            }
        }
        assert!(fake.sent_drafts().is_empty());
    }
}

#[tokio::test]
async fn draft_attachment_resources_old_schedule_marker_blocks_managed_but_allows_legacy() {
    let (state, fake, mut draft) = fixture().await;
    let send_at = chrono::Utc::now() + chrono::Duration::days(1);
    let request = Request::ScheduleSend {
        draft_id: draft.id.clone(),
        expected_revision: draft.revision,
        send_at,
        managed_attachments_reviewed: false,
    };
    ok(call(&state, request).await);
    ok(call(
        &state,
        Request::CancelScheduledSend {
            draft_id: draft.id.clone(),
        },
    )
    .await);
    // Scheduling advances the owner revision; fetch it before attaching bytes.
    draft = saved(
        call(
            &state,
            Request::GetDraft {
                draft_id: draft.id.clone(),
            },
        )
        .await,
    );
    let resource = upload(&state, &draft, b"invisible to an old client").await;
    draft.managed_attachments = Some(vec![resource]);
    draft = saved(call(&state, Request::UpdateDraft { draft }).await);
    let mut old = serde_json::to_value(Request::ScheduleSend {
        draft_id: draft.id.clone(),
        expected_revision: draft.revision,
        send_at,
        managed_attachments_reviewed: true,
    })
    .unwrap();
    old.as_object_mut()
        .unwrap()
        .remove("managed_attachments_reviewed");
    error(
        call(&state, serde_json::from_value(old).unwrap()).await,
        "upgrade",
    );
    assert!(state
        .store
        .list_pending_scheduled_sends(None)
        .await
        .unwrap()
        .is_empty());
    assert!(fake.sent_drafts().is_empty());
}
