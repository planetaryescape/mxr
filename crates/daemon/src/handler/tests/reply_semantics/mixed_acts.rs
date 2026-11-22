use super::*;

async fn delivered(fx: &Fixture, mail: &Envelope, at: chrono::DateTime<Utc>) -> (bool, bool, bool) {
    let desk = crate::handler::desk::compose_desk(&fx.state, Some(&fx.account), at)
        .await
        .unwrap();
    let messages = crate::handler::messages::messages_at(
        &fx.state,
        Some(&fx.account),
        Default::default(),
        50,
        at,
    )
    .await
    .unwrap();
    let front = crate::handler::now::now_at(
        &fx.state,
        Some(&fx.account),
        at,
        &chrono::FixedOffset::east_opt(0).unwrap(),
    )
    .await
    .unwrap();
    (
        desk.rows.iter().any(|r| {
            r.thread_id == mail.thread_id
                && matches!(
                    r.lane,
                    mxr_protocol::DeskLaneKind::Owed | mxr_protocol::DeskLaneKind::PeopleNew
                )
        }),
        messages
            .your_turn
            .iter()
            .any(|r| r.topics.iter().any(|t| t.thread_id == mail.thread_id)),
        front
            .people
            .rows
            .iter()
            .any(|r| r.row.thread_id == mail.thread_id),
    )
}

#[tokio::test]
async fn review7_mixed_and_pure_acts_keep_explicit_reply_later_and_exact_undo() {
    for text in [
        "I'll send the report by tomorrow. Could you confirm the meeting time?",
        "I'll send the report by tomorrow.",
        "Could you confirm the meeting time?",
    ] {
        let fx = fixture().await;
        let mail = source(&fx, text).await;
        let expected = text.contains('?');
        let (before, _, errors) = observe(&fx, &mail, expected).await;
        assert_eq!(errors, 0);
        let deferred_at = Utc::now();
        let until = deferred_at + Duration::hours(1);
        let changes: i64 = sqlx::query_scalar("SELECT total_changes()")
            .fetch_one(fx.state.store.writer())
            .await
            .unwrap();
        let ResponseData::ThreadsDeferred { items: preview, .. } =
            crate::handler::deferral::defer_threads_at(
                &fx.state,
                std::slice::from_ref(&mail.thread_id),
                until,
                true,
                deferred_at,
            )
            .await
            .unwrap()
        else {
            panic!("preview")
        };
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT total_changes()")
                .fetch_one(fx.state.store.writer())
                .await
                .unwrap(),
            changes
        );
        let ResponseData::ThreadsDeferred {
            items,
            mutation_id: Some(mutation_id),
            ..
        } = crate::handler::deferral::defer_threads_at(
            &fx.state,
            std::slice::from_ref(&mail.thread_id),
            until,
            false,
            deferred_at,
        )
        .await
        .unwrap()
        else {
            panic!("defer")
        };
        assert_eq!(items, preview);
        assert_eq!(items[0].kind, Some(mxr_protocol::DeferKindData::ReplyLater));
        assert!(items[0].error.is_none());
        assert_eq!(
            delivered(&fx, &mail, until - Duration::seconds(1)).await,
            (false, false, false)
        );
        assert_eq!(
            delivered(&fx, &mail, until + Duration::seconds(1)).await,
            (true, true, true),
            "{text}"
        );
        let after = fx
            .state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&mail.id))
            .await
            .unwrap();
        assert_eq!(after[&mail.id].units, before.units);
        request(&fx, Request::UndoMutation { mutation_id }).await;
        assert_eq!(
            delivered(&fx, &mail, now()).await,
            (expected, expected, expected)
        );
    }
}
