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
