use super::*;

#[tokio::test]
async fn draft_attachment_resources_cleanup_retains_owner_absent_inflight_bytes() {
    for accepted in [false, true] {
        let (state, _, draft) = fixture().await;
        let resource = upload(&state, &draft, b"retained provider bytes").await;
        let identity = SendIdentity::for_draft(&draft);
        assert!(state
            .store
            .claim_draft_send(
                &identity,
                &draft.id,
                &draft.account_id,
                draft.revision.unwrap(),
                "owned-operation"
            )
            .await
            .unwrap());
        if accepted {
            state
                .store
                .accept_draft_send(
                    &identity,
                    &mxr_core::SendReceipt {
                        provider_message_id: Some("accepted-not-ingested".into()),
                        sent_at: chrono::Utc::now(),
                        rfc2822_message_id: "<retained@fake.local>".into(),
                    },
                )
                .await
                .unwrap();
        }
        // Account deletion can cascade the owner row; the durable outcome remains a reference.
        state.store.delete_draft(&draft.id).await.unwrap();
        sqlx::query("UPDATE draft_attachment_resources SET updated_at = 0")
            .execute(state.store.writer())
            .await
            .unwrap();
        assert!(state
            .store
            .stale_draft_attachments(1)
            .await
            .unwrap()
            .is_empty());
        let mut new_owner = draft.clone();
        new_owner.id = mxr_core::DraftId::new();
        new_owner.revision = None;
        let new_owner = saved(call(&state, Request::SaveDraft { draft: new_owner }).await);
        let trigger = metadata(&new_owner, b"cleanup trigger");
        ok(call(&state, Request::BeginDraftAttachment { metadata: trigger }).await);
        assert!(state
            .store
            .draft_attachment_dir()
            .join(resource.id.as_str())
            .join("final")
            .join(resource.filename)
            .exists());
        assert!(state
            .store
            .get_draft_attachment(&resource.id)
            .await
            .unwrap()
            .is_some());
    }
}

#[tokio::test]
async fn draft_attachment_resources_concurrent_sets_have_one_cas_winner() {
    let (state, _, draft) = fixture().await;
    let left = upload(&state, &draft, b"left bytes").await;
    let right = upload(&state, &draft, b"right bytes").await;
    let mut left_edit = draft.clone();
    left_edit.managed_attachments = Some(vec![left]);
    let mut right_edit = draft.clone();
    right_edit.managed_attachments = Some(vec![right]);
    let (left, right) = tokio::join!(
        call(&state, Request::UpdateDraft { draft: left_edit }),
        call(&state, Request::UpdateDraft { draft: right_edit })
    );
    assert_eq!(
        [&left, &right]
            .iter()
            .filter(|response| matches!(
                response,
                Response::Ok {
                    data: ResponseData::Draft { .. }
                }
            ))
            .count(),
        1
    );
    let winner = match left {
        Response::Ok {
            data: ResponseData::Draft { draft },
        } => {
            error(right, "revision");
            draft
        }
        other => {
            error(other, "revision");
            saved(right)
        }
    };
    assert_eq!(winner.revision, Some(2));
    assert_eq!(winner.managed_attachments.as_ref().unwrap().len(), 1);
}
