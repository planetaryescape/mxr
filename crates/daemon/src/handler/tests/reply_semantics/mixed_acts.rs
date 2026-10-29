//! Stored independent acts retain owner attention across Messages, Desk and Now.
use super::*;
use mxr_core::asks::Actor;

#[tokio::test]
async fn review7_a_correspondent_promise_does_not_hide_a_separate_owner_reply_request() {
    const PROMISE: &str = "I'll send the report by tomorrow.";
    const QUESTION: &str = "Could you confirm the meeting time?";
    let mut failures = Vec::new();
    for age in [Duration::minutes(5), Duration::hours(13)] {
        for (text, expected) in [
            (format!("{PROMISE} {QUESTION}"), true),
            (PROMISE.into(), false),
            (QUESTION.into(), true),
        ] {
            let fx = fixture().await;
            let prior = fx
                .message(
                    &ThreadId::new(),
                    ME,
                    "riley@example.com",
                    Duration::days(2),
                    None,
                )
                .await;
            body(&fx, &prior, "Thanks for the earlier conversation.", None).await;
            settle(&fx, &prior).await;
            let mut mail = source(&fx, &text).await;
            mail.date = now() - age;
            fx.store_envelope(&mail, MessageDirection::Inbound).await;
            let (answer, turn, errors) = observe(&fx, &mail, expected).await;
            assert_eq!(errors, 0);
            if text.contains(PROMISE) {
                assert!(answer
                    .units
                    .iter()
                    .any(|u| u.answer == AskAnswer::TheyPromisedYou
                        && answer.quote(u) == Some(PROMISE)
                        && u.tags
                            .as_ref()
                            .is_some_and(|t| t.actor == Actor::Correspondent)));
            }
            if expected {
                assert!(answer
                    .units
                    .iter()
                    .any(|u| u.answer == AskAnswer::AsksYouToReply
                        && answer.quote(u) == Some(QUESTION)
                        && u.tags.as_ref().is_some_and(|t| t.actor == Actor::Owner)));
            }
            let messages = crate::handler::messages::messages_at(
                &fx.state,
                Some(&fx.account),
                Default::default(),
                50,
                now(),
            )
            .await
            .unwrap();
            let front = crate::handler::now::now_at(
                &fx.state,
                Some(&fx.account),
                now(),
                &chrono::FixedOffset::east_opt(0).unwrap(),
            )
            .await
            .unwrap();
            let desk = crate::handler::desk::compose_desk(&fx.state, Some(&fx.account), now())
                .await
                .unwrap();
            eprintln!("REVIEW7_MIXED age={age:?} text={text:?} turn={turn} messages={messages:?} now={front:?} desk={:?}", desk.rows);
            let messages_turn = !messages.your_turn.is_empty();
            if turn != expected || messages_turn != expected || (front.people.total > 0) != expected
            {
                failures.push(format!(
                    "age={age:?} text={text:?} turn={turn} Messages={} Now={}",
                    messages.your_turn.len(),
                    front.people.total
                ));
            }
            if !expected && age == Duration::hours(13) {
                assert!(desk.rows.iter().any(|r| r.thread_id == mail.thread_id
                    && r.lane == mxr_protocol::DeskLaneKind::Waiting));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

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
                &[],
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
            &[],
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
            delivered(&fx, &mail, until + Duration::seconds(1)).await,
            (expected, expected, expected)
        );
    }
}

#[tokio::test]
async fn review7_mixed_acts_keep_archive_automation_copy_and_owner_response_gates() {
    const TEXT: &str = "I'll send the report by tomorrow. Could you confirm the meeting time?";
    for control in ["archive", "automation", "copied", "owner_response"] {
        let fx = fixture().await;
        let mut mail = source(&fx, TEXT).await;
        let (_, turn, errors) = observe(&fx, &mail, true).await;
        assert_eq!(errors, 0);
        assert!(turn);
        match control {
            "archive" => fx
                .state
                .store
                .set_message_labels(&mail.id, &[], mxr_core::EventSource::User)
                .await
                .unwrap(),
            "automation" => {
                body(&fx, &mail, TEXT, Some("Auto-Submitted: auto-generated\r\n")).await;
            }
            "copied" => {
                mail.cc = mail.to.clone();
                mail.to[0].email = "dana@example.com".into();
                fx.store_envelope(&mail, MessageDirection::Inbound).await;
            }
            _ => {
                let reply = fx
                    .message(
                        &mail.thread_id,
                        ME,
                        "riley@example.com",
                        Duration::minutes(0),
                        mail.message_id_header.as_deref(),
                    )
                    .await;
                body(&fx, &reply, "Confirmed the meeting time.", None).await;
            }
        }
        let (_, turn, errors) = observe(&fx, &mail, control == "archive").await;
        assert_eq!(errors, 0, "{control}");
        assert!(!turn, "{control}");
        assert_eq!(
            delivered(&fx, &mail, now()).await,
            (false, false, false),
            "{control}"
        );
        assert!(fx
            .state
            .store
            .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
            .await
            .unwrap()
            .is_empty());
    }
}
